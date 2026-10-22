//! The floating bar: a panel that never steals focus, toggled by a global hotkey (hotkey.rs).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use statuspal_core::placement::{self, Rect};
use statuspal_core::settings::BarPosition;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Monitor, WebviewUrl};
use tauri_nspanel::{
    CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask, tauri_panel,
};

use crate::settings;

const BAR_LABEL: &str = "bar";

/// Starting size: a horizontal pill. The page resizes the window to fit as soon as it draws (`resize_bar`).
const START_WIDTH: f64 = 392.0;
const START_HEIGHT: f64 = 54.0;

/// How often we check whether the mouse is over the bar while it's showing.
const HOVER_POLL: Duration = Duration::from_millis(100);
/// How often we check whether the mouse button is still down during a drag.
const DRAG_POLL: Duration = Duration::from_millis(40);
/// Quicker while the mouse is over the bar, so the page's hover highlights keep up with it.
const POINTER_POLL: Duration = Duration::from_millis(30);

/// Whether the bar is showing. The hover thread does nothing while it's hidden.
static VISIBLE: AtomicBool = AtomicBool::new(false);
/// Whether the mouse was over the bar on the last check, so we only emit when it changes.
static HOVERED: AtomicBool = AtomicBool::new(false);
/// Where the mouse was on the last check (page pixels from the top-left), `None` when outside.
static POINTER: Mutex<Option<(f64, f64)>> = Mutex::new(None);

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
        // Small, so it never rounds off the stepped pixel corners the page draws (--shape-pill / --shape-panel).
        .corner_radius(4.0)
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

    place(app);
    Ok(())
}

/// Puts the bar where you left it on the display it was on last. A display it hasn't been on yet gets the
/// default spot: top centre, just under the menu bar (where the notch is on MacBooks).
fn place(app: &AppHandle) {
    let Some(window) = app.get_webview_window(BAR_LABEL) else {
        return;
    };
    let saved = settings::get(app);
    let monitors = app.available_monitors().unwrap_or_default();
    let last = saved
        .bar_display
        .as_ref()
        .and_then(|name| monitors.iter().find(|m| m.name() == Some(name)).cloned());
    let Some(monitor) = last.or_else(|| app.primary_monitor().ok().flatten()) else {
        return;
    };

    let area = usable_area(&monitor);
    let bar = Rect {
        x: 0.0,
        y: 0.0,
        width: START_WIDTH,
        height: START_HEIGHT,
    };
    let (x, y) = match monitor
        .name()
        .and_then(|name| saved.bar_positions.get(name))
    {
        // `keep_inside` in case the display got smaller since.
        Some(spot) => placement::keep_inside(
            Rect {
                x: area.x + spot.x,
                y: area.y + spot.y,
                ..bar
            },
            area,
        ),
        None => placement::top_center(START_WIDTH, area),
    };
    let _ = window.set_position(LogicalPosition::new(x, y));
}

/// A display's usable part (no menu bar, no Dock) in points, the same units `set_position` takes.
fn usable_area(monitor: &Monitor) -> Rect {
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    Rect {
        x: area.position.x as f64 / scale,
        y: area.position.y as f64 / scale,
        width: area.size.width as f64 / scale,
        height: area.size.height as f64 / scale,
    }
}

/// Pressing and moving the pill (Bar.tsx). macOS moves the window while the mouse is down; once it's let go,
/// the bar snaps to a nearby edge, corner or the notch, and its spot is saved for this display.
#[tauri::command]
pub fn drag_bar(app: AppHandle) {
    let Some(window) = app.get_webview_window(BAR_LABEL) else {
        return;
    };
    let _ = window.start_dragging();
    thread::spawn(move || {
        while objc2_app_kit::NSEvent::pressedMouseButtons() != 0 {
            thread::sleep(DRAG_POLL);
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || settle(&handle));
    });
}

fn settle(app: &AppHandle) {
    let Some(window) = app.get_webview_window(BAR_LABEL) else {
        return;
    };
    let (Ok(Some(monitor)), Ok(position), Ok(size), Ok(scale)) = (
        window.current_monitor(),
        window.outer_position(),
        window.outer_size(),
        window.scale_factor(),
    ) else {
        return;
    };
    let bar = Rect {
        x: position.x as f64 / scale,
        y: position.y as f64 / scale,
        width: size.width as f64 / scale,
        height: size.height as f64 / scale,
    };
    let area = usable_area(&monitor);

    // Hold ⌥ while dragging to place it freely.
    let free = objc2_app_kit::NSEvent::modifierFlags_class()
        .contains(objc2_app_kit::NSEventModifierFlags::Option);
    let (x, y) = if free {
        placement::keep_inside(bar, area)
    } else {
        placement::snap(bar, area)
    };
    if (x, y) != (bar.x, bar.y) {
        let _ = window.set_position(LogicalPosition::new(x, y));
    }
    if let Some(name) = monitor.name() {
        let spot = BarPosition {
            x: x - area.x,
            y: y - area.y,
        };
        settings::set_bar_position(app, name.clone(), spot);
    }
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
        // A bar hidden while faded would otherwise come back click-through.
        if let Some(window) = app.get_webview_window(BAR_LABEL) {
            let _ = window.set_ignore_cursor_events(false);
        }
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

/// Sizes the window to what the page draws (the pill, plus the panel while it's open), keeping the top-left
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

/// While the bar is faded, clicks go through it to whatever is underneath. Hovering wakes it back up (the page
/// turns this off again), so you can still click it.
#[tauri::command]
pub fn set_click_through(app: AppHandle, enabled: bool) {
    if let Some(window) = app.get_webview_window(BAR_LABEL) {
        let _ = window.set_ignore_cursor_events(enabled);
    }
    // The shadow follows the page's opacity, so redraw it for the faded (or unfaded) look.
    if let Ok(panel) = app.get_webview_panel(BAR_LABEL) {
        panel.as_panel().invalidateShadow();
    }
}

/// The webview gets no mouse-move events while StatusPal isn't the active app, and it never is (the bar doesn't
/// take focus). So while the bar is showing, we check the mouse position ourselves and tell the page:
/// - `bar-hover` (true/false) when the mouse goes over the bar or leaves it. This keeps working while the bar
///   is click-through, which is how hovering wakes a faded bar.
/// - `bar-pointer` (`[x, y]` or null) as it moves, so the page can highlight the button under it.
fn watch_hover(app: AppHandle) {
    thread::spawn(move || {
        loop {
            let over = HOVERED.load(Ordering::Relaxed);
            thread::sleep(if over { POINTER_POLL } else { HOVER_POLL });
            if !VISIBLE.load(Ordering::Relaxed) {
                HOVERED.store(false, Ordering::Relaxed);
                *POINTER.lock().unwrap() = None;
                continue;
            }
            // AppKit calls must run on the main thread.
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let Ok(panel) = handle.get_webview_panel(BAR_LABEL) else {
                    return;
                };
                let frame = panel.as_panel().frame();
                // Same bottom-left screen coordinates as `frame`.
                let mouse = objc2_app_kit::NSEvent::mouseLocation();
                let inside = mouse.x >= frame.origin.x
                    && mouse.x <= frame.origin.x + frame.size.width
                    && mouse.y >= frame.origin.y
                    && mouse.y <= frame.origin.y + frame.size.height;
                if HOVERED.swap(inside, Ordering::Relaxed) != inside {
                    let _ = handle.emit_to(BAR_LABEL, "bar-hover", inside);
                }

                // The page measures from the top-left, AppKit from the bottom-left.
                let pointer = inside.then_some((
                    mouse.x - frame.origin.x,
                    frame.origin.y + frame.size.height - mouse.y,
                ));
                let mut last = POINTER.lock().unwrap();
                if *last != pointer {
                    *last = pointer;
                    let _ = handle.emit_to(BAR_LABEL, "bar-pointer", pointer);
                }
            });
        }
    });
}
