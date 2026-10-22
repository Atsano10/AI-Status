//! Settings ▸ Codex: the default effort and the `codex` profiles. The file editing (keeping the rest of
//! Codex's config exactly as it was, with a backup) is in `crates/core/src/adapters/codex_config.rs`.

use statuspal_core::adapters::codex_config::{self, CodexSetup};

fn config() -> Result<std::path::PathBuf, String> {
    codex_config::config_file().ok_or_else(|| "can't find your home folder".into())
}

#[tauri::command]
pub fn get_codex_setup() -> Option<CodexSetup> {
    codex_config::config_file().map(|path| codex_config::setup(&path))
}

/// Only called when you pick an effort in Settings.
#[tauri::command]
pub fn set_codex_effort(effort: String) -> Result<CodexSetup, String> {
    let path = config()?;
    codex_config::set_effort(&path, &effort).map_err(|e| e.to_string())?;
    Ok(codex_config::setup(&path))
}

#[tauri::command]
pub fn undo_codex_effort() -> Result<CodexSetup, String> {
    let path = config()?;
    codex_config::undo_effort(&path).map_err(|e| e.to_string())?;
    Ok(codex_config::setup(&path))
}

/// Only called when you flip the profiles switch.
#[tauri::command]
pub fn set_codex_profiles(on: bool) -> Result<CodexSetup, String> {
    let path = config()?;
    codex_config::set_profiles(&path, on).map_err(|e| e.to_string())?;
    Ok(codex_config::setup(&path))
}
