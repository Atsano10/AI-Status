//! The StatusPal logo in the menu bar. With no Dock icon and no main window, it's the only always-visible part of the app.

use statuspal_core::model::AgentId;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::active::PinnedAgent;
use crate::bar;

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    // Each item gets an id ("toggle", "quit") so the click handler can tell them apart.
    let toggle = MenuItem::with_id(app, "toggle", "Show/Hide Bar   ⌃⌥Space", true, None::<&str>)?;

    // "Pin agent ▸ Auto / Claude / Codex". Exactly one has a checkmark; Auto is on at launch.
    let auto = CheckMenuItem::with_id(app, "pin-auto", "Auto", true, true, None::<&str>)?;
    let claude = CheckMenuItem::with_id(app, "pin-claude", "Claude", true, false, None::<&str>)?;
    let codex = CheckMenuItem::with_id(app, "pin-codex", "Codex", true, false, None::<&str>)?;
    let pin_menu = Submenu::with_items(app, "Pin agent", true, &[&auto, &claude, &codex])?;

    let quit = MenuItem::with_id(app, "quit", "Quit StatusPal", true, Some("cmd+q"))?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&toggle, &pin_menu, &separator, &quit])?;

    TrayIconBuilder::with_id("statuspal")
        // 36×36 (18pt @2x) black-on-clear pixel pal, drawn from icons/tray.svg. As a "template" image,
        // macOS recolors it to match the menu bar (black on light, white on dark).
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip("StatusPal")
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            match id {
                "toggle" => bar::toggle_bar(app),
                "quit" => app.exit(0),
                "pin-auto" => app.state::<PinnedAgent>().set(None),
                "pin-claude" => app.state::<PinnedAgent>().set(Some(AgentId::Claude)),
                "pin-codex" => app.state::<PinnedAgent>().set(Some(AgentId::Codex)),
                _ => {}
            }

            // macOS flips a checkmark on its own when you click it, so set all three ourselves to keep
            // exactly one checked.
            if id.starts_with("pin-") {
                let _ = auto.set_checked(id == "pin-auto");
                let _ = claude.set_checked(id == "pin-claude");
                let _ = codex.set_checked(id == "pin-codex");
            }
        })
        .build(app)?;
    Ok(())
}
