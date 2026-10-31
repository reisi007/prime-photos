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

/// Page background behind the content (`#F7F8F8`). Cards and controls stay
/// white, so the page reads as one surface with raised cards.
pub const PAGE_BG: Color32 = Color32::from_rgb(0xF7, 0xF8, 0xF8);
/// Horizontal gutter between the window edge and every page's content.
pub const PAGE_MARGIN_X: i8 = 16;
/// Vertical gutter above and below every page's content.
pub const PAGE_MARGIN_Y: i8 = 12;
/// Horizontal gutter of the navigation sidebar's content.
pub const SIDEBAR_MARGIN_X: i8 = 16;
/// Vertical gutter of the navigation sidebar's content.
pub const SIDEBAR_MARGIN_Y: i8 = 12;

pub fn muted(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(MUTED)
}

pub fn apply(ctx: &egui::Context) {
    // Apply to **both** style maps (light and dark), not only the active one.
    // `set_style` writes just the style of the currently selected theme; egui
    // then switches to the system theme (light) on the first frame and selects
    // its default light style, discarding every custom value — spacing,
    // reserved scroll-bar width, panel fill, fonts. `all_styles_mut` keeps the
    // custom look whichever theme preference the OS/app ends up using (the app
    // is intentionally light-only: `dark_mode = false` in both).
    ctx.all_styles_mut(apply_style);
}

/// The custom style, shared by light and dark so a theme switch cannot drop it.
fn apply_style(style: &mut egui::Style) {
    // `Style::default()` carries `Visuals::dark()` (egui 0.31: `Visuals::default()
    // = Visuals::dark()`), so start from the **light** visuals explicitly. Both
    // maps get the same light-based style, and fields this theme does not
    // overwrite (window stroke/shadow, menu and warn colours) have no dark
    // leftovers in windows and popups.
    *style = egui::Style {
        visuals: egui::Visuals::light(),
        ..egui::Style::default()
    };

    // --- Colours -------------------------------------------------------
    let v = &mut style.visuals;
    v.dark_mode = false;
    v.panel_fill = PANEL_BG;
    v.window_fill = PANEL_BG;
    v.faint_bg_color = Color32::from_rgb(0xF7, 0xF8, 0xF8);
    v.extreme_bg_color = Color32::from_rgb(0xFF, 0xFF, 0xFF);
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

    // Platz für den Floating-Balken reservieren, damit kein Inhalt unter ihm
    // liegt. egui 0.31 nutzt per Default `ScrollStyle::floating()`, dessen
    // `floating_allocated_width` 0 ist — die ScrollArea rechnete deshalb mit
    // voller Innenbreite, und der rechte Rand der Ordner-Karte lag unter dem
    // 10 px breiten Griff (der erst bei Hover so breit wird). Bugreport 2026-10-17.
    // Der reservierte Streifen besteht aus breitester Griff **plus** sichtbarem
    // Abstand: egui malt den Griff rechtsbündig in den Streifen, ein reiner
    // Griffsstreifen ließ ihn also bündig am Kartenrand kleben (im Live-Bild vom
    // Nutzer am 2026-10-17 als „nicht gefixt" zurückgemeldet). Der Abstand
    // entspricht dem `item_spacing` des Themes.
    const SCROLL_BAR_GAP: f32 = 8.0;
    let scroll = &mut style.spacing.scroll;
    scroll.floating_allocated_width = scroll.bar_width + SCROLL_BAR_GAP;

    // --- Type scale ----------------------------------------------------
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(24.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();
}

/// A prominent dark, pill-shaped primary button (like "Datensicherung hinzufügen …").
pub fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).size(14.0)).fill(INK)
}

/// Secondary, bordered button.
pub fn secondary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(INK).size(14.0))
        .fill(Color32::from_rgb(0xFA, 0xFB, 0xFB))
        .stroke(Stroke::new(1.0_f32, BORDER))
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    #[test]
    fn floating_scroll_bar_reserves_its_full_width() {
        // Der reservierte Streifen muss den breitesten Griff decken (sonst malt
        // egui den Balken über die Ordner-Karte) und darüber hinaus den
        // sichtbaren Abstand zwischen Inhalt und Balken freihalten.
        let ctx = egui::Context::default();
        super::apply(&ctx);
        let scroll = ctx.style().spacing.scroll;
        assert!(scroll.allocated_width() > 0.0);
        assert!(scroll.allocated_width() >= scroll.bar_width);
        // Der Reststreifen neben dem Griff ist der sichtbare Abstand zum Inhalt.
        assert!(scroll.allocated_width() - scroll.bar_width >= 8.0);
    }

    #[test]
    fn custom_style_survives_a_theme_switch() {
        // Regression (Issue #30): `set_style` wrote only the active style, and
        // the first frame's switch to the system theme replaced it with egui's
        // default light style. `all_styles_mut` must keep the custom values in
        // both theme maps, whichever preference is active afterwards.
        let ctx = egui::Context::default();
        super::apply(&ctx);
        // Fields the custom theme deliberately does not overwrite must still
        // come from the **light** visuals. `Style::default()` would carry
        // `Visuals::dark()` (egui 0.31), so a future revert would fail here:
        // the About and backup-config windows would then get a dark window
        // stroke/shadow and dark warn colours.
        let light = egui::Visuals::light();
        let dark = egui::Visuals::dark();
        assert_ne!(light.window_stroke.color, dark.window_stroke.color);
        assert_ne!(light.window_shadow.color, dark.window_shadow.color);
        assert_ne!(light.warn_fg_color, dark.warn_fg_color);
        for preference in [egui::ThemePreference::Light, egui::ThemePreference::Dark] {
            ctx.set_theme(preference);
            let style = ctx.style();
            assert!(
                !style.visuals.dark_mode,
                "the app is light-only, also under {preference:?}"
            );
            assert_eq!(
                style.spacing.item_spacing,
                egui::vec2(8.0, 8.0),
                "spacing lost under {preference:?}"
            );
            assert_eq!(style.spacing.button_padding, egui::vec2(12.0, 7.0));
            assert_eq!(style.spacing.scroll.allocated_width(), 18.0);
            assert_eq!(style.spacing.scroll.floating_allocated_width, 18.0);
            assert_eq!(style.visuals.panel_fill, super::PANEL_BG);
            assert_eq!(style.visuals.widgets.noninteractive.fg_stroke.color, super::INK);
            assert_eq!(
                style.visuals.window_stroke.color, light.window_stroke.color,
                "dark window stroke under {preference:?}"
            );
            assert_eq!(
                style.visuals.window_shadow.color, light.window_shadow.color,
                "dark window shadow under {preference:?}"
            );
            assert_eq!(
                style.visuals.warn_fg_color, light.warn_fg_color,
                "dark warn colour under {preference:?}"
            );
            assert_eq!(
                style.text_styles[&egui::TextStyle::Body].size,
                14.0,
                "type scale lost under {preference:?}"
            );
        }
    }
}
