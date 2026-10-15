//! Embedded login. Amazon's real sign-in page is shown in a WKWebView that is a
//! **child view of the main application window** — no extra window, no external
//! browser, no Chrome DevTools Protocol.
//!
//! The loaded page is the **device** portal URL ([`crate::urls::auth_portal_url`],
//! PKCE); a navigation handler captures the `openid.oa2.authorization_code` when
//! Amazon redirects to an `/ap/…` URL and **ignores** that navigation (the
//! original does the same, `docs/SPEC_LOGIN.md` § 3.4). The code is redeemed
//! (`register_device`), the refresh token stored in the OS keychain
//! (`KeyringTokenStore`) and web cookies fetched for the unchanged,
//! cookie-based upload engine.
//!
//! The plain cookie observation stays the **fallback**: once the probe sees
//! authenticated cookies it records them, but they are only honoured after a
//! short grace period ([`LOGIN_COOKIE_GRACE`]), so a code that arrives with the
//! very same redirect wins the race. Our own sign-in screen keeps its browser
//! link to the plain web sign-in URL.
//!
//! Cookies are read asynchronously. On macOS the probe is scheduled on the main
//! dispatch queue, because wry's `WebView::cookies` is synchronous and spins the
//! macOS run loop, which would re-enter winit's event handler (and panic) if
//! called directly from egui's `update`. That run-loop re-entrancy is a
//! macOS-specific problem: off macOS the probe runs directly.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{Rect, WebView, WebViewBuilder};

use crate::app::{PrimePhotoApp, HEADER_H};
use crate::auth::{self, Pkce, TokenStore};
use crate::countries::{country_label, parse_cookie_string, COUNTRIES, ParsedCookies};
use crate::i18n::{tr, tr1};
use crate::icons::upload_logo;
use crate::login;
use crate::theme;
use eframe::egui::{self, RichText};

thread_local! {
    /// The login webview lives on the main thread only (a WebKit requirement).
    static WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

/// How often the cookie store is checked while the sign-in page is open.
const PROBE_INTERVAL: Duration = Duration::from_millis(500);

/// How long the cookie fallback must wait before it may win the race against
/// the device path's authorization code.
///
/// The original has no such concept — it simply waits for the code — but our
/// cookie path is kept as a **fallback**, so it must not overtake a code that is
/// still on its way (the navigation that carries it is ignored, so the code is
/// only observed by the navigation handler). A documented own default.
const LOGIN_COOKIE_GRACE: Duration = Duration::from_secs(3);

/// The **sign-in completion signal** of the device flow: the authorization code
/// carried as `openid.oa2.authorization_code` on a `/ap/` navigation (original
/// `LoginWebPolicyDelegate`, `docs/SPEC_LOGIN.md` § 3.4).
///
/// Only `/ap/` URLs are considered (the portal's `return_to` is
/// `/ap/maplanding`); the raw value is taken up to the next `&` — the original
/// does **not** percent-decode it, so neither do we.
fn extract_authorization_code(url: &str) -> Option<String> {
    if !url.contains("/ap/") {
        return None;
    }
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("openid.oa2.authorization_code=") {
            return Some(value.to_string());
        }
    }
    None
}

/// A rectangle in egui/logical points with a **top-left** origin, as wry wants.
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect {
        position: LogicalPosition::new(x as f64, y as f64).into(),
        size: LogicalSize::new(w.max(1.0) as f64, h.max(1.0) as f64).into(),
    }
}

/// True once the **authenticated** Amazon cookies exist (not just an anonymous
/// session): `session-id` plus the `at-acb*` / `x-acb*` auth cookies.
fn has_login(cookies: &[wry::cookie::Cookie<'static>]) -> bool {
    let has_session = cookies.iter().any(|c| c.name() == "session-id");
    let has_at = cookies.iter().any(|c| c.name().starts_with("at-acb"));
    let has_x = cookies.iter().any(|c| c.name().starts_with("x-acb"));
    has_session && has_at && has_x
}

fn cookie_string(cookies: &[wry::cookie::Cookie<'static>]) -> String {
    let mut s = String::new();
    for c in cookies {
        s.push_str(c.name());
        s.push('=');
        s.push_str(c.value());
        s.push_str("; ");
    }
    s
}

/// State shared with the asynchronous cookie probe.
#[derive(Default)]
struct Probe {
    in_flight: bool,
    last: Option<Instant>,
    /// First observation of an authenticated cookie set, with the instant it was
    /// seen: the fallback may only win the race after [`LOGIN_COOKIE_GRACE`].
    cookies: Option<(Instant, String)>,
    error: Option<String>,
}

/// Owns the login webview for the duration of the sign-in.
pub struct WebviewLogin {
    pub running: bool,
    /// Raw cookie string ("name=value; …") once login completed.
    pub outcome: Option<String>,
    pub error: Option<String>,
    probe: Arc<Mutex<Probe>>,
    /// PKCE pair of the current device attempt, set by `begin_login`; consumed
    /// when the authorization code is redeemed.
    pkce: Option<Pkce>,
    /// Authorization code captured by the navigation handler.
    auth_code: Arc<Mutex<Option<String>>>,
    /// True while the code is being redeemed on the runtime: the cookie
    /// fallback stays quiet until the device result is in.
    device_busy: bool,
    /// Result of the device redemption, written by the background task.
    device_result: Arc<Mutex<Option<Result<ParsedCookies, String>>>>,
}

impl Default for WebviewLogin {
    fn default() -> Self {
        Self::new()
    }
}

impl WebviewLogin {
    pub fn new() -> Self {
        Self {
            running: false,
            outcome: None,
            error: None,
            probe: Arc::new(Mutex::new(Probe::default())),
            pkce: None,
            auth_code: Arc::new(Mutex::new(None)),
            device_busy: false,
            device_result: Arc::new(Mutex::new(None)),
        }
    }

    /// Create the webview as a child of `window`, filling `bounds`.
    ///
    /// A navigation handler watches for the device flow's completion signal: on
    /// an `/ap/` URL carrying `openid.oa2.authorization_code` the code is stored
    /// and the navigation is **ignored** (`false`), exactly like the original
    /// (`docs/SPEC_LOGIN.md` § 3.4). Every other navigation is used.
    pub fn start<W: HasWindowHandle>(&mut self, window: &W, url: &str, bounds: Rect) {
        if self.running {
            return;
        }
        // A fresh attempt starts clean. `pkce` is deliberately untouched: it is
        // set by `begin_login` immediately before this call.
        *self.auth_code.lock().unwrap() = None;
        *self.device_result.lock().unwrap() = None;
        self.device_busy = false;
        let auth_code = Arc::clone(&self.auth_code);
        match WebViewBuilder::new()
            .with_url(url)
            .with_bounds(bounds)
            .with_navigation_handler(move |url: String| match extract_authorization_code(&url) {
                Some(code) => {
                    *auth_code.lock().unwrap() = Some(code);
                    false
                }
                None => true,
            })
            .build_as_child(window)
        {
            Ok(wv) => {
                WEBVIEW.with(|slot| *slot.borrow_mut() = Some(wv));
                *self.probe.lock().unwrap() = Probe::default();
                self.running = true;
                self.outcome = None;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(tr1("Anmeldeseite konnte nicht geöffnet werden: {}", e));
            }
        }
    }

    /// Keep the webview aligned with the area the UI reserved for it.
    pub fn set_bounds(&self, bounds: Rect) {
        WEBVIEW.with(|slot| {
            if let Some(wv) = slot.borrow().as_ref() {
                let _ = wv.set_bounds(bounds);
            }
        });
    }

    pub fn cancel(&mut self) {
        WEBVIEW.with(|slot| {
            slot.borrow_mut().take();
        });
        self.running = false;
    }

    /// Called every UI frame: schedule a cookie probe and pick up its result.
    ///
    /// The webview is **not** torn down here any more — a completed login is
    /// consumed by [`PrimePhotoApp::poll_login`], which closes it. That keeps the
    /// device path (async code redemption) and the cookie fallback in one place.
    /// The fallback only fires once [`LOGIN_COOKIE_GRACE`] has elapsed **and** no
    /// device redemption is in flight, so a late authorization code always wins.
    pub fn poll(&mut self) {
        if !self.running || self.outcome.is_some() {
            return;
        }

        let device_busy = self.device_busy;
        let mut error = None;
        let mut schedule = false;
        let mut ready_cookie = None;
        {
            let mut p = self.probe.lock().unwrap();
            if let Some((at, cookies)) = p.cookies.as_ref() {
                if !device_busy && at.elapsed() >= LOGIN_COOKIE_GRACE {
                    ready_cookie = Some(cookies.clone());
                }
            } else {
                error = p.error.take();
                let due = p
                    .last
                    .map(|t| t.elapsed() >= PROBE_INTERVAL)
                    .unwrap_or(true);
                if !p.in_flight && due {
                    p.in_flight = true;
                    p.last = Some(Instant::now());
                    schedule = true;
                }
            }
        }

        if let Some(cs) = ready_cookie {
            self.outcome = Some(cs);
        }
        if let Some(e) = error {
            self.error = Some(e);
        }
        if schedule {
            schedule_probe(Arc::clone(&self.probe));
        }
    }

    /// Take the captured authorization code; a `Some` marks the device path as
    /// busy so the cookie fallback stays quiet until its result arrives.
    pub fn take_auth_code(&mut self) -> Option<String> {
        let code = self.auth_code.lock().unwrap().take();
        if code.is_some() {
            self.device_busy = true;
        }
        code
    }

    /// Take the device redemption result published by the background task.
    pub fn take_device_result(&mut self) -> Option<Result<ParsedCookies, String>> {
        self.device_result.lock().unwrap().take()
    }

    /// Take the PKCE pair of the current attempt (consumed by the redemption).
    pub fn take_pkce(&mut self) -> Option<Pkce> {
        self.pkce.take()
    }

    /// The cookie set observed by the probe, independent of the grace period —
    /// used to fall back when the device path was rejected.
    pub fn cookie_fallback(&self) -> Option<String> {
        self.probe
            .lock()
            .unwrap()
            .cookies
            .as_ref()
            .map(|(_, cookies)| cookies.clone())
    }
}

/// Read the webview's cookie store and record the outcome in `probe`. Shared by
/// both targets; `in_flight` is cleared when the probe finishes.
fn run_probe(probe: &Arc<Mutex<Probe>>) {
    WEBVIEW.with(|slot| {
        if let Some(wv) = slot.borrow().as_ref() {
            match wv.cookies() {
                Ok(cookies) => {
                    if has_login(&cookies) {
                        let mut p = probe.lock().unwrap();
                        if p.cookies.is_none() {
                            p.cookies = Some((Instant::now(), cookie_string(&cookies)));
                        }
                    }
                }
                Err(e) => {
                    let mut p = probe.lock().unwrap();
                    if p.error.is_none() {
                        p.error = Some(tr1("Cookies konnten nicht gelesen werden: {}", e));
                    }
                }
            }
        }
    });
    probe.lock().unwrap().in_flight = false;
}

/// macOS: schedule the probe on the main dispatch queue, i.e. outside winit's
/// event handler (avoids the run-loop re-entrancy described above).
#[cfg(target_os = "macos")]
fn schedule_probe(probe: Arc<Mutex<Probe>>) {
    DispatchQueue::main().exec_async(move || run_probe(&probe));
}

/// Non-macOS: there is no run-loop re-entrancy problem, so run the probe
/// directly/synchronously on the calling (UI) thread.
#[cfg(not(target_os = "macos"))]
fn schedule_probe(probe: Arc<Mutex<Probe>>) {
    run_probe(&probe);
}

// -- login --------------------------------------------------------------

impl PrimePhotoApp {
    pub(crate) fn cancel_login(&mut self) {
        self.login.cancel();
    }

    /// Sign out: drop the stored cookies and return to the login screen.
    pub(crate) fn logout(&mut self) {
        self.config_form = None;
        self.config.session_id.clear();
        self.config.cookie_x_acb.clear();
        self.config.cookie_at_acb.clear();
        self.config.cookie_ubid_acb.clear();
        self.config.cookie_x_amz_access_token.clear();
        match self.config.save() {
            Ok(_) => self.log(tr("Abgemeldet.")),
            Err(e) => self.config_error = Some(e.to_string()),
        }
    }

    /// Create the login webview. Needs the frame for the window handle.
    ///
    /// The URL is the **device** portal (PKCE), not the old web sign-in: our UI
    /// (header, marketplace picker, "Abbrechen") is unchanged, only the
    /// parameters the page receives follow the original's desktop flow
    /// (`docs/SPEC_LOGIN.md` § 3.3).
    pub(crate) fn begin_login(&mut self, frame: &eframe::Frame, ctx: &egui::Context) {
        if self.login.running {
            return;
        }
        // Normally already set by the bootstrap in `App::new`; keep the safety
        // net so a hand-edited `config.toml` cannot break the flow.
        if self.config.device_serial.is_empty() {
            self.config.device_serial = auth::new_device_serial();
        }
        let pkce = Pkce::generate();
        let client_id = auth::client_id(&self.config.device_serial, auth::DEVICE_TYPE);
        let url = crate::urls::auth_portal_url(
            &self.config.country,
            &client_id,
            &pkce.challenge,
            crate::urls::portal_language(&self.config.country),
        );
        self.login.pkce = Some(pkce);
        let screen = ctx.screen_rect();
        // Start just below an estimated header; the exact bounds are corrected
        // on the next frame once the header has been measured.
        let bounds = login::rect(0.0, HEADER_H, screen.width(), screen.height() - HEADER_H);
        self.login.start(frame, &url, bounds);
    }

    pub(crate) fn poll_login(&mut self) {
        self.login.poll();

        // (1) The device redemption (background task) reported back.
        if let Some(result) = self.login.take_device_result() {
            // The user cancelled while the task ran: drop the result.
            if !self.login.running {
                return;
            }
            match result {
                Ok(parsed) => {
                    self.apply_parsed(&parsed);
                    match self.config.save() {
                        Ok(_) => self.log(tr("Login erfolgreich – Refresh-Token gespeichert.")),
                        Err(e) => self.config_error = Some(e.to_string()),
                    }
                    self.login.cancel();
                    return;
                }
                Err(e) => {
                    // The device path was rejected: use the cookie fallback when
                    // the webview did observe a cookie set, otherwise report the
                    // rejection (never swallow it).
                    if let Some(cookie_string) = self.login.cookie_fallback() {
                        self.apply_cookie_string(&cookie_string);
                        match self.config.save() {
                            Ok(_) => {
                                self.log(tr1("Geräteweg abgelehnt ({}) – Cookie-Weg verwendet.", e))
                            }
                            Err(err) => self.config_error = Some(err.to_string()),
                        }
                    } else {
                        self.login.error = Some(
                            tr("Die Anmeldung wurde von Amazon abgelehnt. Bitte erneut versuchen.")
                                .to_string(),
                        );
                        log::warn!("{e}");
                    }
                    self.login.cancel();
                    return;
                }
            }
        }

        // (2) The portal navigation carried the authorization code: redeem it on
        // the runtime (never blocking a frame).
        if self.login.running {
            if let Some(code) = self.login.take_auth_code() {
                let Some(pkce) = self.login.take_pkce() else {
                    // Cannot happen in practice (`begin_login` always sets the
                    // pair first); drop the code instead of guessing a verifier.
                    log::warn!("Anmeldecode ohne PKCE-Paket – verworfen");
                    self.login.cancel();
                    return;
                };
                let country = self.config.country.clone();
                let serial = self.config.device_serial.clone();
                let client_id = auth::client_id(&serial, auth::DEVICE_TYPE);
                let language = crate::urls::portal_language(&country).to_string();
                let slot = Arc::clone(&self.login.device_result);
                self.rt.spawn(async move {
                    let client = reqwest::Client::new();
                    let result = match auth::register_device(
                        &client,
                        &country,
                        &language,
                        &serial,
                        &code,
                        &pkce.verifier,
                        &client_id,
                    )
                    .await
                    {
                        Ok(outcome) => {
                            // The refresh token is persisted; a keychain failure
                            // is reported but not fatal — the cookies are still
                            // worth fetching.
                            if let Err(e) = auth::KeyringTokenStore.save(&outcome.refresh_token) {
                                log::warn!(
                                    "Refresh-Token konnte nicht im Schlüsselbund gespeichert werden: {e}"
                                );
                            }
                            auth::fetch_auth_cookies(
                                &client,
                                &country,
                                &language,
                                &outcome.refresh_token,
                            )
                            .await
                            .map_err(|e| e.to_string())
                        }
                        Err(e) => Err(e.to_string()),
                    };
                    *slot.lock().unwrap() = Some(result);
                });
                return;
            }
        }

        // (3) Cookie fallback (the webview observed an authenticated cookie set
        // and the grace period has passed without a device result).
        let outcome = self.login.outcome.take();
        if let Some(cookie_string) = outcome {
            self.apply_cookie_string(&cookie_string);
            match self.config.save() {
                Ok(_) => self.log(tr("Login erfolgreich – Cookies übernommen.")),
                Err(e) => self.config_error = Some(e.to_string()),
            }
            self.login.cancel();
        }
    }

    /// Copy a parsed cookie set into the config (shared by the webview fallback
    /// and the device path's `fetch_auth_cookies`).
    pub(crate) fn apply_parsed(&mut self, parsed: &ParsedCookies) {
        self.config.session_id = parsed.session_id.clone();
        if !parsed.country.is_empty() {
            self.config.country = parsed.country.clone();
        }
        self.config.cookie_x_acb = parsed.cookie_x_acb.clone();
        self.config.cookie_at_acb = parsed.cookie_at_acb.clone();
        self.config.cookie_ubid_acb = parsed.cookie_ubid_acb.clone();
        self.config.cookie_x_amz_access_token = parsed.cookie_x_amz_access_token.clone();
    }

    pub(crate) fn apply_cookie_string(&mut self, cookie_string: &str) {
        let parsed = parse_cookie_string(cookie_string);
        self.apply_parsed(&parsed);
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
                        .fill(theme::SIDEBAR_BG)
                        .inner_margin(egui::Margin::symmetric(16, 8)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        upload_logo(ui, 30.0);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(tr("Prime Photos")).strong().size(15.0));
                            ui.label(theme::muted(tr("Bei Amazon anmelden")));
                        });
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.add(theme::secondary_button(tr("Abbrechen"))).clicked() {
                                    cancel = true;
                                }
                                if let Some(e) = &self.login.error {
                                    ui.colored_label(theme::RED, e);
                                } else {
                                    ui.spinner();
                                }
                            },
                        );
                    });
                });
            let top_h = header.response.rect.height();
            let screen = ctx.screen_rect();
            self.login.set_bounds(login::rect(
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

        egui::CentralPanel::default().show(ctx, |ui| self.login_screen_ui(ui));
    }

    /// Our own sign-in screen, shown before the Amazon page is opened.
    pub(crate) fn login_screen_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(56.0);
            upload_logo(ui, 84.0);
            ui.add_space(12.0);
            ui.label(RichText::new(tr("Prime Photos")).size(28.0).strong());
            ui.add_space(4.0);
            ui.label(theme::muted(tr("Fotos & Videos zu Amazon Photos hochladen")));
            ui.add_space(26.0);

            // Marketplace selection before signing in.
            ui.horizontal(|ui| {
                ui.label(tr("Land / Amazon-Marktplatz:"));
                let current = self.config.country.clone();
                egui::ComboBox::from_id_salt("login-country")
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
            ui.add_space(14.0);

            if ui.add(theme::primary_button(tr("Mit Amazon anmelden"))).clicked() {
                self.pending_login = true;
            }
            if let Some(e) = &self.login.error {
                ui.add_space(6.0);
                ui.colored_label(theme::RED, e);
            }

            ui.add_space(16.0);
            if ui.link(tr("Amazon Photos im Browser öffnen")).clicked() {
                let url = crate::urls::login_url(&self.config.country);
                let _ = crate::platform::open_url(&url);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_authorization_code_reads_the_return_to_query() {
        // The portal's `return_to` (`/ap/maplanding`) with the completion signal.
        let url = "https://www.amazon.de/ap/maplanding\
                   ?openid.oa2.scope=device_auth_access\
                   &openid.oa2.authorization_code=AN-EXAMPLE-CODE\
                   &openid.oa2.client_id=device%3Aabc";
        assert_eq!(
            extract_authorization_code(url),
            Some("AN-EXAMPLE-CODE".to_string())
        );
    }

    #[test]
    fn extract_authorization_code_ignores_ap_urls_without_the_code() {
        assert_eq!(
            extract_authorization_code("https://www.amazon.de/ap/maplanding?foo=bar"),
            None
        );
        // A `/ap/` URL with no query at all.
        assert_eq!(
            extract_authorization_code("https://www.amazon.de/ap/signin"),
            None
        );
    }

    #[test]
    fn extract_authorization_code_ignores_other_hosts() {
        // The code parameter alone is not enough: only `/ap/` URLs count.
        assert_eq!(
            extract_authorization_code("https://example.com/?openid.oa2.authorization_code=CODE"),
            None
        );
    }
}
