//! "Open at login", through the autostart plugin (a Launch Agent in `~/Library/LaunchAgents`). macOS is the
//! source of truth rather than our settings file, so turning it off in System Settings shows up here too.

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub fn get_launch_at_login(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| e.to_string())?;
    Ok(get_launch_at_login(app))
}
