//! Headless-only signed-out login fixture for the Rune migration (Issue #49).
//!
//! This is visual/interactions test coverage only. It deliberately has no
//! authentication, account, config, keychain, webview, or network integration.

use crate::countries::{self, COUNTRIES};
use crate::i18n::{self, Lang};
use rune::tok;
use rune::{
    App, AppSpec, Cx, El, LiveRegion, PopupMenu, ScrollSpec, Tx, col, el, row, text,
};

#[cfg(test)]
mod tests;

const WINDOW_SIZE: (f32, f32) = (960.0, 680.0);
const FIXTURE_ERROR: &str =
    "Fixture-only: Diese Rune-Testansicht führt keine Anmeldung aus.";

struct LoginState {
    country: String,
    error: Option<&'static str>,
    marketplaces_open: bool,
}

impl Default for LoginState {
    fn default() -> Self {
        Self {
            country: "de".to_owned(),
            error: None,
            marketplaces_open: false,
        }
    }
}

/// True only for Rune's three explicit headless actions used by this fixture.
pub(crate) fn has_headless_action(args: &[String]) -> bool {
    let args = rune::tooling::Args::parse_from(args.to_vec());
    args.headless() && (args.snap.is_some() || args.tree.is_some() || args.script.is_some())
}

pub(crate) fn run(args: Vec<String>) -> Result<(), String> {
    if !has_headless_action(&args) {
        return Err(
            "usage: --rune-login-fixture requires --snap FILE, --tree FILE, or --script FILE"
                .to_owned(),
        );
    }

    // A fixed language and country keep Rune screenshots independent of the
    // user's locale and do not inspect app configuration or system settings.
    i18n::set_lang(Lang::De);

    let spec = AppSpec {
        scenes: &[
            ("signed-out", |_: &mut LoginState| {}),
            ("error", error_scene),
        ],
        ..AppSpec::new("Prime Photos", WINDOW_SIZE, LoginState::default, view)
    };

    App::new(spec).init(LoginState::default).args(args).try_run()
}

fn error_scene(state: &mut LoginState) {
    state.error = Some(FIXTURE_ERROR);
}

fn view(state: &LoginState, cx: &mut Cx<LoginState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    let accent = tok::accent(0, dark);
    let alert = tok::accent(2, dark);

    let label = i18n::tr("Land / Amazon-Marktplatz:");
    let current_country = countries::country_label(&state.country);
    let selector = el()
        .key("marketplace-selector")
        .role("combobox")
        .label(current_country.clone())
        .focusable(true)
        .w_full()
        .h(46.0)
        .px(14.0)
        .center_y()
        .radius(9.0)
        .fill(theme.control_track)
        .hover(theme.hover)
        .on_press_tx(|state: &mut LoginState, tx: &mut Tx| {
            if rune::popup_menu_supported() {
                let menu = PopupMenu::picker(
                    COUNTRIES
                        .iter()
                        .map(|&(code, _)| (code, countries::country_label(code))),
                    &state.country,
                );
                tx.popup_menu(
                    menu,
                    "marketplace-selector",
                    |state: &mut LoginState, picked, _| {
                        if let Some(country) = picked {
                            state.country = country.to_owned();
                        }
                    },
                );
            } else {
                state.marketplaces_open = !state.marketplaces_open;
            }
        })
        .child(
            row()
                .w_full()
                .center_y()
                .justify_between()
                .child(
                    text(current_country)
                        .key("marketplace-value")
                        .font(tok::type_::BODY)
                        .color(theme.text),
                )
                .child(
                    text("⌄")
                        .key("marketplace-chevron")
                        .font(tok::type_::BODY)
                        .color(theme.text_secondary),
                ),
        );

    let sign_in_label = i18n::tr("Mit Amazon anmelden");
    let sign_in = el()
        .key("sign-in-cta")
        .role("button")
        .label(sign_in_label)
        .focusable(true)
        .w_full()
        .h(48.0)
        .center()
        .radius(10.0)
        .fill(accent)
        .hover(theme.hover)
        .on_press(|state: &mut LoginState| state.error = Some(FIXTURE_ERROR))
        .child(
            text(sign_in_label)
                .font(tok::TextStyle::new(14.0, 650.0))
                .color(if dark {
                    rune::Color::WHITE
                } else {
                    theme.text
                })
                .a11y_hidden(),
        );

    let mut marketplace_form = col()
        .key("marketplace-form")
        .w_full()
        .gap(8.0)
        .child(
            text(label)
                .key("marketplace-label")
                .font(tok::type_::LABEL)
                .color(theme.text),
        )
        .child(selector);

    if !rune::popup_menu_supported() && state.marketplaces_open {
        let mut options = col()
            .key("marketplace-options")
            .role("listbox")
            .label(label)
            .w_full()
            .h(176.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .padding(6.0)
            .gap(2.0)
            .fill(theme.control_track)
            .radius(9.0);
        for &(country, _) in COUNTRIES {
            let title = countries::country_label(country);
            let selected = country == state.country;
            options = options.child(
                el()
                    .key(format!("marketplace-option-{country}"))
                    .role("option")
                    .label(title.clone())
                    .checked(selected)
                    .focusable(true)
                    .w_full()
                    .h(30.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(if selected {
                        theme.indicator
                    } else {
                        rune::Color::TRANSPARENT
                    })
                    .hover(theme.hover)
                    .on_press(move |state: &mut LoginState| {
                        state.country = country.to_owned();
                        state.marketplaces_open = false;
                    })
                    .child(
                        text(title)
                            .font(tok::type_::LABEL)
                            .color(theme.text)
                            .a11y_hidden(),
                    ),
            );
        }
        marketplace_form = marketplace_form.child(options);
    }

    let mut card = col()
        .key("login-card")
        .w(480.0)
        .padding(32.0)
        .gap(24.0)
        .fill(theme.card)
        .radius(18.0)
        .child(
            col()
                .key("login-brand")
                .w_full()
                .gap(8.0)
                .child(
                    text("Prime Photos")
                        .key("login-title")
                        .font(tok::TextStyle::new(25.0, 700.0))
                        .color(theme.text),
                )
                .child(
                    text(i18n::tr("Fotos & Videos zu Amazon Photos hochladen"))
                        .key("login-subtitle")
                        .font(tok::TextStyle::new(14.0, 400.0))
                        .color(theme.text_secondary)
                        .wrap(),
                ),
        )
        .child(marketplace_form)
        .child(sign_in);

    if let Some(error) = state.error {
        card = card.child(
            text(error)
                .key("fixture-error")
                .role("alert")
                .live_region(LiveRegion::Assertive)
                .font(tok::type_::BODY)
                .color(alert)
                .wrap(),
        );
    }

    col()
        .key("login-screen")
        .role("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .center()
        .child(card)
}
