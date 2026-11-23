//! Lifecycle of Breathe's own reminder popup.
//!
//! This module decides *what* the popup shows and *when* it hides. It knows
//! nothing about windows, so every rule is unit-testable; `popup_window`
//! turns the decisions into a real window.
//!
//! Rules:
//!
//! * There is at most one popup. Showing new content replaces the old one.
//! * A reminder popup has no timer: it stays until the reminder stops waiting
//!   (the user chose, paused, or settings released it), or the user hides it.
//!   Hiding it never answers the reminder; the main window and tray still do.
//! * Informational popups (break over, test) hide themselves after a while.
//! * Showing the reminder that is already on screen is a no-op, so a repeated
//!   event can never produce a second popup or restart its animation.

use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::model::{PendingReminder, ReminderKind, ScheduleState, Snapshot};

/// How long an informational popup stays on screen.
pub const INFO_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PopupContent {
    /// A reminder is due; the popup offers the break and the snooze.
    Reminder { reminder: PendingReminder },
    /// The break the user took has ended.
    BreakOver { kind: ReminderKind },
    /// Preview from Settings. Shows a reminder without actions.
    Test { kind: ReminderKind },
}

/// What the popup window renders. `seq` changes with every new popup, so the
/// frontend can restart its entrance animation and dismiss exactly the popup
/// it is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopupView {
    pub seq: u64,
    pub content: PopupContent,
}

/// A change the window layer has to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupChange {
    Show(PopupView),
    Hide,
}

#[derive(Debug, Default)]
pub struct PopupManager {
    current: Option<Shown>,
    last_seq: u64,
}

#[derive(Debug, Clone, Copy)]
struct Shown {
    view: PopupView,
    hide_at: Option<SystemTime>,
}

impl PopupManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<PopupView> {
        self.current.map(|shown| shown.view)
    }

    /// Shows `content`, replacing whatever is visible. Returns `None` when the
    /// same reminder is already on screen.
    pub fn show(&mut self, content: PopupContent, now: SystemTime) -> Option<PopupChange> {
        if let Some(shown) = self.current {
            if shown.view.content == content && matches!(content, PopupContent::Reminder { .. }) {
                return None;
            }
        }

        self.last_seq = self.last_seq.wrapping_add(1);
        let view = PopupView {
            seq: self.last_seq,
            content,
        };
        let hide_at = match content {
            PopupContent::Reminder { .. } => None,
            PopupContent::BreakOver { .. } | PopupContent::Test { .. } => {
                now.checked_add(INFO_TIMEOUT)
            }
        };
        self.current = Some(Shown { view, hide_at });
        Some(PopupChange::Show(view))
    }

    /// Hides the popup the user dismissed. A stale `seq` (the popup was already
    /// replaced) is ignored.
    pub fn dismiss(&mut self, seq: u64) -> Option<PopupChange> {
        match self.current {
            Some(shown) if shown.view.seq == seq => {
                self.current = None;
                Some(PopupChange::Hide)
            }
            _ => None,
        }
    }

    /// Called on every scheduler tick. Hides an informational popup whose time
    /// is up, and a reminder popup whose reminder is no longer waiting.
    pub fn reconcile(&mut self, snapshot: &Snapshot, now: SystemTime) -> Option<PopupChange> {
        let shown = self.current?;

        let expired = shown.hide_at.is_some_and(|hide_at| now >= hide_at);
        let answered = match shown.view.content {
            PopupContent::Reminder { reminder } => {
                snapshot.state != ScheduleState::Awaiting || snapshot.pending != Some(reminder)
            }
            PopupContent::BreakOver { .. } | PopupContent::Test { .. } => false,
        };

        if expired || answered {
            self.current = None;
            Some(PopupChange::Hide)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + seconds)
    }

    fn reminder(id: u64) -> PendingReminder {
        PendingReminder {
            id,
            kind: ReminderKind::Eyes,
        }
    }

    fn snapshot(state: ScheduleState, pending: Option<PendingReminder>) -> Snapshot {
        Snapshot {
            state,
            window_hidden: true,
            interval_minutes: 45,
            remaining_ms: 0,
            remaining_fraction: 0.0,
            next_kind: Some(ReminderKind::Eyes),
            next_at_ms: None,
            snooze_remaining_ms: None,
            pending,
        }
    }

    fn shown(change: Option<PopupChange>) -> PopupView {
        match change {
            Some(PopupChange::Show(view)) => view,
            other => panic!("expected the popup to show, got {other:?}"),
        }
    }

    #[test]
    fn a_reminder_popup_shows_and_waits_without_a_timer() {
        let mut popup = PopupManager::new();
        let content = PopupContent::Reminder {
            reminder: reminder(1),
        };
        shown(popup.show(content, at(0)));

        let waiting = snapshot(ScheduleState::Awaiting, Some(reminder(1)));
        assert_eq!(popup.reconcile(&waiting, at(3600)), None);
        assert_eq!(popup.current().map(|view| view.content), Some(content));
    }

    #[test]
    fn the_same_reminder_is_never_shown_twice() {
        let mut popup = PopupManager::new();
        let content = PopupContent::Reminder {
            reminder: reminder(1),
        };
        let first = shown(popup.show(content, at(0)));
        assert_eq!(popup.show(content, at(1)), None);
        assert_eq!(popup.current().map(|view| view.seq), Some(first.seq));
    }

    #[test]
    fn a_new_reminder_replaces_the_old_one() {
        let mut popup = PopupManager::new();
        let first = shown(popup.show(
            PopupContent::Reminder {
                reminder: reminder(1),
            },
            at(0),
        ));
        let second = shown(popup.show(
            PopupContent::Reminder {
                reminder: reminder(2),
            },
            at(1),
        ));
        assert_ne!(first.seq, second.seq);
        assert_eq!(popup.current(), Some(second));
    }

    #[test]
    fn the_reminder_popup_hides_once_the_reminder_is_answered() {
        let mut popup = PopupManager::new();
        popup.show(
            PopupContent::Reminder {
                reminder: reminder(1),
            },
            at(0),
        );

        let on_break = snapshot(ScheduleState::OnBreak, Some(reminder(1)));
        assert_eq!(popup.reconcile(&on_break, at(1)), Some(PopupChange::Hide));
        assert_eq!(popup.current(), None);
        assert_eq!(popup.reconcile(&on_break, at(2)), None, "hidden once");
    }

    #[test]
    fn the_reminder_popup_hides_when_paused_or_snoozed() {
        for state in [ScheduleState::Paused, ScheduleState::Snoozed] {
            let mut popup = PopupManager::new();
            popup.show(
                PopupContent::Reminder {
                    reminder: reminder(1),
                },
                at(0),
            );
            assert_eq!(
                popup.reconcile(&snapshot(state, None), at(1)),
                Some(PopupChange::Hide),
                "{state:?}"
            );
        }
    }

    #[test]
    fn a_reminder_popup_for_a_superseded_reminder_hides() {
        let mut popup = PopupManager::new();
        popup.show(
            PopupContent::Reminder {
                reminder: reminder(1),
            },
            at(0),
        );
        let newer = snapshot(ScheduleState::Awaiting, Some(reminder(2)));
        assert_eq!(popup.reconcile(&newer, at(1)), Some(PopupChange::Hide));
    }

    #[test]
    fn informational_popups_hide_themselves() {
        let mut popup = PopupManager::new();
        shown(popup.show(
            PopupContent::BreakOver {
                kind: ReminderKind::Movement,
            },
            at(0),
        ));
        let running = snapshot(ScheduleState::Running, None);

        assert_eq!(popup.reconcile(&running, at(7)), None);
        assert_eq!(popup.reconcile(&running, at(8)), Some(PopupChange::Hide));
    }

    #[test]
    fn repeating_an_informational_popup_restarts_it() {
        let mut popup = PopupManager::new();
        let content = PopupContent::Test {
            kind: ReminderKind::Eyes,
        };
        let first = shown(popup.show(content, at(0)));
        let second = shown(popup.show(content, at(5)));
        assert_ne!(first.seq, second.seq);

        let running = snapshot(ScheduleState::Running, None);
        assert_eq!(popup.reconcile(&running, at(12)), None, "timer restarted");
        assert_eq!(popup.reconcile(&running, at(13)), Some(PopupChange::Hide));
    }

    #[test]
    fn dismissing_hides_only_the_popup_on_screen() {
        let mut popup = PopupManager::new();
        let first = shown(popup.show(
            PopupContent::Reminder {
                reminder: reminder(1),
            },
            at(0),
        ));
        let second = shown(popup.show(
            PopupContent::Reminder {
                reminder: reminder(2),
            },
            at(1),
        ));

        assert_eq!(popup.dismiss(first.seq), None, "stale dismiss is ignored");
        assert_eq!(popup.current(), Some(second));
        assert_eq!(popup.dismiss(second.seq), Some(PopupChange::Hide));
        assert_eq!(popup.dismiss(second.seq), None, "double dismiss");
    }

    #[test]
    fn content_uses_tagged_camel_case_on_the_wire() {
        let view = PopupView {
            seq: 4,
            content: PopupContent::BreakOver {
                kind: ReminderKind::Eyes,
            },
        };
        assert_eq!(
            serde_json::to_string(&view).expect("serialises"),
            r#"{"seq":4,"content":{"type":"breakOver","kind":"eyes"}}"#
        );
    }
}
