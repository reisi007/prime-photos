//! Cross-platform tray / menu-bar icon (PORT-3).
//!
//! Provides a three-entry menu — "Fenster anzeigen", "Anhalten"/"Fortsetzen",
//! "Beenden" — plus the icon itself. The `Tray` value owns the live
//! `TrayIcon`; dropping it removes the icon from the menu bar, so `main.rs`
//! keeps it for the whole app lifetime.
//!
//! ## Event delivery (deviation from the original brief, on purpose)
//!
//! `tray-icon` (via `muda`) routes a `MenuEvent` either to its channel —
//! `MenuEvent::receiver()` — **or** to a handler registered with
//! `MenuEvent::set_event_handler(Some(..))`, never both (see
//! `muda/src/menu_event.rs`: `send()` picks the handler when one is set). The
//! UI here only repaints on demand, so without a handler an idle app would
//! never wake up for a menu click. We therefore register the handler as the
//! wake-up and have it hand the event over through this module's own queue,
//! which `main.rs` drains each frame. Same for tray-icon clicks/wakes.
//!
//! ## macOS
//!
//! `tray-icon` requires an already-running event loop on the main thread;
//! eframe invokes the app creator (`PrimePhotoApp::new`) from winit's
//! `resumed`, i.e. inside the running loop — so building the tray in `new()`
//! is correct.

use std::sync::Mutex;

use eframe::egui;

/// Menu events captured by the global [`MenuEvent`] handler, drained by the UI.
static PENDING: Mutex<Vec<tray_icon::menu::MenuEvent>> = Mutex::new(Vec::new());

/// A live tray icon together with its three menu items.
///
/// The struct owns the `TrayIcon` so it stays alive for as long as the app
/// holds the value.
pub struct Tray {
    /// The icon itself; kept only so it is not dropped (dropping removes it).
    _tray: tray_icon::TrayIcon,
    pub show_item: tray_icon::menu::MenuItem,
    pub pause_item: tray_icon::menu::MenuItem,
    pub quit_item: tray_icon::menu::MenuItem,
}

/// Side of the generated RGBA icon in pixels.
const ICON_SIZE: u32 = 32;

/// Build the tray icon and menu, wiring menu/tray events so the given egui
/// context repaints when they arrive.
///
/// Returns `None` on any failure (logged), so the app degrades to an ordinary
/// close-to-quit window.
pub fn build(ctx: egui::Context) -> Option<Tray> {
    let menu = tray_icon::menu::Menu::new();
    let show_item = tray_icon::menu::MenuItem::new("Fenster anzeigen", true, None);
    let pause_item = tray_icon::menu::MenuItem::new("Anhalten", true, None);
    let quit_item = tray_icon::menu::MenuItem::new("Beenden", true, None);
    let separator = tray_icon::menu::PredefinedMenuItem::separator();
    if let Err(e) = menu.append_items(&[&show_item, &pause_item, &separator, &quit_item]) {
        log::error!("tray: Menü konnte nicht aufgebaut werden: {e}");
        return None;
    }

    let icon = match tray_icon::Icon::from_rgba(upload_glyph_rgba(), ICON_SIZE, ICON_SIZE) {
        Ok(icon) => icon,
        Err(e) => {
            log::error!("tray: Icon konnte nicht erzeugt werden: {e}");
            return None;
        }
    };

    let builder = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Prime Photos");
    // `with_icon_templated` is the non-deprecated replacement for
    // `with_icon_as_template(true)`. It takes the icon itself and exists on
    // macOS only, so the icon is set per-platform here.
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_icon(icon);
    let tray = match builder.build() {
        Ok(tray) => tray,
        Err(e) => {
            log::error!("tray: TrayIcon konnte nicht erstellt werden: {e}");
            return None;
        }
    };

    // Wake the UI when a menu entry is activated. See the module docs for why
    // the event is queued here instead of read from `MenuEvent::receiver()`.
    let menu_ctx = ctx.clone();
    tray_icon::menu::MenuEvent::set_event_handler(Some(move |ev: tray_icon::menu::MenuEvent| {
        PENDING.lock().unwrap().push(ev);
        menu_ctx.request_repaint();
    }));
    // Wake the UI on tray-icon interaction (click/enter/leave…).
    tray_icon::TrayIconEvent::set_event_handler(Some(move |_| ctx.request_repaint()));

    Some(Tray {
        _tray: tray,
        show_item,
        pause_item,
        quit_item,
    })
}

/// Take all menu events collected since the previous call.
pub fn drain_events() -> Vec<tray_icon::menu::MenuEvent> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

impl Tray {
    /// Reflect the global pause state in the menu label:
    /// "Fortsetzen" while paused, "Anhalten" otherwise.
    pub fn set_paused(&self, paused: bool) {
        self.pause_item
            .set_text(if paused { "Fortsetzen" } else { "Anhalten" });
    }
}

// ---------------------------------------------------------------------------
// Icon generation (no asset file)
// ---------------------------------------------------------------------------

/// Draw the app's upload glyph — a teal disc with a white up-arrow and tray
/// bar — as a `ICON_SIZE`×`ICON_SIZE` RGBA image with a transparent
/// background.
///
/// The arrow/tray are punched out (alpha 0) rather than painted white so the
/// icon also reads correctly as a macOS menu-bar template (which uses only the
/// alpha channel) while keeping the teal shape on Windows/Linux.
fn upload_glyph_rgba() -> Vec<u8> {
    let size = ICON_SIZE as f32;
    // Same 84-unit reference geometry as the in-app `upload_logo`.
    let s = size / 84.0;
    let stroke = 4.0 * s;
    let cx = size / 2.0;
    let cy = size / 2.0;
    let radius = size / 2.0 - 1.0; // 1 px margin so the disc is not clipped
    let arrow_top = cy - 16.0 * s;
    let arrow_bottom = cy + 6.0 * s;
    let tray_y = cy + 19.0 * s;

    let stroke_segments: [((f32, f32), (f32, f32)); 4] = [
        ((cx, arrow_bottom), (cx, arrow_top)),
        ((cx, arrow_top), (cx - 11.0 * s, arrow_top + 11.0 * s)),
        ((cx, arrow_top), (cx + 11.0 * s, arrow_top + 11.0 * s)),
        ((cx - 18.0 * s, tray_y), (cx + 18.0 * s, tray_y)),
    ];

    const TEAL: [u8; 3] = [0x00, 0x71, 0x85];
    const SUB: u32 = 2; // 2×2 supersampling for smooth edges

    let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    for py in 0..ICON_SIZE {
        for px in 0..ICON_SIZE {
            let mut hits = 0u32;
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let x = px as f32 + (sx as f32 + 0.5) / SUB as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SUB as f32;
                    let inside = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() <= radius;
                    let on_stroke = stroke_segments
                        .iter()
                        .any(|&(a, b)| dist_to_segment((x, y), a, b) <= stroke / 2.0);
                    if inside && !on_stroke {
                        hits += 1;
                    }
                }
            }
            let alpha = (hits * 255 / (SUB * SUB)) as u8;
            let i = ((py * ICON_SIZE + px) * 4) as usize;
            rgba[i] = TEAL[0];
            rgba[i + 1] = TEAL[1];
            rgba[i + 2] = TEAL[2];
            rgba[i + 3] = alpha;
        }
    }
    rgba
}

/// Distance from point `p` to the segment `a`–`b`.
fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (px, py) = p;
    let (ax, ay) = a;
    let (bx, by) = b;
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (qx, qy) = (ax + t * dx, ay + t * dy);
    ((px - qx).powi(2) + (py - qy).powi(2)).sqrt()
}
