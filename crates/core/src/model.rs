use serde::{Deserialize, Serialize};

use crate::activity::{Activity, Pace};
use crate::tips::Tip;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum AgentId {
    Claude,
    Codex,
    Gemini,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
}

impl TokenUsage {
    /// The bigger of each number. When the same reply is logged twice, the later line has the final counts.
    pub fn max(self, other: TokenUsage) -> TokenUsage {
        TokenUsage {
            input: self.input.max(other.input),
            output: self.output.max(other.output),
            cache_read: self.cache_read.max(other.cache_read),
            cache_write: self.cache_write.max(other.cache_write),
            reasoning: self.reasoning.max(other.reasoning),
        }
    }

    pub fn effective(&self) -> u64 {
        self.input + self.output + self.cache_write + (self.cache_read / 10)
    }
}

/// One reply (Claude) or turn (Codex) and the tokens it used.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageEvent {
    pub agent: AgentId,
    /// Which reply this is. An agent can log the same reply more than once (Claude writes one line per part
    /// of a reply), and the Tracker keeps one event per id.
    pub id: String,
    pub ts: i64,
    pub session_id: String,
    pub model: Option<String>,
    /// The reasoning effort it ran at, e.g. `"high"` (Claude) or `"xhigh"` (Codex).
    pub effort: Option<String>,
    /// Ran in Claude Code's fast mode (logged as a `speed` other than `"standard"`).
    pub fast: bool,
    pub tokens: TokenUsage,
}

/// One rate-limit window, like "6% of your 30-day limit, resets Jul 31".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct LimitWindow {
    pub used_pct: f64,
    pub window_minutes: u32,
    /// Unix seconds.
    // ts-rs would make i64 a TypeScript `bigint`, but JSON sends a plain number.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub resets_at: Option<i64>,
    /// Unix seconds.
    #[cfg_attr(test, ts(type = "number"))]
    pub observed_at: i64,
    /// True when the source rounds its numbers (Claude Code sends whole-number % and resets on the hour),
    /// so the bar shows "~6%" instead of claiming more precision than we have.
    pub approximate: bool,
}

impl LimitWindow {
    /// True once the reset time has passed, so this reading is out of date (the limit is back to 0%).
    pub fn has_reset(&self, now: i64) -> bool {
        match self.resets_at {
            Some(reset_time) => now >= reset_time,
            None => false,
        }
    }
}

/// Everything the bar shows for one agent. Grows later (tokens today, model, gait...).
/// `Serialize` lets Tauri send it to the bar as JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub agent: AgentId,
    /// 0, 1 or 2 limit bars.
    pub windows: Vec<LimitWindow>,
    /// How hard this agent is working right now (the critter's gait).
    pub activity: Activity,
    /// How fast each window is filling, for the windows we have enough readings for.
    pub pace: Vec<Pace>,
    /// The best tip for this agent right now that you haven't hidden (see `tips.rs`).
    pub tip: Option<Tip>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_counts_cache_reads_as_a_tenth() {
        let usage = TokenUsage {
            input: 100,
            output: 50,
            cache_read: 1000,
            ..Default::default()
        };
        assert_eq!(usage.effective(), 250);
    }

    #[test]
    fn has_reset_after_reset_time() {
        let window = LimitWindow {
            used_pct: 80.0,
            window_minutes: 300,
            resets_at: Some(1000),
            observed_at: 500,
            approximate: false,
        };
        assert!(!window.has_reset(999)); // one second before: still counts
        assert!(window.has_reset(1000)); // exactly at reset: back to 0
        assert!(window.has_reset(2000));
    }

    #[test]
    fn unknown_reset_time_never_resets() {
        let window = LimitWindow {
            used_pct: 80.0,
            window_minutes: 300,
            resets_at: None,
            observed_at: 500,
            approximate: false,
        };
        assert!(!window.has_reset(i64::MAX));
    }
}
