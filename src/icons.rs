use crate::theme;
use eframe::egui::{self, Color32};

/// Small line icons for the sidebar navigation.
#[derive(Clone, Copy)]
pub(crate) enum NavIcon {
    Backup,
    Settings,
}

/// Draws a small, crisp line icon that aligns to the fixed icon column.
pub(crate) fn nav_icon(painter: &egui::Painter, kind: NavIcon, rect: egui::Rect, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let stroke = egui::Stroke::new(1.6, color);
    match kind {
        NavIcon::Backup => {
            // Upload arrow into a tray.
            let top = c.y - s * 0.30;
            let mid = c.y + s * 0.02;
            painter.line_segment([egui::pos2(c.x, mid), egui::pos2(c.x, top)], stroke);
            painter.line_segment(
                [egui::pos2(c.x, top), egui::pos2(c.x - s * 0.17, top + s * 0.17)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x, top), egui::pos2(c.x + s * 0.17, top + s * 0.17)],
                stroke,
            );
            let ty = c.y + s * 0.34;
            painter.line_segment(
                [egui::pos2(c.x - s * 0.32, ty - s * 0.14), egui::pos2(c.x - s * 0.32, ty)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x + s * 0.32, ty - s * 0.14), egui::pos2(c.x + s * 0.32, ty)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - s * 0.32, ty), egui::pos2(c.x + s * 0.32, ty)],
                stroke,
            );
        }
        NavIcon::Settings => {
            // Three sliders.
            for (i, kx) in [-0.18f32, 0.12, -0.06].iter().enumerate() {
                let y = c.y + (i as f32 - 1.0) * s * 0.30;
                painter.line_segment(
                    [egui::pos2(c.x - s * 0.34, y), egui::pos2(c.x + s * 0.34, y)],
                    stroke,
                );
                painter.circle_filled(egui::pos2(c.x + kx * s, y), s * 0.10, color);
            }
        }
    }
}

/// Our own upload logo: a teal circle with a white upload arrow and tray.
pub(crate) fn upload_logo(ui: &mut egui::Ui, diameter: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(diameter, diameter), egui::Sense::hover());
    let painter = ui.painter();
    let c = rect.center();
    painter.circle_filled(c, diameter / 2.0, theme::TEAL);
    let white = Color32::WHITE;
    // Geometry scaled from the 84 pt reference size.
    let s = diameter / 84.0;
    let w = 4.0 * s;
    let cx = c.x;
    let arrow_top = c.y - 16.0 * s;
    let arrow_bottom = c.y + 6.0 * s;
    painter.line_segment(
        [egui::pos2(cx, arrow_bottom), egui::pos2(cx, arrow_top)],
        egui::Stroke::new(w, white),
    );
    painter.line_segment(
        [
            egui::pos2(cx, arrow_top),
            egui::pos2(cx - 11.0 * s, arrow_top + 11.0 * s),
        ],
        egui::Stroke::new(w, white),
    );
    painter.line_segment(
        [
            egui::pos2(cx, arrow_top),
            egui::pos2(cx + 11.0 * s, arrow_top + 11.0 * s),
        ],
        egui::Stroke::new(w, white),
    );
    painter.line_segment(
        [
            egui::pos2(cx - 18.0 * s, c.y + 19.0 * s),
            egui::pos2(cx + 18.0 * s, c.y + 19.0 * s),
        ],
        egui::Stroke::new(w, white),
    );
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
