//! The StatusPal logo in the menu bar. With no Dock icon and no main window, it's the only always-visible part of the app.

use statuspal_core::model::AgentId;
use statuspal_core::settings::Settings;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Listener, Wry};

use crate::{bar, settings, settings_window};

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    // Each item gets an id ("toggle", "quit") so the click handler can tell them apart.
    let toggle = MenuItem::with_id(app, "toggle", "Show/Hide Bar   ⌃⌥Space", true, None::<&str>)?;

    // "Pin agent ▸ Auto / Claude / Codex". Exactly one has a checkmark.
    let pins = PinItems {
        auto: CheckMenuItem::with_id(app, "pin-auto", "Auto", true, false, None::<&str>)?,
        claude: CheckMenuItem::with_id(app, "pin-claude", "Claude", true, false, None::<&str>)?,
        codex: CheckMenuItem::with_id(app, "pin-codex", "Codex", true, false, None::<&str>)?,
    };
    pins.show(settings::get(app).pinned);
    let pin_menu = Submenu::with_items(
        app,
        "Pin agent",
        true,
        &[&pins.auto, &pins.claude, &pins.codex],
    )?;

    let open_settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("cmd+,"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit StatusPal", true, Some("cmd+q"))?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&toggle, &pin_menu, &separator, &open_settings, &quit],
    )?;

    // The settings window can change the pin too, so follow every change, not just our own clicks.
    let followed = pins.clone();
    app.listen_any("settings-changed", move |event| {
        if let Ok(new) = serde_json::from_str::<Settings>(event.payload()) {
            followed.show(new.pinned);
        }
    });

    TrayIconBuilder::with_id("statuspal")
        // 36×36 (18pt @2x) black-on-clear pixel pal, drawn from icons/tray.svg. As a "template" image,
        // macOS recolors it to match the menu bar (black on light, white on dark).
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip("StatusPal")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "toggle" => bar::toggle_bar(app),
            "settings" => settings_window::open(app),
            "quit" => app.exit(0),
            id @ ("pin-auto" | "pin-claude" | "pin-codex") => {
                settings::set_pinned(app, pin_for(id));
                // macOS flips a checkmark on its own when clicked. Clicking the pin you already have changes
                // nothing (so no event), so put the checkmarks right here too.
                pins.show(settings::get(app).pinned);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[derive(Clone)]
struct PinItems {
    auto: CheckMenuItem<Wry>,
    claude: CheckMenuItem<Wry>,
    codex: CheckMenuItem<Wry>,
}

impl PinItems {
    /// Checks the item for `pinned` and unchecks the other two.
    fn show(&self, pinned: Option<AgentId>) {
        let _ = self.auto.set_checked(pinned.is_none());
        let _ = self.claude.set_checked(pinned == Some(AgentId::Claude));
        let _ = self.codex.set_checked(pinned == Some(AgentId::Codex));
    }
}

fn pin_for(id: &str) -> Option<AgentId> {
    match id {
        "pin-claude" => Some(AgentId::Claude),
        "pin-codex" => Some(AgentId::Codex),
        _ => None,
    }
}
