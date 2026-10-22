//! Reads the JSON that Claude Code sends to its statusLine command. Our `statuspal-statusline` program
//! (`src/bin/statusline.rs`) saves it to `~/Library/Application Support/com.atsano10.statuspal/claude-statusline.json`
//! and prints the one-line summary you see under Claude Code's prompt.
//!
//! The part we need: `{"rate_limits": {"five_hour": {"used_percentage": 3, "resets_at": 1792526400}, "seven_day": {...}}}`.
//! Either window can be missing (and on this Mac `seven_day` hasn't shown up yet).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Deserialize;

use crate::model::LimitWindow;

/// The name of the file our statusLine program saves.
pub const FILE_NAME: &str = "claude-statusline.json";

/// Where our statusLine script saves the JSON. `None` if we can't find your home folder.
pub fn statusline_file() -> Option<PathBuf> {
    crate::app_dir().map(|dir| dir.join(FILE_NAME))
}

/// What Claude Code runs as its status line (`statuspal --statusline`, or the `statuspal-statusline` program):
/// saves the JSON from stdin for the app, then prints one line. If you had a status line before StatusPal
/// (see `bridge.rs`), that's the line you see; otherwise ours, e.g. `Opus 5.5 · 5h 8% (3h 12m)`.
///
/// It must never crash or hang: if anything fails we still print a line, so Claude Code's status line keeps working.
pub fn run_bridge() {
    use std::io::Read;

    let mut contents = String::new();
    if std::io::stdin().read_to_string(&mut contents).is_err() {
        println!("StatusPal: couldn't read stdin");
        return;
    }

    // eprintln goes to stderr, which Claude Code ignores, so a failed save doesn't clutter the status line.
    match statusline_file() {
        Some(path) => {
            if let Err(e) = save(&path, &contents) {
                eprintln!("StatusPal: couldn't save {}: {e}", path.display());
            }
        }
        None => eprintln!("StatusPal: can't find your home folder"),
    }

    let line = super::bridge::run_chained(&contents).unwrap_or_else(|| {
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        summary(&contents, now)
    });
    println!("{line}");
}

#[derive(Debug, Deserialize)]
struct StatusLine {
    model: Option<RawModel>,
    rate_limits: Option<RawRateLimits>,
}

/// `model`, e.g. `{"id": "claude-opus-5-5[1m]", "display_name": "Opus 5.5 (1M context)"}`.
/// `Option` matters: if a required field were missing, serde would reject the whole JSON, rate limits included.
#[derive(Debug, Deserialize)]
struct RawModel {
    display_name: Option<String>,
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
            // Claude Code sends a whole-number % and a reset time rounded to the hour.
            approximate: true,
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

/// Saves the statusLine JSON to `path` without a reader ever seeing a half-written file.
/// Creates the folder if it's missing.
pub fn save(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

/// The line Claude Code shows under its prompt, e.g. `Opus 5.5 · 5h 8% (3h 12m)`.
pub fn summary(contents: &str, now: i64) -> String {
    let mut line = model_name(contents);
    for window in parse(contents, now) {
        let label = match window.window_minutes {
            300 => "5h",
            10080 => "wk",
            _ => "limit",
        };
        if window.has_reset(now) {
            line.push_str(&format!(" · {label} 0%"));
            continue;
        }
        line.push_str(&format!(" · {label} {:.0}%", window.used_pct));
        if let Some(reset) = window.resets_at {
            line.push_str(&format!(" ({})", until(reset - now)));
        }
    }
    line
}

/// "Opus 5.5 (1M context)" → "Opus 5.5". Just "Claude" if the JSON has no model.
fn model_name(contents: &str) -> String {
    serde_json::from_str::<StatusLine>(contents)
        .ok()
        .and_then(|status| status.model)
        .and_then(|model| model.display_name)
        .map(|name| match name.split_once(" (") {
            Some((short, _)) => short.to_string(),
            None => name,
        })
        .unwrap_or_else(|| "Claude".to_string())
}

/// 11520 → "3h 12m", 353000 → "4d 2h".
fn until(secs: i64) -> String {
    let (days, hours, minutes) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else {
        format!("{hours}h {minutes}m")
    }
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
                    approximate: true,
                },
                LimitWindow {
                    used_pct: 41.5,
                    window_minutes: 10080,
                    resets_at: Some(1792500000),
                    observed_at: 1792090000,
                    approximate: true,
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

    #[test]
    fn summary_with_five_hour_window() {
        let json = r#"{"model":{"display_name":"Opus 5.5 (1M context)"},"rate_limits":{"five_hour":{"used_percentage":8,"resets_at":1792526400}}}"#;
        // 3h 12m before the reset.
        assert_eq!(
            summary(json, 1792526400 - 11520),
            "Opus 5.5 · 5h 8% (3h 12m)"
        );
    }

    #[test]
    fn summary_with_both_windows() {
        let json = r#"{"model":{"display_name":"Sonnet 5.5"},"rate_limits":{"five_hour":{"used_percentage":14,"resets_at":1792092000},"seven_day":{"used_percentage":41,"resets_at":1792445000}}}"#;
        assert_eq!(
            summary(json, 1792090000),
            "Sonnet 5.5 · 5h 14% (0h 33m) · wk 41% (4d 2h)"
        );
    }

    #[test]
    fn summary_after_the_reset() {
        let json = r#"{"model":{"display_name":"Opus 5.5"},"rate_limits":{"five_hour":{"used_percentage":90,"resets_at":1000}}}"#;
        assert_eq!(summary(json, 2000), "Opus 5.5 · 5h 0%");
    }

    #[test]
    fn summary_without_limits_or_model() {
        assert_eq!(
            summary(r#"{"model":{"display_name":"Opus 5.5"}}"#, 0),
            "Opus 5.5"
        );
        assert_eq!(summary("not json", 0), "Claude");
    }

    #[test]
    fn save_creates_the_folder_and_leaves_no_temp_file() {
        let dir =
            std::env::temp_dir().join(format!("statuspal-test-statusline-{}", std::process::id()));
        let path = dir.join("nested/claude-statusline.json");

        save(&path, "first").unwrap();
        save(&path, "second").unwrap(); // overwriting works too
        let contents = fs::read_to_string(&path).unwrap();
        let tmp_left = path.with_extension("json.tmp").exists();
        let _ = fs::remove_dir_all(&dir); // clean up before asserting, so a failure doesn't leave junk

        assert_eq!(contents, "second");
        assert!(!tmp_left);
    }
}
