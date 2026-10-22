//! Gemini CLI: replies from its chat logs, `~/.gemini/tmp/<project>/chats/session-*.jsonl`, and an estimated
//! daily limit.
//!
//! ```json
//! {"sessionId": "007d…", "startTime": "…", "kind": "main"}                              ← first line
//! {"id": "6f8a…", "timestamp": "2026-04-30T01:59:44.891Z", "type": "gemini", "model": "gemini-3-flash-preview",
//!  "tokens": {"input": 11538, "output": 174, "cached": 0, "thoughts": 265, "tool": 0, "total": 11977}}
//! ```
//!
//! A reply is logged again as it fills in (same `id`), and the Tracker keeps one per id. Only these fields
//! are read; what you and Gemini wrote isn't, and neither is `logs.json` (your prompts).
//!
//! **The limit:** Gemini doesn't save a %, but its plans allow a number of model *requests* a day, reset at
//! midnight Pacific time. Each reply is one request, so we count today's and compare with your plan's
//! allowance (`GeminiPlan`, picked in Settings or worked out from how you sign in).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{Datelike, NaiveDate, TimeZone, Utc, Weekday};
use serde::Deserialize;

use crate::adapters::Adapter;
use crate::model::{AgentId, LimitWindow, TokenUsage, UsageEvent};
use crate::settings::{self, GeminiPlan};
use crate::tail::Follower;

/// Files older than this are skipped the first time we look: the limit is daily, the critter wants minutes.
const LOOKBACK_SECS: u64 = 2 * 24 * 3600;

/// `~/.gemini`.
pub fn gemini_dir() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join(".gemini"))
}

/// Every chat log: `tmp/<project>/chats/session-*.jsonl`.
pub fn find_chats(gemini: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for project in std::fs::read_dir(gemini.join("tmp"))? {
        let chats = project?.path().join("chats");
        let Ok(entries) = std::fs::read_dir(&chats) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// True for Gemini's chat logs.
pub fn is_chat(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "jsonl")
        && path.parent().is_some_and(|p| p.ends_with("chats"))
        && gemini_dir().is_some_and(|dir| path.starts_with(dir))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    timestamp: Option<String>,
    model: Option<String>,
    tokens: Option<RawTokens>,
    /// On the first line of a file.
    session_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawTokens {
    input: u64,
    output: u64,
    cached: u64,
    thoughts: u64,
}

/// Reads one line, remembering the session id from the file's first line in `session`.
pub fn parse_line(line: &str, session: &mut String) -> Option<UsageEvent> {
    let line: Line = serde_json::from_str(line).ok()?;
    if let Some(id) = line.session_id.filter(|_| line.kind.is_none()) {
        *session = id;
        return None;
    }
    if line.kind.as_deref() != Some("gemini") {
        return None;
    }
    let t = line.tokens?;
    // `input` includes what came from the cache; `thoughts` is thinking, which Claude counts as output.
    let tokens = TokenUsage {
        input: t.input.saturating_sub(t.cached),
        output: t.output + t.thoughts,
        cache_read: t.cached,
        cache_write: 0,
        reasoning: t.thoughts,
    };
    if tokens == TokenUsage::default() {
        return None; // the first copy of a reply, before its counts are in
    }
    Some(UsageEvent {
        agent: AgentId::Gemini,
        id: line.id?,
        ts: chrono::DateTime::parse_from_rfc3339(line.timestamp.as_deref()?)
            .ok()?
            .timestamp(),
        session_id: session.clone(),
        model: line.model,
        effort: None,
        fast: false,
        tokens,
    })
}

/// How many requests a day your plan allows. `None` for plans without a fixed number (paid API key, Vertex).
pub fn daily_requests(plan: GeminiPlan, gemini: Option<&Path>) -> Option<u32> {
    match plan {
        GeminiPlan::Auto => daily_requests(sign_in_plan(gemini), gemini),
        GeminiPlan::Personal => Some(1000),
        GeminiPlan::Pro | GeminiPlan::Standard => Some(1500),
        GeminiPlan::Ultra | GeminiPlan::Enterprise => Some(2000),
        GeminiPlan::ApiFree => Some(250),
        GeminiPlan::ApiPaid => None,
    }
}

/// The plan your sign-in suggests, from `~/.gemini/settings.json` (`security.auth.selectedType`). A Google
/// account is Personal (it can't tell AI Pro or Ultra apart), an API key the free tier.
fn sign_in_plan(gemini: Option<&Path>) -> GeminiPlan {
    #[derive(Deserialize)]
    struct Settings {
        security: Option<Security>,
        #[serde(rename = "selectedAuthType")]
        old_auth: Option<String>,
    }
    #[derive(Deserialize)]
    struct Security {
        auth: Option<Auth>,
    }
    #[derive(Deserialize)]
    struct Auth {
        #[serde(rename = "selectedType")]
        selected: Option<String>,
    }
    let auth = gemini
        .and_then(|dir| std::fs::read_to_string(dir.join("settings.json")).ok())
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .and_then(|s| {
            s.security
                .and_then(|s| s.auth)
                .and_then(|a| a.selected)
                .or(s.old_auth)
        });
    match auth.as_deref() {
        Some("gemini-api-key") => GeminiPlan::ApiFree,
        Some("vertex-ai") => GeminiPlan::ApiPaid,
        _ => GeminiPlan::Personal,
    }
}

/// Midnight Pacific time (when Gemini's daily quota resets) at or before `utc`, in Unix seconds.
pub fn pacific_day_start(utc: i64) -> i64 {
    let offset = pacific_offset(utc);
    let local = utc + offset;
    let midnight_local = local - local.rem_euclid(86400);
    midnight_local - pacific_offset(midnight_local - offset)
}

/// The next midnight Pacific time after `utc`.
pub fn next_pacific_midnight(utc: i64) -> i64 {
    // 26 hours on, then back to that day's midnight: right on both 23- and 25-hour days.
    pacific_day_start(pacific_day_start(utc) + 26 * 3600)
}

/// Pacific time's offset from UTC: −7h in daylight time (second Sunday of March, 2am, to the first Sunday
/// of November, 2am), −8h otherwise.
fn pacific_offset(utc: i64) -> i64 {
    let year = Utc
        .timestamp_opt(utc, 0)
        .single()
        .map_or(2026, |t| t.year());
    let sunday = |month: u32, nth: u8| {
        NaiveDate::from_weekday_of_month_opt(year, month, Weekday::Sun, nth)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map_or(0, |d| d.and_utc().timestamp())
    };
    let starts = sunday(3, 2) + 10 * 3600; // 2am PST
    let ends = sunday(11, 1) + 9 * 3600; // 2am PDT
    if (starts..ends).contains(&utc) {
        -7 * 3600
    } else {
        -8 * 3600
    }
}

/// Gemini: replies from its chat logs, and today's requests against your plan's daily allowance.
#[derive(Debug, Default)]
pub struct GeminiAdapter {
    files: Follower<String>,
    /// Every reply's id and time from the last two days, to count today's requests.
    requests: HashMap<String, i64>,
    limit: Option<LimitWindow>,
}

impl Adapter for GeminiAdapter {
    fn agent(&self) -> AgentId {
        AgentId::Gemini
    }

    fn refresh(&mut self) -> Vec<UsageEvent> {
        let Some(dir) = gemini_dir().filter(|d| d.is_dir()) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        if let Ok(paths) = find_chats(&dir) {
            self.files.poll(paths, LOOKBACK_SECS, |session, line| {
                events.extend(parse_line(line, session));
            });
        }
        for event in &events {
            self.requests.insert(event.id.clone(), event.ts);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        self.requests
            .retain(|_, ts| *ts > now - LOOKBACK_SECS as i64);
        let plan = settings::settings_file()
            .map(|path| settings::load(&path).gemini_plan)
            .unwrap_or(GeminiPlan::Auto);
        // Nothing to show until you've used Gemini CLI lately (it may not even be installed).
        self.limit = daily_requests(plan, Some(&dir))
            .filter(|_| !self.requests.is_empty())
            .map(|allowed| {
                let today = pacific_day_start(now);
                let used = self.requests.values().filter(|ts| **ts >= today).count();
                LimitWindow {
                    used_pct: used as f64 / allowed as f64 * 100.0,
                    window_minutes: 24 * 60,
                    resets_at: Some(next_pacific_midnight(now)),
                    observed_at: self.requests.values().copied().max().unwrap_or(now),
                    // Counted from the logs, not reported by Google.
                    approximate: true,
                }
            });
        events
    }

    fn limits(&self) -> Vec<LimitWindow> {
        self.limit.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_reply_and_its_session() {
        let mut session = String::new();
        assert_eq!(
            parse_line(
                r#"{"sessionId":"s1","projectHash":"h","startTime":"2026-04-30T01:58:56Z","kind":"main"}"#,
                &mut session
            ),
            None
        );
        assert_eq!(session, "s1");
        let reply = r#"{"id":"r1","timestamp":"2026-04-30T01:59:44.891Z","type":"gemini","content":"secret","tokens":{"input":11538,"output":174,"cached":1000,"thoughts":265,"tool":0,"total":11977},"model":"gemini-3-flash-preview"}"#;
        let event = parse_line(reply, &mut session).unwrap();
        assert_eq!(event.id, "r1");
        assert_eq!(event.session_id, "s1");
        assert_eq!(event.ts, 1777514384);
        assert_eq!(
            event.tokens,
            TokenUsage {
                input: 10538,
                output: 439,
                cache_read: 1000,
                cache_write: 0,
                reasoning: 265,
            }
        );
    }

    #[test]
    fn skips_your_messages_and_empty_copies() {
        let mut session = String::new();
        let you = r#"{"id":"u1","timestamp":"2026-04-30T01:59:40Z","type":"user","content":"secret prompt"}"#;
        let empty =
            r#"{"id":"r1","timestamp":"2026-04-30T01:59:41Z","type":"gemini","content":""}"#;
        assert_eq!(parse_line(you, &mut session), None);
        assert_eq!(parse_line(empty, &mut session), None);
        assert_eq!(
            parse_line(r#"{"$set":{"lastUpdated":"x"}}"#, &mut session),
            None
        );
    }

    #[test]
    fn plans() {
        assert_eq!(daily_requests(GeminiPlan::Personal, None), Some(1000));
        assert_eq!(daily_requests(GeminiPlan::Ultra, None), Some(2000));
        assert_eq!(daily_requests(GeminiPlan::ApiFree, None), Some(250));
        assert_eq!(daily_requests(GeminiPlan::ApiPaid, None), None);
        // Auto with no settings file: a Google account.
        assert_eq!(daily_requests(GeminiPlan::Auto, None), Some(1000));
    }

    #[test]
    fn resets_at_midnight_pacific() {
        // 2026-10-22 12:00 UTC is 05:00 PDT, so the day started at 07:00 UTC.
        assert_eq!(pacific_day_start(1792670400), 1792652400);
        assert_eq!(next_pacific_midnight(1792670400), 1792652400 + 86400);
        // 2026-01-15 12:00 UTC is 04:00 PST, so the day started at 08:00 UTC.
        assert_eq!(pacific_day_start(1768478400), 1768464000);
        // The day clocks go back (2026-11-01) is 25 hours long.
        let start = pacific_day_start(1793541600); // 2026-11-01 14:00 UTC
        assert_eq!(start, 1793516400);
        assert_eq!(next_pacific_midnight(1793541600) - start, 25 * 3600);
    }
}
