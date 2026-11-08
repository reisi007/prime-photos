//! Explicit debug-only Rune host for the existing production device login.

use std::sync::{Arc, Mutex};

use rune::tok;
use rune::{App, AppSpec, Cx, El, LiveRegion, PopupMenu, ScrollSpec, Tx, Waker, WindowHandle, col, el, row, text};
use crate::config::Config;
use crate::countries::{self, COUNTRIES};
use crate::i18n::{self, Lang, tr};
use crate::login::{self, LoginPoll, Session, WebviewLogin};
use crate::wry_host;

const WINDOW_SIZE: (f32, f32) = (960.0, 680.0);
const HEADER_HEIGHT: f32 = 72.0;

struct LoginState {
    config: Config,
    config_error: Option<String>,
    persist_config: bool,
    login: WebviewLogin,
    session: Arc<Mutex<Session>>,
    runtime: Arc<tokio::runtime::Runtime>,
    native_window: Option<WindowHandle>,
    child_bounds: Option<[f32; 4]>,
    status: Option<String>,
    marketplaces_open: bool,
}

impl LoginState {
    fn new(config: Config, config_error: Option<String>, persist_config: bool) -> Result<Self, String> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|error| format!("Tokio runtime could not start: {error}"))?;
        Ok(Self {
            config,
            config_error,
            persist_config,
            login: WebviewLogin::new(),
            session: Arc::new(Mutex::new(Session::default())),
            runtime: Arc::new(runtime),
            native_window: None,
            child_bounds: None,
            status: None,
            marketplaces_open: false,
        })
    }
}

pub(crate) fn run(args: Vec<String>) -> Result<(), String> {
    let parsed = rune::tooling::Args::parse_from(args.clone());
    if parsed.headless() {
        return Err("--rune-login is an interactive developer route, not a headless fixture".to_owned());
    }

    let (config, load_error, persist_config) = match Config::load() {
        Ok(config) => (config, None, true),
        Err(error) => (Config::default(), Some(error.to_string()), false),
    };
    let language = config
        .language
        .as_deref()
        .and_then(Lang::from_code)
        .unwrap_or_else(i18n::system_lang);
    i18n::set_lang(language);
    if let Some(error) = &load_error {
        log::warn!("Einstellungen konnten nicht geladen werden: {error}");
    }
    let config_error = load_error.map(|_| tr("Die Einstellungen konnten nicht geladen werden.").to_owned());
    let state = LoginState::new(config, config_error, persist_config)?;

    let spec = AppSpec {
        min_size: Some((760.0, 520.0)),
        setup: Some(setup),
        poll: Some(poll),
        ..AppSpec::new("Prime Photos", WINDOW_SIZE, empty_state, view)
    };
    let app = App::new(spec)
        .init(move || state)
        .args(args)
        .standard_menus()
        .on_close(close);
    let app_handle = app.handle();
    app.on_window(move |window| {
        let native_window = window.clone();
        app_handle.update(move |state, _| state.native_window = Some(native_window));
    })
    .try_run()
}

fn empty_state() -> LoginState {
    unreachable!("Rune login initializes from the production configuration")
}

fn setup(state: &mut LoginState, wakeup: Waker) {
    state.login.set_wakeup(wakeup);
}

fn poll(state: &mut LoginState, _: f64, tx: &mut Tx) -> bool {
    match login::poll_login_flow(
        &mut state.login,
        &mut state.config,
        &state.session,
        &state.runtime,
        &mut state.config_error,
        state.persist_config,
    ) {
        LoginPoll::Idle => false,
        LoginPoll::Redeeming => {
            state.status = Some(tr("Anmeldecode empfangen; Anmeldung wird abgeschlossen.").to_owned());
            true
        }
        LoginPoll::Succeeded => {
            state.status = Some(tr("Anmeldung erfolgreich – Sitzung eingerichtet.").to_owned());
            tx.set_title("Prime Photos");
            true
        }
        LoginPoll::Failed => {
            state.status = None;
            tx.set_title("Prime Photos");
            true
        }
    }
}

fn close(state: &mut LoginState, _: &mut Tx) {
    state.login.cancel();
}

fn view(state: &LoginState, cx: &mut Cx<LoginState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    let colors = crate::rune_theme::palette(dark);
    cx.foreground(theme.text);
    cx.focus_ring(2.0, 2.0, colors.focus_ring);
    let titlebar_inset = cx.titlebar_inset();

    let header = row()
        .key("production-login-header")
        .w_full()
        .h(HEADER_HEIGHT)
        .pl(titlebar_inset + 26.0)
        .pr(24.0)
        .center_y()
        .justify_between()
        .fill(theme.card)
        .window_drag()
        .child(
            col()
                .gap(2.0)
                .child(
                    text("Prime Photos")
                        .key("production-login-brand")
                        .font(tok::type_::TITLE)
                        .color(theme.text),
                )
                .child(
                    text(if state.login.running {
                        tr("Bei Amazon anmelden")
                    } else {
                        tr("Fotos & Videos zu Amazon Photos hochladen")
                    })
                    .font(tok::type_::LABEL)
                    .color(theme.text_secondary),
                ),
        );
    let header = if state.login.running {
        header.child(
            el()
                .key("cancel-login")
                .role("button")
                .label(tr("Abbrechen"))
                .focusable(true)
                .h(38.0)
                .px(16.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_track)
                .hover(theme.hover)
                .on_press_tx(|state: &mut LoginState, tx: &mut Tx| {
                    state.login.cancel();
                    state.status = Some(tr("Anmeldung abgebrochen.").to_owned());
                    tx.set_title("Prime Photos");
                })
                .child(text(tr("Abbrechen")).font(tok::type_::BODY).color(theme.text)),
        )
    } else {
        header
    };

    let body = if state.login.running {
        col()
            .key("production-login-webview-region")
            .w_full()
            .h_full()
            .fill(theme.bg)
            .center()
            .child(
                text(tr("Anmeldeseite wird geladen …"))
                    .key("webview-loading")
                    .font(tok::type_::BODY)
                    .color(theme.text_secondary),
            )
    } else {
        signed_out(state, theme, colors)
    };

    col()
        .key("production-login-window")
        .role("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(header)
        .child(
            body.key("webview-area")
                .on_layout(|state: &mut LoginState, rect| {
                    let bounds = wry_host::bounds_from_layout(rect);
                    state.child_bounds = Some(bounds);
                    if state.login.running {
                        state.login.set_bounds(login::rect(
                            bounds[0], bounds[1], bounds[2], bounds[3],
                        ));
                    }
                }),
        )
}

fn signed_out(
    state: &LoginState,
    theme: rune::tok::Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let label = tr("Land / Amazon-Marktplatz:");
    let current_country = countries::country_label(&state.config.country);
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
        .border(1.0, colors.control_border)
        .hover(theme.hover)
        .on_press_tx(|state: &mut LoginState, tx: &mut Tx| {
            if rune::popup_menu_supported() {
                tx.popup_menu(
                    PopupMenu::picker(
                        COUNTRIES
                            .iter()
                            .map(|&(code, _)| (code, countries::country_label(code))),
                        &state.config.country,
                    ),
                    "marketplace-selector",
                    |state: &mut LoginState, picked, _| {
                        if let Some(country) = picked {
                            state.config.country = country.to_owned();
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
                .child(text("⌄").font(tok::type_::BODY).color(theme.text_secondary)),
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
            .border(1.0, colors.control_border)
            .radius(9.0);
        for &(country, _) in COUNTRIES {
            let title = countries::country_label(country);
            let selected = country == state.config.country;
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
                        state.config.country = country.to_owned();
                        state.marketplaces_open = false;
                    })
                    .child(text(title).font(tok::type_::LABEL).color(theme.text)),
            );
        }
        marketplace_form = marketplace_form.child(options);
    }

    let sign_in_label = tr("Mit Amazon anmelden");
    let sign_in = el()
        .key("sign-in-cta")
        .role("button")
        .label(sign_in_label)
        .focusable(true)
        .w_full()
        .h(48.0)
        .center()
        .radius(10.0)
        .fill(colors.primary_fill)
        .hover(theme.hover)
        .on_press_tx(start_login)
        .child(
            text(sign_in_label)
                .font(tok::TextStyle::new(14.0, 650.0))
                .color(colors.primary_foreground)
                .a11y_hidden(),
        );

    let mut card = col()
        .key("production-login-card")
        .w(480.0)
        .padding(32.0)
        .gap(22.0)
        .fill(theme.card)
        .radius(18.0)
        .child(
            col()
                .w_full()
                .gap(8.0)
                .child(
                    text("Prime Photos")
                        .key("login-title")
                        .font(tok::TextStyle::new(25.0, 700.0))
                        .color(theme.text),
                )
                .child(
                    text(tr("Fotos & Videos zu Amazon Photos hochladen"))
                        .key("login-subtitle")
                        .font(tok::TextStyle::new(14.0, 400.0))
                        .color(theme.text_secondary)
                        .wrap(),
                ),
        )
        .child(marketplace_form)
        .child(sign_in);

    if let Some(error) = &state.login.error {
        card = card.child(
            text(error.clone())
                .key("login-error")
                .role("alert")
                .live_region(LiveRegion::Assertive)
                .font(tok::type_::BODY)
                .color(colors.alert_foreground)
                .wrap(),
        );
    }
    if let Some(error) = &state.config_error {
        card = card.child(
            text(error.clone())
                .key("config-error")
                .role("alert")
                .font(tok::type_::BODY)
                .color(colors.alert_foreground)
                .wrap(),
        );
    }
    if let Some(status) = &state.status {
        card = card.child(
            text(status.clone())
                .key("login-status")
                .role("status")
                .live_region(LiveRegion::Polite)
                .font(tok::type_::BODY)
                .color(theme.text_secondary)
                .wrap(),
        );
    }

    col()
        .w_full()
        .h_full()
        .fill(theme.bg)
        .center()
        .child(card)
}

fn start_login(state: &mut LoginState, tx: &mut Tx) {
    state.status = None;
    let Some(native_window) = state.native_window.clone() else {
        state.login.error = Some(tr("Das Prime-Photos-Fenster ist noch nicht bereit.").to_owned());
        return;
    };
    let Some(rect) = state.child_bounds else {
        state.login.error = Some(tr("Der Anmeldebereich ist noch nicht bereit.").to_owned());
        return;
    };
    let bounds = login::rect(rect[0], rect[1], rect[2], rect[3]);
    match native_window.with_raw_handle(|raw| {
        login::start_device_login(&mut state.login, &mut state.config, &raw, bounds);
    }) {
        Ok(()) if state.login.running => tx.set_title("Login – Prime Photos"),
        Ok(()) => {}
        Err(error) => {
            log::warn!("Rune-Fensterhandle für Anmelde-WebView fehlgeschlagen: {error}");
            state.login.error = Some(tr("Anmeldeseite konnte nicht geöffnet werden.").to_owned());
        }
    }
}
