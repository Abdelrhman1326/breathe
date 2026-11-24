//! Localized text for the parts of Breathe drawn by Rust: the tray menu and
//! tooltip, window titles and the native-notification fallback.
//!
//! The catalogs are the same JSON files the frontend uses (`src/locales/`),
//! embedded at compile time, so the two sides can never disagree. A key missing
//! from a catalog falls back to English, then to the key itself.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::model::Language;

const EN: &str = include_str!("../../src/locales/en.json");
const AR: &str = include_str!("../../src/locales/ar.json");

type Catalog = HashMap<String, String>;

fn parse(source: &str) -> Catalog {
    serde_json::from_str(source).unwrap_or_else(|error| {
        eprintln!("[breathe] a translation catalog is invalid: {error}");
        Catalog::new()
    })
}

fn english() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| parse(EN))
}

fn arabic() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| parse(AR))
}

fn catalog(language: Language) -> &'static Catalog {
    match language {
        Language::En => english(),
        Language::Ar => arabic(),
    }
}

/// Right-to-left mark. Native surfaces (tray menu, tooltip, window title)
/// have no `dir` attribute and guess the paragraph direction from the first
/// strong character; an Arabic string that begins with the Latin brand name
/// would be laid out left to right and read out of order. Leading with RLM
/// makes them render right to left.
const RLM: char = '\u{200F}';

/// The message for `key` in `language`.
pub fn text(language: Language, key: &str) -> String {
    let message = catalog(language)
        .get(key)
        .or_else(|| english().get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string());
    match language {
        Language::Ar => format!("{RLM}{message}"),
        Language::En => message,
    }
}

/// The message for `key` with `{name}` placeholders filled from `vars`.
pub fn text_with(language: Language, key: &str, vars: &[(&str, &str)]) -> String {
    vars.iter()
        .fold(text(language, key), |message, (name, value)| {
            message.replace(&format!("{{{name}}}"), value)
        })
}

/// Every key Rust looks up. Tests check each exists in every catalog.
#[cfg(test)]
pub const RUST_KEYS: &[&str] = &[
    "tray.open",
    "tray.pause",
    "tray.resume",
    "tray.break",
    "tray.snooze",
    "tray.settings",
    "tray.quit",
    "tooltip.off",
    "tooltip.running",
    "tooltip.paused",
    "tooltip.snoozed",
    "tooltip.awaiting",
    "tooltip.break",
    "tooltip.stopped",
    "menubar.awaiting",
    "reminder.eyes.title",
    "reminder.eyes.body",
    "reminder.movement.title",
    "reminder.movement.body",
    "reminder.breakOver.title",
    "reminder.breakOver.body",
    "window.settings",
    "window.popup",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_rust_uses_exists_in_english() {
        for key in RUST_KEYS {
            assert!(english().contains_key(*key), "en.json is missing {key}");
        }
    }

    #[test]
    fn every_key_rust_uses_is_translated_into_arabic() {
        for key in RUST_KEYS {
            assert!(arabic().contains_key(*key), "ar.json is missing {key}");
        }
    }

    #[test]
    fn the_tray_speaks_arabic() {
        assert_eq!(text(Language::Ar, "tray.open"), "\u{200F}افتح Breathe");
        assert_eq!(
            text_with(Language::Ar, "tooltip.break", &[("time", "02:41")]),
            "\u{200F}في استراحة، بقي 02:41 — Breathe"
        );
    }

    #[test]
    fn arabic_native_text_starts_right_to_left_and_english_is_untouched() {
        for key in RUST_KEYS {
            assert!(text(Language::Ar, key).starts_with(RLM), "{key}");
            assert!(!text(Language::En, key).starts_with(RLM), "{key}");
        }
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            text_with(Language::En, "tooltip.running", &[("time", "12:00")]),
            "Breathe — next break in 12:00"
        );
    }

    #[test]
    fn an_unknown_key_falls_back_to_itself() {
        assert_eq!(text(Language::En, "no.such.key"), "no.such.key");
    }
}
