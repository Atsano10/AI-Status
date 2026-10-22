//! Keeps track of everything we know about your usage: the limit % for each agent and every reply from the
//! last day. The app keeps one `Tracker` and calls `refresh` whenever an agent writes (see `watch.rs`).
//! Each refresh reads only what's new.

use std::collections::HashMap;

use crate::adapters::claude::statusline;
use crate::adapters::claude::transcripts::{self, ClaudeReader};
use crate::adapters::codex::{self, CodexReader};
use crate::model::{AgentId, UsageEvent, UsageSnapshot};

/// Replies older than this are dropped. The critter looks at the last 10 minutes and tips at the last few
/// hours, so a day is plenty.
pub const KEEP_SECS: i64 = 24 * 3600;

#[derive(Debug, Default)]
pub struct Tracker {
    codex: CodexReader,
    claude: ClaudeReader,
    /// One event per reply. Logs can mention the same reply more than once, so they're kept by id.
    by_id: HashMap<(AgentId, String), UsageEvent>,
    /// The same events, oldest first.
    events: Vec<UsageEvent>,
}

impl Tracker {
    /// Reads whatever the agents wrote since the last refresh. `now` is Unix seconds.
    pub fn refresh(&mut self, now: i64) {
        let mut new = Vec::new();
        if let Some(dir) = codex::sessions_dir()
            && dir.is_dir()
        {
            new.extend(self.codex.poll(&dir));
        }
        if let Some(dir) = transcripts::projects_dir()
            && dir.is_dir()
        {
            new.extend(self.claude.poll(&dir));
        }
        self.add(new, now);
    }

    /// Adds events, merging ones about the same reply, and drops the ones older than `KEEP_SECS`.
    fn add(&mut self, new: Vec<UsageEvent>, now: i64) {
        for event in new {
            let key = (event.agent, event.id.clone());
            match self.by_id.get_mut(&key) {
                // The same reply again: the later line has the final counts.
                Some(seen) => seen.tokens = seen.tokens.max(event.tokens),
                None => {
                    self.by_id.insert(key, event);
                }
            }
        }
        self.by_id.retain(|_, event| event.ts >= now - KEEP_SECS);
        self.events = self.by_id.values().cloned().collect();
        self.events.sort_by_key(|event| event.ts);
    }

    /// Every reply from the last day, oldest first.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TokenUsage;

    fn reply(id: &str, ts: i64, output: u64) -> UsageEvent {
        UsageEvent {
            agent: AgentId::Claude,
            id: id.into(),
            ts,
            session_id: "s".into(),
            model: None,
            effort: None,
            tokens: TokenUsage {
                output,
                ..Default::default()
            },
        }
    }

    #[test]
    fn the_same_reply_twice_is_one_event_with_the_final_counts() {
        let mut tracker = Tracker::default();
        tracker.add(
            vec![reply("msg_1", 100, 200), reply("msg_1", 101, 228)],
            200,
        );
        tracker.add(vec![reply("msg_1", 102, 228)], 200);
        assert_eq!(tracker.events().len(), 1);
        assert_eq!(tracker.events()[0].tokens.output, 228);
        assert_eq!(tracker.events()[0].ts, 100, "keeps when the reply started");
    }

    #[test]
    fn keeps_a_day_in_time_order() {
        let mut tracker = Tracker::default();
        let now = 200_000;
        tracker.add(
            vec![
                reply("new", now - 10, 1),
                reply("old", now - KEEP_SECS - 1, 1),
                reply("mid", now - 3600, 1),
            ],
            now,
        );
        let ids: Vec<_> = tracker.events().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["mid", "new"]);
    }
}
