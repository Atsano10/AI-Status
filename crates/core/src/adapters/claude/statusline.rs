//! Reads the JSON that Claude Code sends to its statusLine command, which our script saves to
//! `~/Library/Application Support/com.atsano10.statuspal/claude-statusline.json`.
//!
//! The part we need: `{"rate_limits": {"five_hour": {"used_percentage": 3, "resets_at": 1792526400}, "seven_day": {...}}}`.
//! Either window can be missing (and on this Mac `seven_day` hasn't shown up yet).

use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Deserialize;

use crate::model::LimitWindow;

/// Where our statusLine script saves the JSON. `None` if we can't find your home folder.
pub fn statusline_file() -> Option<PathBuf> {
    std::env::home_dir().map(|home| {
        home.join("Library/Application Support/com.atsano10.statuspal/claude-statusline.json")
    })
}

#[derive(Debug, Deserialize)]
struct StatusLine {
    rate_limits: Option<RawRateLimits>,
}

/// `rate_limits`: the 5-hour and the weekly window, each one optional.
#[derive(Debug, Deserialize)]
struct RawRateLimits {
    five_hour: Option<RawWindow>,
    seven_day: Option<RawWindow>,
}

/// One window, e.g. `{"used_percentage": 3, "resets_at": 1792526400}`.
#[derive(Debug, Clone, Copy, Deserialize)]
struct RawWindow {
    used_percentage: f64,
    resets_at: Option<i64>,
}

impl RawWindow {
    fn to_limit_window(self, window_minutes: u32, observed_at: i64) -> LimitWindow {
        LimitWindow {
            used_pct: self.used_percentage,
            window_minutes,
            resets_at: self.resets_at,
            observed_at,
        }
    }
}

/// The limit windows in one statusLine JSON: 0, 1 or 2 of them.
/// `observed_at` is when the JSON was written; it has no timestamp of its own, so we use the file's.
pub fn parse(contents: &str, observed_at: i64) -> Vec<LimitWindow> {
    let Ok(status) = serde_json::from_str::<StatusLine>(contents) else {
        return Vec::new();
    };
    let Some(limits) = status.rate_limits else {
        return Vec::new();
    };

    let mut windows = Vec::new();
    if let Some(window) = limits.five_hour {
        windows.push(window.to_limit_window(300, observed_at));
    }
    if let Some(window) = limits.seven_day {
        windows.push(window.to_limit_window(10080, observed_at));
    }
    windows
}

/// Reads the statusLine file. Errors if it doesn't exist yet (Claude Code hasn't run since the script was set up).
pub fn read(path: &Path) -> io::Result<Vec<LimitWindow>> {
    let contents = std::fs::read_to_string(path)?;
    let modified = std::fs::metadata(path)?.modified()?;
    let observed_at = modified
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(parse(&contents, observed_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_windows() {
        let json = r#"{"model":{"id":"claude-opus-5-5"},"rate_limits":{"five_hour":{"used_percentage":14,"resets_at":1792092000},"seven_day":{"used_percentage":41.5,"resets_at":1792500000}}}"#;
        let windows = parse(json, 1792090000);
        assert_eq!(
            windows,
            [
                LimitWindow {
                    used_pct: 14.0,
                    window_minutes: 300,
                    resets_at: Some(1792092000),
                    observed_at: 1792090000,
                },
                LimitWindow {
                    used_pct: 41.5,
                    window_minutes: 10080,
                    resets_at: Some(1792500000),
                    observed_at: 1792090000,
                },
            ]
        );
    }

    #[test]
    fn five_hour_only() {
        // What this Mac actually gets right now.
        let json = r#"{"rate_limits":{"five_hour":{"used_percentage":3,"resets_at":1792526400}}}"#;
        let windows = parse(json, 1792520000);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].window_minutes, 300);
        assert_eq!(windows[0].used_pct, 3.0);
    }

    #[test]
    fn no_rate_limits_yet() {
        // Before the first reply of a session, `rate_limits` is null.
        assert!(parse(r#"{"rate_limits":null}"#, 0).is_empty());
        assert!(parse(r#"{"model":{"id":"claude-opus-5-5"}}"#, 0).is_empty());
        assert!(parse("not json", 0).is_empty());
    }
}
