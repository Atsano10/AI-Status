//! Tells us when an agent writes new usage data, so the bar updates live instead of only when it's toggled.
//!
//! How the pieces connect:
//!
//! ```text
//! macOS (FSEvents) ──event──▶ notify's callback ──()──▶ channel ──▶ our thread: wait for quiet ──▶ on_change()
//!                             (keeps only our files)                (one save = one call)
//! ```

use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::adapters::claude::{statusline, transcripts};
use crate::adapters::{codex, gemini};

/// How long the files must stay quiet before we re-read them. One save can fire several events
/// (write the temp file, then rename it over the real one), and this turns them into one update.
const QUIET: Duration = Duration::from_millis(300);

/// Starts watching Claude's statusLine folder and chat logs and Codex's sessions folder, and calls `on_change` on a
/// background thread after each burst of changes.
///
/// Keep the returned watcher alive for as long as you want updates: when it's dropped, watching stops.
pub fn spawn(on_change: impl Fn() + Send + 'static) -> notify::Result<RecommendedWatcher> {
    // A channel is a one-way pipe between threads: `tx` (transmitter) sends, `rx` (receiver) receives.
    let (tx, rx) = mpsc::channel();

    // notify runs this closure on its own thread for every file event anywhere in the watched folders.
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res
            && event.paths.iter().any(|path| is_relevant(path))
        {
            // Only fails if our thread below has stopped, and then there's no one left to tell.
            let _ = tx.send(());
        }
    })?;

    // Claude: watch the folder, not the file. `statusline::save` replaces the file by renaming a new one
    // over it, and a watch on the old file would go stale. Create the folder so we can watch it before
    // Claude Code's first save.
    if let Some(file) = statusline::statusline_file()
        && let Some(dir) = file.parent()
    {
        std::fs::create_dir_all(dir)?;
        watcher.watch(dir, RecursiveMode::NonRecursive)?;
    }

    // Claude Code's chat logs: one folder per project, new sessions and subagents land below them.
    if let Some(dir) = transcripts::projects_dir()
        && dir.is_dir()
    {
        watcher.watch(&dir, RecursiveMode::Recursive)?;
    }

    // Gemini CLI: chat logs under ~/.gemini/tmp/<project>/chats.
    if let Some(dir) = gemini::gemini_dir().map(|d| d.join("tmp"))
        && dir.is_dir()
    {
        watcher.watch(&dir, RecursiveMode::Recursive)?;
    }

    // Codex: new sessions land in new year/month/day folders, so watch everything below. Skip it if
    // Codex isn't installed.
    if let Some(dir) = codex::sessions_dir()
        && dir.is_dir()
    {
        watcher.watch(&dir, RecursiveMode::Recursive)?;
    }

    thread::spawn(move || {
        while wait_for_burst(&rx, QUIET) {
            on_change();
        }
    });

    Ok(watcher)
}

/// True for the files we read: Claude's statusLine JSON and chat logs, and Codex's `rollout-*.jsonl` logs.
/// Everything else in those folders (the `.json.tmp` file, folders, other files) is ignored.
fn is_relevant(path: &Path) -> bool {
    path.file_name() == Some(std::ffi::OsStr::new(statusline::FILE_NAME))
        || path.file_name() == Some(std::ffi::OsStr::new(crate::classify::GUESS_FILE))
        || codex::is_rollout_file(path)
        || transcripts::is_transcript(path)
        || gemini::is_chat(path)
}

/// Waits for the next burst of messages and swallows all of it.
///
/// Blocks until at least one message arrives, then keeps taking messages until none has come for
/// `quiet`. Returns `true` if there was a burst (go re-read the files), or `false` if the channel closed
/// with nothing in it (stop watching).
fn wait_for_burst<T>(rx: &Receiver<T>, quiet: Duration) -> bool {
    if rx.recv().is_err() {
        false
    } else {
        loop {
            match rx.recv_timeout(quiet) {
                Ok(_) => continue,
                Err(RecvTimeoutError::Timeout) => return true,
                Err(RecvTimeoutError::Disconnected) => return true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn statusline_file_is_relevant() {
        let path = statusline::statusline_file().expect("home folder");
        assert!(is_relevant(&path));
    }

    #[test]
    fn statusline_temp_file_is_ignored() {
        let path = PathBuf::from("/tmp/statuspal/claude-statusline.json.tmp");
        assert!(!is_relevant(&path));
    }

    #[test]
    fn codex_rollout_is_relevant() {
        let path = PathBuf::from(
            "/Users/me/.codex/sessions/2026/10/20/rollout-2026-10-20T14-00-00-abc.jsonl",
        );
        assert!(is_relevant(&path));
    }

    #[test]
    fn other_files_and_folders_are_ignored() {
        assert!(!is_relevant(Path::new(
            "/Users/me/.codex/sessions/2026/10/20"
        )));
        assert!(!is_relevant(Path::new(
            "/Users/me/.codex/sessions/notes.jsonl"
        )));
        assert!(!is_relevant(Path::new("/Users/me/.DS_Store")));
    }

    #[test]
    fn a_burst_is_swallowed_whole() {
        let (tx, rx) = mpsc::channel();
        for _ in 0..3 {
            tx.send(()).unwrap();
        }
        assert!(wait_for_burst(&rx, Duration::from_millis(20)));
        // All three were taken, so nothing is left for a second update.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn closed_channel_with_a_burst_still_reports_it_once() {
        let (tx, rx) = mpsc::channel();
        tx.send(()).unwrap();
        drop(tx);
        assert!(wait_for_burst(&rx, Duration::from_millis(20)));
        assert!(!wait_for_burst(&rx, Duration::from_millis(20)));
    }

    #[test]
    fn closed_empty_channel_stops() {
        let (tx, rx) = mpsc::channel::<()>();
        drop(tx);
        assert!(!wait_for_burst(&rx, Duration::from_millis(20)));
    }

    #[test]
    fn bursts_separated_by_quiet_are_two_updates() {
        let (tx, rx) = mpsc::channel();
        let sender = thread::spawn(move || {
            tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(200)); // longer than `quiet` below
            tx.send(()).unwrap();
        });
        assert!(wait_for_burst(&rx, Duration::from_millis(50)));
        assert!(wait_for_burst(&rx, Duration::from_millis(50)));
        sender.join().unwrap();
        assert!(!wait_for_burst(&rx, Duration::from_millis(50)));
    }
}
