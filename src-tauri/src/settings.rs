//! Keeps your settings in memory, saves them on every change, and tells every window when they change
//! (the `settings-changed` event). The file format lives in `crates/core/src/settings.rs`.

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;
use statuspal_core::model::AgentId;
#[cfg(target_os = "macos")]
use statuspal_core::settings::BarPosition;
use statuspal_core::settings::{self, Settings};
use tauri::{AppHandle, Emitter, Manager};

struct SettingsState {
    /// `None` if we couldn't find your home folder. Changes then last until you quit.
    path: Option<PathBuf>,
    current: Mutex<Settings>,
}

/// Loads the settings file. Runs first, since the tray and the detector read the settings straight away.
pub fn setup(app: &AppHandle) {
    let path = settings::settings_file();
    let current = path.as_deref().map(settings::load).unwrap_or_default();
    app.manage(SettingsState {
        path,
        current: Mutex::new(current),
    });
}

/// A copy of the current settings.
pub fn get(app: &AppHandle) -> Settings {
    app.state::<SettingsState>().current.lock().unwrap().clone()
}

/// Remembers the agent you pinned from the tray (`None` = Auto).
pub fn set_pinned(app: &AppHandle, pinned: Option<AgentId>) {
    let _ = replace(app, |old| {
        Ok(Settings {
            pinned,
            ..old.clone()
        })
    });
}

/// Remembers where the bar was left on a display, and that it's the display it was on last.
#[cfg(target_os = "macos")]
pub fn set_bar_position(app: &AppHandle, display: String, spot: BarPosition) {
    let _ = replace(app, |old| {
        let mut bar_positions = old.bar_positions.clone();
        bar_positions.insert(display.clone(), spot);
        Ok(Settings {
            bar_positions,
            bar_display: Some(display),
            ..old.clone()
        })
    });
}

/// Saves a new hotkey. Only `hotkey::set_hotkey` calls this, after the shortcut registered fine.
#[cfg(target_os = "macos")]
pub fn set_hotkey(app: &AppHandle, hotkey: String) -> Settings {
    replace(app, |old| {
        Ok(Settings {
            hotkey,
            ..old.clone()
        })
    })
    .unwrap_or_else(|_| get(app))
}

/// Works out new settings from the current ones, saves them, and tells every window. Returns the new
/// settings, or the error from `change` (nothing is saved then).
fn replace(
    app: &AppHandle,
    change: impl FnOnce(&Settings) -> Result<Settings, String>,
) -> Result<Settings, String> {
    let state = app.state::<SettingsState>();
    let updated = {
        let mut current = state.current.lock().unwrap();
        let updated = change(&current)?;
        if updated == *current {
            return Ok(updated); // nothing changed, so no save and no event
        }
        // Save while still holding the lock, so two quick changes can't land in the file out of order.
        if let Some(path) = &state.path
            && let Err(e) = settings::save(path, &updated)
        {
            eprintln!("can't save settings, they last until you quit: {e}");
        }
        *current = updated.clone();
        updated
    }; // the lock is released here
    let _ = app.emit("settings-changed", &updated);
    Ok(updated)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    get(&app)
}

/// The windows send only what they changed, e.g. `invoke("update_settings", {changes: {orientation: "vertical"}})`.
#[tauri::command]
pub fn update_settings(app: AppHandle, changes: Value) -> Result<Settings, String> {
    // A new hotkey has to be registered before it's saved, so it goes through `set_hotkey` instead.
    if changes.get("hotkey").is_some() {
        return Err("change the hotkey with set_hotkey".into());
    }
    replace(&app, |old| {
        old.with_changes(changes).map_err(|e| e.to_string())
    })
}
