use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::adapters::Adapter;
use crate::model::{AgentId, LimitWindow, TokenUsage, UsageEvent};
use crate::tail::Follower;

/// One line of a rollout file.
#[derive(Debug, Deserialize)]
struct LogLine {
    timestamp: String,
    // `type` is a Rust keyword, so the field gets another name and serde maps it back.
    #[serde(rename = "type")]
    kind: String,
    payload: Payload,
}

/// The parts of `payload` we care about. Each line type fills in different fields, so all are optional.
#[derive(Debug, Deserialize)]
struct Payload {
    /// `"token_count"` on usage lines.
    #[serde(rename = "type")]
    kind: Option<String>,
    /// Session id, on the `session_meta` line.
    id: Option<String>,
    /// Model name, on `turn_context` lines.
    model: Option<String>,
    /// Reasoning effort (`"low"` … `"xhigh"`), on `turn_context` lines.
    effort: Option<String>,
    /// Token totals, on `token_count` lines. Sometimes `null`.
    info: Option<TokenInfo>,
    /// Your Codex limit, on `token_count` lines.
    rate_limits: Option<RawRateLimits>,
}

#[derive(Debug, Deserialize)]
struct TokenInfo {
    total_token_usage: RawUsage,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize)]
#[serde(default)]
struct RawUsage {
    input_tokens: u64,
    cached_input_tokens: u64,
    /// Newer Codex versions log cache writes too; older ones leave it out (0).
    cache_write_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
}

impl RawUsage {
    /// Usage since `prev`: this running total minus the previous one.
    /// `saturating_sub` stops at 0 instead of crashing if a number ever goes down.
    fn since(&self, prev: &RawUsage) -> RawUsage {
        RawUsage {
            input_tokens: self.input_tokens.saturating_sub(prev.input_tokens),
            cached_input_tokens: self
                .cached_input_tokens
                .saturating_sub(prev.cached_input_tokens),
            cache_write_input_tokens: self
                .cache_write_input_tokens
                .saturating_sub(prev.cache_write_input_tokens),
            output_tokens: self.output_tokens.saturating_sub(prev.output_tokens),
            reasoning_output_tokens: self
                .reasoning_output_tokens
                .saturating_sub(prev.reasoning_output_tokens),
        }
    }

    fn to_token_usage(self) -> TokenUsage {
        TokenUsage {
            input: self.input_tokens.saturating_sub(self.cached_input_tokens),
            output: self.output_tokens,
            cache_read: self.cached_input_tokens,
            cache_write: self.cache_write_input_tokens,
            reasoning: self.reasoning_output_tokens,
        }
    }
}

/// `payload.rate_limits`. Your plan has one window (`primary`); `secondary` is always null, so we skip it.
#[derive(Debug, Deserialize)]
struct RawRateLimits {
    primary: Option<RawWindow>,
}

/// One limit window, e.g. `{"used_percent": 6.0, "window_minutes": 43200, "resets_at": 1785537790}`.
#[derive(Debug, Clone, Copy, Deserialize)]
struct RawWindow {
    used_percent: f64,
    window_minutes: u32,
    resets_at: Option<i64>,
}

impl RawWindow {
    /// Converts Codex's window into our shared `LimitWindow`.
    fn to_limit_window(self, observed_at: i64) -> LimitWindow {
        LimitWindow {
            used_pct: self.used_percent,
            window_minutes: self.window_minutes,
            resets_at: self.resets_at,
            observed_at,
            approximate: false,
        }
    }
}

/// Parses one line. Returns `None` if it isn't valid JSON or doesn't have the shape we expect.
fn parse_line(line: &str) -> Option<LogLine> {
    serde_json::from_str(line).ok()
}

/// Where Codex keeps its logs: `~/.codex/sessions`. `None` if we can't find your home folder.
pub fn sessions_dir() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join(".codex").join("sessions"))
}

/// True for Codex log files: `rollout-<date>-<id>.jsonl`.
pub(crate) fn is_rollout_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.starts_with("rollout-") && name.ends_with(".jsonl")
}

/// Every rollout file under `dir`, including the year/month/day folders inside it, sorted by path
/// (which is also by date, since the folders are named by date).
pub fn find_session_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(find_session_files(&path)?);
        } else if is_rollout_file(&path) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Turns an ISO timestamp like `2026-07-01T22:43:18.175Z` into Unix seconds.
fn parse_timestamp(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|t| t.timestamp())
}

/// Works through one rollout file a line at a time, remembering what it needs between lines: the session id,
/// the model, and the last running total. Feed it lines as they arrive (`CodexReader` does), or a whole file
/// at once (`parse_session`).
#[derive(Debug, Default)]
pub struct SessionParser {
    /// When the newest line was written, and since when Codex has been waiting for you (it finished its
    /// task and you haven't started another).
    newest: i64,
    waiting_since: Option<i64>,
    session_id: String,
    model: Option<String>,
    effort: Option<String>,
    prev_total: Option<RawUsage>,
    latest_limit: Option<LimitWindow>,
}

impl SessionParser {
    /// Reads one line. Returns the turn it finished, if it was a usage line with new usage.
    ///
    /// Codex writes *running totals*, so each turn is "this total minus the previous one". Lines it can't
    /// use (messages, tool calls, broken lines) are skipped.
    pub fn feed(&mut self, raw_line: &str) -> Option<UsageEvent> {
        let line = parse_line(raw_line)?;
        let ts = parse_timestamp(&line.timestamp);

        // Limit readings ride along on usage lines. Lines are written in time order, so the last one wins.
        if let (Some(window), Some(observed_at)) = (
            line.payload.rate_limits.and_then(|limits| limits.primary),
            ts,
        ) {
            self.latest_limit = Some(window.to_limit_window(observed_at));
        }

        if let Some(ts) = ts {
            self.newest = self.newest.max(ts);
            match line.payload.kind.as_deref() {
                Some("task_complete") => self.waiting_since = Some(ts),
                Some("task_started" | "user_message") => self.waiting_since = None,
                _ => {}
            }
        }

        match line.kind.as_str() {
            "session_meta" => {
                if let Some(id) = line.payload.id {
                    self.session_id = id;
                }
                None
            }
            "turn_context" => {
                if line.payload.model.is_some() {
                    self.model = line.payload.model;
                }
                if line.payload.effort.is_some() {
                    self.effort = line.payload.effort;
                }
                None
            }
            "event_msg" if line.payload.kind.as_deref() == Some("token_count") => {
                let total = line.payload.info?.total_token_usage;

                // Codex sometimes repeats the previous total. That isn't a new turn.
                if self.prev_total == Some(total) {
                    return None;
                }
                let turn = match self.prev_total {
                    Some(prev) => total.since(&prev),
                    None => total, // first turn of the session
                };
                self.prev_total = Some(total);

                Some(UsageEvent {
                    agent: AgentId::Codex,
                    // The running total only ever grows, so it names this turn within the session.
                    id: format!(
                        "{}:{}",
                        self.session_id,
                        total.input_tokens + total.output_tokens
                    ),
                    ts: ts?,
                    session_id: self.session_id.clone(),
                    model: self.model.clone(),
                    effort: self.effort.clone(),
                    fast: false, // Codex has no fast mode in its logs
                    tokens: turn.to_token_usage(),
                })
            }
            _ => None,
        }
    }

    /// Since when Codex has been waiting for you in this session, if it is.
    pub fn waiting_since(&self) -> Option<i64> {
        self.waiting_since
    }

    /// The newest limit reading seen so far.
    pub fn latest_limit(&self) -> Option<LimitWindow> {
        self.latest_limit.clone()
    }
}

/// Reads a whole rollout file and returns one `UsageEvent` per turn.
pub fn parse_session(contents: &str) -> Vec<UsageEvent> {
    let mut parser = SessionParser::default();
    contents
        .lines()
        .filter_map(|line| parser.feed(line))
        .collect()
}

/// The newest rate-limit reading in a rollout file, if it has any.
pub fn latest_limit(contents: &str) -> Option<LimitWindow> {
    let mut parser = SessionParser::default();
    for line in contents.lines() {
        parser.feed(line);
    }
    parser.latest_limit()
}

/// Session files older than this are skipped the first time we look. Your Codex limit window is 30 days, so
/// nothing older still matters.
const LOOKBACK_SECS: u64 = 31 * 24 * 3600;

/// Follows every Codex session file, reading only what was added since last time (see `tail.rs`).
#[derive(Debug, Default)]
pub struct CodexReader {
    files: Follower<SessionParser>,
}

impl CodexReader {
    /// The turns written since the last call. The first call reads every session from the last month.
    /// A file that's read again from the top hands out its turns again; the Tracker keeps one per id.
    pub fn poll(&mut self, dir: &Path) -> Vec<UsageEvent> {
        let Ok(paths) = find_session_files(dir) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        self.files.poll(paths, LOOKBACK_SECS, |parser, line| {
            events.extend(parser.feed(line));
        });
        events
    }

    /// Since when Codex has been waiting for you, from the session written to last.
    pub fn waiting_since(&self) -> Option<i64> {
        self.files
            .states()
            .max_by_key(|parser| parser.newest)
            .and_then(SessionParser::waiting_since)
    }

    /// The newest limit reading across every file.
    pub fn latest_limit(&self) -> Option<LimitWindow> {
        self.files
            .states()
            .filter_map(|parser| parser.latest_limit())
            .max_by_key(|limit| limit.observed_at)
    }
}

/// Codex: turns and the limit, both from its session logs.
#[derive(Debug, Default)]
pub struct CodexAdapter {
    reader: CodexReader,
}

impl Adapter for CodexAdapter {
    fn agent(&self) -> AgentId {
        AgentId::Codex
    }

    fn refresh(&mut self) -> Vec<UsageEvent> {
        match sessions_dir() {
            Some(dir) if dir.is_dir() => self.reader.poll(&dir),
            _ => Vec::new(),
        }
    }

    fn limits(&self) -> Vec<LimitWindow> {
        self.reader.latest_limit().into_iter().collect()
    }

    fn waiting_since(&self) -> Option<i64> {
        self.reader.waiting_since()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_count_line() {
        let line = r#"{"timestamp":"2026-07-01T22:43:18.175Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":30594,"cached_input_tokens":24064,"output_tokens":307,"reasoning_output_tokens":57,"total_tokens":30901},"last_token_usage":{"input_tokens":15441,"cached_input_tokens":14080,"output_tokens":131,"reasoning_output_tokens":21,"total_tokens":15572},"model_context_window":353400},"rate_limits":{"primary":{"used_percent":6.0,"window_minutes":43200,"resets_at":1785537790}}}}"#;

        let parsed = parse_line(line).expect("line should parse");
        assert_eq!(parsed.kind, "event_msg");
        assert_eq!(parsed.payload.kind.as_deref(), Some("token_count"));

        let total = parsed
            .payload
            .info
            .expect("info should be present")
            .total_token_usage;
        assert_eq!(total.input_tokens, 30594);
        assert_eq!(total.cached_input_tokens, 24064);
        assert_eq!(total.output_tokens, 307);
        assert_eq!(total.reasoning_output_tokens, 57);
    }

    #[test]
    fn null_info_parses_as_none() {
        let line = r#"{"timestamp":"2026-07-01T22:43:10.000Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#;
        let parsed = parse_line(line).expect("line should parse");
        assert!(parsed.payload.info.is_none());
    }

    #[test]
    fn message_text_is_ignored() {
        // A chat message line: its text has no field in our structs, so it's skipped.
        let line = r#"{"timestamp":"2026-07-01T22:43:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"secret prompt"}]}}"#;
        let parsed = parse_line(line).expect("line should parse");
        assert_eq!(parsed.kind, "response_item");
        assert!(parsed.payload.info.is_none());
    }

    #[test]
    fn garbage_returns_none() {
        assert!(parse_line("not json").is_none());
        // A half-written last line, which happens while Codex is still writing.
        assert!(parse_line(r#"{"timestamp":"2026-07-01T22:4"#).is_none());
    }

    #[test]
    fn to_token_usage_splits_out_cached_input() {
        let raw = RawUsage {
            input_tokens: 30594,
            cached_input_tokens: 24064,
            cache_write_input_tokens: 0,
            output_tokens: 307,
            reasoning_output_tokens: 57,
        };
        let usage = raw.to_token_usage();
        assert_eq!(usage.input, 6530); // 30594 - 24064
        assert_eq!(usage.cache_read, 24064);
        assert_eq!(usage.output, 307);
        assert_eq!(usage.reasoning, 57);
        assert_eq!(usage.cache_write, 0); // not in this (older) line
    }

    #[test]
    fn parse_session_makes_one_event_per_turn() {
        // A tiny fake session with every trap we found in the real logs:
        // a null `info`, a repeated total, a chat message, and a half-written last line.
        let contents = r#"{"timestamp":"2026-07-01T22:42:12.764Z","type":"session_meta","payload":{"id":"test-session","cwd":"/somewhere"}}
{"timestamp":"2026-07-01T22:42:13.000Z","type":"turn_context","payload":{"model":"gpt-5.5","effort":"xhigh"}}
{"timestamp":"2026-07-01T22:43:10.000Z","type":"event_msg","payload":{"type":"token_count","info":null}}
{"timestamp":"2026-07-01T22:43:13.922Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":600,"output_tokens":50,"reasoning_output_tokens":10}}}}
{"timestamp":"2026-07-01T22:43:14.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":600,"output_tokens":50,"reasoning_output_tokens":10}}}}
{"timestamp":"2026-07-01T22:44:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"secret prompt"}]}}
{"timestamp":"2026-07-01T22:45:02.500Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":2500,"cached_input_tokens":1800,"output_tokens":120,"reasoning_output_tokens":30}}}}
{"timestamp":"2026-07-01T22:46:00.000Z","type":"event_msg","payload":{"type":"token_c"#;

        let events = parse_session(contents);
        assert_eq!(
            events.len(),
            2,
            "null info, the repeat and the broken line must not become turns"
        );

        assert_eq!(
            events[0],
            UsageEvent {
                agent: AgentId::Codex,
                id: "test-session:1050".to_string(),
                ts: 1782945793,
                session_id: "test-session".to_string(),
                model: Some("gpt-5.5".to_string()),
                effort: Some("xhigh".to_string()),
                fast: false,
                tokens: TokenUsage {
                    input: 400, // 1000 - 600 cached
                    output: 50,
                    cache_read: 600,
                    cache_write: 0,
                    reasoning: 10,
                },
            }
        );

        // Second turn = second total minus the first.
        assert_eq!(events[1].ts, 1782945902);
        assert_eq!(
            events[1].tokens,
            TokenUsage {
                input: 300, // (2500 - 1000) - (1800 - 600)
                output: 70,
                cache_read: 1200,
                cache_write: 0,
                reasoning: 20,
            }
        );
    }

    #[test]
    fn waiting_for_you_after_a_task() {
        let mut parser = SessionParser::default();
        parser.feed(r#"{"timestamp":"2026-07-01T22:42:00Z","type":"event_msg","payload":{"type":"task_started"}}"#);
        assert_eq!(parser.waiting_since(), None);
        parser.feed(r#"{"timestamp":"2026-07-01T22:43:00Z","type":"event_msg","payload":{"type":"task_complete"}}"#);
        assert_eq!(parser.waiting_since(), Some(1782945780));
        parser.feed(r#"{"timestamp":"2026-07-01T22:50:00Z","type":"event_msg","payload":{"type":"user_message"}}"#);
        assert_eq!(parser.waiting_since(), None);
    }

    #[test]
    fn to_limit_window_copies_the_fields() {
        let raw = RawWindow {
            used_percent: 6.0,
            window_minutes: 43200,
            resets_at: Some(1785537790),
        };
        assert_eq!(
            raw.to_limit_window(1782945793),
            LimitWindow {
                used_pct: 6.0,
                window_minutes: 43200,
                resets_at: Some(1785537790),
                observed_at: 1782945793,
                approximate: false,
            }
        );
    }

    #[test]
    fn latest_limit_takes_the_last_reading() {
        let contents = r#"{"timestamp":"2026-07-01T22:43:13.922Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":5.0,"window_minutes":43200,"resets_at":1785537788},"secondary":null}}}
{"timestamp":"2026-07-01T22:44:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[]}}
{"timestamp":"2026-07-01T22:45:02.500Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":6.0,"window_minutes":43200,"resets_at":1785537790},"secondary":null}}}
{"timestamp":"2026-07-01T22:46:00.000Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":null}}"#;

        let limit = latest_limit(contents).expect("there are readings");
        assert_eq!(limit.used_pct, 6.0);
        assert_eq!(limit.resets_at, Some(1785537790));
        assert_eq!(limit.observed_at, 1782945902); // the 22:45:02 line, not the later line without limits
    }

    #[test]
    fn latest_limit_is_none_without_readings() {
        let contents = r#"{"timestamp":"2026-07-01T22:42:12.764Z","type":"session_meta","payload":{"id":"test-session"}}"#;
        assert!(latest_limit(contents).is_none());
    }

    #[test]
    fn reader_hands_out_each_turn_once_as_the_file_grows() {
        let root = std::env::temp_dir().join(format!(
            "statuspal-test-codex-reader-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let day = root.join("2026").join("10").join("23");
        std::fs::create_dir_all(&day).unwrap();
        let file = day.join("rollout-2026-10-23T09-00-00-abc.jsonl");
        let turn = |ts: &str, input: u64| {
            format!(
                r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0}}}},"rate_limits":{{"primary":{{"used_percent":7.0,"window_minutes":43200,"resets_at":1785537790}}}}}}}}"#
            )
        };
        std::fs::write(
            &file,
            format!(
                "{}\n{}",
                turn("2026-10-23T09:01:00Z", 100),
                &turn("2026-10-23T09:02:00Z", 300)[..40]
            ),
        )
        .unwrap();

        let mut reader = CodexReader::default();
        let first = reader.poll(&root);
        assert_eq!(first.len(), 1, "the half-written second turn waits");
        assert_eq!(first[0].tokens.input, 100);
        assert_eq!(reader.latest_limit().unwrap().used_pct, 7.0);
        assert!(reader.poll(&root).is_empty(), "nothing new");

        // Codex finishes the line, then writes another turn.
        let rest = format!(
            "{}\n{}\n",
            &turn("2026-10-23T09:02:00Z", 300)[40..],
            turn("2026-10-23T09:03:00Z", 700)
        );
        let mut handle = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        std::io::Write::write_all(&mut handle, rest.as_bytes()).unwrap();
        let second = reader.poll(&root);
        assert_eq!(
            second.iter().map(|e| e.tokens.input).collect::<Vec<_>>(),
            [200, 400]
        );
    }

    #[test]
    fn finds_rollout_files_in_nested_folders() {
        // Build a fake ~/.codex/sessions in a temporary folder.
        let root =
            std::env::temp_dir().join(format!("statuspal-test-codex-{}", std::process::id()));
        let day = root.join("2026").join("07").join("01");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(day.join("rollout-2026-07-01T18-42-12-abc.jsonl"), "").unwrap();
        std::fs::write(day.join("notes.txt"), "").unwrap(); // not a rollout file
        std::fs::write(root.join("rollout-2026-05-17T21-04-24-def.jsonl"), "").unwrap();

        let files = find_session_files(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        let names: Vec<_> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "rollout-2026-07-01T18-42-12-abc.jsonl", // sorted by path: "2026/..." comes before "rollout-..."
                "rollout-2026-05-17T21-04-24-def.jsonl",
            ]
        );
    }

    #[test]
    fn missing_folder_is_an_error() {
        let result = find_session_files(Path::new("/this/folder/does/not/exist"));
        assert!(result.is_err());
    }
}
