//! How hard you're working right now, from the replies the Tracker keeps:
//! - **gait**: how fast the critter moves, from the tokens used in the last 10 minutes
//! - **pace**: how fast each limit is filling, from the limit % readings over the last hour, and when you'd
//!   run out at that pace
//!
//! Tokens are "effective tokens" (`TokenUsage::effective`): cache reads count a tenth, since they cost about a
//! tenth of fresh input. Otherwise one turn's re-sent context would swamp everything.

use serde::{Deserialize, Serialize};

use crate::model::{LimitWindow, UsageEvent};

/// The tokens that count towards the gait are the ones from this long ago until now.
pub const WINDOW_SECS: i64 = 10 * 60;
/// No reply for this long and the critter falls asleep.
pub const SLEEP_AFTER_SECS: i64 = 5 * 60;

/// How fast the critter moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Gait {
    /// No replies for `SLEEP_AFTER_SECS`.
    Sleeping,
    Walking,
    Jogging,
    Running,
}

/// Where walking turns into jogging and jogging into running, in effective tokens per `WINDOW_SECS`.
/// Defaults from your own data: 150k is your quieter quarter, 500k your busiest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(default, rename_all = "camelCase")]
pub struct GaitThresholds {
    #[cfg_attr(test, ts(type = "number"))]
    pub jog_at: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub run_at: u64,
}

impl Default for GaitThresholds {
    fn default() -> Self {
        GaitThresholds {
            jog_at: 150_000,
            run_at: 500_000,
        }
    }
}

/// What one agent (or all of them) is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub gait: Gait,
    /// Effective tokens in the last `WINDOW_SECS`.
    #[cfg_attr(test, ts(type = "number"))]
    pub recent_tokens: u64,
    /// When the last reply came (Unix seconds), if there was one in the last day.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub last_reply_at: Option<i64>,
}

/// Works out the activity from `events` (any order, any agents: filter them first for one agent).
pub fn activity<'a>(
    events: impl IntoIterator<Item = &'a UsageEvent>,
    now: i64,
    thresholds: GaitThresholds,
) -> Activity {
    let mut recent_tokens = 0;
    let mut last_reply_at: Option<i64> = None;
    for event in events {
        if event.ts > now {
            continue; // a clock that's a bit off; it'll count once we get there
        }
        if event.ts > now - WINDOW_SECS {
            recent_tokens += event.tokens.effective();
        }
        last_reply_at = last_reply_at.max(Some(event.ts));
    }

    let asleep = last_reply_at.is_none_or(|last| now - last >= SLEEP_AFTER_SECS);
    let gait = if asleep {
        Gait::Sleeping
    } else if recent_tokens >= thresholds.run_at {
        Gait::Running
    } else if recent_tokens >= thresholds.jog_at {
        Gait::Jogging
    } else {
        Gait::Walking
    };
    Activity {
        gait,
        recent_tokens,
        last_reply_at,
    }
}

/// Only readings from this long ago count towards the pace, so it follows how you're working *now*.
pub const PACE_SPAN_SECS: i64 = 60 * 60;
/// Readings must span at least this long for a pace. Claude reports whole percents, so a few minutes of
/// readings would jump between 0 and huge.
pub const PACE_MIN_SPAN_SECS: i64 = 10 * 60;

/// How fast one limit window is filling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Pace {
    pub window_minutes: u32,
    /// Percentage points per hour, e.g. 12.5. It's an estimate from a few readings.
    pub pct_per_hour: f64,
    /// When you'd reach 100% at this pace (Unix seconds). `None` if you won't before the window resets.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub runs_out_at: Option<i64>,
}

/// One limit % reading: when (Unix seconds) and how much was used.
pub type Reading = (i64, f64);

/// The pace of `window`, from its past `readings` (oldest first). `None` until there are enough readings.
pub fn pace(readings: &[Reading], window: &LimitWindow, now: i64) -> Option<Pace> {
    // A drop means the window reset, and readings from before it say nothing about now.
    let since_reset = readings
        .windows(2)
        .rposition(|pair| pair[1].1 < pair[0].1 - 0.5)
        .map_or(0, |i| i + 1);
    let recent: Vec<Reading> = readings[since_reset..]
        .iter()
        .copied()
        .filter(|(at, _)| *at >= now - PACE_SPAN_SECS)
        .collect();
    let (&(first_at, first_pct), &(last_at, last_pct)) = (recent.first()?, recent.last()?);
    let span = last_at - first_at;
    if span < PACE_MIN_SPAN_SECS {
        return None;
    }

    let pct_per_hour = ((last_pct - first_pct) / span as f64 * 3600.0).max(0.0);
    let runs_out_at = (pct_per_hour > 0.0)
        .then(|| now + ((100.0 - window.used_pct).max(0.0) / pct_per_hour * 3600.0) as i64)
        .filter(|at| window.resets_at.is_none_or(|reset| *at < reset));
    Some(Pace {
        window_minutes: window.window_minutes,
        pct_per_hour,
        runs_out_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentId, TokenUsage};

    const NOW: i64 = 1_800_000_000;

    fn reply(secs_ago: i64, output: u64) -> UsageEvent {
        UsageEvent {
            agent: AgentId::Claude,
            id: format!("msg_{secs_ago}"),
            ts: NOW - secs_ago,
            session_id: "s".into(),
            model: None,
            effort: None,
            fast: false,
            tokens: TokenUsage {
                output,
                ..Default::default()
            },
        }
    }

    fn gait(events: &[UsageEvent]) -> Gait {
        activity(events, NOW, GaitThresholds::default()).gait
    }

    #[test]
    fn every_gait() {
        assert_eq!(gait(&[]), Gait::Sleeping);
        assert_eq!(gait(&[reply(30, 10_000)]), Gait::Walking);
        assert_eq!(
            gait(&[reply(30, 100_000), reply(200, 60_000)]),
            Gait::Jogging
        );
        assert_eq!(
            gait(&[reply(30, 400_000), reply(500, 150_000)]),
            Gait::Running
        );
    }

    #[test]
    fn falls_asleep_five_minutes_after_the_last_reply() {
        assert_eq!(gait(&[reply(SLEEP_AFTER_SECS - 1, 600_000)]), Gait::Running);
        assert_eq!(gait(&[reply(SLEEP_AFTER_SECS, 600_000)]), Gait::Sleeping);
    }

    #[test]
    fn only_the_last_ten_minutes_count() {
        let a = activity(
            &[reply(60, 100_000), reply(WINDOW_SECS + 1, 900_000)],
            NOW,
            GaitThresholds::default(),
        );
        assert_eq!(a.recent_tokens, 100_000);
        assert_eq!(a.gait, Gait::Walking);
        assert_eq!(a.last_reply_at, Some(NOW - 60));
    }

    #[test]
    fn cache_reads_count_a_tenth() {
        let mut big_context = reply(30, 1_000);
        big_context.tokens.cache_read = 1_000_000; // a turn that re-sends a huge context
        assert_eq!(
            activity(&[big_context], NOW, GaitThresholds::default()).recent_tokens,
            101_000
        );
    }

    #[test]
    fn your_own_thresholds() {
        let calm = GaitThresholds {
            jog_at: 5_000,
            run_at: 20_000,
        };
        assert_eq!(
            activity(&[reply(30, 25_000)], NOW, calm).gait,
            Gait::Running
        );
    }

    fn five_hour(used_pct: f64, resets_in: i64) -> LimitWindow {
        LimitWindow {
            used_pct,
            window_minutes: 300,
            resets_at: Some(NOW + resets_in),
            observed_at: NOW,
            approximate: true,
        }
    }

    #[test]
    fn pace_from_readings() {
        // 40% → 60% over the last 40 minutes: 30 points an hour. 40 points left → 80 minutes.
        let readings = [(NOW - 2400, 40.0), (NOW - 1200, 50.0), (NOW, 60.0)];
        let p = pace(&readings, &five_hour(60.0, 3 * 3600), NOW).unwrap();
        assert_eq!(p.pct_per_hour, 30.0);
        assert_eq!(p.runs_out_at, Some(NOW + 80 * 60));
    }

    #[test]
    fn no_running_out_if_the_reset_comes_first() {
        let readings = [(NOW - 2400, 40.0), (NOW, 60.0)];
        let p = pace(&readings, &five_hour(60.0, 30 * 60), NOW).unwrap();
        assert_eq!(p.runs_out_at, None);
    }

    #[test]
    fn needs_ten_minutes_of_readings() {
        let readings = [(NOW - 300, 40.0), (NOW, 42.0)];
        assert_eq!(pace(&readings, &five_hour(42.0, 3600), NOW), None);
    }

    #[test]
    fn readings_before_a_reset_are_ignored() {
        // Was at 95% an hour ago, the window reset, now 2% → 8% over 20 minutes.
        let readings = [(NOW - 3000, 95.0), (NOW - 1200, 2.0), (NOW, 8.0)];
        let p = pace(&readings, &five_hour(8.0, 4 * 3600), NOW).unwrap();
        assert_eq!(p.pct_per_hour, 18.0);
    }

    #[test]
    fn not_using_it_is_a_pace_of_zero() {
        let readings = [(NOW - 1800, 20.0), (NOW, 20.0)];
        let p = pace(&readings, &five_hour(20.0, 3600), NOW).unwrap();
        assert_eq!(p.pct_per_hour, 0.0);
        assert_eq!(p.runs_out_at, None);
    }
}
