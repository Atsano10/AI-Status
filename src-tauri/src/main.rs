// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Claude Code runs us as its status line (`statuspal --statusline`, set up in Settings ▸ Claude Code):
    // save what it sends, print one line, and exit, without starting the app.
    if std::env::args().nth(1).as_deref() == Some("--statusline") {
        statuspal_core::adapters::claude::statusline::run_bridge();
        return;
    }
    statuspal_lib::run()
}
