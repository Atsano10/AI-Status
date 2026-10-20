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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_limits])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
