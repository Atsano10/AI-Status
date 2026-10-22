//! Your usage numbers, kept in one `Tracker` (crates/core/src/tracker.rs). It's refreshed whenever Claude or
//! Codex writes new usage data, and the bar gets `limits-updated`.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use statuspal_core::model::UsageSnapshot;
use statuspal_core::tracker::Tracker;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings;

pub struct UsageState(Mutex<Tracker>);

pub fn setup(app: &AppHandle) {
    let mut tracker = Tracker::default();
    tracker.refresh(now_secs()); // the first read: every session from the last month
    app.manage(UsageState(Mutex::new(tracker)));

    let handle = app.clone();
    match statuspal_core::watch::spawn(move || {
        let snapshots = refresh(&handle);
        let _ = handle.emit("limits-updated", snapshots);
    }) {
        // Tauri keeps managed values alive until the app quits. Without this, `watcher` would be dropped at
        // the end of `setup` and watching would stop right away.
        Ok(watcher) => {
            app.manage(watcher);
        }
        Err(e) => eprintln!("can't watch usage files, the bar only updates when shown: {e}"),
    }
}

/// Reads what's new and returns what the bar shows.
fn refresh(app: &AppHandle) -> Vec<UsageSnapshot> {
    let state = app.state::<UsageState>();
    let mut tracker = state.0.lock().unwrap();
    let now = now_secs();
    tracker.refresh(now);
    tracker.snapshots(now, settings::get(app).gait_thresholds)
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
