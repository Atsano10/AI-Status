//! The global hotkey that shows/hides the bar. It's saved in the settings (`hotkey`, e.g. `"Control+Alt+Space"`)
//! and changed by the settings window's shortcut recorder through `set_hotkey`.

use serde::Serialize;
use statuspal_core::settings::Settings;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::{bar, settings};

/// Why a shortcut can't be the hotkey. The settings window shows a message for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum HotkeyError {
    /// Not a key combo we can register.
    Invalid,
    /// A plain key (or Shift + key) would be swallowed in every app, so you couldn't type it anymore.
    NeedsModifier,
    /// macOS (or every Mac app) already uses it, like ⌘Space for Spotlight or ⌘Q for Quit.
    Reserved,
    /// Another app registered it first.
    Taken,
}

/// Shortcuts the Mac already uses everywhere. Registering one would either fail silently or break it.
const RESERVED: &[(Modifiers, Code)] = &[
    (Modifiers::SUPER, Code::Space),   // Spotlight
    (Modifiers::CONTROL, Code::Space), // switch input source
    (Modifiers::SUPER, Code::Tab),     // app switcher
    (Modifiers::SUPER, Code::KeyQ),
    (Modifiers::SUPER, Code::KeyW),
    (Modifiers::SUPER, Code::KeyH),
    (Modifiers::SUPER, Code::KeyM),
    (Modifiers::SUPER, Code::KeyA),
    (Modifiers::SUPER, Code::KeyC),
    (Modifiers::SUPER, Code::KeyV),
    (Modifiers::SUPER, Code::KeyX),
    (Modifiers::SUPER, Code::KeyZ),
    (Modifiers::SUPER, Code::KeyS),
    (Modifiers::SUPER, Code::KeyF),
    (Modifiers::SUPER, Code::KeyN),
    (Modifiers::SUPER, Code::KeyT),
    (Modifiers::SUPER, Code::KeyP),
    (Modifiers::SUPER, Code::Comma),
    (Modifiers::SUPER.union(Modifiers::SHIFT), Code::Digit3), // screenshots
    (Modifiers::SUPER.union(Modifiers::SHIFT), Code::Digit4),
    (Modifiers::SUPER.union(Modifiers::SHIFT), Code::Digit5),
    (Modifiers::SUPER.union(Modifiers::ALT), Code::Escape), // Force Quit
    (Modifiers::SUPER.union(Modifiers::CONTROL), Code::KeyQ), // Lock Screen
];

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    // We only ever register one shortcut, so any press means "toggle the bar".
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    bar::toggle_bar(app);
                }
            })
            .build(),
    )?;

    // Report a problem instead of crashing: you can still open the bar from the menu bar and pick another.
    let hotkey = settings::get(app).hotkey;
    match register_saved(app) {
        Ok(()) => println!("hotkey {hotkey} registered"),
        Err(e) => eprintln!("hotkey {hotkey} can't be used ({e:?}), pick another in Settings"),
    }
    Ok(())
}

/// Turns a saved hotkey into a shortcut we can register, or says why it can't be one.
pub fn check(hotkey: &str) -> Result<Shortcut, HotkeyError> {
    let shortcut: Shortcut = hotkey.parse().map_err(|_| HotkeyError::Invalid)?;
    let real_modifier = shortcut
        .mods
        .intersects(Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT);
    if !real_modifier && !is_function_key(shortcut.key) {
        return Err(HotkeyError::NeedsModifier);
    }
    if RESERVED.contains(&(shortcut.mods, shortcut.key)) {
        return Err(HotkeyError::Reserved);
    }
    Ok(shortcut)
}

/// F1–F20 aren't typed in text, so they can be a hotkey on their own.
fn is_function_key(key: Code) -> bool {
    use Code::*;
    matches!(
        key,
        F1 | F2
            | F3
            | F4
            | F5
            | F6
            | F7
            | F8
            | F9
            | F10
            | F11
            | F12
            | F13
            | F14
            | F15
            | F16
            | F17
            | F18
            | F19
            | F20
    )
}

/// Registers the hotkey from the settings.
fn register_saved(app: &AppHandle) -> Result<(), HotkeyError> {
    let shortcut = check(&settings::get(app).hotkey)?;
    app.global_shortcut()
        .register(shortcut)
        .map_err(|_| HotkeyError::Taken)
}

/// The recorder's "save": swaps the hotkey over and saves it. If the new one can't be registered, the old one
/// stays. `async` so it runs off the main thread, which the shortcut plugin needs to wait on.
#[tauri::command]
pub async fn set_hotkey(app: AppHandle, hotkey: String) -> Result<Settings, HotkeyError> {
    let shortcut = check(&hotkey)?;
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    if shortcuts.register(shortcut).is_err() {
        let _ = register_saved(&app); // put the old one back
        return Err(HotkeyError::Taken);
    }
    Ok(settings::set_hotkey(&app, hotkey))
}

/// While the recorder listens, the current hotkey is switched off. Otherwise pressing it would toggle the bar
/// instead of reaching the recorder.
#[tauri::command]
pub async fn pause_hotkey(app: AppHandle, paused: bool) {
    let _ = app.global_shortcut().unregister_all();
    if !paused {
        let _ = register_saved(&app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_hotkey_is_fine() {
        assert!(check(&Settings::default().hotkey).is_ok());
    }

    #[test]
    fn accepts_what_the_recorder_sends() {
        assert!(check("Control+Alt+Shift+Command+KeyK").is_ok());
        assert!(check("Alt+Digit1").is_ok());
        assert!(check("Command+Shift+ArrowUp").is_ok());
    }

    #[test]
    fn plain_keys_need_a_modifier() {
        assert_eq!(check("KeyK").unwrap_err(), HotkeyError::NeedsModifier);
        assert_eq!(check("Shift+KeyK").unwrap_err(), HotkeyError::NeedsModifier);
        assert_eq!(check("Space").unwrap_err(), HotkeyError::NeedsModifier);
    }

    #[test]
    fn function_keys_work_alone() {
        assert!(check("F13").is_ok());
        assert!(check("Shift+F5").is_ok());
    }

    #[test]
    fn mac_shortcuts_are_reserved() {
        assert_eq!(check("Command+Space").unwrap_err(), HotkeyError::Reserved);
        assert_eq!(check("Command+KeyQ").unwrap_err(), HotkeyError::Reserved);
        assert_eq!(
            check("Command+Shift+Digit4").unwrap_err(),
            HotkeyError::Reserved
        );
    }

    #[test]
    fn nonsense_is_invalid() {
        assert_eq!(check("").unwrap_err(), HotkeyError::Invalid);
        assert_eq!(check("Control+Banana").unwrap_err(), HotkeyError::Invalid);
    }
}
