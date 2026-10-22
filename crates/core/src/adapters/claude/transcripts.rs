//! Claude Code's chat logs: `~/.claude/projects/<project>/<session>.jsonl`, plus `<session>/subagents/*.jsonl`
//! for helper agents (they use your limit too). Every reply Claude sends is logged with the tokens it used:
//!
//! ```json
//! {"type": "assistant", "timestamp": "2026-10-22T06:57:10.387Z", "sessionId": "ce65…", "requestId": "req_…",
//!  "effort": "high", "message": {"id": "msg_…", "model": "claude-opus-5-5",
//!  "usage": {"input_tokens": 2, "output_tokens": 228, "cache_read_input_tokens": 22153,
//!            "cache_creation_input_tokens": 12001, "output_tokens_details": {"thinking_tokens": 23}}}}
//! ```
//!
//! A reply made of several parts (text, then a tool call…) is logged once per part, each line with the same
//! `message.id` and almost always the same usage. When they differ, the last line has the final counts. We
//! hand out every line and the Tracker keeps one event per id.
//!
//! Only these fields are read. What you and Claude wrote isn't.

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::{AgentId, TokenUsage, UsageEvent};
use crate::tail::Follower;

/// Files older than this are skipped the first time we look. Claude's longest limit window is a week.
const LOOKBACK_SECS: u64 = 8 * 24 * 3600;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogLine {
    #[serde(rename = "type")]
    kind: String,
    timestamp: Option<String>,
    session_id: Option<String>,
    effort: Option<String>,
    message: Option<Message>,
}

#[derive(Debug, Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawUsage {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_input_tokens: u64,
    cache_creation_input_tokens: u64,
    output_tokens_details: OutputDetails,
    /// `"standard"`, or something else in fast mode.
    speed: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OutputDetails {
    thinking_tokens: u64,
}

/// The reply on one log line, if it's a reply with usage. Other lines (your messages, tool results,
/// snapshots) and broken lines give `None`.
pub fn parse_line(line: &str) -> Option<UsageEvent> {
    // Cheap check first: most lines aren't replies, and parsing JSON is the slow part.
    if !line.contains("\"assistant\"") {
        return None;
    }
    let line: LogLine = serde_json::from_str(line).ok()?;
    if line.kind != "assistant" {
        return None;
    }
    let message = line.message?;
    let usage = message.usage?;
    let fast = usage
        .speed
        .as_deref()
        .is_some_and(|speed| speed != "standard");
    let tokens = TokenUsage {
        input: usage.input_tokens,
        output: usage.output_tokens,
        cache_read: usage.cache_read_input_tokens,
        cache_write: usage.cache_creation_input_tokens,
        reasoning: usage.output_tokens_details.thinking_tokens,
    };
    // Claude Code logs some messages it made up itself (model "<synthetic>", all zeros).
    if tokens == TokenUsage::default() {
        return None;
    }
    let ts = chrono::DateTime::parse_from_rfc3339(line.timestamp.as_deref()?)
        .ok()?
        .timestamp();
    Some(UsageEvent {
        agent: AgentId::Claude,
        id: message.id?,
        ts,
        session_id: line.session_id.unwrap_or_default(),
        model: message.model,
        effort: line.effort,
        fast,
        tokens,
    })
}

/// `~/.claude/projects`.
pub fn projects_dir() -> Option<PathBuf> {
    super::config_dir().map(|dir| dir.join("projects"))
}

/// True for Claude Code's chat logs (any `.jsonl` file under `projects`).
pub fn is_transcript(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "jsonl")
        && projects_dir().is_some_and(|dir| path.starts_with(dir))
}

/// Every `.jsonl` file under `dir`, at any depth (sessions and their subagents).
pub fn find_transcripts(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(find_transcripts(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "jsonl") {
            files.push(path);
        }
    }
    Ok(files)
}

/// Follows every Claude Code chat log, reading only what was added since last time (see `tail.rs`).
#[derive(Debug, Default)]
pub struct ClaudeReader {
    files: Follower<()>,
    turn: Turn,
}

impl ClaudeReader {
    /// The replies logged since the last call. The first call reads the last week.
    pub fn poll(&mut self, dir: &Path) -> Vec<UsageEvent> {
        let Ok(paths) = find_transcripts(dir) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        let turn = &mut self.turn;
        self.files.poll(paths, LOOKBACK_SECS, |_, line| {
            events.extend(parse_line(line));
            turn.feed(line);
        });
        events
    }

    /// Since when Claude has been waiting for you: it finished its reply and you haven't answered yet.
    pub fn waiting_since(&self) -> Option<i64> {
        self.turn.waiting_since
    }
}

/// Whose turn it is, from the newest line in any chat. Claude ends a finished reply with
/// `"stop_reason": "end_turn"` (`"tool_use"` while it's still working), and your answer is a `"user"` line.
/// Helper agents' lines (`"isSidechain": true`) don't count: they're Claude talking to itself.
#[derive(Debug, Default)]
struct Turn {
    newest: i64,
    waiting_since: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnLine {
    #[serde(rename = "type")]
    kind: String,
    timestamp: Option<String>,
    #[serde(default)]
    is_sidechain: bool,
    message: Option<TurnMessage>,
}

#[derive(Debug, Deserialize)]
struct TurnMessage {
    stop_reason: Option<String>,
}

impl Turn {
    fn feed(&mut self, line: &str) {
        if !line.contains("\"assistant\"") && !line.contains("\"user\"") {
            return;
        }
        let Ok(line) = serde_json::from_str::<TurnLine>(line) else {
            return;
        };
        let Some(ts) = line
            .timestamp
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.timestamp())
        else {
            return;
        };
        if line.is_sidechain || ts < self.newest {
            return;
        }
        let stop = line.message.and_then(|m| m.stop_reason);
        match (line.kind.as_str(), stop.as_deref()) {
            ("assistant", Some("end_turn")) => self.waiting_since = Some(ts),
            ("assistant", Some(_)) | ("user", _) => self.waiting_since = None,
            _ => return, // a line that doesn't say whose turn it is
        }
        self.newest = ts;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &str = r#"{"parentUuid":"p","isSidechain":false,"message":{"model":"claude-opus-5-5","id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"secret reply"}],"usage":{"input_tokens":2,"cache_creation_input_tokens":12001,"cache_read_input_tokens":22153,"output_tokens":228,"output_tokens_details":{"thinking_tokens":23},"service_tier":"standard","speed":"standard"}},"requestId":"req_1","type":"assistant","timestamp":"2026-10-22T06:57:10.387Z","effort":"high","sessionId":"s1"}"#;

    #[test]
    fn reads_a_reply() {
        assert_eq!(
            parse_line(REPLY),
            Some(UsageEvent {
                agent: AgentId::Claude,
                id: "msg_1".into(),
                ts: 1792652230,
                session_id: "s1".into(),
                model: Some("claude-opus-5-5".into()),
                effort: Some("high".into()),
                fast: false,
                tokens: TokenUsage {
                    input: 2,
                    output: 228,
                    cache_read: 22153,
                    cache_write: 12001,
                    reasoning: 23,
                },
            })
        );
    }

    #[test]
    fn skips_lines_that_arent_replies() {
        let you = r#"{"type":"user","message":{"role":"user","content":"secret prompt"},"timestamp":"2026-10-22T06:57:00Z"}"#;
        let synthetic = r#"{"type":"assistant","timestamp":"2026-10-22T06:57:00Z","message":{"id":"msg_s","model":"<synthetic>","usage":{"input_tokens":0,"output_tokens":0}}}"#;
        let no_usage =
            r#"{"type":"assistant","timestamp":"2026-10-22T06:57:00Z","message":{"id":"msg_n"}}"#;
        assert_eq!(parse_line(you), None);
        assert_eq!(parse_line(synthetic), None);
        assert_eq!(parse_line(no_usage), None);
        assert_eq!(parse_line(r#"{"type":"assistant","timest"#), None);
    }

    #[test]
    fn waiting_for_you_after_a_finished_reply() {
        let line = |kind: &str, ts: &str, stop: &str| {
            format!(
                r#"{{"type":"{kind}","timestamp":"2026-10-22T07:{ts}Z","message":{{"stop_reason":{stop}}}}}"#
            )
        };
        let mut turn = Turn::default();
        turn.feed(&line("assistant", "00:00", r#""tool_use""#));
        assert_eq!(turn.waiting_since, None, "still working");
        turn.feed(&line("assistant", "01:00", r#""end_turn""#));
        assert_eq!(turn.waiting_since, Some(1792652460));
        // A helper agent finishing isn't Claude waiting for you.
        turn.feed(r#"{"type":"assistant","timestamp":"2026-10-22T07:02:00Z","isSidechain":true,"message":{"stop_reason":"tool_use"}}"#);
        assert_eq!(turn.waiting_since, Some(1792652460));
        turn.feed(&line("user", "03:00", "null"));
        assert_eq!(turn.waiting_since, None, "you answered");
        // An older line from another chat doesn't change it.
        turn.feed(&line("assistant", "00:30", r#""end_turn""#));
        assert_eq!(turn.waiting_since, None);
    }

    #[test]
    fn reader_follows_sessions_and_subagents() {
        let root = std::env::temp_dir().join(format!(
            "statuspal-test-claude-reader-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let subagents = root.join("-Users-me-project").join("s1").join("subagents");
        std::fs::create_dir_all(&subagents).unwrap();
        std::fs::write(
            root.join("-Users-me-project").join("s1.jsonl"),
            format!("{REPLY}\n"),
        )
        .unwrap();
        let helper = REPLY.replace("msg_1", "msg_2");
        std::fs::write(subagents.join("agent-a1.jsonl"), format!("{helper}\n")).unwrap();

        let mut reader = ClaudeReader::default();
        let mut ids: Vec<_> = reader.poll(&root).into_iter().map(|e| e.id).collect();
        ids.sort();
        assert_eq!(ids, ["msg_1", "msg_2"]);
        assert!(reader.poll(&root).is_empty(), "nothing new");
    }
}
