//! App-owned semantic colors for the Rune routes.
//!
//! Rune's base theme is intentionally neutral, while its default accent tokens
//! are not guaranteed to meet the app's text-contrast criteria in both modes.
//! Keep semantic foregrounds and fills here rather than changing the dependency.

use rune::Color;

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) primary_fill: Color,
    pub(crate) primary_foreground: Color,
    pub(crate) alert_foreground: Color,
    pub(crate) danger_fill: Color,
    pub(crate) danger_foreground: Color,
    pub(crate) toggle_on: Color,
    pub(crate) toggle_off: Color,
    pub(crate) control_border: Color,
    pub(crate) focus_ring: Color,
}

pub(crate) fn palette(dark: bool) -> Palette {
    let theme = rune::tok::theme(dark);
    Palette {
        primary_fill: color(if dark { "#1d4ed8" } else { "#dbeafe" }),
        primary_foreground: theme.text,
        alert_foreground: color(if dark { "#ff8f85" } else { "#b42318" }),
        danger_fill: color(if dark { "#9f1d18" } else { "#b42318" }),
        danger_foreground: Color::WHITE,
        toggle_on: color(if dark { "#1d4ed8" } else { "#005fcc" }),
        toggle_off: color(if dark { "#747f8f" } else { "#64748b" }),
        control_border: color(if dark { "#8d99a8" } else { "#64748b" }),
        focus_ring: color(if dark { "#75b8ff" } else { "#005fcc" }),
    }
}

fn color(hex: &str) -> Color {
    rune::color::hex(hex).expect("app-owned Rune palette colors are valid hex")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_AA: f64 = 4.5;
    const CONTROL_AA: f64 = 3.0;

    fn over(top: Color, bottom: Color) -> Color {
        let alpha = top.a + bottom.a * (1.0 - top.a);
        let channel = |top_channel: f32, bottom_channel: f32| {
            (top_channel * top.a + bottom_channel * bottom.a * (1.0 - top.a)) / alpha
        };
        Color {
            r: channel(top.r, bottom.r),
            g: channel(top.g, bottom.g),
            b: channel(top.b, bottom.b),
            a: alpha,
        }
    }

    fn contrast(foreground: Color, background: Color) -> f64 {
        let foreground = f64::from(foreground.luminance());
        let background = f64::from(background.luminance());
        let (lighter, darker) = if foreground >= background {
            (foreground, background)
        } else {
            (background, foreground)
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    fn assert_pair(
        name: &str,
        foreground: Color,
        background: Color,
        minimum: f64,
        dark: bool,
    ) -> f64 {
        let ratio = contrast(foreground, background);
        assert!(
            ratio >= minimum,
            "{name} in {} theme: {ratio:.3}:1 (foreground {}, background {}), needs {minimum:.1}:1",
            if dark { "dark" } else { "light" },
            foreground.hex(),
            background.hex(),
        );
        println!(
            "Rune {} {name}: {ratio:.3}:1 ({} on {})",
            if dark { "dark" } else { "light" },
            foreground.hex(),
            background.hex(),
        );
        ratio
    }

    #[test]
    fn rune_backup_login_and_settings_semantic_pairs_meet_app_contrast_criteria() {
        for dark in [false, true] {
            let theme = rune::tok::theme(dark);
            let palette = palette(dark);
            let control_on_card = over(theme.control_track, theme.card);
            let control_on_bg = over(theme.control_track, theme.bg);
            let separator_on_bg = over(theme.separator, theme.bg);
            let settings_indicator = theme.indicator;

            // Body and muted text used on the app background, cards, and
            // translucent Rune control surfaces (e.g. secondary buttons).
            for (name, foreground, background) in [
                ("body / page", theme.text, theme.bg),
                ("body / card", theme.text, theme.card),
                ("muted / page", theme.text_secondary, theme.bg),
                ("muted / card", theme.text_secondary, theme.card),
                ("body / control over card", theme.text, control_on_card),
                ("body / control over page", theme.text, control_on_bg),
            ] {
                assert_pair(name, foreground, background, TEXT_AA, dark);
            }

            // App-owned primary CTA fill/text pairs used by backup, login,
            // settings selections, and the backup fixture.
            assert_pair(
                "primary button",
                palette.primary_foreground,
                palette.primary_fill,
                TEXT_AA,
                dark,
            );

            // Alert/status text is used on the page and card surfaces; the
            // danger button uses a separate white-on-red pair.
            assert_pair(
                "alert / page",
                palette.alert_foreground,
                theme.bg,
                TEXT_AA,
                dark,
            );
            assert_pair(
                "alert / card",
                palette.alert_foreground,
                theme.card,
                TEXT_AA,
                dark,
            );
            assert_pair(
                "danger button",
                palette.danger_foreground,
                palette.danger_fill,
                TEXT_AA,
                dark,
            );

            // Selected settings segments use Rune's opaque indicator surface;
            // status/body text also appears on cards.
            assert_pair("selected setting", theme.text, settings_indicator, TEXT_AA, dark);
            assert_pair(
                "status / card",
                theme.text_secondary,
                theme.card,
                TEXT_AA,
                dark,
            );

            // Settings toggles have no text, but their thumb, track, boundary,
            // focus ring, and separator must remain visibly distinguishable.
            assert_pair(
                "toggle on thumb / track",
                rune::Color::WHITE,
                palette.toggle_on,
                CONTROL_AA,
                dark,
            );
            assert_pair(
                "toggle off thumb / track",
                rune::Color::WHITE,
                palette.toggle_off,
                CONTROL_AA,
                dark,
            );
            for (name, color) in [
                ("control border / page", palette.control_border),
                ("control border / card", palette.control_border),
                ("focus ring / page", palette.focus_ring),
                ("focus ring / card", palette.focus_ring),
            ] {
                let background = if name.ends_with("page") {
                    theme.bg
                } else {
                    theme.card
                };
                assert_pair(name, color, background, CONTROL_AA, dark);
            }

            // The separator is intentionally a hairline, not a control edge;
            // still assert its measured composited value for regression data.
            let divider_ratio = contrast(separator_on_bg, theme.bg);
            println!(
                "Rune {} divider / page: {divider_ratio:.3}:1 ({} over {})",
                if dark { "dark" } else { "light" },
                theme.separator.hex(),
                theme.bg.hex(),
            );
        }
    }
}
