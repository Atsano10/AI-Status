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
pub mod hook;
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
    /// When your Claude Code replies came, for the last 6 hours (`combine` uses them to date a window).
    uses: Vec<i64>,
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
        let events = match transcripts::projects_dir() {
            Some(dir) if dir.is_dir() => self.transcripts.poll(&dir),
            _ => Vec::new(),
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        self.uses.extend(events.iter().map(|e| e.ts));
        self.uses.retain(|t| now - t < 6 * 3600);
        self.limits = combine(bridge, &self.desktop, &self.uses, now);
        events
    }

    fn limits(&self) -> Vec<LimitWindow> {
        self.limits.clone()
    }

    fn waiting_since(&self) -> Option<i64> {
        self.transcripts.waiting_since()
    }

    fn prompt_guess(&self) -> Option<crate::classify::PromptGuess> {
        crate::classify::guess_file().and_then(|path| crate::classify::read(&path))
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

/// The bridge's windows, brought up to date with Claude Desktop's newest readings. `uses` are the times
/// of your Claude Code replies, for guessing when a window started.
///
/// Usage only goes up within a window, so for the same window the higher reading is the more current one.
/// A Desktop reading from a newer window than the bridge's (the bridge's reset time has passed) replaces
/// it. Desktop has no reset times, so the 5-hour one is worked out: windows reset on the hour, 5 hours
/// after they start, and the window can't have started after the first use we saw following the old
/// reset. If someone else started it earlier, it resets earlier, so it's shown with a "~".
fn combine(
    mut windows: Vec<LimitWindow>,
    samples: &[desktop::Sample],
    uses: &[i64],
    now: i64,
) -> Vec<LimitWindow> {
    type Value = fn(&desktop::Sample) -> Option<f64>;
    let kinds: [(u32, Value); 2] = [(300, |s| s.five_hour), (10080, |s| s.week)];
    for (minutes, value) in kinds {
        let length = minutes as i64 * 60;
        // The newest reading that has this window (Desktop also writes readings without one).
        let Some((at, pct)) = samples.iter().rev().find_map(|s| Some((s.at, value(s)?))) else {
            continue;
        };
        if now - at > length {
            continue; // older than the window itself: out of date
        }
        let bridge = windows.iter_mut().find(|w| w.window_minutes == minutes);
        let same_window = |w: &LimitWindow| match w.resets_at {
            Some(reset) => reset > at,
            None => at - w.observed_at < length,
        };
        match bridge {
            Some(w) if same_window(w) => {
                w.used_pct = w.used_pct.max(pct);
                w.observed_at = w.observed_at.max(at);
            }
            // Desktop says 0% weekly even when there's no weekly limit, so it only adds a weekly window
            // that shows something.
            None if minutes != 300 && pct <= 0.0 => {}
            stale => {
                let previous_reset = stale.as_ref().and_then(|w| w.resets_at);
                let resets_at = (minutes == 300)
                    .then(|| started_by(samples, uses, previous_reset.unwrap_or(at - length), at))
                    .flatten()
                    .map(|start| start - start.rem_euclid(3600) + length)
                    .filter(|reset| *reset > now);
                let fresh = LimitWindow {
                    used_pct: pct,
                    window_minutes: minutes,
                    resets_at,
                    observed_at: at,
                    approximate: true,
                };
                match stale {
                    Some(w) => *w = fresh,
                    None => windows.push(fresh),
                }
            }
        }
    }
    windows.sort_by_key(|w| w.window_minutes);
    windows
}

/// The first sign of use after `after` (Unix seconds) and no later than `at`: a Claude Code reply, or a
/// Desktop reading above 0%.
fn started_by(samples: &[desktop::Sample], uses: &[i64], after: i64, at: i64) -> Option<i64> {
    let desktop = samples
        .iter()
        .filter(|s| s.five_hour.is_some_and(|pct| pct > 0.0))
        .map(|s| s.at);
    uses.iter()
        .copied()
        .chain(desktop)
        .filter(|t| *t > after && *t <= at)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Your real afternoon on 2026-10-22: the bridge's reading came from the window before, Desktop saw 43% at
    /// 18:11, the bridge then saw 48% at 18:43 with the reset at 21:00, and Claude Code replies from 16:00 on.
    const NOW: i64 = 1792709020; // 18:43:40 (your time)
    const AT_1811: i64 = 1792707114;
    const RESET_21: i64 = 1792717200;

    fn bridge(pct: f64, observed_at: i64, resets_at: i64) -> LimitWindow {
        LimitWindow {
            used_pct: pct,
            window_minutes: 300,
            resets_at: Some(resets_at),
            observed_at,
            approximate: true,
        }
    }

    fn sample(at: i64, fh: Option<f64>) -> desktop::Sample {
        desktop::Sample {
            at,
            five_hour: fh,
            week: Some(0.0),
        }
    }

    #[test]
    fn same_window_takes_the_higher_reading_and_keeps_the_reset() {
        let windows = combine(
            vec![bridge(48.0, NOW - 1, RESET_21)],
            &[sample(AT_1811, Some(43.0))],
            &[],
            NOW,
        );
        assert_eq!(windows[0].used_pct, 48.0);
        assert_eq!(windows[0].resets_at, Some(RESET_21));

        // And the other way round: Desktop newer and higher.
        let windows = combine(
            vec![bridge(40.0, AT_1811 - 600, RESET_21)],
            &[sample(AT_1811, Some(43.0))],
            &[],
            NOW,
        );
        assert_eq!(windows[0].used_pct, 43.0);
        assert_eq!(windows[0].resets_at, Some(RESET_21));
    }

    #[test]
    fn a_newer_window_from_desktop_gets_a_worked_out_reset() {
        // The bridge's last reading is from the window that reset at 15:00. Claude Code replies from 16:04,
        // Desktop's first reading only at 18:11: the window started by 16:04, so it resets by 21:00.
        let old_reset = RESET_21 - 6 * 3600;
        let windows = combine(
            vec![bridge(90.0, old_reset - 600, old_reset)],
            &[sample(AT_1811 - 5000, None), sample(AT_1811, Some(43.0))],
            &[RESET_21 - 5 * 3600 + 240, AT_1811 - 60],
            NOW,
        );
        assert_eq!(windows[0].used_pct, 43.0);
        assert_eq!(windows[0].resets_at, Some(RESET_21));
        assert!(windows[0].approximate);
    }

    #[test]
    fn skips_desktop_readings_without_the_window() {
        // Desktop's newest reading has no 5-hour number (an "xu" one), so the one before it counts.
        let windows = combine(
            Vec::new(),
            &[sample(AT_1811, Some(43.0)), sample(AT_1811 + 10, None)],
            &[],
            NOW,
        );
        assert_eq!(windows[0].used_pct, 43.0);
    }

    #[test]
    fn desktop_fills_in_without_the_bridge() {
        let windows = combine(Vec::new(), &[sample(NOW - 60, Some(22.0))], &[], NOW);
        assert_eq!(windows.len(), 1, "no weekly window from a 0%");
        assert_eq!(
            (windows[0].window_minutes, windows[0].used_pct),
            (300, 22.0)
        );
    }

    #[test]
    fn an_old_desktop_reading_is_ignored() {
        assert!(combine(Vec::new(), &[sample(NOW - 6 * 3600, Some(50.0))], &[], NOW).is_empty());
    }
}
