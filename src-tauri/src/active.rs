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

/// The agent you pinned from the tray's "Pin agent" menu. `None` means Auto (let the rules decide).
///
/// Two threads touch this: the tray's click handler writes it, and the detector thread below reads it
/// every 2 seconds. The `Mutex` makes sure they take turns instead of reading half-written data.
#[derive(Default)]
pub struct PinnedAgent(Mutex<Option<AgentId>>);

impl PinnedAgent {
    /// Called by the tray when you click Auto / Claude / Codex.
    pub fn set(&self, agent: Option<AgentId>) {
        *self.0.lock().unwrap() = agent;
    }

    /// Called by the detector thread every 2 seconds.
    pub fn get(&self) -> Option<AgentId> {
        *self.0.lock().unwrap()
    }
}

#[tauri::command]
pub fn get_active_agent(state: State<'_, ActiveAgent>) -> Option<AgentId> {
    *state.0.lock().unwrap()
}

pub fn setup(app: &AppHandle) {
    app.manage(ActiveAgent::default());
    app.manage(PinnedAgent::default());

    let app = app.clone();
    thread::spawn(move || {
        let mut previous = None;
        loop {
            let inputs = Inputs {
                pinned: app.state::<PinnedAgent>().get(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn starts_on_auto() {
        assert_eq!(PinnedAgent::default().get(), None);
    }

    #[test]
    fn remembers_the_last_pin() {
        let pin = PinnedAgent::default();
        pin.set(Some(AgentId::Claude));
        assert_eq!(pin.get(), Some(AgentId::Claude));
        pin.set(Some(AgentId::Codex));
        assert_eq!(pin.get(), Some(AgentId::Codex));
        pin.set(None);
        assert_eq!(pin.get(), None);
    }

    /// The real reason for the Mutex: a pin set on one thread (the tray) is seen on another (the detector).
    /// `Arc` lets both threads own the same `PinnedAgent`; Tauri's `app.manage` does this for us in the app.
    #[test]
    fn another_thread_sees_the_pin() {
        let pin = Arc::new(PinnedAgent::default());
        let tray_side = Arc::clone(&pin);
        thread::spawn(move || tray_side.set(Some(AgentId::Codex)))
            .join()
            .unwrap();
        assert_eq!(pin.get(), Some(AgentId::Codex));
    }
}
