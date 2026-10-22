//! `statuspal-statusline`: the command Claude Code runs for its status line, as its own small program.
//!
//! The app can do the same job itself (`statuspal --statusline`), and that's what Settings ▸ Claude Code
//! connects. This program stays for running from a `cargo install`:
//! `cargo install --path crates/core --bin statuspal-statusline`.

fn main() {
    statuspal_core::adapters::claude::statusline::run_bridge();
}
