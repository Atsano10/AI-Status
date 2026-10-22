//! Claude adapter. Claude's data comes from several places, one module each:
//! - `statusline`: the 5-hour and weekly limit %, written by our statusLine bridge
//! - `transcripts`: Claude Code's chat logs, for the tokens each reply used
//! - `desktop`: Claude Desktop's own usage history, a backup for the limit %
//! - `bridge`: connecting the statusLine bridge in Claude Code's settings
//! - `agents`: the helper pack, small subagents on cheaper models

use std::path::PathBuf;

use crate::activity::Reading;
use crate::adapters::Adapter;
use crate::model::{AgentId, LimitWindow, UsageEvent};

pub mod agents;
pub mod bridge;
pub mod desktop;
pub mod statusline;
pub mod transcripts;

/// Claude Code's folder: `~/.claude`, or `CLAUDE_CONFIG_DIR` if you set one.
pub fn config_dir() -> Option<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => Some(std::env::home_dir()?.join(".claude")),
    }
}

/// Claude: replies from Claude Code's chat logs, limits from the statusLine bridge with Claude Desktop's
/// history as a backup.
#[derive(Debug, Default)]
pub struct ClaudeAdapter {
    transcripts: transcripts::ClaudeReader,
    limits: Vec<LimitWindow>,
    desktop: Vec<desktop::Sample>,
}

impl Adapter for ClaudeAdapter {
    fn agent(&self) -> AgentId {
        AgentId::Claude
    }

    fn refresh(&mut self) -> Vec<UsageEvent> {
        // Both limit files are small, so they're simply read again each time.
        let bridge = statusline::statusline_file()
            .and_then(|path| statusline::read(&path).ok())
            .unwrap_or_default();
        self.desktop = desktop::history_file()
            .and_then(|path| desktop::read(&path).ok())
            .unwrap_or_default();
        self.limits = combine(bridge, self.desktop.last());

        match transcripts::projects_dir() {
            Some(dir) if dir.is_dir() => self.transcripts.poll(&dir),
            _ => Vec::new(),
        }
    }

    fn limits(&self) -> Vec<LimitWindow> {
        self.limits.clone()
    }

    fn waiting_since(&self) -> Option<i64> {
        self.transcripts.waiting_since()
    }

    fn past_readings(&self) -> Vec<(u32, Reading)> {
        self.desktop
            .iter()
            .flat_map(|s| {
                [(300, s.five_hour), (10080, s.week)]
                    .into_iter()
                    .filter_map(move |(minutes, pct)| Some((minutes, (s.at, pct?))))
            })
            .collect()
    }
}

/// The bridge's windows, updated with Claude Desktop's latest reading where that's newer. A Desktop reading
/// older than its window is out of date and ignored. Desktop has no reset times: a newer reading keeps the
/// bridge's reset time if that hadn't passed yet, otherwise the reset time is unknown.
fn combine(mut windows: Vec<LimitWindow>, latest: Option<&desktop::Sample>) -> Vec<LimitWindow> {
    let Some(latest) = latest else {
        return windows;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for (minutes, pct) in [(300u32, latest.five_hour), (10080, latest.week)] {
        let Some(pct) = pct else { continue };
        if now - latest.at > minutes as i64 * 60 {
            continue;
        }
        match windows.iter_mut().find(|w| w.window_minutes == minutes) {
            Some(w) if w.observed_at >= latest.at => {}
            Some(w) => {
                w.used_pct = pct;
                if w.resets_at.is_some_and(|reset| reset <= latest.at) {
                    w.resets_at = None;
                }
                w.observed_at = latest.at;
            }
            // Desktop says 0% weekly even when there's no weekly limit, so it only adds a weekly window
            // that shows something.
            None if minutes == 300 || pct > 0.0 => windows.push(LimitWindow {
                used_pct: pct,
                window_minutes: minutes,
                resets_at: None,
                observed_at: latest.at,
                approximate: true,
            }),
            None => {}
        }
    }
    windows.sort_by_key(|w| w.window_minutes);
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn bridge(pct: f64, observed_ago: i64, resets_in: i64) -> LimitWindow {
        LimitWindow {
            used_pct: pct,
            window_minutes: 300,
            resets_at: Some(now() + resets_in),
            observed_at: now() - observed_ago,
            approximate: true,
        }
    }

    fn sample(ago: i64, fh: f64, cw: f64) -> desktop::Sample {
        desktop::Sample {
            at: now() - ago,
            five_hour: Some(fh),
            week: Some(cw),
        }
    }

    #[test]
    fn the_newer_reading_wins() {
        let newer_desktop = combine(vec![bridge(10.0, 600, 3600)], Some(&sample(60, 14.0, 0.0)));
        assert_eq!(newer_desktop[0].used_pct, 14.0);
        assert!(
            newer_desktop[0].resets_at.is_some(),
            "same window, so the reset time stays"
        );

        let newer_bridge = combine(vec![bridge(10.0, 60, 3600)], Some(&sample(600, 14.0, 0.0)));
        assert_eq!(newer_bridge[0].used_pct, 10.0);
    }

    #[test]
    fn desktop_fills_in_without_the_bridge() {
        let windows = combine(Vec::new(), Some(&sample(60, 22.0, 0.0)));
        assert_eq!(windows.len(), 1, "no weekly window from a 0%");
        assert_eq!(
            (windows[0].window_minutes, windows[0].used_pct),
            (300, 22.0)
        );
        assert_eq!(windows[0].resets_at, None);
    }

    #[test]
    fn an_old_desktop_reading_is_ignored() {
        assert!(combine(Vec::new(), Some(&sample(6 * 3600, 50.0, 0.0))).is_empty());
    }
}
