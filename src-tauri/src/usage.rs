//! Your usage numbers, kept in one `Tracker` (crates/core/src/tracker.rs). The bar gets `limits-updated`
//! whenever they change:
//! - when Claude or Codex writes new usage data (the file watcher)
//! - as time passes, since the critter's gait depends on the last 10 minutes and falls asleep on its own
//! - when you change the gait thresholds or hide a tip
//!
//! Each update also checks for notifications (crates/core/src/notify.rs): a limit getting close, running out,
//! or resetting.

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use statuspal_core::model::UsageSnapshot;
use statuspal_core::notify::Notifier;
use statuspal_core::tracker::Tracker;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::settings;

/// How often we look again while nothing is being written, so the gait catches up with the clock.
const TICK: Duration = Duration::from_secs(20);

struct UsageState {
    tracker: Mutex<Tracker>,
    /// What the bar was last sent, so an unchanged tick sends nothing.
    sent: Mutex<Vec<UsageSnapshot>>,
    notifier: Mutex<Notifier>,
}

pub fn setup(app: &AppHandle) {
    let mut tracker = Tracker::default();
    tracker.refresh(now_secs()); // the first read: every session from the last week or month
    app.manage(UsageState {
        tracker: Mutex::new(tracker),
        sent: Mutex::new(Vec::new()),
        notifier: Mutex::new(Notifier::default()),
    });

    let handle = app.clone();
    match statuspal_core::watch::spawn(move || update(&handle)) {
        // Tauri keeps managed values alive until the app quits. Without this, `watcher` would be dropped at
        // the end of `setup` and watching would stop right away.
        Ok(watcher) => {
            app.manage(watcher);
        }
        Err(e) => eprintln!("can't watch usage files, the bar only updates when shown: {e}"),
    }

    let handle = app.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(TICK);
            update(&handle);
        }
    });

    // New thresholds change the gait straight away.
    let handle = app.clone();
    app.listen_any("settings-changed", move |_| update(&handle));
}

/// Reads what's new, tells the bar if anything it shows changed, and sends any notifications.
fn update(app: &AppHandle) {
    let snapshots = refresh(app);
    let state = app.state::<UsageState>();
    notify(app, &state.notifier, &snapshots);
    let mut sent = state.sent.lock().unwrap();
    if *sent != snapshots {
        let _ = app.emit("limits-updated", &snapshots);
        *sent = snapshots;
    }
}

/// Shows a notification for each limit that just got close, ran out, or reset. The `Notifier` keeps
/// checking while notifications are off, so turning them on later doesn't fire for something old.
fn notify(app: &AppHandle, notifier: &Mutex<Notifier>, snapshots: &[UsageSnapshot]) {
    let notifications = notifier.lock().unwrap().check(snapshots, now_secs());
    if !settings::get(app).notifications {
        return;
    }
    for n in notifications {
        if let Err(e) = app
            .notification()
            .builder()
            .title(&n.title)
            .body(&n.body)
            .show()
        {
            eprintln!("can't show a notification: {e}");
        }
    }
}

/// Reads what's new and returns what the bar shows.
fn refresh(app: &AppHandle) -> Vec<UsageSnapshot> {
    let state = app.state::<UsageState>();
    let mut tracker = state.tracker.lock().unwrap();
    let now = now_secs();
    tracker.refresh(now);
    let settings = settings::get(app);
    tracker.snapshots(now, settings.gait_thresholds, |rule| {
        settings.shows_tip(rule, now)
    })
}

/// The tip's Hide (for an hour) and Never buttons.
#[tauri::command]
pub fn hide_tip(app: AppHandle, rule: String, forever: bool) {
    let until = if forever { i64::MAX } else { now_secs() + 3600 };
    settings::hide_tip(&app, rule, until);
}

/// The bar asks for this when it loads and every time it's shown.
#[tauri::command]
pub fn get_limits(app: AppHandle) -> Vec<UsageSnapshot> {
    refresh(&app)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
