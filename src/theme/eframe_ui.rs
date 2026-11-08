//! Prime Photos light and dark palettes and egui styles.

use crate::config::Appearance;
use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, Vec2};

/// Horizontal gutter between the window edge and every page's content.
pub const PAGE_MARGIN_X: i8 = 16;
/// Vertical gutter above and below every page's content.
pub const PAGE_MARGIN_Y: i8 = 12;
/// Horizontal gutter of the navigation sidebar's content.
pub const SIDEBAR_MARGIN_X: i8 = 16;
/// Vertical gutter of the navigation sidebar's content.
pub const SIDEBAR_MARGIN_Y: i8 = 12;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Palette {
    pub(crate) page_bg: Color32,
    pub(crate) sidebar_bg: Color32,
    pub(crate) panel_bg: Color32,
    pub(crate) card_bg: Color32,
    pub(crate) border: Color32,
    pub(crate) ink: Color32,
    pub(crate) muted: Color32,
    weak_text_target: Color32,
    pub(crate) teal: Color32,
    pub(crate) green: Color32,
    pub(crate) amber: Color32,
    pub(crate) red: Color32,
    pub(crate) control_bg: Color32,
    pub(crate) control_hover_bg: Color32,
    pub(crate) control_active_bg: Color32,
    pub(crate) slider_track: Color32,
    pub(crate) slider_active: Color32,
    pub(crate) focus: Color32,
    pub(crate) primary_bg: Color32,
    pub(crate) primary_fg: Color32,
}

const LIGHT: Palette = Palette {
    page_bg: Color32::from_rgb(0xF7, 0xF8, 0xF8),
    sidebar_bg: Color32::from_rgb(0xE7, 0xEB, 0xEC),
    panel_bg: Color32::WHITE,
    card_bg: Color32::WHITE,
    border: Color32::from_rgb(0x70, 0x7A, 0x7D),
    ink: Color32::from_rgb(0x10, 0x14, 0x14),
    muted: Color32::from_rgb(0x4A, 0x55, 0x58),
    weak_text_target: Color32::from_rgb(0x84, 0x96, 0x9C),
    teal: Color32::from_rgb(0x00, 0x67, 0x75),
    green: Color32::from_rgb(0x12, 0x63, 0x2D),
    amber: Color32::from_rgb(0x70, 0x44, 0x00),
    red: Color32::from_rgb(0xB4, 0x23, 0x18),
    control_bg: Color32::from_rgb(0xF1, 0xF4, 0xF4),
    control_hover_bg: Color32::from_rgb(0xE4, 0xEF, 0xF0),
    control_active_bg: Color32::from_rgb(0xD5, 0xE8, 0xEB),
    slider_track: Color32::from_rgb(0x53, 0x5E, 0x61),
    slider_active: Color32::from_rgb(0x68, 0xCA, 0xDB),
    focus: Color32::from_rgb(0x00, 0x67, 0x75),
    primary_bg: Color32::from_rgb(0x11, 0x17, 0x19),
    primary_fg: Color32::WHITE,
};

const DARK: Palette = Palette {
    page_bg: Color32::from_rgb(0x11, 0x17, 0x19),
    sidebar_bg: Color32::from_rgb(0x1A, 0x22, 0x24),
    panel_bg: Color32::from_rgb(0x20, 0x28, 0x2A),
    card_bg: Color32::from_rgb(0x25, 0x2D, 0x30),
    border: Color32::from_rgb(0x77, 0x85, 0x89),
    ink: Color32::from_rgb(0xF1, 0xF5, 0xF5),
    muted: Color32::from_rgb(0xBA, 0xC4, 0xC6),
    weak_text_target: Color32::from_rgb(0x84, 0x94, 0x98),
    teal: Color32::from_rgb(0x72, 0xD8, 0xE8),
    green: Color32::from_rgb(0x83, 0xDA, 0x9C),
    amber: Color32::from_rgb(0xFF, 0xD2, 0x77),
    red: Color32::from_rgb(0xFF, 0x93, 0x88),
    control_bg: Color32::from_rgb(0x2B, 0x34, 0x36),
    control_hover_bg: Color32::from_rgb(0x35, 0x42, 0x45),
    control_active_bg: Color32::from_rgb(0x3C, 0x4B, 0x4E),
    slider_track: Color32::from_rgb(0x78, 0x87, 0x8A),
    slider_active: Color32::from_rgb(0x00, 0x41, 0x4A),
    focus: Color32::from_rgb(0x72, 0xD8, 0xE8),
    primary_bg: Color32::from_rgb(0xE6, 0xF7, 0xF9),
    primary_fg: Color32::from_rgb(0x17, 0x21, 0x23),
};

pub(crate) const fn palette(dark: bool) -> Palette {
    if dark { DARK } else { LIGHT }
}

/// Use egui's weak-text role so it follows the active theme's tested palette.
pub fn muted(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).weak()
}

pub fn apply(ctx: &egui::Context, appearance: Appearance) {
    ctx.style_mut_of(egui::Theme::Light, |style| apply_style(style, false));
    ctx.style_mut_of(egui::Theme::Dark, |style| apply_style(style, true));
    // Unsupported native theme reporting falls back to the long-standing light
    // appearance instead of egui's dark default. On supported platforms System
    // still follows the Theme in native eframe/winit input.
    ctx.options_mut(|options| options.fallback_theme = egui::Theme::Light);
    ctx.set_theme(match appearance {
        Appearance::System => egui::ThemePreference::System,
        Appearance::Light => egui::ThemePreference::Light,
        Appearance::Dark => egui::ThemePreference::Dark,
    });
}

fn apply_style(style: &mut egui::Style, dark: bool) {
    let colors = palette(dark);
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.dark_mode = dark;
    visuals.panel_fill = colors.panel_bg;
    visuals.window_fill = colors.panel_bg;
    visuals.window_stroke = Stroke::new(1.0_f32, colors.border);
    visuals.faint_bg_color = colors.page_bg;
    visuals.extreme_bg_color = if dark {
        Color32::from_rgb(0x0B, 0x10, 0x11)
    } else {
        Color32::WHITE
    };
    visuals.code_bg_color = colors.control_bg;
    visuals.hyperlink_color = colors.teal;
    visuals.warn_fg_color = colors.amber;
    visuals.error_fg_color = colors.red;
    visuals.text_cursor.stroke = Stroke::new(1.0_f32, colors.ink);
    visuals.selection.bg_fill = colors.slider_active;
    visuals.selection.stroke = Stroke::new(2.0_f32, colors.focus);
    visuals.slider_trailing_fill = true;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = colors.panel_bg;
    widgets.noninteractive.weak_bg_fill = colors.weak_text_target;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, colors.border);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, colors.ink);
    widgets.inactive.bg_fill = colors.control_bg;
    widgets.inactive.weak_bg_fill = colors.control_bg;
    widgets.inactive.bg_stroke = Stroke::new(1.0_f32, colors.border);
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, colors.ink);
    widgets.hovered.bg_fill = colors.control_hover_bg;
    widgets.hovered.weak_bg_fill = colors.control_hover_bg;
    widgets.hovered.bg_stroke = Stroke::new(2.0_f32, colors.focus);
    widgets.hovered.fg_stroke = Stroke::new(1.0_f32, colors.ink);
    widgets.active.bg_fill = colors.control_active_bg;
    widgets.active.weak_bg_fill = colors.control_active_bg;
    widgets.active.bg_stroke = Stroke::new(2.0_f32, colors.focus);
    widgets.active.fg_stroke = Stroke::new(1.0_f32, colors.ink);
    widgets.open.bg_fill = colors.control_hover_bg;
    widgets.open.weak_bg_fill = colors.control_hover_bg;
    widgets.open.bg_stroke = Stroke::new(1.0_f32, colors.focus);
    widgets.open.fg_stroke = Stroke::new(1.0_f32, colors.ink);

    *style = egui::Style {
        visuals,
        ..egui::Style::default()
    };
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size = Vec2::new(36.0, 26.0);
    style.spacing.slider_rail_height = 4.0;

    // Keep the reserved scrollbar width (10 px handle + 8 px content gap).
    const SCROLL_BAR_GAP: f32 = 8.0;
    let scroll = &mut style.spacing.scroll;
    scroll.floating_allocated_width = scroll.bar_width + SCROLL_BAR_GAP;

    style.text_styles = [
        (TextStyle::Heading, FontId::new(24.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
    ]
    .into();
}

/// Prominent action button with theme-specific high-contrast colors.
pub fn primary_button(text: &str, dark: bool) -> egui::Button<'static> {
    let colors = palette(dark);
    egui::Button::new(
        egui::RichText::new(text)
            .color(colors.primary_fg)
            .size(14.0)
            .strong(),
    )
    .fill(colors.primary_bg)
}

/// Secondary action with a theme-specific fill and state-dependent outline.
pub fn secondary_button(text: &str, dark: bool) -> egui::Button<'static> {
    let colors = palette(dark);
    egui::Button::new(egui::RichText::new(text).color(colors.ink).size(14.0))
        .fill(colors.control_bg)
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    fn luminance(color: egui::Color32) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    fn contrast(a: egui::Color32, b: egui::Color32) -> f64 {
        let (lighter, darker) = if luminance(a) >= luminance(b) {
            (luminance(a), luminance(b))
        } else {
            (luminance(b), luminance(a))
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn text_and_semantic_colors_meet_wcag_aa_on_every_surface() {
        for dark in [false, true] {
            let colors = super::palette(dark);
            let surfaces = [
                colors.page_bg,
                colors.sidebar_bg,
                colors.panel_bg,
                colors.card_bg,
                colors.control_bg,
            ];
            for text in [
                colors.ink,
                colors.muted,
                colors.teal,
                colors.green,
                colors.amber,
                colors.red,
            ] {
                for surface in surfaces {
                    assert!(
                        contrast(text, surface) >= 4.5,
                        "text {text:?} on {surface:?} in dark={dark}: {}:1",
                        contrast(text, surface)
                    );
                }
            }
            assert!(contrast(colors.primary_fg, colors.primary_bg) >= 4.5);
        }
    }

    #[test]
    fn control_boundaries_slider_rails_and_focus_are_distinguishable() {
        for dark in [false, true] {
            let colors = super::palette(dark);
            for surface in [colors.page_bg, colors.panel_bg, colors.card_bg] {
                assert!(contrast(colors.border, surface) >= 3.0);
                assert!(contrast(colors.focus, surface) >= 3.0);
            }
            assert!(contrast(colors.slider_track, colors.panel_bg) >= 3.0);
            assert!(contrast(colors.slider_active, colors.slider_track) >= 3.0);
        }
    }

    #[test]
    fn both_style_maps_keep_their_palette_and_existing_layout() {
        let ctx = egui::Context::default();
        super::apply(&ctx, crate::config::Appearance::Light);

        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let dark = theme == egui::Theme::Dark;
            let colors = super::palette(dark);
            let style = ctx.style_of(theme);
            assert_eq!(style.visuals.dark_mode, dark);
            assert_eq!(style.visuals.panel_fill, colors.panel_bg);
            assert_eq!(style.visuals.weak_text_color(), colors.muted);
            assert_eq!(style.visuals.selection.bg_fill, colors.slider_active);
            assert!(style.visuals.slider_trailing_fill);
            assert_eq!(style.spacing.item_spacing, egui::vec2(8.0, 8.0));
            assert_eq!(style.spacing.button_padding, egui::vec2(12.0, 7.0));
            assert_eq!(style.spacing.scroll.allocated_width(), 18.0);
            assert_eq!(style.spacing.scroll.floating_allocated_width, 18.0);
            assert_eq!(style.spacing.slider_rail_height, 4.0);
            assert_eq!(style.text_styles[&egui::TextStyle::Body].size, 14.0);
        }

        for (preference, expected) in [
            (crate::config::Appearance::Light, egui::Theme::Light),
            (crate::config::Appearance::Dark, egui::Theme::Dark),
        ] {
            super::apply(&ctx, preference);
            assert_eq!(ctx.theme(), expected);
        }
    }

    #[test]
    fn system_appearance_follows_native_egui_input() {
        let ctx = egui::Context::default();
        super::apply(&ctx, crate::config::Appearance::System);
        for system_theme in [egui::Theme::Light, egui::Theme::Dark] {
            let input = egui::RawInput {
                system_theme: Some(system_theme),
                ..Default::default()
            };
            let _ = ctx.run(input, |_| {});
            assert_eq!(ctx.system_theme(), Some(system_theme));
            assert_eq!(ctx.theme(), system_theme);
            assert_eq!(ctx.style().visuals.dark_mode, system_theme == egui::Theme::Dark);
        }
        // Explicit choices override a conflicting OS appearance.
        super::apply(&ctx, crate::config::Appearance::Light);
        let input = egui::RawInput {
            system_theme: Some(egui::Theme::Dark),
            ..Default::default()
        };
        let _ = ctx.run(input, |_| {});
        assert_eq!(ctx.theme(), egui::Theme::Light);
    }

    #[test]
    fn floating_scroll_bar_reserves_its_full_width() {
        let ctx = egui::Context::default();
        super::apply(&ctx, crate::config::Appearance::System);
        let scroll = ctx.style().spacing.scroll;
        assert!(scroll.allocated_width() > 0.0);
        assert!(scroll.allocated_width() >= scroll.bar_width);
        assert!(scroll.allocated_width() - scroll.bar_width >= 8.0);
    }
}
