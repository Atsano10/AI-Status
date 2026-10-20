//! Debug view: prints your real usage numbers, before any UI exists.
//! Run with `cargo run -p statuspal-core --example status`.

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use statuspal_core::adapters::claude::statusline;
use statuspal_core::adapters::codex;
use statuspal_core::model::LimitWindow;

// Returning a Result from main lets us use `?`: any error stops the program and gets printed.
fn main() -> Result<(), Box<dyn Error>> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

    let dir = codex::sessions_dir().ok_or("can't find your home folder")?;
    let files = codex::find_session_files(&dir)?;

    let mut turns = 0;
    let mut effective_tokens = 0;
    let mut latest: Option<LimitWindow> = None;

    for file in &files {
        let contents = std::fs::read_to_string(file)?;

        let events = codex::parse_session(&contents);
        turns += events.len();
        effective_tokens += events.iter().map(|e| e.tokens.effective()).sum::<u64>();

        // Keep whichever reading is newest across all files.
        if let Some(limit) = codex::latest_limit(&contents)
            && latest
                .as_ref()
                .is_none_or(|l| limit.observed_at > l.observed_at)
        {
            latest = Some(limit);
        }
    }

    println!(
        "Codex · {} sessions · {} turns · {} effective tokens",
        files.len(),
        turns,
        short(effective_tokens)
    );
    match latest {
        Some(limit) => println!("  {}", describe(&limit, now)),
        None => println!("  no limit readings yet"),
    }

    let statusline_path = statusline::statusline_file().ok_or("can't find your home folder")?;
    match statusline::read(&statusline_path) {
        Ok(windows) if windows.is_empty() => {
            println!("Claude · no limit readings yet (send a message in Claude Code)")
        }
        Ok(windows) => {
            println!("Claude");
            for window in &windows {
                println!("  {}", describe(window, now));
            }
        }
        Err(e) => println!("Claude · can't read the statusLine file: {e}"),
    }
    Ok(())
}

/// "100% of 30-day limit (reset Jul 31, so 0% now)" or "3% of 5-hour limit, resets in 2h 10m".
fn describe(limit: &LimitWindow, now: i64) -> String {
    let window = match limit.window_minutes {
        300 => "5-hour".to_string(),
        10080 => "weekly".to_string(),
        43200 => "30-day".to_string(),
        m => format!("{m}-minute"),
    };
    let used = limit.used_pct;
    let approx = if limit.approximate { "~" } else { "" };

    match limit.resets_at {
        Some(reset) if limit.has_reset(now) => {
            format!(
                "{used:.0}% of {window} limit (reset {}, so 0% now)",
                date(reset)
            )
        }
        Some(reset) => format!(
            "{approx}{used:.0}% of {window} limit, resets in {approx}{}",
            duration(reset - now)
        ),
        None => format!("{approx}{used:.0}% of {window} limit, reset time unknown"),
    }
}

/// 7800 seconds → "2h 10m"; 280000 → "3d 5h"
fn duration(secs: i64) -> String {
    let (days, hours, minutes) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else {
        format!("{hours}h {minutes}m")
    }
}

fn date(unix_secs: i64) -> String {
    chrono::DateTime::from_timestamp(unix_secs, 0)
        .map(|t| t.format("%b %-d").to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// 2294054 → "2.3M"
fn short(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}
