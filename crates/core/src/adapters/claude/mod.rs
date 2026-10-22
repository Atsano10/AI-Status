//! Claude adapter. Claude's data comes from several places, one module each:
//! - `statusline`: the 5-hour and weekly limit %, written by our statusLine bridge
//! - `transcripts`: Claude Code's chat logs, for the tokens each reply used
//! - `bridge`: connecting the statusLine bridge in Claude Code's settings
//! - later maybe: Claude Desktop's usage history (fallback)

use std::path::PathBuf;

pub mod bridge;
pub mod statusline;
pub mod transcripts;

/// Claude Code's folder: `~/.claude`, or `CLAUDE_CONFIG_DIR` if you set one.
pub fn config_dir() -> Option<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => Some(std::env::home_dir()?.join(".claude")),
    }
}
