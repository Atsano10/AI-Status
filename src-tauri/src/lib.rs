use tauri::{Emitter, Manager};

mod active;
#[cfg(target_os = "macos")]
mod bar;
#[cfg(target_os = "macos")]
mod tray;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_limits() -> Vec<statuspal_core::model::UsageSnapshot> {
    statuspal_core::snapshot::current_snapshots()
}

/// The ☰ menu's "Quit StatusPal".
#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    // The floating panel is macOS-only for now.
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                // A menu bar app: no Dock icon and no app menu. Set this before building the bar.
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                bar::setup(app.handle())?;
                tray::setup(app.handle())?;
            }

            active::setup(app.handle());

            // Send fresh numbers to the bar whenever Claude or Codex writes new usage data.
            let handle = app.handle().clone();
            match statuspal_core::watch::spawn(move || {
                let _ = handle.emit(
                    "limits-updated",
                    statuspal_core::snapshot::current_snapshots(),
                );
            }) {
                // Tauri keeps managed values alive until the app quits. Without this, `watcher` would be
                // dropped at the end of `setup` and watching would stop right away.
                Ok(watcher) => {
                    app.manage(watcher);
                }
                Err(e) => {
                    eprintln!("can't watch usage files, the bar only updates when shown: {e}")
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_limits,
            quit,
            active::get_active_agent,
            #[cfg(target_os = "macos")]
            bar::resize_bar,
            #[cfg(target_os = "macos")]
            bar::hide_bar,
            #[cfg(target_os = "macos")]
            bar::set_click_through
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
