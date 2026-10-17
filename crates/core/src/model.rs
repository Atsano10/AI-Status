//! Shared types that every agent adapter produces and the rest of the app consumes.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub struct UsageEvent{
    pub agent: AgentId,
    pub ts: i64,
    pub session_id: String,
    pub model: Option<String>,
    pub tokens: TokenUsage,
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
}
