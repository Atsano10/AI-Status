//! Codex's settings file, `~/.codex/config.toml` (or `$CODEX_HOME/config.toml`), for two things in Settings ▸
//! Codex:
//! - **default effort**: `model_reasoning_effort`, which the Codex app and the `codex` command both use
//! - **profiles** for the `codex` command: `[profiles.quick]` / `normal` / `deep`, started with
//!   `codex --profile quick`
//!
//! The Codex app writes this file too, so we change only our keys and keep everything else exactly as it was
//! (`toml_edit` keeps comments, order and spacing). Before our first change we save a copy next to it,
//! `config.toml.statuspal-backup`, and Undo puts the effort back to what that copy says. Our profiles carry a
//! marker comment, so we never touch profiles of yours with the same names.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use toml_edit::{DocumentMut, Item, Table, value};

const MARKER: &str = "# Added by StatusPal (Settings ▸ Codex). Remove them there.";

/// The efforts we offer, lowest first.
pub const EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];

/// Our profiles: name and effort. The model stays whatever your default is.
pub const PROFILES: [(&str, &str); 3] = [("quick", "low"), ("normal", "medium"), ("deep", "xhigh")];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ProfilesState {
    Off,
    On,
    /// You already have a profile called quick, normal or deep, so we don't add ours.
    Yours,
}

/// What Settings ▸ Codex shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CodexSetup {
    /// False if Codex isn't set up on this Mac (no `~/.codex`).
    pub found: bool,
    /// The default effort, e.g. `"xhigh"`. `None` means Codex's own default.
    pub effort: Option<String>,
    /// The effort before StatusPal changed it (from the backup), if it's different now. Undo goes back to it.
    pub effort_before: Option<String>,
    pub profiles: ProfilesState,
}

/// `~/.codex/config.toml`, or `$CODEX_HOME/config.toml`.
pub fn config_file() -> Option<PathBuf> {
    let dir = match std::env::var_os("CODEX_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => std::env::home_dir()?.join(".codex"),
    };
    Some(dir.join("config.toml"))
}

fn backup_path(config: &Path) -> PathBuf {
    config.with_extension("toml.statuspal-backup")
}

fn read(config: &Path) -> io::Result<DocumentMut> {
    match fs::read_to_string(config) {
        Ok(text) => text.parse().map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(e),
    }
}

/// Saves the copy before our first change (later changes keep that first copy), then writes `doc` safely.
fn write(config: &Path, doc: &DocumentMut) -> io::Result<()> {
    let backup = backup_path(config);
    if config.exists() && !backup.exists() {
        fs::copy(config, &backup)?;
    }
    if let Some(dir) = config.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = config.with_extension("toml.statuspal-tmp");
    fs::write(&tmp, doc.to_string())?;
    fs::rename(&tmp, config)
}

fn effort_in(doc: &DocumentMut) -> Option<String> {
    doc.get("model_reasoning_effort")?
        .as_str()
        .map(String::from)
}

fn is_ours(table: &Table) -> bool {
    table
        .decor()
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .is_some_and(|prefix| prefix.contains(MARKER))
}

fn profiles_state(doc: &DocumentMut) -> ProfilesState {
    let Some(profiles) = doc.get("profiles").and_then(Item::as_table) else {
        return ProfilesState::Off;
    };
    let ours: Vec<Option<bool>> = PROFILES
        .iter()
        .map(|(name, _)| profiles.get(name).and_then(Item::as_table).map(is_ours))
        .collect();
    if ours.contains(&Some(false)) {
        ProfilesState::Yours
    } else if ours.iter().all(|o| *o == Some(true)) {
        ProfilesState::On
    } else {
        ProfilesState::Off
    }
}

pub fn setup(config: &Path) -> CodexSetup {
    let found = config.parent().is_some_and(Path::is_dir);
    let doc = read(config).unwrap_or_default();
    let effort = effort_in(&doc);
    let before = read(&backup_path(config))
        .ok()
        .filter(|_| backup_path(config).exists())
        .map(|backup| effort_in(&backup));
    CodexSetup {
        found,
        effort_before: before.flatten().filter(|b| Some(b) != effort.as_ref()),
        effort,
        profiles: profiles_state(&doc),
    }
}

/// Sets the default effort. Only `EFFORTS` are allowed.
pub fn set_effort(config: &Path, effort: &str) -> io::Result<()> {
    if !EFFORTS.contains(&effort) {
        return Err(io::Error::other(format!(
            "{effort} isn't an effort Codex knows"
        )));
    }
    let mut doc = read(config)?;
    doc["model_reasoning_effort"] = value(effort);
    write(config, &doc)
}

/// Puts the effort back to what it was before StatusPal's first change.
pub fn undo_effort(config: &Path) -> io::Result<()> {
    let backup = read(&backup_path(config))?;
    let mut doc = read(config)?;
    match effort_in(&backup) {
        Some(effort) => doc["model_reasoning_effort"] = value(effort),
        None => {
            doc.remove("model_reasoning_effort");
        }
    }
    write(config, &doc)
}

/// Adds our three profiles, or removes them. Profiles of yours are never touched.
pub fn set_profiles(config: &Path, on: bool) -> io::Result<()> {
    let mut doc = read(config)?;
    if profiles_state(&doc) == ProfilesState::Yours {
        return Err(io::Error::other(
            "you already have a quick, normal or deep profile, so StatusPal leaves your profiles alone",
        ));
    }
    if on {
        let profiles = doc.entry("profiles").or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true); // no empty `[profiles]` line, only `[profiles.quick]` and so on
            Item::Table(table)
        });
        let profiles = profiles
            .as_table_mut()
            .ok_or_else(|| io::Error::other("`profiles` in config.toml isn't a table"))?;
        for (i, (name, effort)) in PROFILES.iter().enumerate() {
            let mut table = Table::new();
            if i == 0 {
                table.decor_mut().set_prefix(format!("\n{MARKER}\n"));
            } else {
                table.decor_mut().set_prefix(format!("{MARKER}\n"));
            }
            table["model_reasoning_effort"] = value(*effort);
            profiles.insert(name, Item::Table(table));
        }
    } else if let Some(profiles) = doc.get_mut("profiles").and_then(Item::as_table_mut) {
        for (name, _) in PROFILES {
            if profiles
                .get(name)
                .and_then(Item::as_table)
                .is_some_and(is_ours)
            {
                profiles.remove(name);
            }
        }
        if profiles.is_empty() {
            doc.remove("profiles");
        }
    }
    write(config, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Like the real file: settings, comments, tables the Codex app owns.
    const CONFIG: &str = r#"model = "gpt-5.5"
model_reasoning_effort = "xhigh"

# Codex app plugins
[plugins."browser@openai-bundled"]
enabled = true

[desktop]
followUpQueueMode = "queue"
"#;

    fn config(test: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "statuspal-test-codex-config-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn reads_the_effort() {
        let path = config("read", CONFIG);
        let s = setup(&path);
        assert!(s.found);
        assert_eq!(s.effort.as_deref(), Some("xhigh"));
        assert_eq!(s.effort_before, None);
        assert_eq!(s.profiles, ProfilesState::Off);
    }

    #[test]
    fn changing_the_effort_keeps_everything_else_and_can_be_undone() {
        let path = config("effort", CONFIG);
        set_effort(&path, "medium").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            CONFIG.replace("\"xhigh\"", "\"medium\""),
            "only the effort changed, comments and all"
        );
        assert_eq!(setup(&path).effort_before.as_deref(), Some("xhigh"));

        set_effort(&path, "low").unwrap();
        undo_effort(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), CONFIG);
        assert_eq!(setup(&path).effort_before, None);
    }

    #[test]
    fn only_known_efforts() {
        let path = config("unknown", CONFIG);
        assert!(set_effort(&path, "ludicrous").is_err());
    }

    #[test]
    fn profiles_on_and_off() {
        let path = config("profiles", CONFIG);
        set_profiles(&path, true).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with(CONFIG),
            "added at the end, the rest untouched"
        );
        assert!(text.contains("[profiles.quick]\nmodel_reasoning_effort = \"low\""));
        assert!(text.contains("[profiles.deep]\nmodel_reasoning_effort = \"xhigh\""));
        assert!(!text.contains("[profiles]\n"));
        assert_eq!(setup(&path).profiles, ProfilesState::On);

        set_profiles(&path, false).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap().trim_end(),
            CONFIG.trim_end()
        );
        assert_eq!(setup(&path).profiles, ProfilesState::Off);
    }

    #[test]
    fn never_touches_your_profiles() {
        let mine = format!("{CONFIG}\n[profiles.quick]\nmodel = \"gpt-5.5-mini\"\n");
        let path = config("mine", &mine);
        assert_eq!(setup(&path).profiles, ProfilesState::Yours);
        assert!(set_profiles(&path, true).is_err());
        assert!(set_profiles(&path, false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
    }
}
