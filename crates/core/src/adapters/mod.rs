//! One adapter per agent. Each reads that agent's local files and produces the shared types in `model`.
//! The Tracker keeps one of each and treats them all the same way (`Adapter`), so a new agent is one new
//! adapter.

use std::fmt::Debug;

use crate::activity::Reading;
use crate::model::{AgentId, LimitWindow, UsageEvent};

pub mod claude;
pub mod codex;
pub mod codex_config;

/// What every agent's adapter does.
pub trait Adapter: Debug + Send {
    fn agent(&self) -> AgentId;

    /// Reads whatever the agent wrote since last time: returns the new replies and updates the limits.
    /// The first call reads the recent past.
    fn refresh(&mut self) -> Vec<UsageEvent>;

    /// The latest limit windows (0, 1 or 2), as of the last `refresh`.
    fn limits(&self) -> Vec<LimitWindow>;

    /// Since when (Unix seconds) the agent has been waiting for you: it finished and it's your turn. `None`
    /// while it's working, or if it can't tell.
    fn waiting_since(&self) -> Option<i64> {
        None
    }

    /// Older limit readings the agent kept itself, by window length in minutes, for the pace. Most agents
    /// have none; we collect our own as we go.
    fn past_readings(&self) -> Vec<(u32, Reading)> {
        Vec::new()
    }
}

/// Every agent StatusPal knows.
pub fn all() -> Vec<Box<dyn Adapter>> {
    vec![
        Box::new(claude::ClaudeAdapter::default()),
        Box::new(codex::CodexAdapter::default()),
    ]
}
