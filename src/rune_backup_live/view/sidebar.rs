use super::super::{LivePage, LiveState, account_identity};
use super::HEADER_HEIGHT;
use crate::i18n::tr;
use crate::icons::rune_ui::{BRAND_LOGO_PX, NAV_ICON_PX, NavIcon, brand_logo, nav_icon};
use crate::rune_theme::Palette;
use rune::tok;
use rune::{El, Tx, col, el, row, text};

const SIDEBAR_WIDTH: f32 = 228.0;
/// Optical left edge shared by the logo, the navigation icons and the account
/// block: the sidebar's 12 pt padding plus the 10 pt inset the navigation rows
/// already use for their icons.
const CONTENT_INSET: f32 = 10.0;
/// Vertical clearance the brand row keeps below the macOS traffic lights. The
/// lights overlay the window's top-left corner because the content view fills
/// the titlebar, so the brand must not be indented horizontally — that would
/// pull the logo off the navigation's left edge.
const BRAND_TITLEBAR_CLEARANCE: f32 = 10.0;

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
                .pl(CONTENT_INSET)
                .pt(if titlebar_inset > 0.0 {
                    BRAND_TITLEBAR_CLEARANCE
                } else {
                    0.0
                })
                .gap(8.0)
                .center_y()
                .window_drag()
                .child(
                    el().key("sidebar-brand-logo")
                        .w(BRAND_LOGO_PX)
                        .h(BRAND_LOGO_PX)
                        .pixels(&brand_logo())
                        .a11y_hidden(),
                )
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
                            NavIcon::Backup,
                            backup_active,
                            theme,
                            colors,
                            |state: &mut LiveState, _| state.page = LivePage::Backup,
                        ))
                        .child(sidebar_nav_item(
                            "sidebar-nav-settings",
                            tr("Einstellungen").to_owned(),
                            NavIcon::Settings,
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
                .child(
                    col()
                        .w_full()
                        .pl(CONTENT_INSET)
                        .child(account_identity::sidebar_account(state, theme)),
                ),
        )
}

fn sidebar_nav_item(
    key: &'static str,
    label: String,
    icon: NavIcon,
    active: bool,
    theme: tok::Theme,
    colors: Palette,
    action: impl Fn(&mut LiveState, &mut Tx) + 'static,
) -> El {
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
        // Selected row: the same subtle fill as the established sidebar. No
        // extra marker — the row fill plus the teal icon carry the state.
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
                .child(
                    el().key(format!("{key}-icon"))
                        .w(NAV_ICON_PX)
                        .h(NAV_ICON_PX)
                        .pixels(&nav_icon(
                            icon,
                            if active {
                                colors.nav_accent
                            } else {
                                colors.nav_muted
                            },
                        ))
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
