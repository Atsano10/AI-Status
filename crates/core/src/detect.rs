//! Works out which agent you're using right now, so the bar can show the matching creature.
//!
//! The rules, first match wins:
//! 1. An agent you **pinned** from the menu bar menu.
//! 2. **Claude.app** or **Codex.app** is the app in front → that agent.
//! 3. A **terminal or editor** is in front → whichever agent wrote its files most recently (within 2 minutes).
//! 4. Otherwise keep the agent we showed last time, so the creature doesn't flicker when you glance at a browser.

use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::adapters::claude::statusline;
use crate::adapters::codex;
use crate::model::AgentId;

/// How recent an agent's last write must be to count as "you're using it" (rule 3).
pub const RECENT_SECS: i64 = 120;

/// Apps where you run Claude Code or Codex CLI. Matched by bundle id, which never changes when an app is
/// renamed or updated. JetBrains IDEs all start with `com.jetbrains.`, so they're checked separately.
const WORKBENCH_APPS: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "dev.warp.Warp-Stable",
    "net.kovidgoyal.kitty",
    "org.alacritty",
    "com.github.wez.wezterm",
    "com.microsoft.VSCode",
    "com.todesktop.230313mzl4w4u92", // Cursor
    "com.exafunction.windsurf",
    "dev.zed.Zed",
    "com.apple.dt.Xcode",
];

/// What kind of app is in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
    /// An agent's own desktop app.
    Agent(AgentId),
    /// A terminal or editor, where you might be running either CLI.
    Workbench,
    /// Anything else (browser, Slack, Finder, ...).
    Other,
}

/// Everything the rules look at. The app gathers these every couple of seconds.
#[derive(Debug, Clone)]
pub struct Inputs {
    /// Agent pinned from the menu bar menu, if any.
    pub pinned: Option<AgentId>,
    /// Bundle id of the app in front, e.g. `com.googlecode.iterm2`. `None` if we couldn't tell.
    pub frontmost: Option<String>,
    /// When each agent last wrote its files, in Unix seconds (from [`last_writes`]).
    pub last_writes: Vec<(AgentId, i64)>,
    /// The agent we picked last time.
    pub previous: Option<AgentId>,
    /// The current time in Unix seconds.
    pub now: i64,
}

/// Sorts an app into [`AppKind`] by its bundle id.
pub fn classify_app(bundle_id: &str) -> AppKind {
    match bundle_id {
        "com.anthropic.claudefordesktop" => AppKind::Agent(AgentId::Claude),
        "com.openai.codex" => AppKind::Agent(AgentId::Codex),
        id if WORKBENCH_APPS.contains(&id) || id.starts_with("com.jetbrains.") => {
            AppKind::Workbench
        }
        _ => AppKind::Other,
    }
}

/// The agent that wrote its files most recently, if that was within [`RECENT_SECS`] of `now`.
fn newest_recent_write(last_writes: &[(AgentId, i64)], now: i64) -> Option<AgentId> {
    last_writes
        .iter()
        .filter(|(_, time)| now - *time <= RECENT_SECS)
        .max_by_key(|(_, time)| *time)
        .map(|(agent, _)| *agent)
}

/// Applies the four rules in order. See the top of this file.
pub fn pick_active(inputs: &Inputs) -> Option<AgentId> {
    if let Some(agent) = inputs.pinned {
        return Some(agent);
    }

    let kind = match &inputs.frontmost {
        Some(id) => classify_app(id),
        None => AppKind::Other,
    };

    match kind {
        AppKind::Agent(agent) => Some(agent),
        AppKind::Workbench => {
            newest_recent_write(&inputs.last_writes, inputs.now).or(inputs.previous)
        }
        AppKind::Other => inputs.previous,
    }
}

/// When each agent last wrote the files we read, in Unix seconds. An agent with no files is left out.
pub fn last_writes() -> Vec<(AgentId, i64)> {
    let mut writes = Vec::new();

    if let Some(file) = statusline::statusline_file()
        && let Some(time) = modified_secs(&file)
    {
        writes.push((AgentId::Claude, time));
    }

    // Any session can be written to (you can resume an old one), so take the newest across all of them.
    if let Some(dir) = codex::sessions_dir()
        && let Ok(files) = codex::find_session_files(&dir)
        && let Some(time) = files.iter().filter_map(|f| modified_secs(f)).max()
    {
        writes.push((AgentId::Codex, time));
    }

    writes
}

/// A file's last-modified time in Unix seconds, or `None` if it's missing or unreadable.
fn modified_secs(path: &Path) -> Option<i64> {
    let modified = path.metadata().ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_500_000;

    /// Inputs with nothing going on: no pin, nothing in front, no writes, nothing shown before.
    fn quiet() -> Inputs {
        Inputs {
            pinned: None,
            frontmost: None,
            last_writes: vec![],
            previous: None,
            now: NOW,
        }
    }

    fn front(bundle_id: &str) -> Option<String> {
        Some(bundle_id.to_string())
    }

    #[test]
    fn classifies_agent_apps() {
        assert_eq!(
            classify_app("com.anthropic.claudefordesktop"),
            AppKind::Agent(AgentId::Claude)
        );
        assert_eq!(
            classify_app("com.openai.codex"),
            AppKind::Agent(AgentId::Codex)
        );
    }

    #[test]
    fn classifies_terminals_and_editors() {
        assert_eq!(classify_app("com.googlecode.iterm2"), AppKind::Workbench);
        assert_eq!(classify_app("com.microsoft.VSCode"), AppKind::Workbench);
        assert_eq!(classify_app("com.jetbrains.rustrover"), AppKind::Workbench);
    }

    #[test]
    fn classifies_everything_else_as_other() {
        assert_eq!(classify_app("com.apple.Safari"), AppKind::Other);
        assert_eq!(classify_app(""), AppKind::Other);
        // Close to a real id isn't enough.
        assert_eq!(
            classify_app("com.anthropic.claude-code-url-handler"),
            AppKind::Other
        );
    }

    #[test]
    fn newest_recent_write_picks_the_latest() {
        let writes = [(AgentId::Claude, NOW - 60), (AgentId::Codex, NOW - 5)];
        assert_eq!(newest_recent_write(&writes, NOW), Some(AgentId::Codex));
    }

    #[test]
    fn newest_recent_write_ignores_old_writes() {
        let writes = [(AgentId::Claude, NOW - 30), (AgentId::Codex, NOW - 3600)];
        assert_eq!(newest_recent_write(&writes, NOW), Some(AgentId::Claude));
        let stale = [(AgentId::Claude, NOW - RECENT_SECS - 1)];
        assert_eq!(newest_recent_write(&stale, NOW), None);
        assert_eq!(newest_recent_write(&[], NOW), None);
    }

    #[test]
    fn pin_beats_everything() {
        let inputs = Inputs {
            pinned: Some(AgentId::Codex),
            frontmost: front("com.anthropic.claudefordesktop"),
            last_writes: vec![(AgentId::Claude, NOW)],
            ..quiet()
        };
        assert_eq!(pick_active(&inputs), Some(AgentId::Codex));
    }

    #[test]
    fn agent_app_in_front_wins_over_recent_writes() {
        let inputs = Inputs {
            frontmost: front("com.openai.codex"),
            last_writes: vec![(AgentId::Claude, NOW)],
            ..quiet()
        };
        assert_eq!(pick_active(&inputs), Some(AgentId::Codex));
    }

    #[test]
    fn terminal_in_front_uses_the_newest_write() {
        let inputs = Inputs {
            frontmost: front("com.googlecode.iterm2"),
            last_writes: vec![(AgentId::Claude, NOW - 10), (AgentId::Codex, NOW - 90)],
            previous: Some(AgentId::Codex),
            ..quiet()
        };
        assert_eq!(pick_active(&inputs), Some(AgentId::Claude));
    }

    #[test]
    fn terminal_in_front_without_recent_writes_keeps_previous() {
        let inputs = Inputs {
            frontmost: front("com.apple.Terminal"),
            last_writes: vec![(AgentId::Claude, NOW - 3600)],
            previous: Some(AgentId::Codex),
            ..quiet()
        };
        assert_eq!(pick_active(&inputs), Some(AgentId::Codex));
    }

    #[test]
    fn other_app_in_front_keeps_previous() {
        let inputs = Inputs {
            frontmost: front("com.apple.Safari"),
            last_writes: vec![(AgentId::Claude, NOW)],
            previous: Some(AgentId::Codex),
            ..quiet()
        };
        assert_eq!(pick_active(&inputs), Some(AgentId::Codex));
    }

    #[test]
    fn unknown_front_app_keeps_previous() {
        let inputs = Inputs {
            previous: Some(AgentId::Claude),
            ..quiet()
        };
        assert_eq!(pick_active(&inputs), Some(AgentId::Claude));
        assert_eq!(pick_active(&quiet()), None);
    }
}
