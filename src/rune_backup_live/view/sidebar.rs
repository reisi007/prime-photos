use super::super::{LivePage, LiveState, account_identity};
use super::HEADER_HEIGHT;
use crate::i18n::tr;
use crate::rune_theme::Palette;
use rune::tok;
use rune::{El, Tx, col, el, row, text};

const SIDEBAR_WIDTH: f32 = 228.0;

pub(super) fn sidebar(
    state: &LiveState,
    theme: tok::Theme,
    colors: Palette,
    titlebar_inset: f32,
) -> El {
    let backup_active = state.page != LivePage::Settings;
    col()
        .key("rune-backup-sidebar")
        .w(SIDEBAR_WIDTH)
        .h_full()
        .padding(12.0)
        .gap(8.0)
        .fill(theme.card)
        .border(1.0, theme.separator)
        .child(
            row()
                .key("sidebar-brand")
                .w_full()
                .h(HEADER_HEIGHT)
                .pl(titlebar_inset)
                .center_y()
                .window_drag()
                .child(
                    text("Prime Photos")
                        .key("sidebar-brand-name")
                        .font(tok::type_::TITLE)
                        .color(theme.text),
                ),
        )
        .child(
            col()
                .key("sidebar-navigation")
                .role("navigation")
                .label(tr("Hauptnavigation"))
                .w_full()
                .child(
                    col()
                        .key("sidebar-tablist")
                        .role("tablist")
                        .label(tr("Hauptbereiche"))
                        .focus_group()
                        .w_full()
                        .gap(4.0)
                        .child(sidebar_nav_item(
                            "sidebar-nav-backup",
                            tr("Datensicherung").to_owned(),
                            "▤",
                            backup_active,
                            theme,
                            colors,
                            |state: &mut LiveState, _| state.page = LivePage::Backup,
                        ))
                        .child(sidebar_nav_item(
                            "sidebar-nav-settings",
                            tr("Einstellungen").to_owned(),
                            "⚙",
                            !backup_active,
                            theme,
                            colors,
                            |state: &mut LiveState, _| state.page = LivePage::Settings,
                        )),
                ),
        )
        .child(col().key("sidebar-navigation-spacer").grow(1.0))
        .child(
            col()
                .key("sidebar-account-footer")
                .w_full()
                .gap(10.0)
                .child(
                    el().key("sidebar-account-divider")
                        .w_full()
                        .h(1.0)
                        .fill(theme.separator),
                )
                .child(account_identity::sidebar_account(state, theme)),
        )
}

fn sidebar_nav_item(
    key: &'static str,
    label: String,
    icon: &'static str,
    active: bool,
    theme: tok::Theme,
    colors: Palette,
    action: impl Fn(&mut LiveState, &mut Tx) + 'static,
) -> El {
    let marker = if active {
        el().key(format!("{key}-active"))
            .w(3.0)
            .h(18.0)
            .radius(2.0)
            .fill(colors.primary_fill)
    } else {
        el().w(3.0).h(18.0)
    };
    el().key(key)
        .role("tab")
        .label(label.clone())
        .focusable(true)
        .focus_selected(active)
        .w_full()
        .h(42.0)
        .px(10.0)
        .center_y()
        .radius(8.0)
        .fill(if active {
            theme.control_track
        } else {
            theme.card
        })
        .hover(theme.hover)
        .focus_ring(2.0, 2.0, colors.focus_ring)
        .on_press_tx(action)
        .child(
            row()
                .gap(10.0)
                .center_y()
                .child(marker)
                .child(
                    text(icon)
                        .font(tok::type_::BODY)
                        .color(if active {
                            colors.primary_fill
                        } else {
                            theme.text_secondary
                        })
                        .a11y_hidden(),
                )
                .child(
                    text(label)
                        .font(tok::type_::BODY)
                        .color(theme.text)
                        .a11y_hidden(),
                ),
        )
}

#[cfg(test)]
mod tests;
