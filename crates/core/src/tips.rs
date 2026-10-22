//! Tips: short, useful hints for the panel, worked out from the numbers we already have. No AI, nothing
//! leaves your Mac, and every tip says what it's based on.
//!
//! Each rule is one small function that looks at one agent and returns a tip or nothing. `tips` runs them
//! all and ranks the results: warnings about your limits always come first, then the most useful hint.
//! The app shows the first one you haven't hidden.

use serde::Serialize;

use crate::classify::PromptClass;
use crate::model::{AgentId, LimitWindow, UsageEvent, UsageSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Good to know.
    Info,
    /// Would save you limit.
    Nudge,
    /// You're about to run out.
    Warning,
}

/// What the tip's button does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TipAction {
    /// Copy a command to paste into the agent, like `/compact`.
    Copy { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Tip {
    /// Which rule made it, e.g. `"huge-context"`. Hiding a tip hides its rule.
    pub rule: String,
    pub agent: AgentId,
    pub severity: Severity,
    /// One short sentence for the panel.
    pub text: String,
    /// What it's based on, in a few words.
    pub why: String,
    pub action: Option<TipAction>,
    /// How much it would help, 0 to 1. Only used to rank tips of the same severity.
    pub impact: f64,
}

/// A reply this short (output tokens) counts as a quick one.
pub const QUICK_REPLY: u64 = 500;
/// How many quick replies in a row the "short replies" and "quick turns" rules look for.
pub const QUICK_STREAK: usize = 10;

/// Every tip that applies right now, best first. `events` is every agent's replies from the last day,
/// oldest first (the Tracker's).
pub fn tips(snapshots: &[UsageSnapshot], events: &[UsageEvent], now: i64) -> Vec<Tip> {
    let mut found = Vec::new();
    for snapshot in snapshots {
        let mine: Vec<&UsageEvent> = events
            .iter()
            .filter(|e| e.agent == snapshot.agent)
            .collect();
        let session = current_session(&mine);
        let agent = snapshot.agent;
        found.extend(short_window_pace(snapshot, now));
        found.extend(ahead_of_pace(snapshot, now));
        found.extend(use_it_or_lose_it(snapshot, now));
        found.extend(huge_context(agent, &mine, &session, now));
        found.extend(cold_cache(agent, &session, now));
        found.extend(short_replies_on_top_model(agent, &session));
        found.extend(quick_turns_on_high_effort(agent, &session));
        found.extend(fast_mode(agent, &session, now));
        found.extend(prompt_size(snapshot, &session));
    }
    found.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(b.impact.total_cmp(&a.impact))
    });
    found
}

/// The replies from the session the newest reply belongs to, oldest first.
fn current_session<'a>(events: &[&'a UsageEvent]) -> Vec<&'a UsageEvent> {
    let Some(latest) = events.last() else {
        return Vec::new();
    };
    events
        .iter()
        .copied()
        .filter(|e| e.session_id == latest.session_id)
        .collect()
}

/// How much context a reply re-sent: everything that went in, fresh or from the cache.
fn context(event: &UsageEvent) -> u64 {
    event.tokens.input + event.tokens.cache_read + event.tokens.cache_write
}

fn tip(
    rule: &str,
    agent: AgentId,
    severity: Severity,
    impact: f64,
    text: String,
    why: String,
) -> Tip {
    Tip {
        rule: rule.into(),
        agent,
        severity,
        text,
        why,
        action: None,
        impact,
    }
}

fn copy(mut tip: Tip, command: &str) -> Tip {
    tip.action = Some(TipAction::Copy {
        text: command.into(),
    });
    tip
}

// ---------- rules about your limits (they work from the % alone) ----------

/// At the pace of the last hour, a short window (5 hours, or Gemini's day) runs out before it resets.
fn short_window_pace(snapshot: &UsageSnapshot, now: i64) -> Option<Tip> {
    snapshot
        .windows
        .iter()
        .filter(|w| w.window_minutes <= 24 * 60)
        .find_map(|window| {
            let pace = snapshot
                .pace
                .iter()
                .find(|p| p.window_minutes == window.window_minutes)?;
            let runs_out = pace.runs_out_at?;
            let resets = window.resets_at?;
            Some(tip(
                "pace-5h",
                snapshot.agent,
                Severity::Warning,
                1.0,
                format!(
                    "At this pace you hit your {} limit in {} (it resets in {}).",
                    window_name(window),
                    duration(runs_out - now),
                    duration(resets - now)
                ),
                format!("Using about {:.0}% an hour lately", pace.pct_per_hour),
            ))
        })
}

/// A long window (a week, or Codex's month) is filling much faster than an even pace would.
fn ahead_of_pace(snapshot: &UsageSnapshot, now: i64) -> Option<Tip> {
    snapshot
        .windows
        .iter()
        .filter(|w| w.window_minutes >= 7 * 24 * 60)
        .find_map(|window| {
            let resets = window.resets_at.filter(|r| *r > now)?;
            let length = window.window_minutes as f64 * 60.0;
            let elapsed = (1.0 - (resets - now) as f64 / length).clamp(0.0, 1.0);
            let even = elapsed * 100.0;
            (window.used_pct >= 25.0 && window.used_pct - even >= 15.0).then(|| {
                tip(
                    "ahead-of-pace",
                    snapshot.agent,
                    Severity::Nudge,
                    0.6,
                    format!(
                        "{:.0}% of your {} limit is used with {} left.",
                        window.used_pct,
                        window_name(window),
                        duration(resets - now)
                    ),
                    format!("An even pace would be {even:.0}% by now"),
                )
            })
        })
}

/// The 5-hour window resets soon and most of it is still unused.
fn use_it_or_lose_it(snapshot: &UsageSnapshot, now: i64) -> Option<Tip> {
    let window = snapshot.windows.iter().find(|w| w.window_minutes == 300)?;
    let left = window.resets_at? - now;
    (left > 0 && left < 45 * 60 && window.used_pct < 50.0).then(|| {
        tip(
            "use-it-or-lose-it",
            snapshot.agent,
            Severity::Info,
            0.3,
            format!(
                "{:.0}% of your 5h limit is left and it resets in {}. Good time for a big task.",
                100.0 - window.used_pct,
                duration(left)
            ),
            "Unused limit doesn't carry over".into(),
        )
    })
}

// ---------- rules about how you use it (they need the agent's logs) ----------

/// Every reply re-sends a very big context: over 200k, or twice what's usual for you today.
fn huge_context(
    agent: AgentId,
    today: &[&UsageEvent],
    session: &[&UsageEvent],
    now: i64,
) -> Option<Tip> {
    let latest = session.last().filter(|e| now - e.ts < 30 * 60)?;
    let mut sizes: Vec<u64> = today.iter().map(|e| context(e)).collect();
    sizes.sort_unstable();
    let usual = sizes[sizes.len() / 2];
    let size = context(latest);
    (size > 200_000.max(usual * 2) || (size > 200_000 && sizes.len() < 10)).then(|| {
        copy(
            tip(
                "huge-context",
                agent,
                Severity::Nudge,
                0.7,
                format!(
                    "Each reply re-sends {} tokens. /compact, or /clear if you've moved on.",
                    tokens(size)
                ),
                format!("Your usual today is {}", tokens(usual)),
            ),
            "/compact",
        )
    })
}

/// You were away long enough for the cache to expire, in a big session, so the next message pays for the
/// whole context again.
fn cold_cache(agent: AgentId, session: &[&UsageEvent], now: i64) -> Option<Tip> {
    let latest = session.last()?;
    let away = now - latest.ts;
    let size = context(latest);
    (agent == AgentId::Claude && (30 * 60..4 * 3600).contains(&away) && size > 150_000).then(|| {
        copy(
            tip(
                "cold-cache",
                agent,
                Severity::Info,
                0.5,
                format!(
                    "You were away {}, so your next message re-sends ~{} tokens. Starting something new? /clear first.",
                    duration(away),
                    tokens(size)
                ),
                "Claude's cache expires after a few idle minutes".into(),
            ),
            "/clear",
        )
    })
}

/// The last replies were all short, on the biggest model.
fn short_replies_on_top_model(agent: AgentId, session: &[&UsageEvent]) -> Option<Tip> {
    if agent != AgentId::Claude {
        return None;
    }
    let streak = quick_streak(session)?;
    let top_model = streak.iter().all(|e| {
        e.model
            .as_deref()
            .is_some_and(|m| m.contains("opus") || m.contains("fable"))
    });
    top_model.then(|| {
        copy(
            tip(
                "short-replies-top-model",
                agent,
                Severity::Nudge,
                0.5,
                "Your last replies were all short. Sonnet would stretch your limit: switch when you start the next task.".into(),
                format!("{QUICK_STREAK} replies under {QUICK_REPLY} tokens on Opus"),
            ),
            "/model sonnet",
        )
    })
}

/// The last replies were all quick, at a high reasoning effort.
fn quick_turns_on_high_effort(agent: AgentId, session: &[&UsageEvent]) -> Option<Tip> {
    let streak = quick_streak(session)?;
    let high = streak
        .iter()
        .all(|e| matches!(e.effort.as_deref(), Some("high" | "xhigh" | "max")));
    if !high {
        return None;
    }
    let effort = streak.last()?.effort.clone().unwrap_or_default();
    let (text, command) = match agent {
        AgentId::Claude => (
            "Quick replies on high effort. /effort medium keeps your context and saves limit.",
            "/effort medium",
        ),
        AgentId::Codex => (
            "Quick turns on high effort burn your Codex limit. Try medium: /model, or Settings ▸ Codex.",
            "/model",
        ),
        AgentId::Gemini => return None, // Gemini CLI doesn't log an effort
    };
    Some(copy(
        tip(
            "quick-turns-high-effort",
            agent,
            Severity::Nudge,
            0.55,
            text.into(),
            format!("{QUICK_STREAK} quick replies in a row on {effort}"),
        ),
        command,
    ))
}

/// The latest reply ran in fast mode, which isn't billed to your plan.
fn fast_mode(agent: AgentId, session: &[&UsageEvent], now: i64) -> Option<Tip> {
    let latest = session.last().filter(|e| e.fast && now - e.ts < 30 * 60)?;
    Some(copy(
        tip(
            "fast-mode",
            agent,
            Severity::Nudge,
            0.8,
            "Fast mode is on. It's billed as extra usage, not your plan. /fast turns it off."
                .into(),
            format!(
                "Your reply {} ago ran in fast mode",
                duration(now - latest.ts)
            ),
        ),
        "/fast",
    ))
}

/// The classifier (`classify.rs`) is sure your latest prompt is a quick one, you're on the top model, and the
/// chat is still small, so switching costs little (a switch re-sends the whole chat once). Or it's a big job
/// on Haiku. Suggestions only: a hook can't switch the model, and a quick-looking prompt can still be hard.
fn prompt_size(snapshot: &UsageSnapshot, session: &[&UsageEvent]) -> Option<Tip> {
    let guess = snapshot
        .prompt_guess
        .as_ref()
        .filter(|g| g.confidence >= 0.7)?;
    let latest = session.last();
    let model = latest.and_then(|e| e.model.as_deref()).unwrap_or("");
    let small_chat = latest.is_none_or(|e| context(e) < 60_000);
    let why = format!("The prompt hint: {}", guess.reasons.join(", "));
    match guess.class {
        PromptClass::Quick if (model.contains("opus") || model.contains("fable")) && small_chat => {
            Some(copy(
                tip(
                    "prompt-looks-quick",
                    snapshot.agent,
                    Severity::Nudge,
                    0.6,
                    "That one looks quick. Sonnet would do it for less of your limit, and this chat is still small enough to switch cheaply.".into(),
                    why,
                ),
                "/model sonnet",
            ))
        }
        PromptClass::Deep if model.contains("haiku") => Some(copy(
            tip(
                "prompt-looks-deep",
                snapshot.agent,
                Severity::Info,
                0.4,
                "That one looks like a big job for Haiku. A bigger model may get it right first time.".into(),
                why,
            ),
            "/model sonnet",
        )),
        _ => None,
    }
}

/// The last `QUICK_STREAK` replies, if they were all quick.
fn quick_streak<'a>(session: &[&'a UsageEvent]) -> Option<Vec<&'a UsageEvent>> {
    let streak = session.get(session.len().checked_sub(QUICK_STREAK)?..)?;
    streak
        .iter()
        .all(|e| e.tokens.output < QUICK_REPLY)
        .then(|| streak.to_vec())
}

// ---------- wording ----------

pub(crate) fn window_name(window: &LimitWindow) -> &'static str {
    match window.window_minutes {
        300 => "5h",
        1440 => "daily",
        10080 => "weekly",
        43200 => "30-day",
        _ => "",
    }
}

/// 3000 → "50m", 7800 → "2h 10m", 400000 → "4d 15h".
pub(crate) fn duration(secs: i64) -> String {
    let secs = secs.max(0);
    let (days, hours, minutes) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{}m", minutes.max(1))
    }
}

/// 240000 → "240k", 1500000 → "1.5M".
fn tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else {
        format!("{}k", n / 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{Activity, Gait, Pace};
    use crate::model::TokenUsage;

    const NOW: i64 = 1_800_000_000;

    fn reply(secs_ago: i64, output: u64, context: u64) -> UsageEvent {
        UsageEvent {
            agent: AgentId::Claude,
            id: format!("msg_{secs_ago}"),
            ts: NOW - secs_ago,
            session_id: "s".into(),
            model: Some("claude-opus-5-5".into()),
            effort: Some("medium".into()),
            fast: false,
            tokens: TokenUsage {
                output,
                cache_read: context,
                ..Default::default()
            },
        }
    }

    fn snapshot(windows: Vec<LimitWindow>, pace: Vec<Pace>) -> UsageSnapshot {
        UsageSnapshot {
            agent: AgentId::Claude,
            windows,
            activity: Activity {
                gait: Gait::Walking,
                recent_tokens: 0,
                last_reply_at: None,
                waiting_since: None,
            },
            pace,
            tip: None,
            prompt_guess: None,
        }
    }

    fn window(minutes: u32, used_pct: f64, resets_in: i64) -> LimitWindow {
        LimitWindow {
            used_pct,
            window_minutes: minutes,
            resets_at: Some(NOW + resets_in),
            observed_at: NOW,
            approximate: true,
        }
    }

    fn rules(snapshots: &[UsageSnapshot], events: &[UsageEvent]) -> Vec<String> {
        tips(snapshots, events, NOW)
            .into_iter()
            .map(|t| t.rule)
            .collect()
    }

    #[test]
    fn nothing_to_say_on_a_normal_day() {
        let events: Vec<_> = (0..5).map(|i| reply(600 - i * 60, 2000, 80_000)).collect();
        let s = snapshot(vec![window(300, 30.0, 3 * 3600)], vec![]);
        assert!(rules(&[s], &events).is_empty());
    }

    #[test]
    fn warns_when_the_5h_window_runs_out_first() {
        let s = snapshot(
            vec![window(300, 70.0, 2 * 3600 + 600)],
            vec![Pace {
                window_minutes: 300,
                pct_per_hour: 36.0,
                runs_out_at: Some(NOW + 50 * 60),
            }],
        );
        let found = tips(&[s], &[], NOW);
        assert_eq!(found[0].rule, "pace-5h");
        assert_eq!(
            found[0].text,
            "At this pace you hit your 5h limit in 50m (it resets in 2h 10m)."
        );
    }

    #[test]
    fn ahead_of_an_even_pace_on_the_week() {
        // 3 of 7 days left, so an even pace is ~57%; 80% used is well ahead.
        let s = snapshot(vec![window(10080, 80.0, 3 * 86400)], vec![]);
        assert_eq!(rules(&[s], &[]), ["ahead-of-pace"]);
        let on_pace = snapshot(vec![window(10080, 60.0, 3 * 86400)], vec![]);
        assert!(rules(&[on_pace], &[]).is_empty());
    }

    #[test]
    fn use_it_or_lose_it_near_a_reset() {
        let s = snapshot(vec![window(300, 38.0, 30 * 60)], vec![]);
        let found = tips(&[s], &[], NOW);
        assert_eq!(found[0].rule, "use-it-or-lose-it");
        assert!(found[0].text.starts_with("62% of your 5h limit is left"));
    }

    #[test]
    fn huge_context_compared_to_your_usual() {
        let mut events: Vec<_> = (0..20)
            .map(|i| reply(3000 - i * 100, 2000, 100_000))
            .collect();
        events.push(reply(60, 2000, 260_000));
        let found = tips(&[snapshot(vec![], vec![])], &events, NOW);
        assert_eq!(found[0].rule, "huge-context");
        assert_eq!(
            found[0].action,
            Some(TipAction::Copy {
                text: "/compact".into()
            })
        );
        assert_eq!(found[0].why, "Your usual today is 100k");
    }

    #[test]
    fn a_big_context_thats_normal_for_you_is_fine() {
        let events: Vec<_> = (0..20)
            .map(|i| reply(3000 - i * 100, 2000, 250_000))
            .collect();
        assert!(rules(&[snapshot(vec![], vec![])], &events).is_empty());
    }

    #[test]
    fn cold_cache_after_a_break() {
        let events = vec![reply(45 * 60, 2000, 240_000)];
        let found = tips(&[snapshot(vec![], vec![])], &events, NOW);
        assert_eq!(found[0].rule, "cold-cache");
        assert!(found[0].text.starts_with("You were away 45m"));
    }

    #[test]
    fn short_replies_on_opus() {
        let events: Vec<_> = (0..12).map(|i| reply(1200 - i * 60, 120, 50_000)).collect();
        assert_eq!(
            rules(&[snapshot(vec![], vec![])], &events),
            ["short-replies-top-model"]
        );
        // One real answer in the last ten breaks the streak.
        let mut broken = events.clone();
        broken[8].tokens.output = 3000;
        assert!(rules(&[snapshot(vec![], vec![])], &broken).is_empty());
    }

    #[test]
    fn quick_turns_on_high_effort() {
        let events: Vec<_> = (0..10)
            .map(|i| {
                let mut e = reply(1200 - i * 60, 120, 50_000);
                e.effort = Some("high".into());
                e.model = Some("claude-sonnet-5-5".into()); // not Opus, so only the effort rule
                e
            })
            .collect();
        let found = tips(&[snapshot(vec![], vec![])], &events, NOW);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, "quick-turns-high-effort");
        assert_eq!(found[0].why, "10 quick replies in a row on high");
    }

    #[test]
    fn fast_mode_is_on() {
        let mut latest = reply(60, 2000, 50_000);
        latest.fast = true;
        assert_eq!(rules(&[snapshot(vec![], vec![])], &[latest]), ["fast-mode"]);
    }

    #[test]
    fn a_quick_prompt_on_opus_in_a_small_chat() {
        let mut s = snapshot(vec![], vec![]);
        s.prompt_guess = Some(crate::classify::PromptGuess {
            at: NOW - 5,
            session_id: "s".into(),
            class: PromptClass::Quick,
            confidence: 0.8,
            reasons: vec!["very short".into(), "says \"typo\"".into()],
        });
        let small = vec![reply(60, 2000, 30_000)];
        let found = tips(std::slice::from_ref(&s), &small, NOW);
        assert_eq!(found[0].rule, "prompt-looks-quick");
        assert_eq!(found[0].why, "The prompt hint: very short, says \"typo\"");
        // A big chat: switching would re-send all of it, so no tip.
        let big = vec![reply(60, 2000, 300_000)];
        assert!(!rules(&[s], &big).contains(&"prompt-looks-quick".to_string()));
    }

    #[test]
    fn limit_warnings_beat_efficiency_tips() {
        let mut latest = reply(60, 2000, 50_000);
        latest.fast = true;
        let s = snapshot(
            vec![window(300, 70.0, 3 * 3600)],
            vec![Pace {
                window_minutes: 300,
                pct_per_hour: 40.0,
                runs_out_at: Some(NOW + 45 * 60),
            }],
        );
        assert_eq!(rules(&[s], &[latest]), ["pace-5h", "fast-mode"]);
    }
}
