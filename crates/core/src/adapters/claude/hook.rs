//! Connects the prompt classifier (`classify.rs`) to Claude Code: a `UserPromptSubmit` hook in
//! `~/.claude/settings.json` that runs `statuspal --prompt-hook` on every prompt you send.
//!
//! Format, from Claude Code's docs (code.claude.com/docs/en/hooks):
//! `"hooks": {"UserPromptSubmit": [{"hooks": [{"type": "command", "command": "…", "timeout": 5}]}]}`.
//! Hooks you already have stay: ours is added next to them, and Remove takes out only ours. Claude Code
//! reloads hooks by itself. A hook that prints nothing changes nothing about your prompt; one that fails or
//! times out is skipped and the prompt goes through.
//!
//! Like the bridge, this only runs when you click Connect or Remove in Settings, with a backup first.

use std::io;
use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use super::bridge;

/// Seconds Claude Code waits for the hook. It takes milliseconds; this only matters if something's wrong.
const TIMEOUT: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum HookStatus {
    Off,
    On,
    /// Connected, but the program it runs isn't there anymore (the app moved, or a dev build).
    Broken,
}

pub fn is_ours(command: &str) -> bool {
    command.contains("--prompt-hook") && command.to_lowercase().contains("statuspal")
}

/// Every command in the `UserPromptSubmit` hooks.
fn commands(settings: &Value) -> impl Iterator<Item = &str> {
    settings
        .pointer("/hooks/UserPromptSubmit")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("hooks")?.as_array())
        .flatten()
        .filter_map(|handler| handler.get("command")?.as_str())
}

pub fn status(settings: &Value) -> HookStatus {
    match commands(settings).find(|c| is_ours(c)) {
        None => HookStatus::Off,
        Some(command) => match bridge::program(command) {
            Some(path) if !Path::new(&path).exists() => HookStatus::Broken,
            _ => HookStatus::On,
        },
    }
}

/// Adds our hook (an old one of ours is replaced, so a moved app reconnects).
pub fn connect(settings: &mut Value, ours: &str) {
    disconnect(settings);
    if !settings.is_object() {
        *settings = json!({});
    }
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }
    let groups = &mut settings["hooks"]["UserPromptSubmit"];
    if !groups.is_array() {
        *groups = json!([]);
    }
    if let Some(groups) = groups.as_array_mut() {
        groups.push(json!({"hooks": [{"type": "command", "command": ours, "timeout": TIMEOUT}]}));
    }
}

/// Takes out our hook and anything left empty by that, and nothing else.
pub fn disconnect(settings: &mut Value) {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    if let Some(groups) = hooks
        .get_mut("UserPromptSubmit")
        .and_then(Value::as_array_mut)
    {
        for group in groups.iter_mut() {
            if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                handlers.retain(|h| {
                    !h.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(is_ours)
                });
            }
        }
        groups.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|h| !h.is_empty())
        });
        if groups.is_empty() {
            hooks.remove("UserPromptSubmit");
        }
    }
    if hooks.is_empty()
        && let Some(fields) = settings.as_object_mut()
    {
        fields.remove("hooks");
    }
}

/// Connects or removes, with a backup of `settings.json` first.
pub fn set_connected(settings_path: &Path, ours: &str, on: bool) -> io::Result<HookStatus> {
    let mut settings = bridge::read_settings(settings_path)?;
    let before = status(&settings);
    if (on && before == HookStatus::On) || (!on && before == HookStatus::Off) {
        return Ok(before);
    }
    if settings_path.exists() {
        std::fs::copy(
            settings_path,
            settings_path.with_extension("json.statuspal-backup"),
        )?;
    }
    if on {
        connect(&mut settings, ours);
    } else {
        disconnect(&mut settings);
    }
    bridge::write_json(settings_path, &settings)?;
    Ok(status(&settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = "'/Applications/StatusPal.app/Contents/MacOS/statuspal' --prompt-hook";

    #[test]
    fn connect_into_empty_settings() {
        let mut settings = json!({"model": "opus"});
        connect(&mut settings, OURS);
        assert_eq!(
            settings,
            json!({"model": "opus", "hooks": {"UserPromptSubmit": [{"hooks": [{"type": "command", "command": OURS, "timeout": 5}]}]}})
        );
    }

    #[test]
    fn your_hooks_stay() {
        let yours = json!({"hooks": [{"type": "command", "command": "~/bin/log-prompt.sh"}]});
        let other =
            json!({"matcher": "Bash", "hooks": [{"type": "command", "command": "lint.sh"}]});
        let mut settings =
            json!({"hooks": {"UserPromptSubmit": [yours.clone()], "PreToolUse": [other.clone()]}});
        connect(&mut settings, OURS);
        assert_eq!(
            settings["hooks"]["UserPromptSubmit"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(settings["hooks"]["UserPromptSubmit"][0], yours);

        disconnect(&mut settings);
        assert_eq!(
            settings,
            json!({"hooks": {"UserPromptSubmit": [yours], "PreToolUse": [other]}})
        );
    }

    #[test]
    fn remove_leaves_nothing_behind() {
        let mut settings = json!({"model": "opus"});
        connect(&mut settings, OURS);
        disconnect(&mut settings);
        assert_eq!(settings, json!({"model": "opus"}));
    }

    #[test]
    fn reconnecting_replaces_ours_instead_of_adding_twice() {
        let mut settings = json!({});
        connect(&mut settings, "'/old/place/statuspal' --prompt-hook");
        connect(&mut settings, OURS);
        assert_eq!(commands(&settings).collect::<Vec<_>>(), [OURS]);
    }

    #[test]
    fn status_of_each_case() {
        assert_eq!(status(&json!({})), HookStatus::Off);
        let mut moved = json!({});
        connect(&mut moved, "'/nowhere/statuspal' --prompt-hook");
        assert_eq!(status(&moved), HookStatus::Broken);
    }
}
