//! The settings window: a normal window, opened from the tray or the bar's ☰ menu.

use tauri::{AppHandle, Manager, Theme, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "settings";

pub fn open(app: &AppHandle) {
    // Already open: bring it to the front instead of opening a second one.
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }

    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
        .title("StatusPal Settings")
        // Fits a 13" MacBook screen; the page scrolls if hidden tips or Peek add a row.
        .inner_size(480.0, 800.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .center()
        // Dark until the page has loaded Settings ▸ Look, which then sets the title bar (appearance.ts).
        .theme(Some(Theme::Dark))
        .build();
    match built {
        // StatusPal has no Dock icon, so macOS won't bring the window forward by itself.
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("can't open the settings window: {e}"),
    }
}

/// The ☰ menu's "Settings…". `async` so it runs off the main thread: building a window from a command on
/// the main thread can deadlock.
#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    open(&app);
}
