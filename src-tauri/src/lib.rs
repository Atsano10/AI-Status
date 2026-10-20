#[cfg(target_os = "macos")]
mod bar;

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
            bar::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_limits])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
