//! System tray integration.
//!
//! Linux uses a StatusNotifierItem on the session bus, so the icon only appears
//! if the user's bar hosts one (`org.kde.StatusNotifierWatcher`). Waybar, KDE,
//! GNOME with the AppIndicator extension, and similar bars do; a bare Wayland
//! compositor does not provide a bar at all.
//!
//! Windows uses the native notification-area tray. The same menu is shown on
//! left and right click, so the controls behave the same on both platforms.
//!
//! Tauri's docs are explicit that `TrayIconEvent::Click` is never emitted on
//! Linux, so every action lives in the menu rather than in a click handler.

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Wry};

use crate::i18n;
use crate::model::{Language, ScheduleState};

pub const TRAY_ID: &str = "breathe-tray";

pub struct TrayHandles {
    open: MenuItem<Wry>,
    pause: MenuItem<Wry>,
    take_break: MenuItem<Wry>,
    snooze: MenuItem<Wry>,
    settings: MenuItem<Wry>,
    quit: MenuItem<Wry>,
    /// Last applied labels, so the 500 ms worker tick only touches the menu when
    /// something actually changed.
    applied: Mutex<Applied>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Applied {
    /// `None` until the first sync, so the first one labels every item.
    language: Option<Language>,
    pause_label: String,
    pause_enabled: bool,
    break_enabled: bool,
    snooze_enabled: bool,
    tooltip: String,
}

/// What the menu should show for a schedule state. Pure, so it is testable
/// without a tray.
fn desired(
    state: ScheduleState,
    snooze_enabled: bool,
    reminders_enabled: bool,
    tooltip: &str,
    language: Language,
) -> Applied {
    let pauses = matches!(
        state,
        ScheduleState::Running | ScheduleState::Awaiting | ScheduleState::OnBreak
    );
    Applied {
        language: Some(language),
        pause_label: i18n::text(language, if pauses { "tray.pause" } else { "tray.resume" }),
        pause_enabled: reminders_enabled,
        break_enabled: state == ScheduleState::Awaiting,
        snooze_enabled: snooze_enabled
            && reminders_enabled
            && matches!(
                state,
                ScheduleState::Running | ScheduleState::Snoozed | ScheduleState::Awaiting
            ),
        tooltip: tooltip.to_string(),
    }
}

impl TrayHandles {
    /// Reflects the current schedule in the menu.
    ///
    /// Only touches the StatusNotifierItem when something actually changed, so
    /// the 500 ms worker tick does not become two D-Bus messages per second.
    /// Returns true when the caller should push `tooltip` to the tray icon.
    pub fn sync(
        &self,
        state: ScheduleState,
        snooze_enabled: bool,
        reminders_enabled: bool,
        tooltip: &str,
        language: Language,
    ) -> bool {
        let next = desired(state, snooze_enabled, reminders_enabled, tooltip, language);

        let mut applied = match self.applied.lock() {
            Ok(applied) => applied,
            Err(poisoned) => poisoned.into_inner(),
        };

        if applied.language != next.language {
            // The language changed: relabel the items whose text does not
            // depend on the schedule. The pause label is handled below.
            let _ = self.open.set_text(i18n::text(language, "tray.open"));
            let _ = self.take_break.set_text(i18n::text(language, "tray.break"));
            let _ = self.snooze.set_text(i18n::text(language, "tray.snooze"));
            let _ = self
                .settings
                .set_text(i18n::text(language, "tray.settings"));
            let _ = self.quit.set_text(i18n::text(language, "tray.quit"));
        }

        if applied.pause_label != next.pause_label {
            let _ = self.pause.set_text(&next.pause_label);
        }
        if applied.pause_enabled != next.pause_enabled {
            let _ = self.pause.set_enabled(next.pause_enabled);
        }
        if applied.break_enabled != next.break_enabled {
            let _ = self.take_break.set_enabled(next.break_enabled);
        }
        if applied.snooze_enabled != next.snooze_enabled {
            let _ = self.snooze.set_enabled(next.snooze_enabled);
        }

        let tooltip_changed = applied.tooltip != next.tooltip;
        *applied = next;
        tooltip_changed
    }
}

pub fn build(app: &AppHandle, language: Language) -> tauri::Result<TrayHandles> {
    let label = |key: &str| i18n::text(language, key);
    let open = MenuItem::with_id(app, "open", label("tray.open"), true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", label("tray.pause"), true, None::<&str>)?;
    let take_break = MenuItem::with_id(app, "break", label("tray.break"), false, None::<&str>)?;
    let snooze = MenuItem::with_id(app, "snooze", label("tray.snooze"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", label("tray.settings"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", label("tray.quit"), true, None::<&str>)?;
    let before_actions = PredefinedMenuItem::separator(app)?;
    let before_quit = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &open,
            &before_actions,
            &pause,
            &take_break,
            &snooze,
            &settings,
            &before_quit,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("Breathe")
        // Linux ignores this (the StatusNotifierItem host decides), but on
        // Windows it makes the menu reachable with a left click as well.
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event);

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;

    Ok(TrayHandles {
        open,
        pause,
        take_break,
        snooze,
        settings,
        quit,
        applied: Mutex::new(Applied::default()),
    })
}

fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "open" => crate::commands::show_main_window(app),
        "pause" => crate::commands::toggle_scheduling(app),
        "break" => crate::commands::break_from_tray(app),
        "snooze" => crate::commands::snooze_from_tray(app),
        "settings" => {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = crate::commands::open_settings_window(&app) {
                    eprintln!("[breathe] could not open the settings window: {error}");
                }
            });
        }
        "quit" => crate::commands::quit(app),
        other => eprintln!("[breathe] ignoring unknown tray action {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_reminder_offers_the_break_and_the_snooze() {
        let menu = desired(ScheduleState::Awaiting, true, true, "", Language::En);
        assert!(menu.break_enabled);
        assert!(menu.snooze_enabled);
        assert_eq!(menu.pause_label, "Pause Reminders");
    }

    #[test]
    fn the_break_is_only_offered_while_a_reminder_waits() {
        for state in [
            ScheduleState::Running,
            ScheduleState::Paused,
            ScheduleState::Snoozed,
            ScheduleState::OnBreak,
        ] {
            assert!(
                !desired(state, true, true, "", Language::En).break_enabled,
                "{state:?}"
            );
        }
    }

    #[test]
    fn snooze_follows_the_setting_and_the_state() {
        assert!(!desired(ScheduleState::Awaiting, false, true, "", Language::En).snooze_enabled);
        assert!(!desired(ScheduleState::Paused, true, true, "", Language::En).snooze_enabled);
        assert!(!desired(ScheduleState::OnBreak, true, true, "", Language::En).snooze_enabled);
        assert!(desired(ScheduleState::Running, true, true, "", Language::En).snooze_enabled);
    }

    #[test]
    fn paused_and_snoozed_schedules_offer_resume() {
        assert_eq!(
            desired(ScheduleState::Paused, true, true, "", Language::En).pause_label,
            "Resume Reminders"
        );
        assert_eq!(
            desired(ScheduleState::Snoozed, true, true, "", Language::En).pause_label,
            "Resume Reminders"
        );
        assert_eq!(
            desired(ScheduleState::OnBreak, true, true, "", Language::En).pause_label,
            "Pause Reminders"
        );
    }
}
