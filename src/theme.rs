//! Visual theme close to the original Amazon Photos desktop app: white content
//! area, light-grey sidebar, teal links and bordered, rounded controls.

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, Vec2};

pub const TEAL: Color32 = Color32::from_rgb(0x00, 0x71, 0x85);
pub const INK: Color32 = Color32::from_rgb(0x0F, 0x11, 0x11);
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(0xF3, 0xF5, 0xF5);
pub const PANEL_BG: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const CARD_BG: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const BORDER: Color32 = Color32::from_rgb(0xDD, 0xE1, 0xE1);
pub const MUTED: Color32 = Color32::from_rgb(0x56, 0x5A, 0x5A);
pub const GREEN: Color32 = Color32::from_rgb(0x06, 0x7D, 0x2F);
pub const AMBER: Color32 = Color32::from_rgb(0xB0, 0x6A, 0x00);
pub const RED: Color32 = Color32::from_rgb(0xC4, 0x00, 0x00);

pub fn muted(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(MUTED)
}

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // --- Colours -------------------------------------------------------
    let v = &mut style.visuals;
    v.dark_mode = false;
    v.panel_fill = PANEL_BG;
    v.window_fill = PANEL_BG;
    v.faint_bg_color = Color32::from_rgb(0xF7, 0xF8, 0xF8);
    v.extreme_bg_color = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    v.override_text_color = Some(INK);
    v.hyperlink_color = TEAL;
    v.selection.bg_fill = Color32::from_rgb(0xD6, 0xEC, 0xF0);
    v.selection.stroke = Stroke::new(1.0_f32, TEAL);

    // Neutral, macOS-like controls.
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL_BG;
    w.noninteractive.weak_bg_fill = PANEL_BG;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);
    w.inactive.bg_fill = Color32::from_rgb(0xFA, 0xFB, 0xFB);
    w.inactive.weak_bg_fill = Color32::from_rgb(0xFA, 0xFB, 0xFB);
    w.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
    w.hovered.bg_fill = Color32::from_rgb(0xEF, 0xF3, 0xF3);
    w.hovered.weak_bg_fill = Color32::from_rgb(0xEF, 0xF3, 0xF3);
    w.hovered.bg_stroke = Stroke::new(1.0_f32, TEAL);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, INK);
    w.active.bg_fill = Color32::from_rgb(0xE4, 0xEE, 0xEF);
    w.active.weak_bg_fill = Color32::from_rgb(0xE4, 0xEE, 0xEF);
    w.active.bg_stroke = Stroke::new(1.0_f32, TEAL);
    w.active.fg_stroke = Stroke::new(1.0_f32, INK);
    w.open.bg_fill = Color32::from_rgb(0xEF, 0xF3, 0xF3);
    w.open.weak_bg_fill = Color32::from_rgb(0xEF, 0xF3, 0xF3);

    // --- Spacing -------------------------------------------------------
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size = Vec2::new(36.0, 26.0);

    // --- Type scale ----------------------------------------------------
    style.text_styles = [
        (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
    ]
    .into();

    ctx.set_style(style);
}

/// A prominent dark, pill-shaped primary button (like "Datensicherung hinzufügen …").
pub fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .color(Color32::WHITE)
            .size(14.0),
    )
    .fill(INK)
}

/// Secondary, bordered button.
pub fn secondary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(INK).size(14.0))
        .fill(Color32::from_rgb(0xFA, 0xFB, 0xFB))
        .stroke(Stroke::new(1.0_f32, BORDER))
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD_BG)
        .inner_margin(egui::Margin::same(12))
        .outer_margin(egui::Margin::same(0))
}
