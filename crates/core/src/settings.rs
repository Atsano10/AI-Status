//! Your StatusPal settings, saved as JSON in `~/Library/Application Support/com.atsano10.statuspal/settings.json`.
//!
//! ```json
//! {"hotkey": "Control+Alt+Space", "behaviour": "fade", "peekSeconds": 4, "orientation": "horizontal", "barShows": "5h",
//!  "pinned": null, "barPositions": {"Built-in Retina Display": {"x": 548, "y": 6}}, "barDisplay": "Built-in Retina Display"}
//! ```
//!
//! Any field can be missing (it gets its default), so files from older versions still load.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::activity::GaitThresholds;
use crate::model::AgentId;

/// The name of the settings file.
pub const FILE_NAME: &str = "settings.json";

/// What the bar does once the hotkey shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Behaviour {
    /// Stays fully visible until you press the hotkey again.
    Toggle,
    /// Fades to see-through when left alone and wakes on hover or when something changes.
    Fade,
    /// Shows for `peek_seconds`, then hides.
    Peek,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// Which window the pill shows when an agent has two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub enum BarShows {
    #[serde(rename = "5h")]
    FiveHour,
    #[serde(rename = "week")]
    Week,
}

/// Where the bar sits on one display: its top-left corner, in points from the top-left of the display's
/// usable area (below the menu bar).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct BarPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
// `default`: a field missing from the file takes its value from `Settings::default()`.
// `camelCase`: `peek_seconds` in Rust is `peekSeconds` in the JSON, which is what the TypeScript side expects.
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// The shortcut that shows/hides the bar, in Tauri's format, e.g. `"Control+Alt+Space"`.
    pub hotkey: String,
    pub behaviour: Behaviour,
    /// How long Peek keeps the bar up.
    pub peek_seconds: u32,
    pub orientation: Orientation,
    pub bar_shows: BarShows,
    /// The agent pinned from the tray's "Pin agent" menu. `None` is Auto (the rules in `detect.rs` decide).
    pub pinned: Option<AgentId>,
    /// Where you left the bar on each display, by display name. A display we haven't seen gets the default
    /// spot (top centre, under the notch).
    pub bar_positions: BTreeMap<String, BarPosition>,
    /// The display the bar was on last, so it comes back there if that display is still connected.
    pub bar_display: Option<String>,
    /// Where the critter starts jogging and running (effective tokens per 10 minutes).
    pub gait_thresholds: GaitThresholds,
    /// Show tips in the panel (see `tips.rs`).
    pub tips: bool,
    /// Tips you hid, by rule id, until when (Unix seconds). "Never" is far in the future.
    #[cfg_attr(test, ts(type = "{ [rule in string]?: number }"))]
    pub hidden_tips: BTreeMap<String, i64>,
}

impl Settings {
    /// Whether a tip from this rule may show right now.
    pub fn shows_tip(&self, rule: &str, now: i64) -> bool {
        self.tips && self.hidden_tips.get(rule).is_none_or(|until| *until <= now)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey: "Control+Alt+Space".into(), // ⌃⌥Space, the macOS default from the plan
            behaviour: Behaviour::Fade,
            peek_seconds: 4,
            orientation: Orientation::Horizontal,
            bar_shows: BarShows::FiveHour,
            pinned: None,
            bar_positions: BTreeMap::new(),
            bar_display: None,
            gait_thresholds: GaitThresholds::default(),
            tips: true,
            hidden_tips: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// These settings with some fields replaced, e.g. `{"orientation": "vertical"}`. The windows send only
    /// what they changed, so two windows changing different settings don't undo each other.
    ///
    /// A bad value (like `{"behaviour": "wobble"}`) is an error, and nothing changes.
    pub fn with_changes(&self, changes: Value) -> Result<Settings, serde_json::Error> {
        let Value::Object(changes) = changes else {
            return Err(serde::de::Error::custom(
                "settings changes must be an object",
            ));
        };
        let mut merged = serde_json::to_value(self)?;
        if let Value::Object(fields) = &mut merged {
            fields.extend(changes);
        }
        serde_json::from_value(merged)
    }
}

/// Where the settings live. `None` if we can't find your home folder.
pub fn settings_file() -> Option<PathBuf> {
    crate::app_dir().map(|dir| dir.join(FILE_NAME))
}

/// Reads the settings from `path`. A missing or broken file gives the defaults, so a bad edit never stops
/// the app from starting.
pub fn load(path: &Path) -> Settings {
    // No file yet is normal on first launch, so that one stays quiet.
    let Ok(text) = fs::read_to_string(path) else {
        return Settings::default();
    };
    match serde_json::from_str(&text) {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("{} is broken, using the defaults: {e}", path.display());
            Settings::default()
        }
    }
}

/// Saves the settings to `path`, creating the folder if needed.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(settings)?;

    // Write a temporary file and then rename it over the real one. A rename can't be half done, so a crash
    // mid-save leaves the old settings instead of a cut-off file.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder per test so they don't trip over each other when run in parallel.
    fn temp_path(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "statuspal-test-settings-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    #[test]
    fn missing_file_gives_defaults() {
        assert_eq!(load(&temp_path("missing")), Settings::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("round-trip");
        let settings = Settings {
            hotkey: "Command+Shift+K".into(),
            behaviour: Behaviour::Peek,
            peek_seconds: 7,
            orientation: Orientation::Vertical,
            bar_shows: BarShows::Week,
            pinned: Some(AgentId::Codex),
            bar_positions: BTreeMap::from([(
                "Studio Display".into(),
                BarPosition { x: 12.0, y: 6.0 },
            )]),
            bar_display: Some("Studio Display".into()),
            gait_thresholds: GaitThresholds {
                jog_at: 50_000,
                run_at: 200_000,
            },
            tips: false,
            hidden_tips: BTreeMap::from([("fast-mode".into(), i64::MAX)]),
        };
        save(&path, &settings).unwrap();
        assert_eq!(load(&path), settings);
    }

    #[test]
    fn missing_fields_get_defaults() {
        let path = temp_path("partial");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"behaviour": "toggle"}"#).unwrap();

        let settings = load(&path);
        assert_eq!(settings.behaviour, Behaviour::Toggle);
        assert_eq!(settings.hotkey, Settings::default().hotkey);
    }

    #[test]
    fn broken_file_gives_defaults() {
        let path = temp_path("broken");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn changes_replace_only_the_fields_sent() {
        let changed = Settings::default()
            .with_changes(serde_json::json!({"orientation": "vertical", "pinned": "Claude"}))
            .unwrap();
        assert_eq!(changed.orientation, Orientation::Vertical);
        assert_eq!(changed.pinned, Some(AgentId::Claude));
        assert_eq!(changed.behaviour, Settings::default().behaviour);
    }

    #[test]
    fn a_bad_change_is_an_error() {
        let settings = Settings::default();
        assert!(
            settings
                .with_changes(serde_json::json!({"behaviour": "wobble"}))
                .is_err()
        );
        assert!(
            settings
                .with_changes(serde_json::json!("vertical"))
                .is_err()
        );
    }

    #[test]
    fn hidden_tips_come_back_when_their_time_is_up() {
        let mut settings = Settings::default();
        settings.hidden_tips.insert("cold-cache".into(), 1000);
        assert!(!settings.shows_tip("cold-cache", 999));
        assert!(settings.shows_tip("cold-cache", 1000));
        assert!(settings.shows_tip("huge-context", 0));
        settings.tips = false;
        assert!(!settings.shows_tip("huge-context", 0));
    }

    #[test]
    fn json_uses_the_names_the_bar_expects() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(json.contains(r#""peekSeconds":4"#), "{json}");
        assert!(json.contains(r#""barShows":"5h""#), "{json}");
        assert!(json.contains(r#""behaviour":"fade""#), "{json}");
    }
}
