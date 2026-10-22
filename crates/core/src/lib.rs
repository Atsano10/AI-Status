//! StatusPal's core: reads AI agents' local usage data and turns it into what the bar shows.
//! No UI code lives here, so everything can be tested with `cargo test -p statuspal-core`.

pub mod activity;
pub mod adapters;
pub mod classify;
pub mod detect;
pub mod model;
pub mod placement;
pub mod settings;
pub mod tail;
pub mod tips;
pub mod tracker;
pub mod watch;

use std::path::PathBuf;

/// StatusPal's own folder: `~/Library/Application Support/com.atsano10.statuspal`. `None` if we can't find your
/// home folder.
pub fn app_dir() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join("Library/Application Support/com.atsano10.statuspal"))
}
