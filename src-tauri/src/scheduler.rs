//! The reminder scheduler.
//!
//! This is the single authoritative clock for the application: the webview never
//! counts down, it only renders snapshots pushed from here. That is what stops
//! re-renders or duplicate listeners from producing duplicate notifications.
//!
//! Lifecycle:
//!
//! ```text
//!   Running ──countdown ends──▶ Awaiting ──take_break──▶ OnBreak ──break ends──▶ Running
//!      ▲                          │  ▲                                              (fresh
//!      │                   snooze │  │ snooze ends                                 countdown)
//!      │                          ▼  │
//!      └──── resume "now" ────── Snoozed
//! ```
//!
//! Design notes:
//!
//! * A countdown that ends never restarts on its own. The schedule waits in
//!   `Awaiting` until the user takes a break or snoozes; only the end of the
//!   break starts the next countdown.
//! * Every due reminder gets an id. Choices name the id they answer, so a
//!   double click, or the popup and tray racing each other, cannot apply twice.
//! * The deadline is stored as a **wall-clock** `SystemTime`, so remaining time
//!   is correct after the process is suspended or the machine sleeps.
//! * A **suspend/resume** is detected by comparing the wall-clock delta against
//!   the monotonic delta between two ticks. `CLOCK_MONOTONIC` does not advance
//!   during suspend, so a large divergence means the machine slept.
//! * Missed deadlines are **never replayed**. A countdown, snooze or break that
//!   ended while the machine slept is dropped and a fresh countdown starts, so a
//!   resume cannot produce a burst or a stale popup.
//! * Settings are persisted; the countdown is not. `start()` always schedules a
//!   fresh interval, so a stale countdown can never be restored.

use std::time::{Duration, SystemTime};

use crate::clock::{add, to_epoch_ms, Clock};
use crate::model::{
    PendingReminder, ReminderKind, ScheduleState, Settings, Snapshot, BREAK_MINUTES,
    DEFAULT_SNOOZE_MINUTES,
};

/// How often the background worker calls [`Scheduler::tick`].
pub const TICK_INTERVAL_MS: u64 = 500;

/// Wall-clock/monotonic divergence above which we assume the machine slept.
/// Anything smaller is ordinary scheduling jitter.
pub const SUSPEND_DETECTION_THRESHOLD: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerConfig {
    pub interval: Duration,
    pub snooze: Duration,
    pub break_length: Duration,
    pub eyes_enabled: bool,
    pub movement_enabled: bool,
    pub snooze_enabled: bool,
}

impl SchedulerConfig {
    pub fn reminders_enabled(&self) -> bool {
        self.eyes_enabled || self.movement_enabled
    }
}

impl From<&Settings> for SchedulerConfig {
    fn from(settings: &Settings) -> Self {
        Self {
            interval: Duration::from_secs(u64::from(settings.interval_minutes) * 60),
            snooze: Duration::from_secs(u64::from(DEFAULT_SNOOZE_MINUTES) * 60),
            break_length: Duration::from_secs(u64::from(BREAK_MINUTES) * 60),
            eyes_enabled: settings.eyes_enabled,
            movement_enabled: settings.movement_enabled,
            snooze_enabled: settings.snooze_enabled,
        }
    }
}

/// Something the worker has to act on, returned by [`Scheduler::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A reminder is due. The schedule now waits for the user's choice.
    ReminderDue {
        reminder: PendingReminder,
        /// True when this fired at the end of a snooze.
        snoozed: bool,
    },
    /// The break the user chose has finished and a new countdown has started.
    BreakEnded { kind: ReminderKind },
}

pub struct Scheduler<C: Clock> {
    clock: C,
    config: SchedulerConfig,
    state: ScheduleState,
    /// Wall-clock end of the current countdown, snooze or break. `None` while
    /// paused, stopped or awaiting a choice.
    deadline: Option<SystemTime>,
    /// Time left when the schedule was paused, restored on resume.
    frozen: Duration,
    /// The category the user most recently took a break for, used to
    /// alternate. A reminder that is snoozed or abandoned is not "used up", so
    /// the same category comes back.
    last_kind: Option<ReminderKind>,
    /// Set while `Awaiting` (the reminder to answer) and `OnBreak` (the
    /// reminder being acted on). Cleared everywhere else.
    pending: Option<PendingReminder>,
    last_id: u64,
    last_mono: Duration,
    last_wall: SystemTime,
    missed: u32,
}

impl<C: Clock> Scheduler<C> {
    pub fn new(clock: C, config: SchedulerConfig) -> Self {
        let last_mono = clock.monotonic();
        let last_wall = clock.wall();
        Self {
            clock,
            config,
            state: ScheduleState::Stopped,
            deadline: None,
            frozen: Duration::ZERO,
            last_kind: None,
            pending: None,
            last_id: 0,
            last_mono,
            last_wall,
            missed: 0,
        }
    }

    pub fn state(&self) -> ScheduleState {
        self.state
    }

    pub fn config(&self) -> SchedulerConfig {
        self.config
    }

    /// Number of deadlines dropped because the machine was asleep across them.
    pub fn missed(&self) -> u32 {
        self.missed
    }

    /// Starts (or restarts) the schedule with a full interval. Never restores a
    /// previous countdown.
    pub fn start(&mut self) {
        self.state = ScheduleState::Running;
        self.frozen = Duration::ZERO;
        self.pending = None;
        self.schedule(self.config.interval);
    }

    /// Pauses from any active state. A countdown or snooze keeps its remaining
    /// time; a waiting reminder or a break is abandoned, so resuming starts a
    /// fresh countdown.
    pub fn pause(&mut self) {
        self.frozen = match self.state {
            ScheduleState::Running | ScheduleState::Snoozed => {
                self.remaining().unwrap_or(Duration::ZERO)
            }
            ScheduleState::Awaiting | ScheduleState::OnBreak => Duration::ZERO,
            ScheduleState::Paused | ScheduleState::Stopped => return,
        };
        self.state = ScheduleState::Paused;
        self.deadline = None;
        self.pending = None;
    }

    /// Resumes from a pause. If time was already up, a fresh interval is used.
    pub fn resume(&mut self) {
        if self.state != ScheduleState::Paused {
            return;
        }
        let remaining = if self.frozen.is_zero() {
            self.config.interval
        } else {
            self.frozen
        };
        self.state = ScheduleState::Running;
        self.frozen = Duration::ZERO;
        self.schedule(remaining);
    }

    /// Takes the break for the waiting reminder. Returns false, changing
    /// nothing, when no reminder is waiting or `expected` names a different one.
    pub fn take_break(&mut self, expected: Option<u64>) -> bool {
        let Some(reminder) = self.waiting_reminder(expected) else {
            return false;
        };
        // The category is used up only now, so alternation follows the breaks
        // the user actually took.
        self.last_kind = Some(reminder.kind);
        self.state = ScheduleState::OnBreak;
        self.frozen = Duration::ZERO;
        self.schedule(self.config.break_length);
        true
    }

    /// Postpones the next reminder by the snooze window. Any pending deadline
    /// is replaced, so repeated snoozes never stack up.
    ///
    /// With `expected`, only the named waiting reminder can be snoozed (the
    /// popup's action). Without it (the tray), a countdown, a snooze or a
    /// waiting reminder can be snoozed. Returns `Ok(false)` when there is
    /// nothing to snooze, and an error when snoozing is not allowed at all.
    pub fn snooze(&mut self, expected: Option<u64>) -> Result<bool, String> {
        if !self.config.reminders_enabled() {
            return Err("no reminder categories are enabled".to_string());
        }
        if !self.config.snooze_enabled {
            return Err("snoozing is turned off in settings".to_string());
        }
        let allowed = match expected {
            Some(_) => self.waiting_reminder(expected).is_some(),
            None => matches!(
                self.state,
                ScheduleState::Running | ScheduleState::Snoozed | ScheduleState::Awaiting
            ),
        };
        if !allowed {
            return Ok(false);
        }
        self.state = ScheduleState::Snoozed;
        self.frozen = Duration::ZERO;
        self.pending = None;
        self.schedule(self.config.snooze);
        Ok(true)
    }

    /// Terminal state reached while the application is shutting down.
    pub fn stop(&mut self) {
        self.state = ScheduleState::Stopped;
        self.deadline = None;
        self.frozen = Duration::ZERO;
        self.pending = None;
    }

    /// Starts or pauses the schedule.
    ///
    /// Starting always hands back a full interval, and pausing from a stopped
    /// schedule is normalised to *paused* rather than *quit* — "quit" only ever
    /// means the process is going away. Starting is ignored while a reminder
    /// waits for a choice or a break is running: those end through the user's
    /// choice or the break's own timer, never through a silent restart.
    pub fn set_running(&mut self, running: bool) {
        match (running, self.state) {
            (true, ScheduleState::Running | ScheduleState::Awaiting | ScheduleState::OnBreak)
            | (false, ScheduleState::Paused) => {}
            (true, ScheduleState::Paused) => self.resume(),
            (true, _) => self.start(),
            (false, _) => {
                if self.state == ScheduleState::Stopped {
                    self.start();
                }
                self.pause();
            }
        }
    }

    /// Replaces the configuration. Changing the interval restarts a running
    /// countdown so the next deadline is predictable; a waiting reminder or a
    /// break is left alone and the new interval applies to the next countdown.
    pub fn set_config(&mut self, config: SchedulerConfig) {
        let interval_changed = config.interval != self.config.interval;
        self.config = config;

        if self.state == ScheduleState::Awaiting && !config.reminders_enabled() {
            // Nothing to answer any more; carry on as an ordinary countdown.
            self.start();
            return;
        }

        if !interval_changed {
            return;
        }

        match self.state {
            ScheduleState::Running | ScheduleState::Snoozed => self.start(),
            // Keep the paused state but hand back a full interval on resume.
            ScheduleState::Paused => self.frozen = self.config.interval,
            ScheduleState::Awaiting | ScheduleState::OnBreak | ScheduleState::Stopped => {}
        }
    }

    /// The category that will be delivered next.
    pub fn next_kind(&self) -> Option<ReminderKind> {
        match (self.config.eyes_enabled, self.config.movement_enabled) {
            (false, false) => None,
            (true, false) => Some(ReminderKind::Eyes),
            (false, true) => Some(ReminderKind::Movement),
            // Both on: alternate, starting with an eye rest.
            (true, true) => Some(match self.last_kind {
                Some(ReminderKind::Eyes) => ReminderKind::Movement,
                _ => ReminderKind::Eyes,
            }),
        }
    }

    /// Advances the schedule. Returns an event when a reminder becomes due or
    /// a break ends.
    ///
    /// At most one event is returned per deadline, because acting on a
    /// deadline always clears or replaces it.
    pub fn tick(&mut self) -> Option<Event> {
        let now_wall = self.clock.wall();
        let now_mono = self.clock.monotonic();

        let wall_delta = now_wall
            .duration_since(self.last_wall)
            .unwrap_or(Duration::ZERO);
        let mono_delta = now_mono.saturating_sub(self.last_mono);
        let slept = wall_delta.abs_diff(mono_delta) > SUSPEND_DETECTION_THRESHOLD;
        self.last_wall = now_wall;
        self.last_mono = now_mono;

        if !matches!(
            self.state,
            ScheduleState::Running | ScheduleState::Snoozed | ScheduleState::OnBreak
        ) {
            return None;
        }

        let deadline = self.deadline?;
        if now_wall < deadline {
            return None;
        }

        if slept {
            // The deadline passed while the machine was asleep. Drop it rather
            // than replaying it, and give the user a full interval from now.
            self.missed = self.missed.saturating_add(1);
            self.start();
            return None;
        }

        if self.state == ScheduleState::OnBreak {
            let kind = self.pending.map(|reminder| reminder.kind);
            self.start();
            return kind.map(|kind| Event::BreakEnded { kind });
        }

        let snoozed = self.state == ScheduleState::Snoozed;
        match self.next_kind() {
            Some(kind) => {
                self.last_id = self.last_id.wrapping_add(1);
                let reminder = PendingReminder {
                    id: self.last_id,
                    kind,
                };
                self.state = ScheduleState::Awaiting;
                self.deadline = None;
                self.pending = Some(reminder);
                Some(Event::ReminderDue { reminder, snoozed })
            }
            // Every category is off: keep cycling so re-enabling one behaves
            // predictably, but never wait on a reminder nobody asked for.
            None => {
                self.start();
                None
            }
        }
    }

    /// Current view of the schedule for the UI.
    pub fn snapshot(&self, window_hidden: bool) -> Snapshot {
        let (remaining, total, next_at) = match self.state {
            ScheduleState::Running => (
                self.remaining().unwrap_or(self.config.interval),
                self.config.interval,
                self.deadline,
            ),
            ScheduleState::Snoozed => (
                self.remaining().unwrap_or(self.config.snooze),
                self.config.snooze,
                self.deadline,
            ),
            ScheduleState::OnBreak => (
                self.remaining().unwrap_or(self.config.break_length),
                self.config.break_length,
                self.deadline,
            ),
            ScheduleState::Paused => (self.frozen, self.config.interval, None),
            ScheduleState::Awaiting | ScheduleState::Stopped => {
                (Duration::ZERO, self.config.interval, None)
            }
        };

        let fraction = if total.is_zero() {
            0.0
        } else {
            (remaining.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
        };

        Snapshot {
            state: self.state,
            window_hidden,
            interval_minutes: (self.config.interval.as_secs() / 60) as u32,
            remaining_ms: remaining.as_millis() as i64,
            remaining_fraction: fraction,
            next_kind: match self.state {
                ScheduleState::Awaiting => self.pending.map(|reminder| reminder.kind),
                _ => self.next_kind(),
            },
            next_at_ms: next_at.map(to_epoch_ms),
            snooze_remaining_ms: (self.state == ScheduleState::Snoozed)
                .then_some(remaining.as_millis() as i64),
            pending: self.pending,
        }
    }

    /// The reminder waiting for a choice, if it matches `expected`.
    fn waiting_reminder(&self, expected: Option<u64>) -> Option<PendingReminder> {
        if self.state != ScheduleState::Awaiting {
            return None;
        }
        self.pending
            .filter(|reminder| expected.is_none_or(|id| id == reminder.id))
    }

    fn schedule(&mut self, length: Duration) {
        self.deadline = Some(add(self.clock.wall(), length));
    }

    fn remaining(&self) -> Option<Duration> {
        let deadline = self.deadline?;
        Some(
            deadline
                .duration_since(self.clock.wall())
                .unwrap_or(Duration::ZERO),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;

    fn minutes(value: u64) -> Duration {
        Duration::from_secs(value * 60)
    }

    fn config() -> SchedulerConfig {
        SchedulerConfig {
            interval: minutes(45),
            snooze: minutes(10),
            break_length: minutes(3),
            eyes_enabled: true,
            movement_enabled: true,
            snooze_enabled: true,
        }
    }

    fn setup() -> (TestClock, Scheduler<TestClock>) {
        let clock = TestClock::new();
        let scheduler = Scheduler::new(clock.clone(), config());
        (clock, scheduler)
    }

    /// Advances to the end of the current countdown and returns the reminder.
    fn reach_reminder(clock: &TestClock, scheduler: &mut Scheduler<TestClock>) -> PendingReminder {
        let remaining = Duration::from_millis(scheduler.snapshot(false).remaining_ms as u64);
        clock.advance(remaining);
        match scheduler.tick() {
            Some(Event::ReminderDue { reminder, .. }) => reminder,
            other => panic!("expected a due reminder, got {other:?}"),
        }
    }

    /// Takes the break for `reminder` and lets it run out.
    fn finish_break(
        clock: &TestClock,
        scheduler: &mut Scheduler<TestClock>,
        reminder: PendingReminder,
    ) {
        assert!(scheduler.take_break(Some(reminder.id)));
        clock.advance(minutes(3));
        assert_eq!(
            scheduler.tick(),
            Some(Event::BreakEnded {
                kind: reminder.kind
            })
        );
    }

    /* ------------------------------------------------------- countdown --- */

    #[test]
    fn a_fresh_scheduler_is_stopped_and_schedules_nothing() {
        let (_clock, scheduler) = setup();
        assert_eq!(scheduler.state(), ScheduleState::Stopped);
        assert_eq!(scheduler.snapshot(false).next_at_ms, None);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 0);
        assert_eq!(scheduler.snapshot(false).pending, None);
    }

    #[test]
    fn start_schedules_one_full_interval_and_starts_with_an_eye_rest() {
        let (_clock, mut scheduler) = setup();
        scheduler.start();

        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.state, ScheduleState::Running);
        assert_eq!(snapshot.remaining_ms, 45 * 60 * 1000);
        assert_eq!(snapshot.interval_minutes, 45);
        assert_eq!(snapshot.next_kind, Some(ReminderKind::Eyes));
        assert_eq!(snapshot.remaining_fraction, 1.0);
        assert!(snapshot.next_at_ms.is_some());
    }

    #[test]
    fn nothing_is_due_before_the_deadline() {
        let (clock, mut scheduler) = setup();
        scheduler.start();

        clock.advance(minutes(44));
        assert_eq!(scheduler.tick(), None);

        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.remaining_ms, 60 * 1000);
        assert!((snapshot.remaining_fraction - 1.0 / 45.0).abs() < 1e-9);
    }

    /* ---------------------------------------------- awaiting a choice --- */

    #[test]
    fn a_due_reminder_waits_for_a_choice_instead_of_restarting() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(45));

        let event = scheduler.tick().expect("a reminder is due");
        let Event::ReminderDue { reminder, snoozed } = event else {
            panic!("expected a reminder, got {event:?}");
        };
        assert_eq!(reminder.kind, ReminderKind::Eyes);
        assert!(!snoozed);

        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.state, ScheduleState::Awaiting);
        assert_eq!(snapshot.pending, Some(reminder));
        assert_eq!(snapshot.next_kind, Some(ReminderKind::Eyes));
        assert_eq!(snapshot.next_at_ms, None, "no countdown runs while waiting");
        assert_eq!(snapshot.remaining_ms, 0);
    }

    #[test]
    fn a_waiting_reminder_never_fires_twice_or_restarts_by_itself() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        reach_reminder(&clock, &mut scheduler);

        for _ in 0..20 {
            clock.advance(minutes(30));
            assert_eq!(scheduler.tick(), None);
        }
        assert_eq!(scheduler.state(), ScheduleState::Awaiting);
    }

    #[test]
    fn each_due_reminder_gets_a_new_id() {
        let (clock, mut scheduler) = setup();
        scheduler.start();

        let first = reach_reminder(&clock, &mut scheduler);
        finish_break(&clock, &mut scheduler, first);
        let second = reach_reminder(&clock, &mut scheduler);

        assert_ne!(first.id, second.id);
    }

    /* ------------------------------------------------------------ break --- */

    #[test]
    fn taking_a_break_runs_three_minutes_then_starts_the_next_countdown() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        assert!(scheduler.take_break(Some(reminder.id)));
        let on_break = scheduler.snapshot(false);
        assert_eq!(on_break.state, ScheduleState::OnBreak);
        assert_eq!(on_break.remaining_ms, 3 * 60 * 1000);
        assert_eq!(on_break.remaining_fraction, 1.0);
        assert_eq!(on_break.pending, Some(reminder));

        clock.advance(minutes(2));
        assert_eq!(scheduler.tick(), None);

        clock.advance(minutes(1));
        assert_eq!(
            scheduler.tick(),
            Some(Event::BreakEnded {
                kind: ReminderKind::Eyes
            })
        );

        let after = scheduler.snapshot(false);
        assert_eq!(after.state, ScheduleState::Running);
        assert_eq!(after.remaining_ms, 45 * 60 * 1000);
        assert_eq!(after.pending, None);
        assert_eq!(after.next_kind, Some(ReminderKind::Movement));
        assert_eq!(
            scheduler.tick(),
            None,
            "the end of a break is reported once"
        );
    }

    #[test]
    fn a_second_break_request_for_the_same_reminder_is_ignored() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        assert!(scheduler.take_break(Some(reminder.id)));
        clock.advance(minutes(1));
        assert!(!scheduler.take_break(Some(reminder.id)), "double click");
        assert_eq!(
            scheduler.snapshot(false).remaining_ms,
            2 * 60 * 1000,
            "the running break was not restarted"
        );
    }

    #[test]
    fn a_choice_for_a_stale_reminder_is_ignored() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        assert!(!scheduler.take_break(Some(reminder.id + 1)));
        assert_eq!(scheduler.snooze(Some(reminder.id + 1)), Ok(false));
        assert_eq!(scheduler.state(), ScheduleState::Awaiting);
    }

    #[test]
    fn popup_and_tray_racing_apply_only_the_first_choice() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        // The popup takes the break, then the tray's snooze arrives for the
        // same reminder.
        assert!(scheduler.take_break(Some(reminder.id)));
        assert_eq!(scheduler.snooze(Some(reminder.id)), Ok(false));
        assert_eq!(scheduler.state(), ScheduleState::OnBreak);

        // And the other way round.
        clock.advance(minutes(3));
        scheduler.tick();
        let next = reach_reminder(&clock, &mut scheduler);
        assert_eq!(scheduler.snooze(Some(next.id)), Ok(true));
        assert!(!scheduler.take_break(Some(next.id)));
        assert_eq!(scheduler.state(), ScheduleState::Snoozed);
    }

    #[test]
    fn breaks_cannot_be_taken_without_a_waiting_reminder() {
        let (_clock, mut scheduler) = setup();
        assert!(!scheduler.take_break(None), "stopped");
        scheduler.start();
        assert!(!scheduler.take_break(None), "running");
        scheduler.pause();
        assert!(!scheduler.take_break(None), "paused");
    }

    #[test]
    fn categories_alternate_with_the_breaks_taken() {
        let (clock, mut scheduler) = setup();
        scheduler.start();

        let mut delivered = Vec::new();
        for _ in 0..4 {
            let reminder = reach_reminder(&clock, &mut scheduler);
            delivered.push(reminder.kind);
            finish_break(&clock, &mut scheduler, reminder);
        }

        assert_eq!(
            delivered,
            vec![
                ReminderKind::Eyes,
                ReminderKind::Movement,
                ReminderKind::Eyes,
                ReminderKind::Movement
            ]
        );
    }

    #[test]
    fn the_enabled_category_is_used_every_time_when_only_one_is_on() {
        let (clock, mut scheduler) = setup();
        let mut config = config();
        config.eyes_enabled = false;
        scheduler.set_config(config);
        scheduler.start();

        for _ in 0..3 {
            let reminder = reach_reminder(&clock, &mut scheduler);
            assert_eq!(reminder.kind, ReminderKind::Movement);
            finish_break(&clock, &mut scheduler, reminder);
        }
        assert_eq!(
            scheduler.snapshot(false).next_kind,
            Some(ReminderKind::Movement)
        );
    }

    #[test]
    fn nothing_is_delivered_when_every_category_is_off() {
        let (clock, mut scheduler) = setup();
        let mut config = config();
        config.eyes_enabled = false;
        config.movement_enabled = false;
        scheduler.set_config(config);
        scheduler.start();

        assert_eq!(scheduler.snapshot(false).next_kind, None);

        for _ in 0..3 {
            clock.advance(minutes(45));
            assert_eq!(scheduler.tick(), None);
        }
        // The cycle still runs, so re-enabling a category behaves predictably.
        assert_eq!(scheduler.state(), ScheduleState::Running);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 45 * 60 * 1000);
    }

    /* ----------------------------------------------------------- snooze --- */

    #[test]
    fn snoozing_a_waiting_reminder_brings_the_same_kind_back_in_ten_minutes() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        assert_eq!(scheduler.snooze(Some(reminder.id)), Ok(true));
        let snoozed = scheduler.snapshot(false);
        assert_eq!(snoozed.state, ScheduleState::Snoozed);
        assert_eq!(snoozed.remaining_ms, 10 * 60 * 1000);
        assert_eq!(snoozed.snooze_remaining_ms, Some(10 * 60 * 1000));
        assert_eq!(snoozed.pending, None);

        clock.advance(minutes(9));
        assert_eq!(scheduler.tick(), None);

        clock.advance(minutes(1));
        let Some(Event::ReminderDue {
            reminder: again,
            snoozed,
        }) = scheduler.tick()
        else {
            panic!("the snoozed reminder comes back");
        };
        assert!(snoozed);
        assert_eq!(again.kind, reminder.kind, "same category, not the next one");
        assert_ne!(again.id, reminder.id);
        assert_eq!(scheduler.state(), ScheduleState::Awaiting);
        assert_eq!(scheduler.tick(), None);
    }

    #[test]
    fn snoozing_a_countdown_from_the_tray_postpones_the_reminder() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(2));

        assert_eq!(scheduler.snooze(None), Ok(true));
        assert_eq!(scheduler.snapshot(false).remaining_ms, 10 * 60 * 1000);

        clock.advance(minutes(10));
        assert!(matches!(scheduler.tick(), Some(Event::ReminderDue { .. })));
    }

    #[test]
    fn snoozing_twice_does_not_queue_two_reminders() {
        let (clock, mut scheduler) = setup();
        scheduler.start();

        clock.advance(minutes(2));
        assert_eq!(scheduler.snooze(None), Ok(true));
        clock.advance(minutes(4));
        assert_eq!(scheduler.snooze(None), Ok(true));

        // The second snooze replaced the first deadline.
        assert_eq!(scheduler.snapshot(false).remaining_ms, 10 * 60 * 1000);
        clock.advance(minutes(8));
        assert_eq!(scheduler.tick(), None);

        clock.advance(minutes(2));
        assert!(scheduler.tick().is_some());
        assert_eq!(scheduler.tick(), None);
    }

    #[test]
    fn snoozing_is_ignored_while_paused_or_on_a_break() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);
        scheduler.take_break(Some(reminder.id));
        assert_eq!(scheduler.snooze(None), Ok(false));
        assert_eq!(scheduler.state(), ScheduleState::OnBreak);

        scheduler.pause();
        assert_eq!(scheduler.snooze(None), Ok(false));
        assert_eq!(scheduler.state(), ScheduleState::Paused);
    }

    #[test]
    fn snoozing_is_refused_when_disabled_or_when_nothing_is_enabled() {
        let (_clock, mut scheduler) = setup();
        let mut no_snooze = config();
        no_snooze.snooze_enabled = false;
        scheduler.set_config(no_snooze);
        scheduler.start();
        assert!(scheduler.snooze(None).is_err());

        let mut quiet = config();
        quiet.eyes_enabled = false;
        quiet.movement_enabled = false;
        scheduler.set_config(quiet);
        assert!(scheduler.snooze(None).is_err());
    }

    /* ---------------------------------------------------- pause/resume --- */

    #[test]
    fn pausing_freezes_the_countdown_and_suspends_delivery() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(30));

        scheduler.pause();
        let paused = scheduler.snapshot(false);
        assert_eq!(paused.state, ScheduleState::Paused);
        assert_eq!(paused.remaining_ms, 15 * 60 * 1000);
        assert_eq!(paused.next_at_ms, None, "nothing is scheduled while paused");

        // Well past the original deadline, still nothing happens.
        clock.advance(minutes(120));
        assert_eq!(scheduler.tick(), None);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 15 * 60 * 1000);
    }

    #[test]
    fn resuming_continues_the_remaining_time() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(30));
        scheduler.pause();
        clock.advance(minutes(10));

        scheduler.resume();
        assert_eq!(scheduler.snapshot(false).remaining_ms, 15 * 60 * 1000);

        clock.advance(minutes(15));
        assert!(matches!(
            scheduler.tick(),
            Some(Event::ReminderDue { reminder, snoozed: false }) if reminder.kind == ReminderKind::Eyes
        ));
    }

    #[test]
    fn set_running_resumes_the_remaining_time_after_a_pause() {
        let (clock, mut scheduler) = setup();
        let mut config = config();
        config.interval = minutes(20);
        scheduler.set_config(config);
        scheduler.start();

        clock.advance(minutes(2));
        scheduler.set_running(false);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 18 * 60 * 1000);

        clock.advance(minutes(30));
        scheduler.set_running(true);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 18 * 60 * 1000);
    }

    #[test]
    fn resuming_after_the_remaining_time_elapsed_uses_a_full_interval() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(45));
        scheduler.pause();
        assert_eq!(scheduler.snapshot(false).remaining_ms, 0);

        scheduler.resume();
        assert_eq!(scheduler.snapshot(false).remaining_ms, 45 * 60 * 1000);
    }

    #[test]
    fn pausing_a_waiting_reminder_abandons_it_and_resume_starts_fresh() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        scheduler.pause();
        let paused = scheduler.snapshot(false);
        assert_eq!(paused.state, ScheduleState::Paused);
        assert_eq!(paused.pending, None);
        assert!(!scheduler.take_break(Some(reminder.id)));

        scheduler.resume();
        let resumed = scheduler.snapshot(false);
        assert_eq!(resumed.state, ScheduleState::Running);
        assert_eq!(resumed.remaining_ms, 45 * 60 * 1000);
        assert_eq!(
            resumed.next_kind,
            Some(ReminderKind::Eyes),
            "an abandoned reminder is not used up"
        );
    }

    #[test]
    fn pausing_a_break_abandons_it_and_resume_starts_fresh() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);
        scheduler.take_break(Some(reminder.id));
        clock.advance(minutes(1));

        scheduler.pause();
        assert_eq!(scheduler.snapshot(false).pending, None);
        clock.advance(minutes(10));
        assert_eq!(
            scheduler.tick(),
            None,
            "a paused break never reports its end"
        );

        scheduler.resume();
        assert_eq!(scheduler.snapshot(false).remaining_ms, 45 * 60 * 1000);
    }

    #[test]
    fn starting_never_skips_a_waiting_reminder_or_a_running_break() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        scheduler.set_running(true);
        assert_eq!(scheduler.state(), ScheduleState::Awaiting);

        scheduler.take_break(Some(reminder.id));
        clock.advance(minutes(1));
        scheduler.set_running(true);
        assert_eq!(scheduler.state(), ScheduleState::OnBreak);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 2 * 60 * 1000);
    }

    #[test]
    fn resume_now_from_a_snooze_starts_a_full_countdown() {
        let (_clock, mut scheduler) = setup();
        scheduler.start();
        scheduler.snooze(None).expect("allowed");

        scheduler.set_running(true);
        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.state, ScheduleState::Running);
        assert_eq!(snapshot.remaining_ms, 45 * 60 * 1000);
    }

    /* ----------------------------------------------------------- config --- */

    #[test]
    fn changing_the_interval_while_running_restarts_the_countdown() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(30));
        assert_eq!(scheduler.snapshot(false).remaining_ms, 15 * 60 * 1000);

        let mut config = config();
        config.interval = minutes(20);
        scheduler.set_config(config);

        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.interval_minutes, 20);
        assert_eq!(snapshot.remaining_ms, 20 * 60 * 1000);

        clock.advance(minutes(20));
        assert!(scheduler.tick().is_some());
    }

    #[test]
    fn changing_the_interval_while_snoozed_cancels_the_snooze() {
        let (_clock, mut scheduler) = setup();
        scheduler.start();
        scheduler.snooze(None).expect("snooze is allowed");

        let mut config = config();
        config.interval = minutes(60);
        scheduler.set_config(config);

        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.state, ScheduleState::Running);
        assert_eq!(snapshot.remaining_ms, 60 * 60 * 1000);
        assert_eq!(snapshot.snooze_remaining_ms, None);
    }

    #[test]
    fn changing_the_interval_while_paused_is_applied_on_resume() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(30));
        scheduler.pause();

        let mut config = config();
        config.interval = minutes(20);
        scheduler.set_config(config);

        assert_eq!(scheduler.snapshot(false).remaining_ms, 20 * 60 * 1000);
        scheduler.resume();
        assert_eq!(scheduler.snapshot(false).remaining_ms, 20 * 60 * 1000);
    }

    #[test]
    fn changing_the_interval_leaves_a_waiting_reminder_and_a_break_alone() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        let mut shorter = config();
        shorter.interval = minutes(20);
        scheduler.set_config(shorter);
        assert_eq!(scheduler.state(), ScheduleState::Awaiting);
        assert_eq!(scheduler.snapshot(false).pending, Some(reminder));

        scheduler.take_break(Some(reminder.id));
        let mut longer = config();
        longer.interval = minutes(60);
        scheduler.set_config(longer);
        assert_eq!(scheduler.state(), ScheduleState::OnBreak);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 3 * 60 * 1000);

        clock.advance(minutes(3));
        scheduler.tick();
        assert_eq!(
            scheduler.snapshot(false).remaining_ms,
            60 * 60 * 1000,
            "the new interval applies to the next countdown"
        );
    }

    #[test]
    fn turning_every_category_off_releases_a_waiting_reminder() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        reach_reminder(&clock, &mut scheduler);

        let mut quiet = config();
        quiet.eyes_enabled = false;
        quiet.movement_enabled = false;
        scheduler.set_config(quiet);

        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.state, ScheduleState::Running);
        assert_eq!(snapshot.pending, None);
    }

    #[test]
    fn toggling_snooze_or_categories_does_not_move_the_deadline() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(10));
        let before = scheduler.snapshot(false).next_at_ms;

        let mut config = config();
        config.snooze_enabled = false;
        config.movement_enabled = false;
        scheduler.set_config(config);

        assert_eq!(scheduler.snapshot(false).next_at_ms, before);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 35 * 60 * 1000);
    }

    /* ---------------------------------------------------- sleep/resume --- */

    #[test]
    fn a_suspend_across_the_deadline_drops_it_instead_of_bursting() {
        let (clock, mut scheduler) = setup();
        scheduler.start();

        // 90 minutes of wall-clock time pass, but only a second of monotonic
        // time: the machine was asleep.
        clock.advance_wall_only(minutes(90));
        clock.advance(Duration::from_secs(1));

        assert_eq!(scheduler.tick(), None, "no burst of missed reminders");
        assert_eq!(scheduler.missed(), 1);

        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.state, ScheduleState::Running);
        assert_eq!(snapshot.remaining_ms, 45 * 60 * 1000);

        // And the schedule keeps working normally afterwards.
        clock.advance(minutes(45));
        assert!(scheduler.tick().is_some());
    }

    #[test]
    fn a_suspend_shorter_than_the_deadline_keeps_the_deadline() {
        let (clock, mut scheduler) = setup();
        scheduler.start();

        // Ten minutes of wall-clock time pass while the machine sleeps, but the
        // monotonic reading does not move at all: CLOCK_MONOTONIC pauses during
        // suspend. That divergence is the signal.
        clock.advance_wall_only(minutes(10));
        scheduler.tick();

        // The deadline is still in the future, so it is kept — the countdown is
        // simply recalculated against the wall clock.
        assert_eq!(scheduler.snapshot(false).remaining_ms, 35 * 60 * 1000);
        assert_eq!(scheduler.missed(), 0);

        clock.advance(minutes(35));
        assert!(scheduler.tick().is_some());
    }

    #[test]
    fn resuming_from_suspend_recalculates_the_remaining_time() {
        let (clock, mut scheduler) = setup();
        scheduler.start();

        // Sleep for 40 minutes: the deadline is now 5 minutes in the future.
        clock.advance_wall_only(minutes(40));
        scheduler.tick();

        assert_eq!(scheduler.snapshot(false).remaining_ms, 5 * 60 * 1000);

        clock.advance(minutes(5));
        assert!(scheduler.tick().is_some());
    }

    #[test]
    fn a_snooze_that_ends_during_sleep_starts_a_fresh_countdown() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        scheduler.snooze(None).expect("allowed");

        clock.advance_wall_only(minutes(60));
        assert_eq!(scheduler.tick(), None, "no stale popup after waking");
        assert_eq!(scheduler.state(), ScheduleState::Running);
        assert_eq!(scheduler.snapshot(false).remaining_ms, 45 * 60 * 1000);
    }

    #[test]
    fn a_break_that_ends_during_sleep_starts_the_countdown_silently() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);
        scheduler.take_break(Some(reminder.id));

        clock.advance_wall_only(minutes(30));
        assert_eq!(scheduler.tick(), None);
        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.state, ScheduleState::Running);
        assert_eq!(snapshot.remaining_ms, 45 * 60 * 1000);
        assert_eq!(snapshot.next_kind, Some(ReminderKind::Movement));
    }

    #[test]
    fn a_waiting_reminder_survives_sleep() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        let reminder = reach_reminder(&clock, &mut scheduler);

        clock.advance_wall_only(minutes(120));
        assert_eq!(scheduler.tick(), None);
        assert_eq!(scheduler.snapshot(false).pending, Some(reminder));
    }

    /* ------------------------------------------------------------ misc --- */

    #[test]
    fn stopped_schedules_never_fire() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        scheduler.stop();

        assert_eq!(scheduler.state(), ScheduleState::Stopped);
        clock.advance(minutes(200));
        assert_eq!(scheduler.tick(), None);
        let snapshot = scheduler.snapshot(false);
        assert_eq!(snapshot.next_at_ms, None);
        assert_eq!(snapshot.remaining_ms, 0);
    }

    #[test]
    fn starting_again_never_restores_a_stale_countdown() {
        let (clock, mut scheduler) = setup();
        scheduler.start();
        clock.advance(minutes(44));

        // Simulate a restart: a brand new scheduler must not resume at 1 minute.
        let fresh = Scheduler::new(clock.clone(), config());
        assert_eq!(fresh.snapshot(false).remaining_ms, 0);

        let mut fresh = fresh;
        fresh.start();
        assert_eq!(fresh.snapshot(false).remaining_ms, 45 * 60 * 1000);
    }

    #[test]
    fn window_visibility_is_tracked_separately_from_the_schedule() {
        let (_clock, mut scheduler) = setup();
        scheduler.start();

        let hidden = scheduler.snapshot(true);
        assert!(hidden.window_hidden);
        assert_eq!(hidden.state, ScheduleState::Running);

        scheduler.pause();
        let paused_and_hidden = scheduler.snapshot(true);
        assert!(paused_and_hidden.window_hidden);
        assert_eq!(paused_and_hidden.state, ScheduleState::Paused);
    }

    #[test]
    fn config_is_derived_from_settings() {
        let settings = Settings {
            interval_minutes: 30,
            eyes_enabled: false,
            movement_enabled: true,
            snooze_enabled: false,
            ..Settings::default()
        };
        let config = SchedulerConfig::from(&settings);

        assert_eq!(config.interval, minutes(30));
        assert_eq!(config.snooze, minutes(10));
        assert_eq!(config.break_length, minutes(3));
        assert!(config.reminders_enabled());
        assert!(!config.snooze_enabled);
    }
}
