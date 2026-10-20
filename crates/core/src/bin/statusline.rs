//! `statuspal-statusline`: the command Claude Code runs for its status line.
//!
//! Claude Code pipes a JSON blob to our stdin after every reply. We save it for the StatusPal app to read,
//! then print one line (`Opus 5.5 · 5h 8% (3h 12m)`) that Claude Code shows under its prompt.
//!
//! Install it with `cargo install --path crates/core --bin statuspal-statusline`.
//! It must never crash or hang: if anything fails we still print a line, so Claude Code's status line keeps working.

use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use statuspal_core::adapters::claude::statusline;

fn main() {
    let mut contents = String::new();
    if std::io::stdin().read_to_string(&mut contents).is_err() {
        println!("StatusPal: couldn't read stdin");
        return;
    }

    // eprintln goes to stderr, which Claude Code ignores, so a failed save doesn't clutter the status line.
    match statusline::statusline_file() {
        Some(path) => {
            if let Err(e) = statusline::save(&path, &contents) {
                eprintln!("StatusPal: couldn't save {}: {e}", path.display());
            }
        }
        None => eprintln!("StatusPal: can't find your home folder"),
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    println!("{}", statusline::summary(&contents, now));
}
