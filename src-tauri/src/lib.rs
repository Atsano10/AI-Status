mod active;
#[cfg(target_os = "macos")]
mod bar;
mod bridge;
#[cfg(target_os = "macos")]
mod hotkey;
mod login;
mod settings;
mod settings_window;
#[cfg(target_os = "macos")]
mod tray;
mod usage;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// The ☰ menu's "Quit StatusPal".
#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        // First, so a second copy of StatusPal quits before doing anything. Opening it again shows Settings,
        // since the bar and the menu bar icon are already there.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            settings_window::open(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::Builder::new().build());

    // The floating panel is macOS-only for now.
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .setup(|app| {
            // First: the tray and the detector read the settings as soon as they start.
            settings::setup(app.handle());
            // Before the bar, which asks for the numbers as soon as it loads.
            usage::setup(app.handle());

            #[cfg(target_os = "macos")]
            {
                // A menu bar app: no Dock icon and no app menu. Set this before building the bar.
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                bar::setup(app.handle())?;
                hotkey::setup(app.handle())?;
                tray::setup(app.handle())?;
            }

            active::setup(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            usage::get_limits,
            usage::hide_tip,
            quit,
            active::get_active_agent,
            settings::get_settings,
            settings::update_settings,
            settings_window::open_settings,
            login::get_launch_at_login,
            login::set_launch_at_login,
            bridge::get_bridge_status,
            bridge::set_bridge_connected,
            #[cfg(target_os = "macos")]
            bar::resize_bar,
            #[cfg(target_os = "macos")]
            bar::hide_bar,
            #[cfg(target_os = "macos")]
            bar::set_click_through,
            #[cfg(target_os = "macos")]
            bar::drag_bar,
            #[cfg(target_os = "macos")]
            bar::copy_text,
            #[cfg(target_os = "macos")]
            hotkey::set_hotkey,
            #[cfg(target_os = "macos")]
            hotkey::pause_hotkey
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
