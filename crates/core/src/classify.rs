//! Guesses how big a job a prompt is (quick / normal / deep), on your Mac, from simple signs: how long it is,
//! words like "typo" or "refactor", a pasted error. It powers the "looks quick, Sonnet would do" tip.
//!
//! Claude Code runs it on every prompt you send, as a `UserPromptSubmit` hook (`statuspal --prompt-hook`,
//! connected in Settings ▸ Claude Code). The hook prints nothing, so Claude sees nothing from it and it costs
//! no tokens. It saves only the guess, never your prompt: `prompt-guess.json` in StatusPal's folder.

use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const GUESS_FILE: &str = "prompt-guess.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum PromptClass {
    Quick,
    Normal,
    Deep,
}

/// The guess for your latest prompt. No text from the prompt is in here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PromptGuess {
    /// When you sent it (Unix seconds).
    #[cfg_attr(test, ts(type = "number"))]
    pub at: i64,
    pub session_id: String,
    pub class: PromptClass,
    /// 0 to 1: how sure the guess is.
    pub confidence: f64,
    /// Why, in a few words each, e.g. "short", "says \"rename\"".
    pub reasons: Vec<String>,
}

/// Words that point to a small job, and to a big one. Matched in lowercase, anywhere in the prompt.
const QUICK_WORDS: &[&str] = &[
    "typo",
    "rename",
    "spelling",
    "lint",
    "bump",
    "comment",
    "docstring",
    "readme",
    "what does",
    "where is",
    "where's",
    "which file",
    "find ",
    "list ",
    "explain",
    "one line",
    "one-line",
    "tweak",
    "colour",
    "color",
    "padding",
    "margin",
    "small change",
    "quick",
];
const DEEP_WORDS: &[&str] = &[
    "design",
    "architect",
    "refactor",
    "migrate",
    "implement",
    "rewrite",
    "debug",
    "why does",
    "why is",
    "doesn't work",
    "not working",
    "crash",
    "performance",
    "optimi",
    "plan ",
    "across",
    "entire",
    "whole",
    "security",
    "race condition",
    "from scratch",
    "new feature",
    "phase",
];
/// Signs of a pasted error or stack trace.
const ERROR_SIGNS: &[&str] = &[
    "traceback",
    "panicked at",
    "error[",
    "exception",
    "    at ",
    "stack trace",
];

/// Quick or deep, how sure, and why.
pub fn classify(prompt: &str) -> (PromptClass, f64, Vec<String>) {
    let text = prompt.to_lowercase();
    let words = text.split_whitespace().count();
    let mut score: i32 = 0;
    let mut reasons = Vec::new();

    if words <= 12 {
        score += 2;
        reasons.push("very short".to_string());
    } else if words <= 30 {
        score += 1;
        reasons.push("short".to_string());
    } else if words > 300 {
        score -= 3;
        reasons.push("very long".to_string());
    } else if words > 120 {
        score -= 2;
        reasons.push("long".to_string());
    }
    for word in QUICK_WORDS.iter().filter(|w| text.contains(*w)).take(2) {
        score += 2;
        reasons.push(format!("says \"{}\"", word.trim()));
    }
    for word in DEEP_WORDS.iter().filter(|w| text.contains(*w)).take(2) {
        score -= 2;
        reasons.push(format!("says \"{}\"", word.trim()));
    }
    if ERROR_SIGNS.iter().any(|s| text.contains(s)) {
        score -= 2;
        reasons.push("has an error in it".to_string());
    }
    let paths = text
        .split_whitespace()
        .filter(|w| w.contains('/') && w.contains('.'))
        .count();
    if paths >= 3 {
        score -= 1;
        reasons.push(format!("{paths} files"));
    }

    let class = if score >= 3 {
        PromptClass::Quick
    } else if score <= -2 {
        PromptClass::Deep
    } else {
        PromptClass::Normal
    };
    let confidence = match class {
        PromptClass::Normal => 0.5,
        _ => (score.abs() as f64 / 5.0).min(1.0),
    };
    (class, confidence, reasons)
}

pub fn guess_file() -> Option<PathBuf> {
    crate::app_dir().map(|dir| dir.join(GUESS_FILE))
}

pub fn read(path: &Path) -> Option<PromptGuess> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// What Claude Code runs for every prompt (`statuspal --prompt-hook`). It reads the hook's JSON from stdin,
/// saves the guess, and prints nothing (anything printed would be added to your prompt). It never fails
/// loudly: if anything goes wrong it just does nothing, so your prompt goes through as normal.
pub fn run_hook() {
    #[derive(Deserialize)]
    struct HookInput {
        prompt: Option<String>,
        session_id: Option<String>,
    }
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(hook) = serde_json::from_str::<HookInput>(&input) else {
        return;
    };
    let Some(prompt) = hook.prompt else { return };
    let (class, confidence, reasons) = classify(&prompt);
    let guess = PromptGuess {
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64),
        session_id: hook.session_id.unwrap_or_default(),
        class,
        confidence,
        reasons,
    };
    if let (Some(path), Ok(json)) = (guess_file(), serde_json::to_string(&guess)) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(prompt: &str) -> PromptClass {
        classify(prompt).0
    }

    #[test]
    fn small_jobs_look_quick() {
        assert_eq!(class("fix the typo in the readme"), PromptClass::Quick);
        assert_eq!(class("rename getUser to fetchUser"), PromptClass::Quick);
        assert_eq!(class("where is the hotkey registered?"), PromptClass::Quick);
        let (_, confidence, reasons) = classify("fix the typo in the readme");
        assert!(confidence >= 0.7, "{confidence}");
        assert!(reasons.contains(&"says \"typo\"".to_string()));
    }

    #[test]
    fn big_jobs_look_deep() {
        assert_eq!(
            class(
                "refactor the settings so every window shares one store and migrate the old file format"
            ),
            PromptClass::Deep
        );
        assert_eq!(
            class(
                "the bar doesn't work after the update, here's the log: thread 'main' panicked at src/lib.rs:12"
            ),
            PromptClass::Deep
        );
    }

    #[test]
    fn in_between_is_normal() {
        assert_eq!(
            class(
                "add a setting for the peek duration with three choices and show it in the settings window under the behaviour row"
            ),
            PromptClass::Normal
        );
    }

    #[test]
    fn a_long_prompt_is_never_quick() {
        let long = "please update this ".repeat(60) + "typo";
        assert_ne!(class(&long), PromptClass::Quick);
    }

    #[test]
    fn the_guess_has_no_prompt_text() {
        let (class, confidence, reasons) = classify("rename my secret project");
        let guess = PromptGuess {
            at: 1,
            session_id: "s".into(),
            class,
            confidence,
            reasons,
        };
        let json = serde_json::to_string(&guess).unwrap();
        assert!(!json.contains("secret"), "{json}");
    }
}
