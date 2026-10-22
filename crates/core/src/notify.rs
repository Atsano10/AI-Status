//! macOS notifications for when a limit gets close, runs out, or resets. The `Notifier` remembers how full
//! each window was last time, so a notification fires once when the window crosses a line, not on every
//! update. The app shows what `check` returns (src-tauri/src/usage.rs).

use std::collections::HashMap;

use crate::model::{AgentId, LimitWindow, UsageSnapshot};
use crate::tips::{duration, window_name};

/// "Close" from here (% used): the same line where the bar turns orange.
const CLOSE_AT: f64 = 80.0;
const OUT_AT: f64 = 100.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// How full a window is. The order matters: `Close > Fine`, so "went up" is a plain comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Fine,
    Close,
    Out,
}

#[derive(Debug, Default)]
pub struct Notifier {
    /// Each window's level last time, by agent and window length. Missing means we haven't seen it yet.
    last: HashMap<(AgentId, u32), Level>,
}

impl Notifier {
    /// What to notify about since the last call.
    pub fn check(&mut self, snapshots: &[UsageSnapshot], now: i64) -> Vec<Notification> {
        let mut notifications = Vec::new();
        for snapshot in snapshots {
            for window in &snapshot.windows {
                let level = level(window, now);
                // `insert` hands back the old level. The first time there's none, and we stay quiet, so
                // opening the app at 85% doesn't notify about something you already know.
                let before = self
                    .last
                    .insert((snapshot.agent, window.window_minutes), level);
                if let Some(before) = before
                    && let Some(n) = message(snapshot.agent, window, before, level, now)
                {
                    notifications.push(n);
                }
            }
        }
        notifications
    }
}

fn level(window: &LimitWindow, now: i64) -> Level {
    // Past the reset time the reading is old news: the window is back to 0%.
    if window.has_reset(now) {
        return Level::Fine;
    }
    if window.used_pct >= OUT_AT {
        Level::Out
    } else if window.used_pct >= CLOSE_AT {
        Level::Close
    } else {
        Level::Fine
    }
}

fn message(
    agent: AgentId,
    window: &LimitWindow,
    before: Level,
    now_level: Level,
    now: i64,
) -> Option<Notification> {
    let name = window_name(window);
    let resets = match window.resets_at {
        Some(at) => format!("Resets in {}.", duration(at - now)),
        None => "Reset time unknown.".into(),
    };
    match (before, now_level) {
        (Level::Fine, Level::Close) => Some(Notification {
            title: format!(
                "{agent:?} is at {}% of its {name} limit",
                window.used_pct.round()
            ),
            body: resets,
        }),
        (Level::Fine | Level::Close, Level::Out) => Some(Notification {
            title: format!("{agent:?} hit its {name} limit"),
            body: resets,
        }),
        // Only worth saying if you were close: a reset at 20% is no news.
        (Level::Close | Level::Out, Level::Fine) => Some(Notification {
            title: format!("{agent:?}'s {name} limit reset"),
            body: "You're back to 0%.".into(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{Activity, Gait};

    const NOW: i64 = 1_800_000_000;

    fn snapshot(used_pct: f64, resets_in: i64) -> Vec<UsageSnapshot> {
        vec![UsageSnapshot {
            agent: AgentId::Claude,
            windows: vec![LimitWindow {
                used_pct,
                window_minutes: 300,
                resets_at: Some(NOW + resets_in),
                observed_at: NOW,
                approximate: true,
            }],
            activity: Activity {
                gait: Gait::Walking,
                recent_tokens: 0,
                last_reply_at: None,
                waiting_since: None,
            },
            pace: vec![],
            tip: None,
            prompt_guess: None,
        }]
    }

    fn titles(n: Vec<Notification>) -> Vec<String> {
        n.into_iter().map(|n| n.title).collect()
    }

    #[test]
    fn first_look_is_quiet() {
        let mut notifier = Notifier::default();
        assert!(notifier.check(&snapshot(85.0, 3600), NOW).is_empty());
    }

    #[test]
    fn crossing_80_notifies_once() {
        let mut notifier = Notifier::default();
        notifier.check(&snapshot(70.0, 3600), NOW);
        let n = notifier.check(&snapshot(82.0, 3600), NOW);
        assert_eq!(n[0].title, "Claude is at 82% of its 5h limit");
        assert_eq!(n[0].body, "Resets in 1h 0m.");
        assert!(notifier.check(&snapshot(88.0, 3600), NOW).is_empty());
    }

    #[test]
    fn hitting_the_limit() {
        let mut notifier = Notifier::default();
        notifier.check(&snapshot(90.0, 3600), NOW);
        assert_eq!(
            titles(notifier.check(&snapshot(100.0, 1800), NOW)),
            ["Claude hit its 5h limit"]
        );
    }

    #[test]
    fn jumping_straight_past_both_lines_says_out() {
        let mut notifier = Notifier::default();
        notifier.check(&snapshot(50.0, 3600), NOW);
        assert_eq!(
            titles(notifier.check(&snapshot(100.0, 3600), NOW)),
            ["Claude hit its 5h limit"]
        );
    }

    #[test]
    fn reset_after_being_close() {
        let mut notifier = Notifier::default();
        notifier.check(&snapshot(100.0, 60), NOW);
        // Same reading, but its reset time has now passed.
        assert_eq!(
            titles(notifier.check(&snapshot(100.0, 60), NOW + 120)),
            ["Claude's 5h limit reset"]
        );
    }

    #[test]
    fn reset_from_low_use_is_quiet() {
        let mut notifier = Notifier::default();
        notifier.check(&snapshot(20.0, 60), NOW);
        assert!(notifier.check(&snapshot(20.0, 60), NOW + 120).is_empty());
    }
}
