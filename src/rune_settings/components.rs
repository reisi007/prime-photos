use crate::i18n;
use rune::tok::{self, Theme};
use rune::{El, col, el, row, text};

use super::SettingsHost;

pub(super) fn section<const N: usize>(
    key: &'static str,
    title: &'static str,
    theme: Theme,
    rows: [El; N],
) -> El {
    let mut card = col()
        .key(format!("{key}-card"))
        .w_full()
        .padding(4.0)
        .gap(0.0)
        .fill(theme.card)
        .radius(12.0)
        .border(1.0, theme.card_border)
        .shadow(1.0, 2.0, theme.shadow)
        .shadow(8.0, 24.0, theme.shadow_soft);
    for (index, setting) in rows.into_iter().enumerate() {
        card = card.child(setting);
        if index + 1 < N {
            card = card.child(
                el().key(format!("{key}-separator-{index}"))
                    .h(1.0)
                    .mx(12.0)
                    .fill(theme.separator)
                    .a11y_hidden(),
            );
        }
    }

    col()
        .key(key)
        .w_full()
        .gap(8.0)
        .child(
            text(i18n::tr(title))
                .key(format!("{key}-heading"))
                .font(tok::type_::SECTION)
                .color(theme.text_secondary)
                .padding(8.0),
        )
        .child(card)
}

pub(super) fn toggle_row<S: SettingsHost + 'static>(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    enabled: bool,
    action: fn(&mut S),
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    row()
        .key(key)
        .role("switch")
        .label(i18n::tr(label))
        .description(i18n::tr(description))
        .checked(enabled)
        .focusable(true)
        .w_full()
        .h(if description.is_empty() { 58.0 } else { 72.0 })
        .px(14.0)
        .center_y()
        .justify_between()
        .hover(theme.hover)
        .press(theme.press)
        .on_press(action)
        .child(
            col()
                .key(format!("{key}-copy"))
                .grow(1.0)
                .gap(4.0)
                .child(
                    text(i18n::tr(label))
                        .font(tok::type_::BODY)
                        .color(theme.text)
                        .a11y_hidden(),
                )
                .child(if description.is_empty() {
                    el().h(0.0).a11y_hidden()
                } else {
                    text(i18n::tr(description))
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary)
                        .wrap()
                        .a11y_hidden()
                }),
        )
        .child(toggle_control(key, enabled, theme, colors).a11y_hidden())
}

pub(super) fn toggle_dedup_default<S: SettingsHost>(state: &mut S) {
    let dedup_default = !state.config().dedup_default;
    state.config_mut().dedup_default = dedup_default;
    state.persist_settings();
}

fn toggle_control(
    key: &'static str,
    enabled: bool,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    const WIDTH: f32 = 38.0;
    const HEIGHT: f32 = 22.0;
    const KNOB: f32 = 18.0;
    let inset = (HEIGHT - KNOB) / 2.0;
    el().key(format!("{key}-switch"))
        .size(WIDTH, HEIGHT)
        .radius(HEIGHT / 2.0)
        .fill(if enabled {
            colors.toggle_on
        } else {
            colors.toggle_off
        })
        .border(1.0, colors.control_border)
        .motion(tok::motion::SNAPPY)
        .child(
            el().key(format!("{key}-knob"))
                .size(KNOB, KNOB)
                .absolute(if enabled { WIDTH - KNOB - inset } else { inset }, inset)
                .radius(KNOB / 2.0)
                .fill(rune::Color::WHITE)
                .shadow(1.0, 3.0, theme.knob_shadow),
        )
}
