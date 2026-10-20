//! The floating bar: a panel that never steals focus, toggled by a global hotkey.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl};
use tauri_nspanel::{
    CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask, tauri_panel,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const BAR_LABEL: &str = "bar";

/// Starting size: a horizontal pill. The page resizes the window to fit as soon as it draws (`resize_bar`).
const START_WIDTH: f64 = 218.0;
const START_HEIGHT: f64 = 32.0;

/// How often we check whether the mouse is over the bar while it's showing.
const HOVER_POLL: Duration = Duration::from_millis(80);

/// Whether the bar is showing. Read by the hover thread so it does nothing while the bar is hidden.
static VISIBLE: AtomicBool = AtomicBool::new(false);
/// Whether the mouse was over the bar on the last check, so we only emit when it changes.
static HOVERED: AtomicBool = AtomicBool::new(false);

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
    watch_hover(app.clone());

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
        .size(tauri::Size::Logical(tauri::LogicalSize::new(
            START_WIDTH,
            START_HEIGHT,
        )))
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
        // macOS draws the shadow from the page's opaque pixels, so it follows the pill and panel shapes.
        .has_shadow(true)
        .corner_radius(16.0)
        .hides_on_deactivate(false)
        .no_activate(true)
        .with_window(|w| {
            w.decorations(false)
                .transparent(true)
                .visible(false)
                // Clicks on the bar work straight away, even though another app has focus.
                .accept_first_mouse(true)
        })
        .build()?;

    place_top_center(app);
    Ok(())
}

/// Puts the bar just under the menu bar, centred, which is where the notch is on MacBooks.
fn place_top_center(app: &AppHandle) {
    let (Some(window), Ok(Some(monitor))) =
        (app.get_webview_window(BAR_LABEL), app.primary_monitor())
    else {
        return;
    };
    let area = monitor.work_area(); // the screen minus the menu bar and Dock, in physical pixels
    let width = START_WIDTH * monitor.scale_factor();
    let x = area.position.x as f64 + (area.size.width as f64 - width) / 2.0;
    let y = area.position.y as f64 + 6.0 * monitor.scale_factor();
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Shows the bar if it's hidden, hides it if it's showing. Used by the hotkey and the menu bar menu.
pub fn toggle_bar(app: &AppHandle) {
    let Ok(panel) = app.get_webview_panel(BAR_LABEL) else {
        return;
    };

    if panel.is_visible() {
        panel.hide();
        VISIBLE.store(false, Ordering::Relaxed);
    } else {
        panel.show();
        VISIBLE.store(true, Ordering::Relaxed);
        // Tell the bar's page to fetch fresh numbers.
        let _ = app.emit_to(BAR_LABEL, "bar-shown", ());
    }
}

/// The ☰ menu's "Hide bar".
#[tauri::command]
pub fn hide_bar(app: AppHandle) {
    if VISIBLE.load(Ordering::Relaxed) {
        toggle_bar(&app);
    }
}

/// Sizes the window to what the page draws (the pill, plus the panel while you hover), keeping the top-left
/// corner where it is. The window is exactly that size so the empty space around it doesn't block clicks.
///
/// Not `async`, so Tauri runs it on the main thread, which AppKit requires.
#[tauri::command]
pub fn resize_bar(app: AppHandle, width: f64, height: f64) {
    let Ok(panel) = app.get_webview_panel(BAR_LABEL) else {
        return;
    };
    let window = panel.as_panel();
    // AppKit measures from the bottom-left of the screen, so keep the top edge fixed by hand.
    let frame = window.frame();
    let top = frame.origin.y + frame.size.height;
    let next = objc2_foundation::NSRect::new(
        objc2_foundation::NSPoint::new(frame.origin.x, top - height),
        objc2_foundation::NSSize::new(width, height),
    );
    window.setFrame_display(next, true);
    window.invalidateShadow();
}

/// The webview gets no mouse-move events while the bar isn't the key window, and it never is. So we check
/// the mouse position ourselves while the bar is showing and tell the page with `bar-hover`.
fn watch_hover(app: AppHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(HOVER_POLL);
            if !VISIBLE.load(Ordering::Relaxed) {
                if HOVERED.swap(false, Ordering::Relaxed) {
                    let _ = app.emit_to(BAR_LABEL, "bar-hover", false);
                }
                continue;
            }
            // AppKit calls must run on the main thread.
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let Ok(panel) = handle.get_webview_panel(BAR_LABEL) else {
                    return;
                };
                let frame = panel.as_panel().frame();
                let mouse = objc2_app_kit::NSEvent::mouseLocation(); // same bottom-left screen coordinates as `frame`
                let inside = mouse.x >= frame.origin.x
                    && mouse.x <= frame.origin.x + frame.size.width
                    && mouse.y >= frame.origin.y
                    && mouse.y <= frame.origin.y + frame.size.height;
                if HOVERED.swap(inside, Ordering::Relaxed) != inside {
                    let _ = handle.emit_to(BAR_LABEL, "bar-hover", inside);
                }
            });
        }
    });
}
