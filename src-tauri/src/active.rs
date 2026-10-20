//! Keeps track of which agent you're using (rules in `crates/core/src/detect.rs`) and tells the bar
//! when it changes.

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use statuspal_core::detect::{self, Inputs};
use statuspal_core::model::AgentId;
use tauri::{AppHandle, Emitter, Manager, State};

/// How often we look at the front app. Cheap: one system call plus a few file timestamps.
const POLL: Duration = Duration::from_secs(2);

/// The agent picked last, so the bar can ask for it when it loads (it may miss the first event).
#[derive(Default)]
pub struct ActiveAgent(Mutex<Option<AgentId>>);

#[tauri::command]
pub fn get_active_agent(state: State<'_, ActiveAgent>) -> Option<AgentId> {
    *state.0.lock().unwrap()
}

pub fn setup(app: &AppHandle) {
    app.manage(ActiveAgent::default());

    let app = app.clone();
    thread::spawn(move || {
        let mut previous = None;
        loop {
            let inputs = Inputs {
                pinned: None, // the tray's "pin agent" item comes later
                frontmost: frontmost_bundle_id(),
                last_writes: detect::last_writes(),
                previous,
                now: now_secs(),
            };
            let active = detect::pick_active(&inputs);

            // Only tell the bar when the answer changes, not every 2 seconds.
            if active != previous {
                *app.state::<ActiveAgent>().0.lock().unwrap() = active;
                let _ = app.emit("active-agent-changed", active);
                previous = active;
            }
            thread::sleep(POLL);
        }
    });
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The bundle id of the app in front, e.g. `com.googlecode.iterm2`. The bar never takes focus, so this
/// is always the app you're actually using.
#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    use objc2_app_kit::NSWorkspace;

    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    app.bundleIdentifier().map(|id| id.to_string())
}

#[cfg(not(target_os = "macos"))]
fn frontmost_bundle_id() -> Option<String> {
    None
}
