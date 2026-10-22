//! The helper pack: small Claude Code subagents on cheaper models. Claude hands them simple jobs by itself
//! (finding files, explaining code, small edits, running tests), each in its own context, so your main chat
//! stays small and your limit lasts longer.
//!
//! Each one is a file in `~/.claude/agents/`, written only when you switch it on in Settings. Our files carry a
//! marker line, so we only ever change or remove files we wrote. A file of yours with the same name is left
//! alone.
//!
//! Format, from Claude Code's docs (code.claude.com/docs/en/sub-agents): a YAML header with `name` and
//! `description` (required), `tools` (comma-separated) and `model` (`haiku`, `sonnet`, `opus`, `fable`,
//! `inherit` or a full model id). Claude Code notices new files within seconds, except that the very first
//! file in a new `~/.claude/agents` folder needs one restart of Claude Code.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// In every file we write (a YAML comment, so Claude Code ignores it).
const MARKER: &str =
    "# Added by StatusPal. Turn it off in StatusPal ▸ Settings ▸ Claude Code helpers.";

/// One helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Helper {
    pub name: &'static str,
    /// `haiku` or `sonnet`: Claude Code's aliases for the current models.
    pub model: &'static str,
    pub tools: &'static str,
    /// What Claude reads to decide when to hand a job over.
    pub description: &'static str,
    pub prompt: &'static str,
    /// One line for the settings window.
    pub summary: &'static str,
}

pub const PACK: [Helper; 4] = [
    Helper {
        name: "quick-search",
        model: "haiku",
        tools: "Read, Grep, Glob",
        description: "Use for finding things in the codebase: where something is defined or used, which files \
                      handle a feature, or listing files that match a pattern. Returns file paths and line \
                      numbers. Doesn't edit anything.",
        prompt: "You find things in a codebase quickly. Search with Grep and Glob, open files with Read only \
                 when you need to confirm a match. Answer with file paths and line numbers, plus one short \
                 line on what's there. Don't explain the code at length and don't suggest changes.",
        summary: "Finds where things are",
    },
    Helper {
        name: "explainer",
        model: "haiku",
        tools: "Read, Grep, Glob",
        description: "Use to explain what a piece of code, a file or an error message does, when the answer \
                      doesn't need any changes. Reads only.",
        prompt: "You explain code clearly and briefly. Read what you need, then say what it does, how the \
                 pieces connect and anything surprising, in plain words. Quote only the lines that matter. \
                 Don't change anything.",
        summary: "Explains code and errors",
    },
    Helper {
        name: "small-edit",
        model: "sonnet",
        tools: "Read, Edit, Grep, Glob",
        description: "Use for small, well-defined edits in one or two files: a rename, a typo, a one-line fix, \
                      updating a string or a constant. Not for design work or changes across many files.",
        prompt: "You make small, precise edits. Read the file first, change exactly what was asked, match \
                 the surrounding style, and touch nothing else. If the job turns out to be bigger than a \
                 small edit, stop and say so instead of guessing.",
        summary: "Makes small, simple edits",
    },
    Helper {
        name: "test-runner",
        model: "sonnet",
        tools: "Bash, Read, Grep, Glob",
        description: "Use to run the tests, a build or a linter and report what failed, with the relevant \
                      error lines. Doesn't fix anything.",
        prompt: "You run checks and report the results. Run the command you were given (or the project's \
                 usual test command), then report what passed and what failed. For each failure give the \
                 test name, the file and line, and the few error lines that explain it. Don't fix anything \
                 and don't paste the whole log.",
        summary: "Runs tests and sums up failures",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum HelperState {
    Off,
    On,
    /// There's a file with this name that we didn't write. We leave it alone.
    Yours,
}

/// A helper as the settings window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HelperInfo {
    pub name: String,
    pub model: String,
    pub summary: String,
    pub state: HelperState,
}

/// `~/.claude/agents`.
pub fn agents_dir() -> Option<PathBuf> {
    super::config_dir().map(|dir| dir.join("agents"))
}

/// The file Claude Code reads: settings in a YAML header, then the instructions.
pub fn file_contents(helper: &Helper) -> String {
    format!(
        // The description is quoted: it has ": " in it, which YAML would otherwise read as a new key.
        "---\n{MARKER}\nname: {}\ndescription: \"{}\"\ntools: {}\nmodel: {}\n---\n\n{}\n",
        helper.name,
        helper.description.replace('"', "'"),
        helper.tools,
        helper.model,
        helper.prompt
    )
}

fn path(dir: &Path, helper: &Helper) -> PathBuf {
    dir.join(format!("{}.md", helper.name))
}

pub fn state(dir: &Path, helper: &Helper) -> HelperState {
    match fs::read_to_string(path(dir, helper)) {
        Ok(text) if text.contains(MARKER) => HelperState::On,
        Ok(_) => HelperState::Yours,
        Err(_) => HelperState::Off,
    }
}

/// Every helper in the pack and whether it's on.
pub fn list(dir: &Path) -> Vec<HelperInfo> {
    PACK.iter()
        .map(|helper| HelperInfo {
            name: helper.name.into(),
            model: helper.model.into(),
            summary: helper.summary.into(),
            state: state(dir, helper),
        })
        .collect()
}

/// Turns a helper on (writes its file, or updates ours to the current version) or off (deletes our file).
/// A file that isn't ours is never touched.
pub fn set(dir: &Path, name: &str, on: bool) -> io::Result<()> {
    let helper = PACK
        .iter()
        .find(|helper| helper.name == name)
        .ok_or_else(|| io::Error::other(format!("no helper called {name}")))?;
    if state(dir, helper) == HelperState::Yours {
        return Err(io::Error::other(format!(
            "you already have your own {name} agent, so StatusPal leaves it alone"
        )));
    }
    let file = path(dir, helper);
    if on {
        fs::create_dir_all(dir)?;
        let tmp = file.with_extension("md.statuspal-tmp");
        fs::write(&tmp, file_contents(helper))?;
        fs::rename(&tmp, &file)
    } else {
        match fs::remove_file(&file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "statuspal-test-agents-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_file_claude_code_reads() {
        let text = file_contents(&PACK[0]);
        assert!(text.starts_with("---\n# Added by StatusPal"));
        assert!(text.contains("\nname: quick-search\n"));
        assert!(text.contains("\ndescription: \"Use for finding things in the codebase: "));
        assert!(text.contains("\ntools: Read, Grep, Glob\nmodel: haiku\n---\n\nYou find things"));
    }

    #[test]
    fn on_and_off() {
        let dir = temp_dir("on-off");
        assert_eq!(state(&dir, &PACK[0]), HelperState::Off);
        set(&dir, "quick-search", true).unwrap();
        assert_eq!(state(&dir, &PACK[0]), HelperState::On);
        set(&dir, "quick-search", false).unwrap();
        assert_eq!(state(&dir, &PACK[0]), HelperState::Off);
        assert!(
            set(&dir, "quick-search", false).is_ok(),
            "already off is fine"
        );
    }

    #[test]
    fn never_touches_your_own_agent() {
        let dir = temp_dir("yours");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("explainer.md"), "---\nname: explainer\n---\nmine").unwrap();
        assert_eq!(state(&dir, &PACK[1]), HelperState::Yours);
        assert!(set(&dir, "explainer", true).is_err());
        assert!(set(&dir, "explainer", false).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("explainer.md")).unwrap(),
            "---\nname: explainer\n---\nmine"
        );
    }

    #[test]
    fn an_older_version_of_ours_gets_updated() {
        let dir = temp_dir("update");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("small-edit.md"),
            format!("---\n{MARKER}\nname: small-edit\n---\nold"),
        )
        .unwrap();
        set(&dir, "small-edit", true).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("small-edit.md")).unwrap(),
            file_contents(&PACK[2])
        );
    }
}
