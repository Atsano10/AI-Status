use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AgentId {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
}

impl TokenUsage {
    pub fn effective(&self) -> u64 {
        self.input + self.output + self.cache_write + (self.cache_read / 10)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageEvent {
    pub agent: AgentId,
    pub ts: i64,
    pub session_id: String,
    pub model: Option<String>,
    pub tokens: TokenUsage,
}

/// One rate-limit window, like "6% of your 30-day limit, resets Jul 31".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LimitWindow {
    pub used_pct: f64,
    pub window_minutes: u32,
    pub resets_at: Option<i64>,
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
pub struct UsageSnapshot {
    pub agent: AgentId,
    /// 0, 1 or 2 limit bars.
    pub windows: Vec<LimitWindow>,
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
