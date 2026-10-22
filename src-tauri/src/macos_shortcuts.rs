//! macOS's own keyboard shortcuts (System Settings ▸ Keyboard ▸ Keyboard Shortcuts), so the hotkey recorder
//! can say "macOS uses that one" instead of registering a hotkey that never fires.
//!
//! They live in `~/Library/Preferences/com.apple.symbolichotkeys.plist` as numbered entries:
//! `{enabled, value: {parameters: [character, key code, modifiers]}}`. The file only lists shortcuts you
//! changed; the rest are at their defaults.

use std::path::PathBuf;

use plist::{Dictionary, Value};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

const SHIFT: u64 = 1 << 17;
const CONTROL: u64 = 1 << 18;
const OPTION: u64 = 1 << 19;
const COMMAND: u64 = 1 << 20;

/// Shortcuts that are on unless you turned them off, with their default keys: (id, key code, modifiers).
/// 60 and 61 switch keyboard languages, so they only matter with more than one (see `input_sources`).
const DEFAULTS: [(u64, u16, u64); 4] = [
    (60, 49, CONTROL),          // ⌃Space
    (61, 49, CONTROL | OPTION), // ⌃⌥Space
    (64, 49, COMMAND),          // ⌘Space
    (65, 49, COMMAND | OPTION), // ⌥⌘Space
];

/// The name macOS gives a shortcut, for the message.
fn name(id: u64) -> &'static str {
    match id {
        60 => "Select the previous input source",
        61 => "Select next source in Input menu",
        64 => "Show Spotlight search",
        65 => "Show Finder search window",
        28..=31 | 184 => "a screenshot shortcut",
        32..=36 => "Mission Control",
        79..=82 => "moving between Spaces",
        118..=133 => "switching to a desktop",
        _ => "one of its shortcuts",
    }
}

/// The macOS shortcut that uses `shortcut`, if one is switched on.
pub fn used_by(shortcut: &Shortcut) -> Option<&'static str> {
    let key = key_code(shortcut.key)?;
    let mods = modifiers(shortcut.mods);
    let home = std::env::home_dir()?;
    let read = |file: &str| -> Option<Dictionary> {
        let path: PathBuf = home.join("Library/Preferences").join(file);
        plist::from_file::<_, Value>(path).ok()?.into_dictionary()
    };
    let hotkeys = read("com.apple.symbolichotkeys.plist")
        .and_then(|d| d.get("AppleSymbolicHotKeys")?.as_dictionary().cloned())
        .unwrap_or_default();
    let languages = read("com.apple.HIToolbox.plist")
        .map(|d| input_sources(&d))
        .unwrap_or(1);
    conflict(&hotkeys, languages, key, mods).map(name)
}

/// The id of the switched-on macOS shortcut that uses `key` + `mods`. `languages` is how many keyboard
/// languages you have.
fn conflict(hotkeys: &Dictionary, languages: usize, key: u16, mods: u64) -> Option<u64> {
    let relevant = |id: u64| !matches!(id, 60 | 61) || languages > 1;

    // Shortcuts you changed, or turned off, are in the file.
    for (id, entry) in hotkeys {
        let Ok(id) = id.parse::<u64>() else { continue };
        let Some(entry) = entry.as_dictionary() else {
            continue;
        };
        let enabled = entry
            .get("enabled")
            .and_then(Value::as_boolean)
            .unwrap_or(false);
        let params = entry
            .get("value")
            .and_then(Value::as_dictionary)
            .and_then(|v| v.get("parameters"))
            .and_then(Value::as_array);
        let (Some(code), Some(flags)) = (
            params
                .and_then(|p| p.get(1))
                .and_then(Value::as_unsigned_integer),
            params
                .and_then(|p| p.get(2))
                .and_then(Value::as_unsigned_integer),
        ) else {
            continue;
        };
        // Only ⇧⌃⌥⌘ count; the fn and keypad bits don't.
        if enabled
            && relevant(id)
            && code == u64::from(key)
            && flags & (SHIFT | CONTROL | OPTION | COMMAND) == mods
        {
            return Some(id);
        }
    }
    // The rest are at their defaults.
    DEFAULTS
        .iter()
        .find(|(id, code, flags)| {
            !hotkeys.contains_key(id.to_string().as_str())
                && relevant(*id)
                && *code == key
                && *flags == mods
        })
        .map(|(id, ..)| *id)
}

/// How many keyboard languages (layouts and input methods) are switched on. The emoji viewer and the like
/// don't count.
fn input_sources(hitoolbox: &Dictionary) -> usize {
    hitoolbox
        .get("AppleEnabledInputSources")
        .and_then(Value::as_array)
        .map(|sources| {
            sources
                .iter()
                .filter(|s| {
                    s.as_dictionary()
                        .and_then(|d| d.get("InputSourceKind"))
                        .and_then(Value::as_string)
                        .is_some_and(|kind| kind == "Keyboard Layout" || kind == "Input Mode")
                })
                .count()
        })
        .unwrap_or(1)
}

fn modifiers(mods: Modifiers) -> u64 {
    [
        (Modifiers::SHIFT, SHIFT),
        (Modifiers::CONTROL, CONTROL),
        (Modifiers::ALT, OPTION),
        (Modifiers::SUPER, COMMAND),
    ]
    .into_iter()
    .filter(|(m, _)| mods.contains(*m))
    .map(|(_, bit)| bit)
    .sum()
}

/// macOS's key code for a key (the `kVK_…` numbers). Only keys a hotkey can use.
fn key_code(key: Code) -> Option<u16> {
    use Code::*;
    Some(match key {
        KeyA => 0,
        KeyS => 1,
        KeyD => 2,
        KeyF => 3,
        KeyH => 4,
        KeyG => 5,
        KeyZ => 6,
        KeyX => 7,
        KeyC => 8,
        KeyV => 9,
        KeyB => 11,
        KeyQ => 12,
        KeyW => 13,
        KeyE => 14,
        KeyR => 15,
        KeyY => 16,
        KeyT => 17,
        Digit1 => 18,
        Digit2 => 19,
        Digit3 => 20,
        Digit4 => 21,
        Digit6 => 22,
        Digit5 => 23,
        Equal => 24,
        Digit9 => 25,
        Digit7 => 26,
        Minus => 27,
        Digit8 => 28,
        Digit0 => 29,
        BracketRight => 30,
        KeyO => 31,
        KeyU => 32,
        BracketLeft => 33,
        KeyI => 34,
        KeyP => 35,
        Enter => 36,
        KeyL => 37,
        KeyJ => 38,
        Quote => 39,
        KeyK => 40,
        Semicolon => 41,
        Backslash => 42,
        Comma => 43,
        Slash => 44,
        KeyN => 45,
        KeyM => 46,
        Period => 47,
        Tab => 48,
        Space => 49,
        Backquote => 50,
        Backspace => 51,
        Escape => 53,
        F17 => 64,
        F18 => 79,
        F19 => 80,
        F20 => 90,
        F5 => 96,
        F6 => 97,
        F7 => 98,
        F3 => 99,
        F8 => 100,
        F9 => 101,
        F11 => 103,
        F13 => 105,
        F16 => 106,
        F14 => 107,
        F10 => 109,
        F12 => 111,
        F15 => 113,
        Home => 115,
        PageUp => 116,
        Delete => 117,
        F4 => 118,
        End => 119,
        F2 => 120,
        PageDown => 121,
        F1 => 122,
        ArrowLeft => 123,
        ArrowRight => 124,
        ArrowDown => 125,
        ArrowUp => 126,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(enabled: bool, key: u64, mods: u64) -> Value {
        let mut value = Dictionary::new();
        value.insert("type".into(), Value::String("standard".into()));
        value.insert(
            "parameters".into(),
            Value::Array(vec![32u64.into(), key.into(), mods.into()]),
        );
        let mut e = Dictionary::new();
        e.insert("enabled".into(), Value::Boolean(enabled));
        e.insert("value".into(), Value::Dictionary(value));
        Value::Dictionary(e)
    }

    const CTRL_OPT: u64 = CONTROL | OPTION;

    #[test]
    fn input_source_switching_only_matters_with_two_languages() {
        // A fresh Mac: nothing in the file, so ⌃⌥Space is at its default (on).
        let fresh = Dictionary::new();
        assert_eq!(conflict(&fresh, 2, 49, CTRL_OPT), Some(61));
        assert_eq!(conflict(&fresh, 1, 49, CTRL_OPT), None);
    }

    #[test]
    fn a_shortcut_you_turned_off_is_free() {
        // Like this Mac: 61 is in the file, turned off.
        let mut hotkeys = Dictionary::new();
        hotkeys.insert("61".into(), entry(false, 49, CTRL_OPT));
        assert_eq!(conflict(&hotkeys, 2, 49, CTRL_OPT), None);
    }

    #[test]
    fn a_shortcut_you_moved_counts_where_it_is_now() {
        // Spotlight moved from ⌘Space to ⌥Space.
        let mut hotkeys = Dictionary::new();
        hotkeys.insert("64".into(), entry(true, 49, OPTION));
        assert_eq!(conflict(&hotkeys, 1, 49, OPTION), Some(64));
        assert_eq!(
            conflict(&hotkeys, 1, 49, COMMAND),
            None,
            "⌘Space is free now"
        );
    }

    #[test]
    fn the_fn_bit_doesnt_matter() {
        let mut hotkeys = Dictionary::new();
        hotkeys.insert("32".into(), entry(true, 126, CONTROL | (1 << 23))); // ⌃↑, Mission Control
        assert_eq!(conflict(&hotkeys, 1, 126, CONTROL), Some(32));
        assert_eq!(name(32), "Mission Control");
    }

    #[test]
    fn modifiers_and_keys() {
        assert_eq!(modifiers(Modifiers::CONTROL | Modifiers::ALT), CTRL_OPT);
        assert_eq!(key_code(Code::Space), Some(49));
        assert_eq!(key_code(Code::KeyK), Some(40));
    }
}
