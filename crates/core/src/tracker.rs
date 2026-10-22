//! Keeps track of everything we know about your usage: the limit % for each agent and every reply from the
//! last day. The app keeps one `Tracker` and calls `refresh` whenever an agent writes (see `watch.rs`).
//! Each refresh reads only what's new.

use std::collections::HashMap;

use crate::activity::{self, GaitThresholds, PACE_SPAN_SECS, Reading};
use crate::adapters::claude::statusline;
use crate::adapters::claude::transcripts::{self, ClaudeReader};
use crate::adapters::codex::{self, CodexReader};
use crate::model::{AgentId, LimitWindow, UsageEvent, UsageSnapshot};

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
    /// The latest limit windows for each agent that has any.
    limits: Vec<(AgentId, Vec<LimitWindow>)>,
    /// Past limit % readings for each agent's window (by its length in minutes), oldest first, for the pace.
    readings: HashMap<(AgentId, u32), Vec<Reading>>,
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

        self.limits.clear();
        // Claude's statusLine file is one small JSON, so it's simply read again each time.
        if let Some(path) = statusline::statusline_file()
            && let Ok(windows) = statusline::read(&path)
            && !windows.is_empty()
        {
            self.limits.push((AgentId::Claude, windows));
        }
        if let Some(limit) = self.codex.latest_limit() {
            self.limits.push((AgentId::Codex, vec![limit]));
        }
        self.remember_readings(now);
    }

    /// Adds the current limit readings to their history (a reading we already have is skipped), and forgets
    /// the ones too old to matter for the pace.
    fn remember_readings(&mut self, now: i64) {
        for (agent, windows) in &self.limits {
            for window in windows {
                let history = self
                    .readings
                    .entry((*agent, window.window_minutes))
                    .or_default();
                if history
                    .last()
                    .is_none_or(|(at, _)| window.observed_at > *at)
                {
                    history.push((window.observed_at, window.used_pct));
                }
            }
        }
        for history in self.readings.values_mut() {
            history.retain(|(at, _)| *at >= now - PACE_SPAN_SECS);
        }
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

    /// What the bar shows: one snapshot per agent that has limit readings or replies from the last day.
    pub fn snapshots(&self, now: i64, thresholds: GaitThresholds) -> Vec<UsageSnapshot> {
        [AgentId::Claude, AgentId::Codex]
            .into_iter()
            .filter_map(|agent| {
                let windows = self
                    .limits
                    .iter()
                    .find(|(a, _)| *a == agent)
                    .map(|(_, windows)| windows.clone())
                    .unwrap_or_default();
                let events = self.events.iter().filter(|e| e.agent == agent);
                if windows.is_empty() && events.clone().next().is_none() {
                    return None;
                }
                let pace = windows
                    .iter()
                    .filter_map(|window| {
                        let history = self.readings.get(&(agent, window.window_minutes))?;
                        activity::pace(history, window, now)
                    })
                    .collect();
                Some(UsageSnapshot {
                    agent,
                    windows,
                    activity: activity::activity(events, now, thresholds),
                    pace,
                })
            })
            .collect()
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

    /// One real, busy hour of Claude Code (made with scripts/sanitize-claude-log.mjs: only ids, models and
    /// token counts, times moved to start at 2026-01-01 00:00 UTC). It has repeated lines for most replies,
    /// your messages in between, and every gait. The expected numbers were worked out separately, in Python.
    #[test]
    fn a_real_busy_hour() {
        const START: i64 = 1767225600; // 2026-01-01 00:00 UTC
        let fixture = include_str!("../tests/fixtures/claude-busy-hour.jsonl");
        let events: Vec<_> = fixture
            .lines()
            .filter_map(transcripts::parse_line)
            .collect();
        let mut tracker = Tracker::default();
        tracker.add(events, START + 3600);
        assert_eq!(tracker.events().len(), 137, "698 lines, 137 replies");

        let at = |minutes: i64| {
            activity::activity(
                tracker.events(),
                START + minutes * 60,
                GaitThresholds::default(),
            )
        };
        use crate::activity::Gait::*;
        assert_eq!((at(5).gait, at(5).recent_tokens), (Walking, 123_257));
        assert_eq!((at(10).gait, at(10).recent_tokens), (Jogging, 322_031));
        assert_eq!((at(30).gait, at(30).recent_tokens), (Running, 532_440));
        assert_eq!((at(59).gait, at(59).recent_tokens), (Running, 707_038));
        assert_eq!(at(70).gait, Sleeping, "you stopped after the hour");
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
