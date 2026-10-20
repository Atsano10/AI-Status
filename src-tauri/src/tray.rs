//! The 🐾 in the menu bar. With no Dock icon and no main window, it's the only always-visible part of the app.

use tauri::AppHandle;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

use crate::bar;

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    // Each item gets an id ("toggle", "quit") so the click handler can tell them apart.
    let toggle = MenuItem::with_id(app, "toggle", "Show/Hide Bar   ⌃⌥Space", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit StatusPal", true, Some("cmd+q"))?;
    let menu = Menu::with_items(app, &[&toggle, &quit])?;

    TrayIconBuilder::with_id("statuspal")
        .title("🐾")
        .tooltip("StatusPal")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => bar::toggle_bar(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
