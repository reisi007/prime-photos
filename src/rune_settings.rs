//! Shared Rune settings controls plus their headless feasibility fixture.
//!
//! The fixture is not the Prime Photos app and must never become an interactive
//! alternate user mode. The explicit Rune backup route embeds the shared
//! controls, while eframe remains the normal UI until all established user
//! flows have been ported to Rune.

use crate::config::{Appearance, Config};
use crate::i18n::{self, Lang};
use rune::tok::{self, Theme};
use rune::{El, LiveRegion, ScrollSpec, TextEdit, col, el, row, text};
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
use rune::{App, AppSpec, Cx};

mod components;
mod controls;
mod general;
#[cfg(all(test, debug_assertions, feature = "rune-ui-spike"))]
mod tests;

use components::{section, toggle_dedup_default, toggle_row};

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
const WINDOW_SIZE: (f32, f32) = (1120.0, 800.0);
const SLIDER_WIDTH: f32 = 300.0;
const SLIDER_HEIGHT: f32 = 28.0;
const SLIDER_KNOB: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsTab {
    General,
    Bandwidth,
}

pub(crate) trait SettingsHost {
    fn config(&self) -> &Config;
    fn config_mut(&mut self) -> &mut Config;
    fn settings_tab(&self) -> SettingsTab;
    fn set_settings_tab(&mut self, tab: SettingsTab);
    fn autostart_enabled(&self) -> bool;
    fn set_autostart_enabled(&mut self, enabled: bool);
    fn slow_mode(&self) -> bool;
    fn set_slow_mode(&mut self, enabled: bool);
    fn limit_edit(&self) -> &TextEdit;
    fn limit_edit_mut(&mut self) -> &mut TextEdit;
    fn slider_origin(&self) -> usize;
    fn set_slider_origin(&mut self, value: usize);
    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    fn save_error(&self) -> Option<&str>;
    fn persist_settings(&mut self);
    fn album_edit(&self) -> &TextEdit;
    fn album_edit_mut(&mut self) -> &mut TextEdit;
    fn album_name_changed(&mut self);

    fn set_language(&mut self, language: Option<&'static str>) {
        self.config_mut().language = language.map(str::to_owned);
        let lang = self
            .config()
            .language
            .as_deref()
            .and_then(Lang::from_code)
            .unwrap_or_else(i18n::system_lang);
        i18n::set_lang(lang);
        self.persist_settings();
    }

    fn set_limit_from_edit(&mut self) {
        let text = self.limit_edit().text.trim().to_owned();
        let Ok(value) = text.parse::<u32>() else {
            return;
        };
        if value != self.config().limit_kbps {
            self.config_mut().limit_kbps = value;
            self.persist_settings();
        }
    }
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
struct SettingsState {
    config: Config,
    tab: SettingsTab,
    /// Fixture-only switch state; it must not inspect or modify OS login items.
    autostart_enabled: bool,
    slow_mode: bool,
    limit_edit: TextEdit,
    save_error: Option<String>,
    slider_origin: usize,
    album_edit: TextEdit,
    /// Persistence is opt-in for unit tests. Headless snapshots/scripts must
    /// never read or modify the user's real config.
    save_path: Option<std::path::PathBuf>,
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
impl SettingsState {
    fn new(mut config: Config) -> Self {
        config.concurrency = config.concurrency();
        let limit_edit = TextEdit::new(config.limit_kbps.to_string());
        let album_edit = TextEdit::new(config.album_name.clone());
        SettingsState {
            config,
            tab: SettingsTab::General,
            autostart_enabled: false,
            slow_mode: false,
            limit_edit,
            save_error: None,
            slider_origin: 1,
            album_edit,
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

}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
impl SettingsHost for SettingsState {
    fn config(&self) -> &Config {
        &self.config
    }

    fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    fn settings_tab(&self) -> SettingsTab {
        self.tab
    }

    fn set_settings_tab(&mut self, tab: SettingsTab) {
        self.tab = tab;
    }

    fn autostart_enabled(&self) -> bool {
        self.autostart_enabled
    }

    fn set_autostart_enabled(&mut self, enabled: bool) {
        self.autostart_enabled = enabled;
    }

    fn slow_mode(&self) -> bool {
        self.slow_mode
    }

    fn set_slow_mode(&mut self, enabled: bool) {
        self.slow_mode = enabled;
    }

    fn limit_edit(&self) -> &TextEdit {
        &self.limit_edit
    }

    fn limit_edit_mut(&mut self) -> &mut TextEdit {
        &mut self.limit_edit
    }

    fn slider_origin(&self) -> usize {
        self.slider_origin
    }

    fn set_slider_origin(&mut self, value: usize) {
        self.slider_origin = value;
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }

    fn persist_settings(&mut self) {
        self.persist();
    }

    fn album_edit(&self) -> &TextEdit {
        &self.album_edit
    }

    fn album_edit_mut(&mut self) -> &mut TextEdit {
        &mut self.album_edit
    }

    fn album_name_changed(&mut self) {
        let album_name = self.album_edit.text.clone();
        if self.config.album_name != album_name {
            self.config.album_name = album_name;
            self.persist_settings();
        }
    }
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
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

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn set_initial_language(config: &Config) {
    let language = config
        .language
        .as_deref()
        .and_then(Lang::from_code)
        .unwrap_or_else(i18n::system_lang);
    i18n::set_lang(language);
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn view(state: &SettingsState, cx: &mut Cx<SettingsState>) -> El {
    let dark = effective_dark(state.config.appearance, cx.system_dark());
    let theme = tok::theme(dark);
    let colors = crate::rune_theme::palette(dark);
    cx.foreground(theme.text);
    cx.focus_ring(2.0, 2.0, colors.focus_ring);

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
        .child(settings_content(state, theme, colors))
        .child(settings_footer(state, theme, colors))
}

pub(crate) fn effective_dark(appearance: Appearance, system_dark: bool) -> bool {
    match appearance {
        Appearance::System => system_dark,
        Appearance::Light => false,
        Appearance::Dark => true,
    }
}

pub(crate) fn embedded_page<S: SettingsHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    settings_content(state, theme, colors)
}

fn settings_content<S: SettingsHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let current = state.settings_tab();
    let page = match current {
        SettingsTab::General => general::general_page(state, theme, colors),
        SettingsTab::Bandwidth => bandwidth_page(state, theme, colors),
    };
    col()
        .key("settings-content-root")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(
            row()
                .key("tabs")
                .w_full()
                .h(58.0)
                .px(26.0)
                .center_y()
                .gap(8.0)
                .focus_group()
                .child(tab_button::<S>(
                    "tab-general",
                    "Allgemeines",
                    SettingsTab::General,
                    current,
                    theme,
                ))
                .child(tab_button::<S>(
                    "tab-bandwidth",
                    "Bandbreite",
                    SettingsTab::Bandwidth,
                    current,
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
}

fn tab_button<S: SettingsHost + 'static>(
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
        .on_press(move |state: &mut S| state.set_settings_tab(tab))
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

fn bandwidth_page<S: SettingsHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    controls::bandwidth_page(state, theme, colors)
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn settings_footer<S: SettingsHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let status = match state.save_error() {
        Some(error) => text(error.to_owned())
            .key("save-error")
            .role("alert")
            .live_region(LiveRegion::Assertive)
            .color(colors.alert_foreground)
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

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn save_config_at_path(config: &Config, path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config).map_err(std::io::Error::other)?;
    std::fs::write(path, content)
}
