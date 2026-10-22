//! Claude Desktop's own usage history, `~/Library/Application Support/Claude/plan-usage-history.json`: the
//! app's readings of your 5-hour (`fh`) and weekly (`cw`) limit %, with times in milliseconds:
//!
//! ```json
//! {"version": 2, "samples": [{"t": 1792665952343, "org": "…", "u": {"fh": 15, "cw": 0}}]}
//! ```
//!
//! It has no reset times, so it's a backup: it fills in when it's newer than the statusLine bridge's reading
//! (or the bridge isn't set up), and its readings help the pace right after StatusPal starts.

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct History {
    samples: Vec<RawSample>,
}

#[derive(Debug, Deserialize)]
struct RawSample {
    t: i64,
    u: RawUsage,
}

#[derive(Debug, Deserialize)]
struct RawUsage {
    fh: Option<f64>,
    cw: Option<f64>,
}

/// One reading: when (Unix seconds) and the 5-hour and weekly %.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub at: i64,
    pub five_hour: Option<f64>,
    pub week: Option<f64>,
}

pub fn history_file() -> Option<PathBuf> {
    std::env::home_dir()
        .map(|home| home.join("Library/Application Support/Claude/plan-usage-history.json"))
}

pub fn parse(contents: &str) -> Vec<Sample> {
    let Ok(history) = serde_json::from_str::<History>(contents) else {
        return Vec::new();
    };
    let mut samples: Vec<Sample> = history
        .samples
        .into_iter()
        .map(|s| Sample {
            at: s.t / 1000,
            five_hour: s.u.fh,
            week: s.u.cw,
        })
        .collect();
    samples.sort_by_key(|s| s.at);
    samples
}

/// Every reading, oldest first. Errors if the file doesn't exist (Claude Desktop isn't installed).
pub fn read(path: &Path) -> io::Result<Vec<Sample>> {
    Ok(parse(&std::fs::read_to_string(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_samples_oldest_first() {
        let json = r#"{"version":2,"samples":[{"t":1792665952343,"org":"o","u":{"fh":15,"cw":0}},{"t":1792512962850,"org":"o","u":{"fh":8}}]}"#;
        assert_eq!(
            parse(json),
            [
                Sample {
                    at: 1792512962,
                    five_hour: Some(8.0),
                    week: None
                },
                Sample {
                    at: 1792665952,
                    five_hour: Some(15.0),
                    week: Some(0.0)
                },
            ]
        );
        assert!(parse("not json").is_empty());
    }
}
