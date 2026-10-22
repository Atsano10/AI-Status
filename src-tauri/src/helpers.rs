//! Settings ▸ Claude Code helpers: switch the helper agents on and off. The files themselves are made in
//! `crates/core/src/adapters/claude/agents.rs`.

use statuspal_core::adapters::claude::agents::{self, HelperInfo};

#[tauri::command]
pub fn get_helpers() -> Vec<HelperInfo> {
    agents::agents_dir()
        .map(|dir| agents::list(&dir))
        .unwrap_or_default()
}

/// Only called when you flip a helper's switch.
#[tauri::command]
pub fn set_helper(name: String, on: bool) -> Result<Vec<HelperInfo>, String> {
    let dir = agents::agents_dir().ok_or("can't find your home folder")?;
    agents::set(&dir, &name, on).map_err(|e| e.to_string())?;
    Ok(agents::list(&dir))
}
