//! Phase 0 spike: a floating bar that never steals focus, toggled by a global hotkey.

use tauri::{AppHandle, WebviewUrl};
use tauri_nspanel::{
    tauri_panel, CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const BAR_LABEL: &str = "bar";

// An NSPanel class that can't become the key window, so typing stays in whatever app you're using.
tauri_panel! {
    panel!(BarPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    create_bar(app)?;

    // ⌃⌥Space, the candidate macOS default from the plan.
    let hotkey = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, shortcut, event| {
                if shortcut == &hotkey && event.state() == ShortcutState::Pressed {
                    toggle_bar(app);
                }
            })
            .build(),
    )?;

    // Registration fails if another app already owns the combo. Report it instead of crashing.
    match app.global_shortcut().register(hotkey) {
        Ok(()) => println!("hotkey ⌃⌥Space registered"),
        Err(e) => eprintln!("hotkey ⌃⌥Space is taken: {e}"),
    }
    Ok(())
}

fn create_bar(app: &AppHandle) -> tauri::Result<()> {
    PanelBuilder::<_, BarPanel>::new(app, BAR_LABEL)
        .url(WebviewUrl::App("bar.html".into()))
        .size(tauri::Size::Logical(tauri::LogicalSize::new(360.0, 48.0)))
        // Status level sits above normal and fullscreen windows.
        .level(PanelLevel::Status)
        .style_mask(StyleMask::empty().borderless().nonactivating_panel())
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces() // every desktop (Space)
                .full_screen_auxiliary() // allowed on top of fullscreen apps
                .stationary() // stays put during Mission Control
                .ignores_cycle(), // not in ⌘` window cycling
        )
        .transparent(true)
        .has_shadow(true)
        .corner_radius(14.0)
        .hides_on_deactivate(false)
        .no_activate(true)
        .with_window(|w| w.decorations(false).transparent(true).visible(false).center())
        .build()?;
    Ok(())
}

fn toggle_bar(app: &AppHandle) {
    let Ok(panel) = app.get_webview_panel(BAR_LABEL) else { return };
    
    if panel.is_visible(){
        panel.hide();
    }else{
        panel.show();
    }
}
