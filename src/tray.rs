//! Cross-platform tray / menu-bar icon (PORT-3).
//!
//! Provides a three-entry menu — "Fenster anzeigen", "Alle anhalten"/"Alle
//! fortsetzen", "Beenden" — plus the icon itself. The tooltip carries the brand
//! name plus the current status ("Prime Photos – …", like the original) and is
//! re-applied whenever the tray state or the UI language changes. The `Tray`
//! value owns the live `TrayIcon`; dropping it removes the icon from the menu
//! bar, so `main.rs` keeps it for the whole app lifetime.
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
use std::time::Duration;

use crate::i18n::tr;
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
    pub about_item: tray_icon::menu::MenuItem,
    pub quit_item: tray_icon::menu::MenuItem,
    /// Decoded build-time glyphs, keyed by [`TrayState::index`].
    icons: TrayIcons,
    /// Last state applied via [`Tray::set_state`]; `set_labels` refreshes the
    /// tooltip from it after a language change.
    state: TrayState,
}

/// Build the tray icon and menu, wiring menu/tray events so the given egui
/// context repaints when they arrive.
///
/// Returns `None` on any failure (logged), so the app degrades to an ordinary
/// close-to-quit window.
pub fn build(ctx: egui::Context) -> Option<Tray> {
    let menu = tray_icon::menu::Menu::new();
    let show_item = tray_icon::menu::MenuItem::new(tr("Fenster anzeigen"), true, None);
    let pause_item = tray_icon::menu::MenuItem::new(tr("Alle anhalten"), true, None);
    let about_item = tray_icon::menu::MenuItem::new(tr("Über Prime Photos"), true, None);
    let quit_item = tray_icon::menu::MenuItem::new(tr("Beenden"), true, None);
    // Separators like the original: "Über Prime Photos" sits between the pause
    // entry and "Beenden", framed by a separator on each side.
    let sep_before_about = tray_icon::menu::PredefinedMenuItem::separator();
    let sep_before_quit = tray_icon::menu::PredefinedMenuItem::separator();
    if let Err(e) = menu.append_items(&[
        &show_item,
        &pause_item,
        &sep_before_about,
        &about_item,
        &sep_before_quit,
        &quit_item,
    ]) {
        log::error!("tray: Menü konnte nicht aufgebaut werden: {e}");
        return None;
    }

    // The icon is applied after the tray exists (see `set_state` below); the
    // OS appearance is read once here for that initial idle glyph.
    let theme = ctx.system_theme();

    let builder = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(tray_tooltip(TrayState::Idle));
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

    let mut tray = Tray {
        _tray: tray,
        show_item,
        pause_item,
        about_item,
        quit_item,
        icons: TrayIcons::build(),
        state: TrayState::Idle,
    };
    // Show the idle glyph right away; later frames re-apply only on change.
    tray.set_state(TrayState::Idle, theme);
    Some(tray)
}

/// Take all menu events collected since the previous call.
pub fn drain_events() -> Vec<tray_icon::menu::MenuEvent> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

impl Tray {
    /// Reflect the global pause state in the menu label (original wording, see
    /// `docs/original/SPEC_OS_INTEGRATION.md` §4): "Alle fortsetzen" while
    /// paused, "Alle anhalten" otherwise.
    pub fn set_paused(&self, paused: bool) {
        self.pause_item
            .set_text(if paused { tr("Alle fortsetzen") } else { tr("Alle anhalten") });
    }

    /// Re-apply every menu label in the current language (called after a
    /// language change). The tooltip carries the current status, which is
    /// translated too, so it is refreshed from the last applied state.
    pub fn set_labels(&self, paused: bool) {
        self.show_item.set_text(tr("Fenster anzeigen"));
        self.set_paused(paused);
        self.about_item.set_text(tr("Über Prime Photos"));
        self.quit_item.set_text(tr("Beenden"));
        let _ = self._tray.set_tooltip(Some(tray_tooltip(self.state)));
    }

    /// Apply the glyph and the tooltip for `state`. Callers re-apply only when
    /// the state (or the OS theme) actually changed — see
    /// `PrimePhotoApp::update_tray_icon`.
    ///
    /// **macOS** draws a *template* image (alpha only, recoloured by the
    /// system), so the black 64-px ("@2x") source is used there — more bitmap
    /// per point keeps the menu-bar glyph retina-sharp. Everywhere else the
    /// glyph follows the OS appearance: dark ⇒ white, light ⇒ black, unknown
    /// ⇒ the coloured logo, which stays legible on any background.
    pub(crate) fn set_state(&mut self, state: TrayState, theme: Option<egui::Theme>) {
        let idx = state.index();
        #[cfg(target_os = "macos")]
        {
            let _ = theme;
            let _ = self
                ._tray
                .set_icon_templated(Some(self.icons.black[idx].clone()));
        }
        #[cfg(not(target_os = "macos"))]
        {
            let icon = match theme {
                Some(egui::Theme::Dark) => self.icons.white[idx].clone(),
                Some(egui::Theme::Light) => self.icons.black[idx].clone(),
                None => self.icons.logo.clone(),
            };
            let _ = self._tray.set_icon(Some(icon));
        }
        // The tooltip follows the state (original: "Amazon Photos - {Status}").
        let _ = self._tray.set_tooltip(Some(tray_tooltip(state)));
        self.state = state;
    }
}

// ---------------------------------------------------------------------------
// State glyphs (build-time PNGs; nothing is drawn any more)
// ---------------------------------------------------------------------------
//
// Coupling contract with `build.rs` — the file names, colours and sizes live in
// exactly one place per side and must be changed together:
//   assets/<stem>.svg  ⇒  $OUT_DIR/<stem>-<colour>-<size>.png
//   stems, in `TrayState::index()` order: tray, tray-active, tray-paused,
//     tray-done, tray-error   (all `currentColor`, so black/white is a raster
//     override the build injects — there are no coloured SVG files)
//   colours: black (#000000) / white (#ffffff)
//   sizes:   32 (1×) / 64 ("@2x") — macOS takes the 64-px source, the other
//     platforms the 32-px one (the tray scales to the menu-bar height).
// The coloured `logo-small.png` (from `assets/logo-small.svg`, 256 px) is the
// fallback glyph for an unknown OS theme.

/// Visual state of the tray icon, derived by the pure [`tray_state`] function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayState {
    /// Nothing running, no recent outcome — the neutral upload glyph.
    Idle,
    /// An upload run is live.
    Active,
    /// All backup activity is paused (global pause).
    Paused,
    /// The last run finished without errors (shown for [`DONE_WINDOW`]).
    Done,
    /// The last run finished with at least one failure.
    Error,
}

impl TrayState {
    /// Index into the embedded glyph arrays. Matches the row order of
    /// `build.rs`'s `TRAY_STEMS`.
    const fn index(self) -> usize {
        match self {
            TrayState::Idle => 0,
            TrayState::Active => 1,
            TrayState::Paused => 2,
            TrayState::Done => 3,
            TrayState::Error => 4,
        }
    }
}

/// The tray tooltip: the brand name, plus the current status when there is one
/// to show — the original puts the status in the status-item tooltip
/// (`docs/original/SPEC_NOTIFICATIONS.md` §2.4). `Idle` has no status to add,
/// so there the tooltip is the brand name alone.
pub(crate) fn tray_tooltip(state: TrayState) -> String {
    let status = match state {
        TrayState::Idle => return tr("Prime Photos").to_string(),
        TrayState::Paused => tr("Angehalten"),
        TrayState::Active => tr("In Bearbeitung"),
        TrayState::Error => tr("Mit Fehlern abgeschlossen"),
        TrayState::Done => tr("Abgeschlossen"),
    };
    format!("{} – {}", tr("Prime Photos"), status)
}

/// How long a finished, error-free run keeps the "done" glyph before the tray
/// falls back to "idle" (user decision 2026-10-14).
pub(crate) const DONE_WINDOW: Duration = Duration::from_secs(5);

/// Pure derivation of the tray glyph state (no I/O, so unit-testable).
///
/// Priority: a deliberate global pause wins, then a live run, then the outcome
/// of the last finished run — a failure sticks until the next run, a success
/// only shows for [`DONE_WINDOW`] — otherwise idle.
pub(crate) fn tray_state(
    paused: bool,
    busy: bool,
    last_run_failed: bool,
    since_done: Option<Duration>,
) -> TrayState {
    if paused {
        TrayState::Paused
    } else if busy {
        TrayState::Active
    } else if last_run_failed {
        TrayState::Error
    } else if since_done.is_some_and(|d| d < DONE_WINDOW) {
        TrayState::Done
    } else {
        TrayState::Idle
    }
}

/// macOS template source: the 64-px ("@2x") black raster, `TrayState`-ordered.
#[cfg(target_os = "macos")]
const BLACK_PNGS_64: [&[u8]; 5] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-black-64.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-active-black-64.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-paused-black-64.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-done-black-64.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-error-black-64.png")),
];

/// Non-macOS black + white 32-px rasters, `TrayState`-ordered. `cfg`-gated (as
/// is the coloured logo below) so the macOS build has no unused items.
#[cfg(not(target_os = "macos"))]
const BLACK_PNGS_32: [&[u8]; 5] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-black-32.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-active-black-32.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-paused-black-32.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-done-black-32.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-error-black-32.png")),
];

#[cfg(not(target_os = "macos"))]
const WHITE_PNGS_32: [&[u8]; 5] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-white-32.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-active-white-32.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-paused-white-32.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-done-white-32.png")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tray-error-white-32.png")),
];

#[cfg(not(target_os = "macos"))]
const LOGO_SMALL_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo-small.png"));

/// Decoded glyphs, keyed by [`TrayState::index`].
struct TrayIcons {
    black: [tray_icon::Icon; 5],
    #[cfg(not(target_os = "macos"))]
    white: [tray_icon::Icon; 5],
    #[cfg(not(target_os = "macos"))]
    logo: tray_icon::Icon,
}

impl TrayIcons {
    fn build() -> Self {
        #[cfg(target_os = "macos")]
        let black = BLACK_PNGS_64.map(decode_icon);
        #[cfg(not(target_os = "macos"))]
        let black = BLACK_PNGS_32.map(decode_icon);
        #[cfg(target_os = "macos")]
        let icons = Self { black };
        #[cfg(not(target_os = "macos"))]
        let icons = Self {
            black,
            white: WHITE_PNGS_32.map(decode_icon),
            logo: decode_icon(LOGO_SMALL_PNG),
        };
        icons
    }
}

/// Decode an embedded PNG into a `tray_icon::Icon`. `build.rs` already guards
/// the sources, so a decode failure here is a programming error — fail fast
/// rather than ship a silently missing icon.
fn decode_icon(png: &[u8]) -> tray_icon::Icon {
    let image = image::load_from_memory(png)
        .expect("Prime Photos: eingebettetes Tray-PNG konnte nicht dekodiert werden")
        .to_rgba8();
    let (width, height) = (image.width(), image.height());
    tray_icon::Icon::from_rgba(image.into_raw(), width, height)
        .expect("Prime Photos: eingebettetes Tray-PNG hat ungültige Maße")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_state_paused_wins_over_everything() {
        assert_eq!(
            tray_state(true, true, true, Some(Duration::ZERO)),
            TrayState::Paused
        );
        assert_eq!(tray_state(true, false, true, None), TrayState::Paused);
        assert_eq!(
            tray_state(true, false, false, Some(Duration::from_secs(1))),
            TrayState::Paused
        );
    }

    #[test]
    fn tray_state_busy_is_active() {
        assert_eq!(tray_state(false, true, false, None), TrayState::Active);
        assert_eq!(
            tray_state(false, true, true, Some(Duration::ZERO)),
            TrayState::Active
        );
    }

    #[test]
    fn tray_state_error_persists_past_the_done_window() {
        // A failed last run keeps the error glyph until the next run — long
        // after the "done" window would have expired.
        assert_eq!(
            tray_state(false, false, true, Some(Duration::ZERO)),
            TrayState::Error
        );
        assert_eq!(
            tray_state(false, false, true, Some(DONE_WINDOW * 10)),
            TrayState::Error
        );
        assert_eq!(tray_state(false, false, true, None), TrayState::Error);
    }

    #[test]
    fn tray_state_done_only_inside_the_window() {
        assert_eq!(
            tray_state(false, false, false, Some(Duration::ZERO)),
            TrayState::Done
        );
        assert_eq!(
            tray_state(
                false,
                false,
                false,
                Some(DONE_WINDOW - Duration::from_millis(1))
            ),
            TrayState::Done
        );
    }

    #[test]
    fn tray_state_idle_at_and_after_the_window_boundary() {
        assert_eq!(
            tray_state(false, false, false, Some(DONE_WINDOW)),
            TrayState::Idle
        );
        assert_eq!(
            tray_state(false, false, false, Some(DONE_WINDOW + Duration::from_secs(1))),
            TrayState::Idle
        );
    }

    #[test]
    fn tray_state_idle_when_nothing_ever_ran() {
        assert_eq!(tray_state(false, false, false, None), TrayState::Idle);
    }

    /// The tooltip carries the brand plus the status — except when idle, where
    /// the original shows the brand alone. The English half also pins the EN
    /// rows: a missing table entry would silently fall back to German.
    #[test]
    fn tray_tooltip_shows_brand_and_status() {
        assert_eq!(tray_tooltip(TrayState::Idle), "Prime Photos");
        assert_eq!(tray_tooltip(TrayState::Paused), "Prime Photos – Angehalten");
        assert_eq!(tray_tooltip(TrayState::Active), "Prime Photos – In Bearbeitung");
        assert_eq!(
            tray_tooltip(TrayState::Error),
            "Prime Photos – Mit Fehlern abgeschlossen"
        );
        assert_eq!(tray_tooltip(TrayState::Done), "Prime Photos – Abgeschlossen");

        crate::i18n::set_lang(crate::i18n::Lang::En);
        assert_eq!(tray_tooltip(TrayState::Idle), "Prime Photos");
        assert_eq!(tray_tooltip(TrayState::Paused), "Prime Photos – Paused");
        assert_eq!(tray_tooltip(TrayState::Active), "Prime Photos – In progress");
        assert_eq!(
            tray_tooltip(TrayState::Error),
            "Prime Photos – Finished with errors"
        );
        assert_eq!(tray_tooltip(TrayState::Done), "Prime Photos – Completed");
        crate::i18n::set_lang(crate::i18n::Lang::De);
    }

    /// The build-time `* { color: … }` override really lands in the raster: the
    /// painted pixels of the white source are white, of the black source black.
    /// Reads the `OUT_DIR` artifacts at run time (they exist once the crate has
    /// been built), so this also holds in the macOS build, where the white set
    /// is `cfg`-ed out of the binary.
    #[test]
    fn build_rasters_carry_the_injected_colour() {
        assert_eq!(strongest_pixel_rgb(&read_out_png("tray-white-32.png")), [255, 255, 255]);
        assert_eq!(strongest_pixel_rgb(&read_out_png("tray-black-32.png")), [0, 0, 0]);
    }

    fn read_out_png(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("OUT_DIR")).join(name);
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// RGB of the most opaque pixel — the glyph's stroke colour (anti-aliased
    /// edge pixels are less opaque).
    fn strongest_pixel_rgb(png: &[u8]) -> [u8; 3] {
        let image = image::load_from_memory(png).unwrap().to_rgba8();
        let mut best = ([0u8, 0, 0], 0u8);
        for px in image.pixels() {
            let [r, g, b, a] = px.0;
            if a > best.1 {
                best = ([r, g, b], a);
            }
        }
        best.0
    }
}
