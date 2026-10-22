//! Keeps track of everything we know about your usage: the limit % for each agent and every turn from the
//! last day. The app keeps one `Tracker` and calls `refresh` whenever an agent writes (see `watch.rs`).
//! Each refresh reads only what's new.

use crate::adapters::claude::statusline;
use crate::adapters::codex::{self, CodexReader};
use crate::model::{AgentId, UsageEvent, UsageSnapshot};

/// Turns older than this are dropped. The critter looks at the last 10 minutes and tips at the last few
/// hours, so a day is plenty.
pub const KEEP_SECS: i64 = 24 * 3600;

#[derive(Debug, Default)]
pub struct Tracker {
    codex: CodexReader,
    /// Every agent's turns from the last `KEEP_SECS`, oldest first.
    events: Vec<UsageEvent>,
}

impl Tracker {
    /// Reads whatever the agents wrote since the last refresh. `now` is Unix seconds.
    pub fn refresh(&mut self, now: i64) {
        if let Some(dir) = codex::sessions_dir()
            && dir.is_dir()
        {
            self.events.extend(self.codex.poll(&dir));
        }
        self.events.retain(|event| event.ts >= now - KEEP_SECS);
        // Several files can add turns in one refresh; keep them in time order.
        self.events.sort_by_key(|event| event.ts);
    }

    /// Every turn from the last day, oldest first.
    pub fn events(&self) -> &[UsageEvent] {
        &self.events
    }

    /// What the bar shows: one snapshot per agent that has a limit reading. An agent whose files are
    /// missing or unreadable is left out rather than failing the whole thing.
    pub fn snapshots(&self) -> Vec<UsageSnapshot> {
        let mut snapshots = Vec::new();

        // Claude's statusLine file is one small JSON, so it's simply read again each time.
        if let Some(path) = statusline::statusline_file()
            && let Ok(windows) = statusline::read(&path)
        {
            snapshots.push(UsageSnapshot {
                agent: AgentId::Claude,
                windows,
            });
        }

        if let Some(limit) = self.codex.latest_limit() {
            snapshots.push(UsageSnapshot {
                agent: AgentId::Codex,
                windows: vec![limit],
            });
        }
        snapshots
    }
}
