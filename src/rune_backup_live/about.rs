use super::widgets::action_button;
use super::{LivePage, LiveState};
use crate::about::LicenseTab;
use crate::i18n::{tr, tr1};
use rune::tok;
use rune::{El, GrowthSpec, ScrollSpec, Tx, col, el, row, text};
use tok::Theme;

const LICENSE_ROW_HEIGHT: f32 = 16.0;

pub(super) trait AboutRouteHost {
    fn page(&self) -> LivePage;
    fn set_page(&mut self, page: LivePage);
    fn license_tab(&self) -> LicenseTab;
    fn set_license_tab(&mut self, tab: LicenseTab);
}

impl AboutRouteHost for LiveState {
    fn page(&self) -> LivePage {
        self.page
    }

    fn set_page(&mut self, page: LivePage) {
        self.page = page;
    }

    fn license_tab(&self) -> LicenseTab {
        self.license_tab
    }

    fn set_license_tab(&mut self, tab: LicenseTab) {
        self.license_tab = tab;
    }
}

pub(super) fn about_entry_button<S: AboutRouteHost + 'static>(
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    action_button::<S>(
        "open-about",
        tr("Über Prime Photos").to_owned(),
        theme,
        theme.control_track,
        theme.text,
        |state: &mut S, _: &mut Tx| state.set_page(LivePage::About),
    )
    .key("open-about")
    .hover(theme.hover)
    .focus_ring(2.0, 2.0, colors.focus_ring)
}

pub(super) fn route_content<S: AboutRouteHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    match state.page() {
        LivePage::About => about_content(state, theme, colors),
        LivePage::Licenses => licenses_content::<S>(state, theme),
        LivePage::Backup | LivePage::Settings => el(),
    }
}

fn about_content<S: AboutRouteHost + 'static>(
    _state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let version = match option_env!("PRIME_PHOTOS_GIT_COMMIT") {
        Some(sha) => format!("{} ({sha})", env!("CARGO_PKG_VERSION")),
        None => env!("CARGO_PKG_VERSION").to_owned(),
    };
    let current_year = crate::about::current_year();
    col()
        .key("rune-about-page")
        .role("region")
        .label(tr("Über Prime Photos"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .child(
            col()
                .key("about-content")
                .w(480.0)
                .gap(12.0)
                .center_y()
                .child(
                    text(tr("Prime Photos"))
                        .key("about-product-name")
                        .font(tok::TextStyle::new(22.0, 700.0))
                        .color(theme.text),
                )
                .child(
                    text(tr1("Version: {}", version))
                        .key("about-version")
                        .font(tok::type_::BODY)
                        .color(theme.text_secondary),
                )
                .child(action_button(
                    "license-open",
                    tr("Drittanbieterlizenzen").to_owned(),
                    theme,
                    theme.control_track,
                    theme.text,
                    |state: &mut S, _: &mut Tx| state.set_page(LivePage::Licenses),
                ))
                .child(action_button(
                    "about-close",
                    tr("Schließen").to_owned(),
                    theme,
                    colors.primary_fill,
                    colors.primary_foreground,
                    |state: &mut S, _: &mut Tx| state.set_page(LivePage::Backup),
                ))
                .child(
                    text(tr1("© {} Prime Photos", current_year))
                        .key("about-copyright")
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
}

fn licenses_content<S: AboutRouteHost + 'static>(state: &S, theme: Theme) -> El {
    let current_tab = state.license_tab();
    let content = match current_tab {
        LicenseTab::Components => component_licenses::<S>(theme),
        LicenseTab::Icons => icon_attributions(theme),
    };
    col()
        .key("rune-licenses-page")
        .role("region")
        .label(tr("Drittanbieterlizenzen"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            row()
                .key("licenses-navigation")
                .w_full()
                .h(52.0)
                .px(20.0)
                .gap(8.0)
                .center_y()
                .child(action_button(
                    "license-back-about",
                    tr("Über Prime Photos").to_owned(),
                    theme,
                    theme.control_track,
                    theme.text,
                    |state: &mut S, _: &mut Tx| state.set_page(LivePage::About),
                ))
                .child(col().grow(1.0))
                .child(action_button(
                    "license-close",
                    tr("Schließen").to_owned(),
                    theme,
                    theme.control_track,
                    theme.text,
                    |state: &mut S, _: &mut Tx| state.set_page(LivePage::Backup),
                )),
        )
        .child(
            row()
                .key("license-tabs")
                .w_full()
                .h(48.0)
                .px(20.0)
                .gap(8.0)
                .center_y()
                .focus_group()
                .child(tab_button::<S>(
                    "license-tab-components",
                    tr("Komponenten und Lizenztexte"),
                    LicenseTab::Components,
                    current_tab,
                    theme,
                ))
                .child(tab_button::<S>(
                    "license-tab-icons",
                    tr("Icons und Grafiken"),
                    LicenseTab::Icons,
                    current_tab,
                    theme,
                )),
        )
        .child(
            col()
                .key("license-content-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .px(12.0)
                .scroll(ScrollSpec { stick_bottom: false })
                .child(content),
        )
        .child(el().key("licenses-bottom-space").h(12.0).fill(theme.bg))
}

fn tab_button<S: AboutRouteHost + 'static>(
    key: &'static str,
    label: &'static str,
    tab: LicenseTab,
    current: LicenseTab,
    theme: Theme,
) -> El {
    let selected = tab == current;
    el()
        .key(key)
        .role("tab")
        .label(label)
        .checked(selected)
        .focusable(true)
        .h(36.0)
        .px(12.0)
        .center()
        .radius(8.0)
        .fill(if selected {
            theme.indicator
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(theme.hover)
        .on_press(move |state: &mut S| state.set_license_tab(tab))
        .child(
            text(label)
                .font(tok::type_::BODY)
                .color(if selected {
                    theme.text
                } else {
                    theme.text_secondary
                })
                .a11y_hidden(),
        )
}

fn component_licenses<S: 'static>(theme: Theme) -> El {
    let lines = crate::about::license_lines();
    col()
        .key("license-lines")
        .role("list")
        .label(tr("Komponenten und Lizenztexte"))
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            lines.len(),
            LICENSE_ROW_HEIGHT,
            |_: &S, index| format!("license-line-{index}").into(),
            move |_: &S, index| {
                text(crate::about::license_lines()[index].clone())
                    .role("listitem")
                    .font(tok::type_::MONO_CAPTION.mono())
                    .color(theme.text)
                    .h(LICENSE_ROW_HEIGHT)
            },
        )
}

fn icon_attributions(theme: Theme) -> El {
    let mut content = col()
        .key("icon-attributions")
        .role("list")
        .label(tr("Icons und Grafiken"))
        .w_full()
        .gap(4.0);
    for (index, source_line) in crate::about::icon_attributions().lines().enumerate() {
        let line = source_line.trim_end();
        let key = format!("attribution-line-{index}");
        if line.trim().is_empty() {
            content = content.child(el().key(key).h(4.0));
        } else if let Some(heading) = line.strip_prefix("### ") {
            content = content.child(
                text(strip_inline(heading))
                    .key(key)
                    .role("heading")
                    .font(tok::type_::BODY)
                    .color(theme.text),
            );
        } else if let Some(heading) = line.strip_prefix("## ") {
            content = content.child(
                text(strip_inline(heading))
                    .key(key)
                    .role("heading")
                    .font(tok::TextStyle::new(16.0, 650.0))
                    .color(theme.text),
            );
        } else if let Some(heading) = line.strip_prefix("# ") {
            content = content.child(
                text(strip_inline(heading))
                    .key(key)
                    .role("heading")
                    .font(tok::TextStyle::new(18.0, 700.0))
                    .color(theme.text),
            );
        } else if let Some(bullet) = line.strip_prefix("- ") {
            content = content.child(
                row()
                    .key(key)
                    .role("listitem")
                    .w_full()
                    .gap(8.0)
                    .child(text("•").color(theme.text_secondary))
                    .child(
                        text(strip_inline(bullet))
                            .w_full()
                            .wrap()
                            .font(tok::type_::BODY)
                            .color(theme.text),
                    ),
            );
        } else {
            content = content.child(
                text(strip_inline(line))
                    .key(key)
                    .w_full()
                    .wrap()
                    .font(tok::type_::BODY)
                    .color(theme.text),
            );
        }
    }
    content
}

/// The attribution source uses headings, bullets, paragraphs, and simple inline
/// emphasis/code. Strip only the inline markers; all source text remains visible.
fn strip_inline(line: &str) -> String {
    line.replace("**", "").replace('`', "")
}

#[cfg(test)]
mod tests;
