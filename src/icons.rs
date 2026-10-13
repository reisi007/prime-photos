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

/// A filled status dot (replaces the "●" glyph, which rendered as a tofu box).
pub(crate) fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// A simple avatar placeholder (head + shoulders inside a filled circle).
pub(crate) fn draw_avatar(ui: &mut egui::Ui, diameter: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(diameter, diameter), egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    let c = rect.center();
    painter.circle_filled(c, diameter / 2.0, color);
    let white = Color32::WHITE;
    painter.circle_filled(egui::pos2(c.x, c.y - diameter * 0.14), diameter * 0.16, white);
    painter.circle_filled(egui::pos2(c.x, c.y + diameter * 0.32), diameter * 0.28, white);
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
