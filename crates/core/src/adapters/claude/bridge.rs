//! Connects StatusPal to Claude Code: sets our program as the `statusLine` command in `~/.claude/settings.json`,
//! so Claude Code hands us its limit % after every reply (see `statusline.rs`).
//!
//! Only ever runs when you click Connect or Remove in the settings window. Before every change we copy
//! `settings.json` to `settings.json.statuspal-backup`. If you already had a status line, we remember it in
//! `statusline-chain.json` and run it for you (its output is what you keep seeing), and Remove puts it back.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::{Map, Value, json};

/// Where we remember the status line you had before (inside StatusPal's own folder).
pub const CHAIN_FILE: &str = "statusline-chain.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum BridgeStatus {
    /// Claude Code has no status line.
    NotConnected,
    /// Claude Code runs someone else's status line. Connecting keeps it (we run it for you).
    OtherStatusLine,
    /// Claude Code runs ours.
    Connected,
    /// Claude Code is set to run ours, but that program isn't there anymore (the app moved, or a dev build).
    Broken,
}

/// `~/.claude/settings.json`, or the folder in `CLAUDE_CONFIG_DIR` if you set one.
pub fn claude_settings_file() -> Option<PathBuf> {
    let dir = match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => std::env::home_dir()?.join(".claude"),
    };
    Some(dir.join("settings.json"))
}

pub fn chain_file() -> Option<PathBuf> {
    crate::app_dir().map(|dir| dir.join(CHAIN_FILE))
}

/// Whether a statusLine command is ours: the app run with `--statusline`, or the `statuspal-statusline`
/// program from `cargo install`.
pub fn is_ours(command: &str) -> bool {
    command.contains("statuspal-statusline")
        || (command.contains("--statusline") && command.to_lowercase().contains("statuspal"))
}

pub fn status(settings: &Value) -> BridgeStatus {
    match settings
        .pointer("/statusLine/command")
        .and_then(Value::as_str)
    {
        None => BridgeStatus::NotConnected,
        Some(command) if !is_ours(command) => BridgeStatus::OtherStatusLine,
        Some(command) => match program(command) {
            Some(path) if !Path::new(&path).exists() => BridgeStatus::Broken,
            _ => BridgeStatus::Connected,
        },
    }
}

/// The program a shell command runs: `'/path/with spaces/x' --flag` → `/path/with spaces/x`.
fn program(command: &str) -> Option<String> {
    let command = command.trim();
    let path = match command.strip_prefix('\'') {
        Some(rest) => rest.split('\'').next()?,
        None => command.split_whitespace().next()?,
    };
    path.starts_with('/').then(|| path.to_string())
}

/// Points `settings.statusLine` at `ours`. Returns the status line it replaces (to run it and to put it back
/// later), or `None` if there wasn't one. Other fields of your status line (like `padding`) are kept.
pub fn connect(settings: &mut Value, ours: &str) -> Option<Value> {
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let previous = settings
        .get("statusLine")
        .filter(|line| {
            !line
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(is_ours)
        })
        .cloned();

    let mut line = previous
        .clone()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    line["type"] = json!("command");
    line["command"] = json!(ours);
    settings["statusLine"] = line;
    previous
}

/// Undoes `connect`: puts back the status line you had before, or removes ours if there wasn't one.
/// Leaves a status line that isn't ours alone.
pub fn disconnect(settings: &mut Value, previous: Option<Value>) {
    let ours = settings
        .pointer("/statusLine/command")
        .and_then(Value::as_str)
        .is_some_and(is_ours);
    let Some(fields) = settings.as_object_mut().filter(|_| ours) else {
        return;
    };
    match previous {
        Some(line) => {
            fields.insert("statusLine".into(), line);
        }
        None => {
            fields.remove("statusLine");
        }
    }
}

/// Reads Claude Code's settings. A missing file is `{}` (Claude Code makes one when you change a setting).
pub fn read_settings(path: &Path) -> io::Result<Value> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e),
    }
}

/// Connects or disconnects, with a backup first. `chain` is where the previous status line is remembered.
pub fn set_connected(
    settings_path: &Path,
    chain: &Path,
    ours: &str,
    connected: bool,
) -> io::Result<BridgeStatus> {
    // A file we can't read is left exactly as it is.
    let mut settings = read_settings(settings_path)?;
    let before = status(&settings);
    let nothing_to_do = if connected {
        before == BridgeStatus::Connected
    } else {
        !matches!(before, BridgeStatus::Connected | BridgeStatus::Broken)
    };
    if nothing_to_do {
        return Ok(before);
    }

    if settings_path.exists() {
        fs::copy(
            settings_path,
            settings_path.with_extension("json.statuspal-backup"),
        )?;
    }
    if connected {
        match connect(&mut settings, ours) {
            Some(previous) => write_json(chain, &previous)?,
            // Reconnecting a Broken bridge: the status line we replaced back then is still in the chain file.
            None if before == BridgeStatus::Broken => {}
            None => remove_if_there(chain)?,
        }
    } else {
        let previous = fs::read_to_string(chain)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        disconnect(&mut settings, previous);
        remove_if_there(chain)?;
    }
    write_json(settings_path, &settings)?;
    Ok(status(&settings))
}

fn remove_if_there(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Pretty JSON (2 spaces, like Claude Code writes it, keys in their original order), written via a temporary
/// file so a crash can't leave half a file.
fn write_json(path: &Path, value: &Value) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    let tmp = path.with_extension("json.statuspal-tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

/// The status line you had before StatusPal: run it with the same JSON Claude Code gave us, and return what
/// it printed. `None` if there's none or it failed, so we print our own line instead.
pub fn run_chained(stdin_json: &str) -> Option<String> {
    let line: Value = serde_json::from_str(&fs::read_to_string(chain_file()?).ok()?).ok()?;
    let command = line.get("command")?.as_str()?;
    if is_ours(command) {
        return None; // never run ourselves in a loop
    }

    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // A command that doesn't read its stdin closes it early; that's fine.
    let _ = child.stdin.take()?.write_all(stdin_json.as_bytes());
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string();
    (output.status.success() && !text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = "'/Applications/StatusPal.app/Contents/MacOS/statuspal' --statusline";

    #[test]
    fn recognises_our_commands() {
        assert!(is_ours(OURS));
        assert!(is_ours("/Users/me/.cargo/bin/statuspal-statusline"));
        assert!(!is_ours("npx ccstatusline"));
        assert!(!is_ours("~/bin/my-line.sh --statusline"));
    }

    #[test]
    fn status_of_each_case() {
        assert_eq!(status(&json!({})), BridgeStatus::NotConnected);
        assert_eq!(
            status(&json!({"statusLine": {"type": "command", "command": "npx ccstatusline"}})),
            BridgeStatus::OtherStatusLine
        );
        assert_eq!(
            status(&json!({"statusLine": {"command": "/nowhere/statuspal --statusline"}})),
            BridgeStatus::Broken
        );
        assert_eq!(
            status(&json!({"statusLine": {"command": "statuspal-statusline"}})),
            BridgeStatus::Connected
        );
    }

    #[test]
    fn program_handles_quotes() {
        assert_eq!(
            program("'/My Apps/StatusPal.app/x' --statusline").as_deref(),
            Some("/My Apps/StatusPal.app/x")
        );
        assert_eq!(program("/usr/bin/x --y").as_deref(), Some("/usr/bin/x"));
        assert_eq!(program("npx thing"), None);
    }

    #[test]
    fn connect_with_no_status_line() {
        let mut settings = json!({"model": "opus"});
        assert_eq!(connect(&mut settings, OURS), None);
        assert_eq!(
            settings,
            json!({"model": "opus", "statusLine": {"type": "command", "command": OURS}})
        );
    }

    #[test]
    fn connect_keeps_yours_to_chain_and_keeps_its_fields() {
        let theirs = json!({"type": "command", "command": "npx ccstatusline", "padding": 0});
        let mut settings = json!({"statusLine": theirs.clone()});
        assert_eq!(connect(&mut settings, OURS), Some(theirs));
        assert_eq!(
            settings["statusLine"],
            json!({"type": "command", "command": OURS, "padding": 0})
        );
    }

    #[test]
    fn disconnect_puts_back_what_was_there() {
        let theirs = json!({"type": "command", "command": "npx ccstatusline"});
        let mut settings = json!({"model": "opus", "statusLine": theirs.clone()});
        let previous = connect(&mut settings, OURS);
        disconnect(&mut settings, previous);
        assert_eq!(settings, json!({"model": "opus", "statusLine": theirs}));

        let mut settings = json!({"model": "opus"});
        connect(&mut settings, OURS);
        disconnect(&mut settings, None);
        assert_eq!(settings, json!({"model": "opus"}));
    }

    #[test]
    fn disconnect_leaves_other_status_lines_alone() {
        let mut settings = json!({"statusLine": {"command": "npx ccstatusline"}});
        disconnect(&mut settings, None);
        assert_eq!(
            settings,
            json!({"statusLine": {"command": "npx ccstatusline"}})
        );
    }

    #[test]
    fn set_connected_round_trip_with_backup() {
        let dir =
            std::env::temp_dir().join(format!("statuspal-test-bridge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let settings_path = dir.join("settings.json");
        let chain = dir.join(CHAIN_FILE);
        let original = "{\n  \"zeta\": 1,\n  \"alpha\": 2,\n  \"statusLine\": {\"type\": \"command\", \"command\": \"npx ccstatusline\"}\n}\n";
        fs::write(&settings_path, original).unwrap();

        set_connected(&settings_path, &chain, "statuspal-statusline", true).unwrap();
        assert!(dir.join("settings.json.statuspal-backup").exists());
        assert!(chain.exists());
        let connected = fs::read_to_string(&settings_path).unwrap();
        assert!(
            connected.find("zeta") < connected.find("alpha"),
            "key order kept"
        );

        set_connected(&settings_path, &chain, "statuspal-statusline", false).unwrap();
        let back = read_settings(&settings_path).unwrap();
        assert_eq!(back, serde_json::from_str::<Value>(original).unwrap());
        assert!(!chain.exists());
    }
}
