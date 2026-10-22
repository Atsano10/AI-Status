//! Settings ▸ Claude Code: connect StatusPal to Claude Code's status line, or remove it. The file editing (with a
//! backup, keeping a status line you already had) lives in `crates/core/src/adapters/claude/bridge.rs`.

use statuspal_core::adapters::claude::bridge::{self, BridgeStatus};

/// What Claude Code should run: this very program with `--statusline` (see main.rs). Quoted for the shell,
/// since the path can have spaces in it.
fn our_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = exe.to_string_lossy().replace('\'', r"'\''");
    Ok(format!("'{path}' --statusline"))
}

#[tauri::command]
pub fn get_bridge_status() -> BridgeStatus {
    bridge::claude_settings_file()
        .and_then(|path| bridge::read_settings(&path).ok())
        .map(|settings| bridge::status(&settings))
        .unwrap_or(BridgeStatus::NotConnected)
}

/// Only called after you confirm in the settings window.
#[tauri::command]
pub fn set_bridge_connected(connected: bool) -> Result<BridgeStatus, String> {
    let (Some(settings), Some(chain)) = (bridge::claude_settings_file(), bridge::chain_file())
    else {
        return Err("can't find your home folder".into());
    };
    bridge::set_connected(&settings, &chain, &our_command()?, connected).map_err(|e| e.to_string())
}
