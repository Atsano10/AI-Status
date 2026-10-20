//! Debug view: prints your real usage numbers, before any UI exists.
//! Run with `cargo run -p statuspal-core --example status`.

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

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
    Ok(())
}

/// "100% of 30-day limit (reset Jul 31, so 0% now)" or "6% of 30-day limit, resets Jul 31".
fn describe(limit: &LimitWindow, now: i64) -> String {
    let window = match limit.window_minutes {
        300 => "5-hour".to_string(),
        10080 => "weekly".to_string(),
        43200 => "30-day".to_string(),
        m => format!("{m}-minute"),
    };
    let reset = limit
        .resets_at
        .map(date)
        .unwrap_or_else(|| "unknown".to_string());

    if limit.has_reset(now) {
        format!(
            "{:.0}% of {window} limit (reset {reset}, so 0% now)",
            limit.used_pct
        )
    } else {
        format!("{:.0}% of {window} limit, resets {reset}", limit.used_pct)
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
