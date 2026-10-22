//! Snapshot tests on real (cleaned) logs, through the same `Adapter` → `Tracker` path the app uses.
//! The fixtures come from `scripts/sanitize-claude-log.mjs` and `scripts/sanitize-codex-log.mjs`: only ids,
//! models, effort, token counts and limit readings, with times moved to start at 2026-01-01 00:00 UTC.
//!
//! Each test writes what it got to `tests/snapshots/`. When the output changes, the test fails and shows the
//! difference; if the change is right, accept it with `cargo insta review` (or run with INSTA_UPDATE=always).

use std::fmt::Write;

use statuspal_core::activity::{self, GaitThresholds};
use statuspal_core::adapters::Adapter;
use statuspal_core::adapters::claude::transcripts;
use statuspal_core::adapters::codex::SessionParser;
use statuspal_core::model::{AgentId, LimitWindow, UsageEvent};
use statuspal_core::tracker::Tracker;

/// 2026-01-01 00:00 UTC, where every fixture starts.
const START: i64 = 1767225600;

/// An adapter that hands out a fixture's events once, like a real one reading a log for the first time.
#[derive(Debug)]
struct Fixture {
    agent: AgentId,
    events: Vec<UsageEvent>,
    limits: Vec<LimitWindow>,
}

impl Adapter for Fixture {
    fn agent(&self) -> AgentId {
        self.agent
    }
    fn refresh(&mut self) -> Vec<UsageEvent> {
        std::mem::take(&mut self.events)
    }
    fn limits(&self) -> Vec<LimitWindow> {
        self.limits.clone()
    }
}

/// One line per reply: minutes:seconds in, model, effort, and the token counts.
fn table(events: &[UsageEvent]) -> String {
    let mut out = String::new();
    for e in events {
        let t = &e.tokens;
        writeln!(
            out,
            "{:>2}:{:02} {} {:<16} {:<6} in {:>6} out {:>5} cache read {:>7} write {:>6} = {:>6} effective",
            (e.ts - START) / 60,
            (e.ts - START) % 60,
            e.id,
            e.model.as_deref().unwrap_or("?"),
            e.effort.as_deref().unwrap_or("-"),
            t.input,
            t.output,
            t.cache_read,
            t.cache_write,
            t.effective(),
        )
        .unwrap();
    }
    out
}

#[test]
fn claude_busy_hour() {
    let events: Vec<_> = include_str!("fixtures/claude-busy-hour.jsonl")
        .lines()
        .filter_map(transcripts::parse_line)
        .collect();
    let mut tracker = Tracker::with(vec![Box::new(Fixture {
        agent: AgentId::Claude,
        events,
        limits: Vec::new(),
    })]);
    tracker.refresh(START + 3600);

    let mut out = table(tracker.events());
    for minutes in [5, 10, 20, 30, 45, 59, 70] {
        let a = activity::activity(
            tracker.events(),
            START + minutes * 60,
            GaitThresholds::default(),
        );
        writeln!(
            out,
            "at {minutes:>2}m: {:?}, {} tokens in 10 min",
            a.gait, a.recent_tokens
        )
        .unwrap();
    }
    let snapshot = &tracker.snapshots(START + 3600, GaitThresholds::default(), |_| true)[0];
    writeln!(
        out,
        "tip at the end: {:?}",
        snapshot.tip.as_ref().map(|t| &t.text)
    )
    .unwrap();
    insta::assert_snapshot!(out);
}

#[test]
fn codex_session() {
    let mut parser = SessionParser::default();
    let events: Vec<_> = include_str!("fixtures/codex-session.jsonl")
        .lines()
        .filter_map(|line| parser.feed(line))
        .collect();
    let mut out = table(&events);
    writeln!(out, "limit: {:?}", parser.latest_limit()).unwrap();
    insta::assert_snapshot!(out);
}
