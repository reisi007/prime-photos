use std::sync::Arc;

use eframe::egui::{self, RichText};

use crate::app::{PrimePhotoApp, HEADER_H};
use crate::auth::{self, TokenStore};
use crate::countries::{country_label, COUNTRIES};
use crate::i18n::tr;
use crate::theme::eframe_ui as theme;

use super::{
    apply_pending_cookies, clear_invalidated_session, prepare_session_worker, session_worker,
    start_device_login, SessionWakeup,
};

impl PrimePhotoApp {
    /// Session upkeep, called every UI frame (S-C, user decision 2026-10-15).
    ///
    /// This is the **only** place a session worker is spawned. It starts one as
    /// soon as a refresh token is available (after a device login, or at start
    /// with a token from the keychain) and no worker is running for the current
    /// generation. It also reacts to an invalidated session and picks up freshly
    /// minted cookies.
    pub(crate) fn poll_session(&mut self, ctx: &egui::Context) {
        // No session worker in the demo: there is no refresh token, and the
        // worker would talk to the network and the keychain.
        if self.is_demo() {
            self.demo_skip("Sitzungs-Aktualisierung");
            return;
        }
        // (1) The worker declared the session invalid (too many failed refresh
        // attempts): drop every token and return to the login screen — **no**
        // app restart (user decision 2026-10-15). This runs **before** the spawn
        // branch below: it bumps the generation and clears the refresh token, so
        // the spawn branch sees `refresh_token == None` and cannot start a worker
        // for the token we just discarded in the very same frame.
        if self.session.lock().unwrap().invalid {
            // An invalidated session is exactly the sign-out case (user decision
            // 2026-10-16): the running upload holds its own `Config` clone with
            // the now-dead cookies and would keep failing against the revoked
            // session. Cancel it the same way `logout` does — the worker stops at
            // its next wait point, a request already in flight is allowed to
            // finish.
            self.upload.lock().unwrap().cancelled = true;
            if let Err(e) = auth::KeyringTokenStore.clear() {
                log::warn!("Refresh-Token konnte nicht gelöscht werden: {e}");
            }
            {
                let mut s = self.session.lock().unwrap();
                clear_invalidated_session(&mut self.config, &mut s);
            }
            if let Err(e) = self.config.save() {
                log::warn!("Einstellungen konnten nicht gespeichert werden: {e}");
                self.config_error =
                    Some(tr("Die Einstellungen konnten nicht gespeichert werden.").to_string());
            }
            self.log(tr("Sitzung abgelaufen – bitte erneut anmelden."));
        }

        // (2) Start the session worker. It must run even when only cookie retry
        // work is due and the access-token appointment is still in the future.
        // `started` is cleared by login, logout and invalidation, so this covers
        // both "app start with a stored token" and "just signed in".
        let start_gen = prepare_session_worker(&mut self.session.lock().unwrap());
        if let Some(my_gen) = start_gen {
            let country = self.config.country.clone();
            let language = crate::urls::portal_language(&country).to_string();
            let session = Arc::clone(&self.session);
            let ctx = ctx.clone();
            let wakeup: SessionWakeup = Arc::new(move || ctx.request_repaint());
            self.rt.spawn(async move {
                session_worker(session, country, language, wakeup, my_gen).await;
            });
        }

        // (3) Freshly minted cookies from the worker: into the in-memory config
        // only, never persisted (`#[serde(skip)]`, see `config.rs`).
        let mut session = self.session.lock().unwrap();
        apply_pending_cookies(&mut self.config, &mut session);
    }

    /// Create the login webview. Needs the frame for the window handle.
    ///
    /// The URL is the **device** portal (PKCE), not the old web sign-in: our UI
    /// (header, marketplace picker, "Abbrechen") is unchanged, only the
    /// parameters the page receives follow the original's desktop flow
    /// (`docs/original/SPEC_LOGIN.md` § 3.3).
    pub(crate) fn begin_login(&mut self, frame: &eframe::Frame, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        // Start just below an estimated header; the exact bounds are corrected
        // on the next frame once the header has been measured.
        let bounds = super::rect(0.0, HEADER_H, screen.width(), screen.height() - HEADER_H);
        start_device_login(&mut self.login, &mut self.config, frame, bounds);
    }

    /// Login phase (same window as the app, like the original).
    ///
    /// While Amazon's page is open we show a slim branded header and let the
    /// webview fill the rest of the window; before that we show our own
    /// sign-in screen with the marketplace picker.
    pub(crate) fn login_ui(&mut self, ctx: &egui::Context) {
        if self.login.running {
            let mut cancel = false;
            let header = egui::TopBottomPanel::top("login_header")
                .exact_height(HEADER_H)
                .frame(
                    egui::Frame::new()
                        .fill(theme::palette(ctx.style().visuals.dark_mode).sidebar_bg)
                        .inner_margin(egui::Margin::symmetric(16, 8)),
                )
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add(crate::icons::eframe_ui::app_logo(ui, 30.0));
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(tr("Prime Photos")).strong().size(15.0));
                            ui.label(theme::muted(tr("Bei Amazon anmelden")));
                        });
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui
                                    .add(theme::secondary_button(
                                        tr("Abbrechen"),
                                        ui.visuals().dark_mode,
                                    ))
                                    .clicked()
                                {
                                    cancel = true;
                                }
                                if let Some(e) = &self.login.error {
                                    ui.colored_label(
                                        theme::palette(ui.visuals().dark_mode).red,
                                        e,
                                    );
                                } else {
                                    ui.spinner();
                                }
                            },
                        );
                    });
                });
            let top_h = header.response.rect.height();
            let screen = ctx.screen_rect();
            self.login.set_bounds(super::rect(
                0.0,
                top_h,
                screen.width(),
                screen.height() - top_h,
            ));
            if cancel {
                self.cancel_login();
            }
            return;
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::palette(ctx.style().visuals.dark_mode).page_bg)
                    .inner_margin(egui::Margin::symmetric(
                        theme::PAGE_MARGIN_X,
                        theme::PAGE_MARGIN_Y,
                    )),
            )
            .show(ctx, |ui| self.login_screen_ui(ui));
    }

    /// Our own sign-in screen, shown before the Amazon page is opened.
    pub(crate) fn login_screen_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(56.0);
            ui.add(crate::icons::eframe_ui::app_logo(ui, 84.0));
            ui.add_space(12.0);
            ui.label(RichText::new(tr("Prime Photos")).size(28.0).strong());
            ui.add_space(4.0);
            ui.label(theme::muted(tr("Fotos & Videos zu Amazon Photos hochladen")));
            ui.add_space(26.0);

            // Marketplace selection before signing in. A bare `ui.horizontal`
            // inside `vertical_centered` starts at the window's left edge
            // (`horizontal` spans the full width, so the centring never sees a
            // narrower rect). The row's own width is therefore measured first
            // and used as the form block's width: the block (and with it the
            // label + combo row) is centred as a whole, exactly on the page
            // centre at 1400 and 760 (Issue #30, F2).
            const COUNTRY_COMBO_W: f32 = 200.0;
            let label = tr("Land / Amazon-Marktplatz:");
            let label_w = ui.fonts(|f| {
                f.layout_no_wrap(
                    label.to_owned(),
                    egui::FontId::proportional(14.0),
                    egui::Color32::WHITE,
                )
                .size()
                .x
            });
            let form_w = label_w + ui.spacing().item_spacing.x + COUNTRY_COMBO_W;
            ui.allocate_ui_with_layout(
                egui::vec2(form_w, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        let current = self.config.country.clone();
                        egui::ComboBox::from_id_salt("login-country")
                            .width(COUNTRY_COMBO_W)
                            .selected_text(country_label(&current))
                            .show_ui(ui, |ui| {
                                for &(code, _) in COUNTRIES {
                                    ui.selectable_value(
                                        &mut self.config.country,
                                        code.to_string(),
                                        country_label(code),
                                    );
                                }
                            });
                    });
                },
            );
            ui.add_space(14.0);

            if ui
                .add(theme::primary_button(
                    tr("Mit Amazon anmelden"),
                    ui.visuals().dark_mode,
                ))
                .clicked()
            {
                self.pending_login = true;
            }
            if let Some(e) = &self.login.error {
                ui.add_space(6.0);
                ui.colored_label(theme::palette(ui.visuals().dark_mode).red, e);
            }
            // A failed config save (e.g. right after signing out) must stay
            // visible on the sign-in screen — the sidebar that normally shows
            // `config_error` is not rendered here (user decision 2026-10-16).
            if let Some(e) = &self.config_error {
                ui.add_space(6.0);
                ui.colored_label(theme::palette(ui.visuals().dark_mode).red, e);
            }
        });
    }
}
