//! Builds what the bar shows, from every agent we can read.

use crate::adapters::claude::statusline;
use crate::adapters::codex;
use crate::model::{AgentId, UsageSnapshot};

/// One snapshot per agent that has data. An agent whose files are missing or unreadable is left out
/// rather than failing the whole thing.
pub fn current_snapshots() -> Vec<UsageSnapshot> {
    let mut snapshots = Vec::new();

    if let Some(path) = statusline::statusline_file()
        && let Ok(windows) = statusline::read(&path)
    {
        snapshots.push(UsageSnapshot {
            agent: AgentId::Claude,
            windows,
        });
    }

    if let Some(dir) = codex::sessions_dir()
        && let Ok(Some(limit)) = codex::read_latest_limit(&dir)
    {
        snapshots.push(UsageSnapshot {
            agent: AgentId::Codex,
            windows: vec![limit],
        });
    }

    snapshots
}
