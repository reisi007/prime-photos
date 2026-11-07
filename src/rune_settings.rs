//! Headless-only Rune feasibility fixture for Issue #49.
//!
//! This settings-only surface is not the Prime Photos app and must never become
//! an interactive alternate user mode. The eframe app remains the normal UI
//! until all established user flows have been ported to Rune.

use crate::config::{Appearance, Config};
use crate::i18n::{self, Lang};
use rune::tok::{self, Theme};
use rune::{App, AppSpec, Cx, El, LiveRegion, ScrollSpec, TextEdit, col, el, row, text};

mod controls;
mod general;
#[cfg(test)]
mod tests;

const WINDOW_SIZE: (f32, f32) = (1120.0, 800.0);
const SLIDER_WIDTH: f32 = 300.0;
const SLIDER_HEIGHT: f32 = 28.0;
const SLIDER_KNOB: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    General,
    Bandwidth,
}

struct SettingsState {
    config: Config,
    tab: SettingsTab,
    limit_edit: TextEdit,
    save_error: Option<String>,
    slider_origin: usize,
    /// Persistence is opt-in for unit tests. Headless snapshots/scripts must
    /// never read or modify the user's real config.
    save_path: Option<std::path::PathBuf>,
}

impl SettingsState {
    fn new(mut config: Config) -> Self {
        config.concurrency = config.concurrency();
        let limit_edit = TextEdit::new(config.limit_kbps.to_string());
        SettingsState {
            config,
            tab: SettingsTab::General,
            limit_edit,
            save_error: None,
            slider_origin: 1,
            save_path: None,
        }
    }

    fn persist(&mut self) {
        let Some(path) = self.save_path.as_deref() else {
            return;
        };
        let result = save_config_at_path(&self.config, path);

        match result {
            Ok(()) => self.save_error = None,
            Err(error) => {
                log::warn!("Rune-Einstellungen konnten nicht gespeichert werden: {error}");
                self.save_error = Some(format!(
                    "{} ({error})",
                    i18n::tr("Die Einstellungen konnten nicht gespeichert werden.")
                ));
            }
        }
    }

    fn set_language(&mut self, language: Option<&'static str>) {
        self.config.language = language.map(str::to_owned);
        let lang = self
            .config
            .language
            .as_deref()
            .and_then(Lang::from_code)
            .unwrap_or_else(i18n::system_lang);
        i18n::set_lang(lang);
        self.persist();
    }

    fn set_limit_from_edit(&mut self) {
        let Ok(value) = self.limit_edit.text.trim().parse::<u32>() else {
            return;
        };
        if value != self.config.limit_kbps {
            self.config.limit_kbps = value;
            self.persist();
        }
    }
}

pub(crate) fn run(args: Vec<String>) -> Result<(), String> {
    // This is a test fixture, not a product settings window. Starting it must
    // not inspect a real user's settings, account, keychain, or folders.
    let config = Config::default();
    set_initial_language(&config);

    let spec = AppSpec {
        scenes: &[
            ("light", |state: &mut SettingsState| {
                state.config.appearance = Appearance::Light;
            }),
            ("dark", |state: &mut SettingsState| {
                state.config.appearance = Appearance::Dark;
            }),
            ("bandwidth", |state: &mut SettingsState| {
                state.tab = SettingsTab::Bandwidth;
            }),
        ],
        ..AppSpec::new(
            "Prime Photos",
            WINDOW_SIZE,
            || SettingsState::new(Config::default()),
            view,
        )
    };

    App::new(spec)
        .init(move || SettingsState::new(config))
        .args(args)
        .standard_menus()
        .try_run()
}

fn set_initial_language(config: &Config) {
    let language = config
        .language
        .as_deref()
        .and_then(Lang::from_code)
        .unwrap_or_else(i18n::system_lang);
    i18n::set_lang(language);
}

fn view(state: &SettingsState, cx: &mut Cx<SettingsState>) -> El {
    let dark = match state.config.appearance {
        Appearance::System => cx.system_dark(),
        Appearance::Light => false,
        Appearance::Dark => true,
    };
    let theme = tok::theme(dark);
    let accent = tok::accent(0, dark);

    let page = match state.tab {
        SettingsTab::General => general::general_page(state, theme, accent),
        SettingsTab::Bandwidth => bandwidth_page(state, theme, accent),
    };

    col()
        .key("settings-window")
        .role("window")
        .fill(theme.bg)
        .w_full()
        .h_full()
        .child(
            row()
                .key("page-header")
                .w_full()
                .h(68.0)
                .pl(cx.titlebar_inset() + 30.0)
                .pr(30.0)
                .center_y()
                .child(
                    text(i18n::tr("Einstellungen"))
                        .key("page-title")
                        .font(tok::type_::TITLE)
                        .color(theme.text),
                ),
        )
        .child(
            el().key("header-divider")
                .w_full()
                .h(1.0)
                .fill(theme.separator),
        )
        .child(
            row()
                .key("tabs")
                .w_full()
                .h(58.0)
                .px(26.0)
                .center_y()
                .gap(8.0)
                .focus_group()
                .child(tab_button(
                    "tab-general",
                    "Allgemeines",
                    SettingsTab::General,
                    state.tab,
                    theme,
                ))
                .child(tab_button(
                    "tab-bandwidth",
                    "Bandbreite",
                    SettingsTab::Bandwidth,
                    state.tab,
                    theme,
                )),
        )
        .child(
            col()
                .key("settings-scroll")
                .grow(1.0)
                .min_h(0.0)
                .w_full()
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(
                    col()
                        .key("settings-content")
                        .w_full()
                        .px(30.0)
                        .pt(18.0)
                        .pb(24.0)
                        .gap(22.0)
                        .child(page),
                ),
        )
        .child(settings_footer(state, theme))
}

fn tab_button(
    key: &'static str,
    title: &'static str,
    tab: SettingsTab,
    current: SettingsTab,
    theme: Theme,
) -> El {
    let selected = tab == current;
    el().key(key)
        .role("tab")
        .label(i18n::tr(title))
        .checked(selected)
        .focusable(true)
        .size(
            if tab == SettingsTab::General {
                128.0
            } else {
                112.0
            },
            36.0,
        )
        .center()
        .radius(8.0)
        .fill(if selected {
            theme.card
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(theme.hover)
        .on_press(move |state: &mut SettingsState| state.tab = tab)
        .child(
            text(i18n::tr(title))
                .font(tok::type_::BODY)
                .color(if selected {
                    theme.text
                } else {
                    theme.text_secondary
                })
                .a11y_hidden(),
        )
}

fn bandwidth_page(state: &SettingsState, theme: Theme, accent: rune::Color) -> El {
    controls::bandwidth_page(state, theme, accent)
}

fn section<const N: usize>(
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

fn toggle_row(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    enabled: bool,
    action: fn(&mut SettingsState),
    theme: Theme,
    accent: rune::Color,
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
        .child(toggle_control(key, enabled, theme, accent).a11y_hidden())
}

fn toggle_control(key: &'static str, enabled: bool, theme: Theme, accent: rune::Color) -> El {
    const WIDTH: f32 = 38.0;
    const HEIGHT: f32 = 22.0;
    const KNOB: f32 = 18.0;
    let inset = (HEIGHT - KNOB) / 2.0;
    el().key(format!("{key}-switch"))
        .size(WIDTH, HEIGHT)
        .radius(HEIGHT / 2.0)
        .fill(if enabled { accent } else { theme.control_off })
        .motion(tok::motion::SNAPPY)
        .child(
            el().key(format!("{key}-knob"))
                .size(KNOB, KNOB)
                .absolute(if enabled { WIDTH - KNOB - inset } else { inset }, inset)
                .radius(KNOB / 2.0)
                .fill(theme.knob)
                .shadow(1.0, 3.0, theme.knob_shadow),
        )
}

fn settings_footer(state: &SettingsState, theme: Theme) -> El {
    let status = match &state.save_error {
        Some(error) => text(error.clone())
            .key("save-error")
            .role("alert")
            .live_region(LiveRegion::Assertive)
            .color(tok::accent(
                2,
                matches!(state.config.appearance, Appearance::Dark),
            ))
            .wrap(),
        None => text(i18n::tr("Änderungen werden sofort gespeichert."))
            .key("save-status")
            .color(theme.text_secondary),
    };
    row()
        .key("settings-footer")
        .w_full()
        .h(48.0)
        .px(30.0)
        .center_y()
        .child(status.font(tok::type_::LABEL))
}

fn toggle_dedup_default(state: &mut SettingsState) {
    state.config.dedup_default = !state.config.dedup_default;
    state.persist();
}

fn save_config_at_path(config: &Config, path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config).map_err(std::io::Error::other)?;
    std::fs::write(path, content)
}
