//! UI icons.
//!
//! Three kinds live here:
//!
//! * Hand-drawn vector glyphs for the app chrome (`draw_flag_badge` and the
//!   flag painters). These stay as they are.
//! * The sidebar navigation icons ([`nav_backup_icon`], [`nav_settings_icon`])
//!   and the backup row actions ([`kebab_icon`], [`refresh_icon`]). These are
//!   **not** drawn here: they are Material Symbols SVGs (Apache-2.0, see
//!   `docs/THIRD_PARTY.md`) rastered to white PNGs by `build.rs` and tinted to
//!   the current theme colour at run time.

use crate::theme;
use eframe::egui::{self, Color32};

// ---------------------------------------------------------------------------
// Sidebar navigation + backup row-action icons (Material Symbols, rastered by
// build.rs)
// ---------------------------------------------------------------------------
//
// Coupling contract with `build.rs` (the other half of the pair):
//   assets/icon-cloud.svg     ⇒ $OUT_DIR/icon-cloud.png     (64 px, white)
//   assets/icon-settings.svg  ⇒ $OUT_DIR/icon-settings.png  (64 px, white)
//   assets/icon-more-vert.svg ⇒ $OUT_DIR/icon-more-vert.png (64 px, white)
//   assets/icon-refresh.svg   ⇒ $OUT_DIR/icon-refresh.png   (64 px, white)
//   assets/logo-small.svg     ⇒ $OUT_DIR/logo-small.png     (256 px, colour)
// The rasters are deliberately white: the buttons and the sidebar tint them to
// the theme colour, so one raster per icon covers light and dark themes.

/// The filled "cloud" sidebar-navigation icon ("Sicherung"), as a build-time PNG.
const NAV_CLOUD_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-cloud.png"));
/// The filled "settings" sidebar-navigation icon ("Einstellungen"), as a
/// build-time PNG.
const NAV_SETTINGS_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-settings.png"));
/// The vertical-kebab ("more") row-action icon, as a build-time PNG.
const MORE_VERT_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-more-vert.png"));
/// The refresh ("scan and upload now") row-action icon, as a build-time PNG.
const REFRESH_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-refresh.png"));
/// The app logo (`assets/logo-small.svg`, 256 px, coloured), as a build-time PNG.
const LOGO_SMALL_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo-small.png"));

/// Logical display size of a sidebar navigation icon, in points. The sidebar
/// icon column is 18 px wide; the row-action icons' 17 pt are a different
/// context and stay as they are. The 64-px sources keep it crisp up to ~3.5× DPI.
const NAV_ICON_PX: f32 = 18.0;

/// Logical display size of a row-action icon, in points. The 64-px sources keep
/// it crisp up to ~3.5× DPI.
const ROW_ICON_PX: f32 = 17.0;

/// The filled "cloud" icon for the sidebar's "Sicherung" entry.
pub(crate) fn nav_backup_icon(ui: &egui::Ui) -> egui::Image<'static> {
    sidebar_icon(ui, "cloud", NAV_CLOUD_PNG)
}

/// The filled "settings" icon for the sidebar's "Einstellungen" entry.
pub(crate) fn nav_settings_icon(ui: &egui::Ui) -> egui::Image<'static> {
    sidebar_icon(ui, "settings", NAV_SETTINGS_PNG)
}

/// The vertical-kebab icon for the backup row's action menu.
pub(crate) fn kebab_icon(ui: &egui::Ui) -> egui::Image<'static> {
    row_icon(ui, "more-vert", MORE_VERT_PNG)
}

/// The refresh icon for the visible "scan and upload now" row button.
pub(crate) fn refresh_icon(ui: &egui::Ui) -> egui::Image<'static> {
    row_icon(ui, "refresh", REFRESH_PNG)
}

/// The app logo at an exact square size. Unlike the navigation icons this is
/// **not** tinted: the logo is coloured and carries the brand identity.
pub(crate) fn app_logo(ui: &egui::Ui, size: f32) -> egui::Image<'static> {
    let texture = icon_texture(ui.ctx(), "logo", LOGO_SMALL_PNG);
    egui::Image::new(egui::load::SizedTexture::from_handle(&texture))
        .fit_to_exact_size(egui::vec2(size, size))
}

/// Build an [`egui::Image`] for one sidebar navigation icon at [`NAV_ICON_PX`].
fn sidebar_icon(ui: &egui::Ui, name: &'static str, png: &'static [u8]) -> egui::Image<'static> {
    let texture = icon_texture(ui.ctx(), name, png);
    egui::Image::new(egui::load::SizedTexture::from_handle(&texture))
        .fit_to_exact_size(egui::vec2(NAV_ICON_PX, NAV_ICON_PX))
}

/// Build an [`egui::Image`] for one row-action icon at [`ROW_ICON_PX`].
fn row_icon(ui: &egui::Ui, name: &'static str, png: &'static [u8]) -> egui::Image<'static> {
    let texture = icon_texture(ui.ctx(), name, png);
    egui::Image::new(egui::load::SizedTexture::from_handle(&texture))
        .fit_to_exact_size(egui::vec2(ROW_ICON_PX, ROW_ICON_PX))
}

/// Decode an embedded icon PNG into an egui texture, cached in the context's
/// temporary data so it is decoded and uploaded **once** per app run instead of
/// once per frame.
///
/// The cache lives in egui memory rather than in `PrimePhotoApp` because the
/// icons are stateless, shared by every screen and used from free functions
/// that only receive a `&Ui`. egui's temp data is cleared only when the context
/// is dropped (`util/id_type_map.rs`: "temporary (cleared when egui is shut
/// down)"), so the handle — and with it the GPU texture — stays alive.
/// **Rule: `Context::load_texture` must never be called inside `data_mut`/`data`.**
/// Both acquire the context's write lock, and `parking_lot`'s lock is not
/// reentrant: `load_texture` goes through `Context::input`/`Context::write`,
/// which wants the very lock the surrounding closure already holds — a
/// self-deadlock that freezes the whole app (and every worker blocked in
/// `request_repaint`). So the cache is read, populated and written back in three
/// separate, non-overlapping lock scopes.
fn icon_texture(ctx: &egui::Context, name: &'static str, png: &'static [u8]) -> egui::TextureHandle {
    let id = egui::Id::new(("prime_photos_row_icon", name));

    // 1. Cache hit under a read lock that ends immediately.
    if let Some(handle) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return handle;
    }

    // 2. Cache miss: decode and upload with **no** context lock held.
    let image = image::load_from_memory(png)
        .expect("Prime Photos: eingebettetes Icon-PNG konnte nicht dekodiert werden")
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, &image.into_raw());
    let handle = ctx.load_texture(
        format!("prime_photos_row_icon_{name}"),
        color,
        egui::TextureOptions::LINEAR,
    );

    // 3. Store the handle under a write lock that ends immediately.
    ctx.data_mut(|d| d.insert_temp(id, handle.clone()));

    handle
}

// ---------------------------------------------------------------------------
// Marketplace flags (round badge for the account block)
// ---------------------------------------------------------------------------

/// Flag geometry: a set of equal bands plus an optional motif on top.
enum FlagBands {
    /// Equal horizontal stripes, top to bottom.
    Horizontal(&'static [Color32]),
    /// Equal vertical stripes, left to right.
    Vertical(&'static [Color32]),
}

/// A simple motif drawn on top of the bands, all kept inside the circle.
enum FlagMotif {
    /// Upright cross (Switzerland), arm width as a fraction of the radius.
    Cross(Color32, f32),
    /// Centred dot (Japan, India, Brazil, …), radius as a fraction of the radius.
    Dot(Color32, f32),
    /// Rhombus (Brazil), half-diagonal as a fraction of the radius.
    Diamond(Color32, f32),
    /// Upper-left canton (USA), size as a fraction of the radius.
    Canton(Color32, f32),
    /// Union-Jack saltire plus upright cross (United Kingdom).
    UnionJack,
}

/// Palette for the simplified marketplace flags.
const BLACK: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
const WHITE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const DE_RED: Color32 = Color32::from_rgb(0xDD, 0x00, 0x00);
const GOLD: Color32 = Color32::from_rgb(0xFF, 0xCE, 0x00);
const US_BLUE: Color32 = Color32::from_rgb(0x3C, 0x3B, 0x6E);
const UK_BLUE: Color32 = Color32::from_rgb(0x01, 0x21, 0x69);
const FR_BLUE: Color32 = Color32::from_rgb(0x00, 0x23, 0x95);
const FR_RED: Color32 = Color32::from_rgb(0xED, 0x29, 0x39);
const IT_GREEN: Color32 = Color32::from_rgb(0x00, 0x8C, 0x45);
const IT_RED: Color32 = Color32::from_rgb(0xCD, 0x21, 0x2A);
const ES_RED: Color32 = Color32::from_rgb(0xAA, 0x15, 0x1B);
const ES_YELLOW: Color32 = Color32::from_rgb(0xF1, 0xBF, 0x00);
const JP_RED: Color32 = Color32::from_rgb(0xBC, 0x00, 0x2D);
const IN_SAFFRON: Color32 = Color32::from_rgb(0xFF, 0x99, 0x33);
const IN_GREEN: Color32 = Color32::from_rgb(0x13, 0x88, 0x08);
const IN_NAVY: Color32 = Color32::from_rgb(0x00, 0x00, 0x80);
const BR_GREEN: Color32 = Color32::from_rgb(0x00, 0x9C, 0x3B);
const BR_YELLOW: Color32 = Color32::from_rgb(0xFF, 0xDF, 0x00);
const BR_BLUE: Color32 = Color32::from_rgb(0x00, 0x27, 0x76);

/// Bands + motifs for the marketplaces we ship. `None` for flags whose real
/// design needs a detailed coat of arms/emblem that cannot be honestly
/// simplified at 30 px — those get a country-code badge instead.
static FLAG_DE: &[Color32] = &[BLACK, DE_RED, GOLD];
static FLAG_AT: &[Color32] = &[DE_RED, WHITE, DE_RED];
static FLAG_CH: &[Color32] = &[DE_RED];
static FLAG_CH_M: &[FlagMotif] = &[FlagMotif::Cross(WHITE, 0.30)];
static FLAG_US: &[Color32] = &[DE_RED, WHITE, DE_RED, WHITE, DE_RED, WHITE, DE_RED];
static FLAG_US_M: &[FlagMotif] = &[FlagMotif::Canton(US_BLUE, 0.62)];
static FLAG_UK: &[Color32] = &[UK_BLUE];
static FLAG_UK_M: &[FlagMotif] = &[FlagMotif::UnionJack];
static FLAG_FR: &[Color32] = &[FR_BLUE, WHITE, FR_RED];
static FLAG_IT: &[Color32] = &[IT_GREEN, WHITE, IT_RED];
static FLAG_ES: &[Color32] = &[ES_RED, ES_YELLOW, ES_RED];
static FLAG_JP: &[Color32] = &[WHITE];
static FLAG_JP_M: &[FlagMotif] = &[FlagMotif::Dot(JP_RED, 0.36)];
static FLAG_IN: &[Color32] = &[IN_SAFFRON, WHITE, IN_GREEN];
static FLAG_IN_M: &[FlagMotif] = &[FlagMotif::Dot(IN_NAVY, 0.15)];
static FLAG_BR: &[Color32] = &[BR_GREEN];
static FLAG_BR_M: &[FlagMotif] = &[FlagMotif::Diamond(BR_YELLOW, 0.82), FlagMotif::Dot(BR_BLUE, 0.26)];
static FLAG_CA: &[Color32] = &[DE_RED, WHITE, DE_RED];
static FLAG_CA_M: &[FlagMotif] = &[FlagMotif::Diamond(DE_RED, 0.30)];

fn flag_for(country: &str) -> Option<(FlagBands, &'static [FlagMotif])> {
    match country {
        "de" => Some((FlagBands::Horizontal(FLAG_DE), &[])),
        "at" => Some((FlagBands::Horizontal(FLAG_AT), &[])),
        "ch" => Some((FlagBands::Horizontal(FLAG_CH), FLAG_CH_M)),
        "us" => Some((FlagBands::Horizontal(FLAG_US), FLAG_US_M)),
        "uk" => Some((FlagBands::Horizontal(FLAG_UK), FLAG_UK_M)),
        "fr" => Some((FlagBands::Vertical(FLAG_FR), &[])),
        "it" => Some((FlagBands::Vertical(FLAG_IT), &[])),
        "es" => Some((FlagBands::Horizontal(FLAG_ES), &[])),
        "jp" => Some((FlagBands::Horizontal(FLAG_JP), FLAG_JP_M)),
        "in" => Some((FlagBands::Horizontal(FLAG_IN), FLAG_IN_M)),
        "br" => Some((FlagBands::Horizontal(FLAG_BR), FLAG_BR_M)),
        "ca" => Some((FlagBands::Vertical(FLAG_CA), FLAG_CA_M)),
        // Australia and Mexico carry a detailed emblem/union-jack corner that
        // cannot be honestly simplified at this size: country-code badge.
        _ => None,
    }
}

/// The account badge: the marketplace flag with a small presence dot in the
/// lower-right corner (green = signed in, red = not), plus a hover tooltip.
/// Never panics; unknown markets fall back to the country code.
pub(crate) fn draw_flag_badge(
    ui: &mut egui::Ui,
    diameter: f32,
    country: &str,
    online: bool,
    tooltip: &str,
) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(diameter, diameter), egui::Sense::hover());
    paint_flag(ui.painter(), rect, country);
    // Presence dot, flipped to the lower-right like a status badge.
    let dot = diameter * 0.17;
    let c = egui::pos2(
        rect.right() - dot * 0.9,
        rect.bottom() - dot * 0.9,
    );
    ui.painter().circle_filled(c, dot + 1.6, Color32::WHITE);
    ui.painter().circle_filled(
        c,
        dot,
        if online { theme::GREEN } else { theme::RED },
    );
    resp.on_hover_text(tooltip);
}

/// Paint the marketplace flag into `rect` (round, or a country-code badge when
/// the real flag needs an emblem we cannot honestly simplify).
fn paint_flag(painter: &egui::Painter, rect: egui::Rect, country: &str) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0;
    let diameter = r * 2.0;
    let Some((bands, motifs)) = flag_for(country) else {
        draw_code_badge(painter, c, r, country);
        return;
    };

    // Rasterise the bands in rows so the round edge stays clean.
    const ROWS: usize = 24;
    let step = diameter / ROWS as f32;
    let top = c.y - r;
    for i in 0..ROWS {
        let y0 = top + i as f32 * step;
        // Slight overlap keeps hairline seams out of the rounded edge.
        let y1 = y0 + step + 0.5;
        let mid_dy = (y0 + step * 0.5 - c.y).abs().min(r);
        let half = (r * r - mid_dy * mid_dy).max(0.0).sqrt();
        if half <= 0.0 {
            continue;
        }
        let x0 = c.x - half;
        let x1 = c.x + half;
        match bands {
            FlagBands::Horizontal(list) => {
                let idx = ((i as f32 + 0.5) / ROWS as f32 * list.len() as f32) as usize;
                let col = list[idx.min(list.len() - 1)];
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)),
                    0.0,
                    col,
                );
            }
            FlagBands::Vertical(list) => {
                let n = list.len();
                for (k, col) in list.iter().enumerate() {
                    let cx0 = c.x - r + diameter * k as f32 / n as f32;
                    let cx1 = cx0 + diameter / n as f32;
                    let a = cx0.max(x0);
                    let b = cx1.min(x1);
                    if b > a {
                        painter.rect_filled(
                            egui::Rect::from_min_max(egui::pos2(a, y0), egui::pos2(b, y1)),
                            0.0,
                            *col,
                        );
                    }
                }
            }
        }
    }
    for m in motifs {
        draw_flag_motif(painter, c, r, m);
    }
    // A touch stronger than the panel border so the darker bands keep a
    // crisp edge against the light sidebar.
    painter.circle_stroke(c, r, egui::Stroke::new(1.0_f32, egui::Color32::from_gray(0xB8)));
}

fn draw_flag_motif(painter: &egui::Painter, c: egui::Pos2, r: f32, m: &FlagMotif) {
    match *m {
        FlagMotif::Cross(col, width_frac) => {
            let w = r * width_frac;
            // Arms stay short enough that the corners remain inside the circle.
            let len = 1.7 * r;
            painter.rect_filled(egui::Rect::from_center_size(c, egui::vec2(w, len)), 0.0, col);
            painter.rect_filled(egui::Rect::from_center_size(c, egui::vec2(len, w)), 0.0, col);
        }
        FlagMotif::Dot(col, radius_frac) => {
            painter.circle_filled(c, r * radius_frac, col);
        }
        FlagMotif::Diamond(col, half_frac) => {
            let h = r * half_frac;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x, c.y - h),
                    egui::pos2(c.x + h, c.y),
                    egui::pos2(c.x, c.y + h),
                    egui::pos2(c.x - h, c.y),
                ],
                col,
                egui::Stroke::NONE,
            ));
        }
        FlagMotif::Canton(col, frac) => {
            let o = r * frac;
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(c.x - o, c.y - o), egui::pos2(c.x - o * 0.05, c.y - o * 0.05)),
                0.0,
                col,
            );
        }
        FlagMotif::UnionJack => {
            let white = Color32::WHITE;
            let red = Color32::from_rgb(0xC8, 0x10, 0x2E);
            let d = r * std::f32::consts::FRAC_1_SQRT_2;
            let diag = |w: f32, col: Color32| {
                painter.line_segment(
                    [egui::pos2(c.x - d, c.y - d), egui::pos2(c.x + d, c.y + d)],
                    egui::Stroke::new(w, col),
                );
                painter.line_segment(
                    [egui::pos2(c.x - d, c.y + d), egui::pos2(c.x + d, c.y - d)],
                    egui::Stroke::new(w, col),
                );
            };
            diag(r * 0.42, white);
            diag(r * 0.20, red);
            let len = 1.7 * r;
            painter.rect_filled(egui::Rect::from_center_size(c, egui::vec2(r * 0.62, len)), 0.0, white);
            painter.rect_filled(egui::Rect::from_center_size(c, egui::vec2(len, r * 0.62)), 0.0, white);
            painter.rect_filled(egui::Rect::from_center_size(c, egui::vec2(r * 0.34, len)), 0.0, red);
            painter.rect_filled(egui::Rect::from_center_size(c, egui::vec2(len, r * 0.34)), 0.0, red);
        }
    }
}

/// Neutral badge with the uppercase country code (flags we do not approximate).
fn draw_code_badge(painter: &egui::Painter, c: egui::Pos2, r: f32, country: &str) {
    painter.circle_filled(c, r, theme::MUTED);
    painter.text(
        c,
        egui::Align2::CENTER_CENTER,
        country.to_uppercase(),
        egui::FontId::proportional(r * 0.85),
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The raster pipeline really produces a white, covered glyph for every UI
    /// icon — the proof that `build.rs`'s `path { fill: … }` override reaches
    /// the shape (a `color` override would leave the bare `<path>` black), which
    /// is what makes the run-time tint to the theme colour possible.
    ///
    /// Anti-aliased edge pixels are written by tiny-skia as *straight* RGBA
    /// (`PixmapRef::encode_png` demultiplies), so every covered pixel — opaque
    /// or not — must be white.
    #[test]
    fn icon_rasters_are_white_and_covered() {
        for (name, png) in [
            ("icon-cloud", NAV_CLOUD_PNG),
            ("icon-settings", NAV_SETTINGS_PNG),
            ("icon-more-vert", MORE_VERT_PNG),
            ("icon-refresh", REFRESH_PNG),
        ] {
            let image = image::load_from_memory(png)
                .unwrap_or_else(|e| panic!("{name}: eingebettetes PNG nicht dekodierbar: {e}"))
                .to_rgba8();
            let mut covered = 0usize;
            for px in image.pixels() {
                let [r, g, b, a] = px.0;
                if a == 0 {
                    continue;
                }
                covered += 1;
                assert_eq!(
                    [r, g, b],
                    [255, 255, 255],
                    "{name}: gedecktes Pixel (alpha {a}) ist nicht weiß"
                );
            }
            assert!(
                covered > 0,
                "{name}: kein gedecktes Pixel — fill-Override wirkungslos?"
            );
        }
    }

    /// Regression guard for the `data_mut`/`load_texture` self-deadlock.
    ///
    /// The old implementation called `ctx.load_texture` *inside*
    /// `ctx.data_mut(…)`, so the closure re-entered the context's write lock
    /// (`parking_lot` is not reentrant) and froze the app on the first cache
    /// miss. Two calls are needed to cover both paths: the first populates the
    /// cache (the deadlocking path), the second is the read-lock cache hit.
    ///
    /// The work runs on its own thread with a fresh `Context` so that a
    /// regression fails as a bounded timeout instead of hanging the test run.
    #[test]
    fn icon_texture_does_not_deadlock() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let ctx = egui::Context::default();
            let first = icon_texture(&ctx, "more-vert", MORE_VERT_PNG);
            let second = icon_texture(&ctx, "more-vert", MORE_VERT_PNG);
            assert_eq!(first.id(), second.id(), "Cache-Treffer liefert anderen Handle");
            let _ = tx.send(());
        });
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("icon_texture deadlocked (data_mut/load_texture reentrancy)");
    }
}
