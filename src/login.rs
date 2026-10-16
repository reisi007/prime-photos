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
//! This is the **only** sign-in path (user decision 2026-10-15): the earlier
//! cookie observation of the plain web sign-in was removed. wry's
//! `WebView::cookies` is synchronous and blocks the macOS main thread, so it
//! cannot be read from egui's `update` — the device path never needs it.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wry::dpi::{LogicalPosition, LogicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{Rect, WebView, WebViewBuilder};

use crate::app::{PrimePhotoApp, HEADER_H};
use crate::auth::{self, Pkce, TokenStore};
use crate::countries::{country_label, COUNTRIES, ParsedCookies};
use crate::i18n::tr;
use crate::icons::upload_logo;
use crate::login;
use crate::theme;
use eframe::egui::{self, RichText};

thread_local! {
    /// The login webview lives on the main thread only (a WebKit requirement).
    static WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

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

/// The successful result of the device redemption: the register outcome (with
/// the refresh token) plus the freshly minted cookies.
///
/// `cookies` is `None` when the register succeeded but the follow-up
/// `fetch_auth_cookies` failed: the login is **valid** (the refresh token is
/// stored) and the worker will mint the cookies on its next pass — it must never
/// be reported as a rejected device path.
pub(crate) struct DeviceLogin {
    pub(crate) register: auth::RegisterOutcome,
    pub(crate) cookies: Option<ParsedCookies>,
}

/// Owns the login webview for the duration of the sign-in.
pub struct WebviewLogin {
    pub running: bool,
    pub error: Option<String>,
    /// PKCE pair of the current device attempt, set by `begin_login`; consumed
    /// when the authorization code is redeemed.
    pkce: Option<Pkce>,
    /// Authorization code captured by the navigation handler.
    auth_code: Arc<Mutex<Option<String>>>,
    /// Result of the device redemption, written by the background task,
    /// together with the generation it belongs to.
    device_result: Arc<Mutex<Option<(u64, Result<DeviceLogin, String>)>>>,
    /// Incremented on every new login attempt; guards against a stale
    /// redemption result landing in a newer attempt.
    login_generation: u64,
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
            error: None,
            pkce: None,
            auth_code: Arc::new(Mutex::new(None)),
            device_result: Arc::new(Mutex::new(None)),
            login_generation: 0,
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
        self.login_generation += 1;
        let auth_code = Arc::clone(&self.auth_code);
        match WebViewBuilder::new()
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
                // Clear the browser storage first, then load: every sign-in
                // starts without the previous session's state (original
                // behaviour, docs/SPEC_LOGIN.md § 3.3).
                if let Err(e) = wv.clear_all_browsing_data() {
                    log::warn!("Login-Speicher konnte nicht geleert werden: {e}");
                }
                match wv.load_url(url) {
                    Ok(()) => {
                        WEBVIEW.with(|slot| *slot.borrow_mut() = Some(wv));
                        self.running = true;
                        self.error = None;
                    }
                    Err(e) => {
                        log::warn!("Anmeldeseite konnte nicht geladen werden: {e}");
                        self.error =
                            Some(tr("Anmeldeseite konnte nicht geöffnet werden.").to_string());
                    }
                }
            }
            Err(e) => {
                log::warn!("Anmeldeseite konnte nicht geöffnet werden: {e}");
                self.error = Some(tr("Anmeldeseite konnte nicht geöffnet werden.").to_string());
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

    /// Take the captured authorization code.
    pub fn take_auth_code(&mut self) -> Option<String> {
        self.auth_code.lock().unwrap().take()
    }

    /// Take the device redemption result published by the background task.
    ///
    /// Only returns the result if it belongs to the **current** login
    /// generation — a stale result from a previous (cancelled) attempt is
    /// discarded. This closes the race "cancel + sign in again immediately ⇒
    /// old result lands in the new attempt".
    pub fn take_device_result(&mut self) -> Option<Result<DeviceLogin, String>> {
        let (result_gen, result) = self.device_result.lock().unwrap().take()?;
        if result_gen == self.login_generation {
            Some(result)
        } else {
            None
        }
    }

    /// Take the PKCE pair of the current attempt (consumed by the redemption).
    pub fn take_pkce(&mut self) -> Option<Pkce> {
        self.pkce.take()
    }
}

// -- session (S-C) --------------------------------------------------------

/// The in-memory cookie set shared with the detached background supervisor
/// (S-C, user decision 2026-10-15).
///
/// Since the `Config` cookie fields are `#[serde(skip)]`, a `Config::load()` from
/// disk always yields **empty** cookies. The supervisor (`background_sync_pass`,
/// `watch_loop`) needs the live material, so `apply_parsed` mirrors it here and
/// the supervisor overlays it onto its loaded config.
#[derive(Default, Clone)]
pub(crate) struct SessionCookies {
    pub(crate) session_id: String,
    pub(crate) cookie_x_acb: String,
    pub(crate) cookie_at_acb: String,
    pub(crate) cookie_ubid_acb: String,
    pub(crate) cookie_x_amz_access_token: String,
}

impl From<&ParsedCookies> for SessionCookies {
    fn from(parsed: &ParsedCookies) -> Self {
        Self {
            session_id: parsed.session_id.clone(),
            cookie_x_acb: parsed.cookie_x_acb.clone(),
            cookie_at_acb: parsed.cookie_at_acb.clone(),
            cookie_ubid_acb: parsed.cookie_ubid_acb.clone(),
            cookie_x_amz_access_token: parsed.cookie_x_amz_access_token.clone(),
        }
    }
}

/// In-memory session state shared between the background refresh worker and
/// the UI thread (user decision 2026-10-15). Only the **refresh token** is
/// persisted (OS keychain); the access token and the cookies live here and
/// are lost on restart.
#[derive(Default)]
pub(crate) struct Session {
    pub(crate) refresh_token: Option<String>,
    pub(crate) access_token: Option<String>,
    /// When the next access-token refresh is due.
    pub(crate) next_refresh: Option<Instant>,
    /// Incremented on every new worker generation (login, logout, invalidated);
    /// a worker that sees a foreign generation stops.
    pub(crate) generation: u64,
    /// True while a session worker is running for the current generation.
    pub(crate) started: bool,
    /// Set by the worker when the session is declared invalid (too many
    /// failed refresh attempts); consumed by [`PrimePhotoApp::poll_session`].
    pub(crate) invalid: bool,
    /// Freshly minted cookies, picked up by `poll_session` every frame and
    /// copied into `Config` (memory only, never persisted).
    pub(crate) pending_cookies: Option<ParsedCookies>,
    /// The live cookie set, mirrored from `Config` by `apply_parsed` so the
    /// detached background supervisor has the session material that the
    /// on-disk `config.toml` no longer carries (`#[serde(skip)]`).
    pub(crate) cookies: SessionCookies,
}

/// Background refresh worker (user decision 2026-10-15). Ticks every
/// [`auth::TICK_INTERVAL_SECONDS`] seconds; refreshes the access token when
/// [`Session::next_refresh`] is due, then mints fresh cookies from the new
/// refresh token. Offline errors do **not** count as attempts and never sign
/// out. After [`auth::ACCESS_TOKEN_MAX_RETRIES`] failed attempts the session
/// is declared invalid.
pub(crate) async fn session_worker(
    session: Arc<Mutex<Session>>,
    country: String,
    language: String,
    ctx: egui::Context,
    my_gen: u64,
) {
    let client = reqwest::Client::new();
    let mut attempts = 0u32;
    loop {
        // Check due-ness **first**, then sleep only when nothing is due: a
        // session restored from the keychain is due immediately (`poll_session`
        // sets `next_refresh = now`), so the access token and the web cookies
        // are renewed right at start instead of after the first tick. Whenever
        // nothing is due we sleep a full tick — never a busy loop.
        let due_token = {
            let s = session.lock().unwrap();
            if s.generation != my_gen {
                return;
            }
            match (s.refresh_token.clone(), s.next_refresh) {
                (Some(rt), Some(t)) if t <= Instant::now() => Some(rt),
                // No token yet, no schedule yet, or the schedule lies in the
                // future: nothing to do this pass.
                _ => None,
            }
        };
        let Some(refresh_token) = due_token else {
            tokio::time::sleep(Duration::from_secs(auth::TICK_INTERVAL_SECONDS)).await;
            continue;
        };
        // Attempt loop (reuses the client).
        loop {
            // Re-check the generation before every attempt.
            {
                if session.lock().unwrap().generation != my_gen {
                    return;
                }
            }
            match auth::refresh_access_token(&client, &country, &language, &refresh_token).await
            {
                Ok(outcome) => {
                    log::info!("Access-Token aufgefrischt ({} s gültig).", outcome.expires_in);
                    let delay = auth::next_refresh_delay(outcome.expires_in);
                    let cookies = auth::fetch_auth_cookies(
                        &client,
                        &country,
                        &language,
                        &refresh_token,
                    )
                    .await;
                    {
                        let mut s = session.lock().unwrap();
                        // Re-check the generation *inside* the lock, after the
                        // awaits: a "log out" during the refresh bumped it, so
                        // writing now would resurrect the discarded session.
                        if s.generation != my_gen {
                            return;
                        }
                        s.access_token = Some(outcome.access_token);
                        // Clamp the schedule to at least one tick, so a tiny
                        // `expires_in` cannot turn the worker into a hot loop.
                        s.next_refresh = Some(
                            Instant::now()
                                + delay.max(Duration::from_secs(auth::TICK_INTERVAL_SECONDS)),
                        );
                        match cookies {
                            Ok(c) => {
                                log::info!("Web-Cookies erneuert.");
                                s.pending_cookies = Some(c);
                            }
                            Err(e) => {
                                log::warn!("Cookies konnten nicht erneuert werden: {e}")
                            }
                        }
                    }
                    attempts = 0;
                    ctx.request_repaint();
                    break;
                }
                Err(e) if !auth::counts_as_attempt(&e) => {
                    // Offline: don't count, keep the schedule. `next_refresh`
                    // still lies in the past, so the outer loop would be due
                    // again immediately — sleep one tick (3 s) before breaking
                    // to keep this a paced retry, never a hot request loop.
                    tokio::time::sleep(Duration::from_secs(auth::TICK_INTERVAL_SECONDS)).await;
                    break;
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= auth::ACCESS_TOKEN_MAX_RETRIES {
                        let mut s = session.lock().unwrap();
                        // Same re-check as the success path: never invalidate a
                        // session that was already superseded while we awaited.
                        if s.generation != my_gen {
                            return;
                        }
                        s.invalid = true;
                        s.access_token = None;
                        s.next_refresh = None;
                        s.started = false;
                        ctx.request_repaint();
                        return;
                    }
                    log::warn!(
                        "Auffrischung fehlgeschlagen ({attempts}/{}): {e}",
                        auth::ACCESS_TOKEN_MAX_RETRIES
                    );
                    tokio::time::sleep(Duration::from_secs(
                        auth::ACCESS_TOKEN_RETRY_DELAY_SECONDS,
                    ))
                    .await;
                }
            }
        }
    }
}

// -- login --------------------------------------------------------------

impl PrimePhotoApp {
    pub(crate) fn cancel_login(&mut self) {
        self.login.cancel();
    }

    /// Sign out: drop the in-memory session and the stored refresh token, tell
    /// the server, and return to the login screen (user decision 2026-10-15).
    ///
    /// The **refresh token** is deleted from the OS keychain and the whole
    /// session (access token + cookies) is dropped — nothing session-related is
    /// left on disk (`Config` keeps the cookie fields in memory only). Telling
    /// the server (`/auth/deregister`) is **best-effort**: a failure only logs a
    /// warning, the local sign-out still happens.
    pub(crate) fn logout(&mut self) {
        self.config_form = None;
        // Cancel a running upload (user decision 2026-10-16): the worker holds
        // its own `Config` clone with the old cookies and would keep failing
        // against the revoked session. It stops at its next wait point; a
        // request already in flight is allowed to finish.
        self.upload.lock().unwrap().cancelled = true;
        // Remember what the server call needs before the session is cleared.
        let access_token = self.session.lock().unwrap().access_token.clone();
        let country = self.config.country.clone();
        let language = crate::urls::portal_language(&country).to_string();
        // Stop the worker for good: bumping the generation makes a running
        // worker return on its next check.
        {
            let mut s = self.session.lock().unwrap();
            s.generation += 1;
            s.started = false;
            s.refresh_token = None;
            s.access_token = None;
            s.next_refresh = None;
            s.invalid = false;
            s.pending_cookies = None;
            s.cookies = SessionCookies::default();
        }
        self.config.session_id.clear();
        self.config.cookie_x_acb.clear();
        self.config.cookie_at_acb.clear();
        self.config.cookie_ubid_acb.clear();
        self.config.cookie_x_amz_access_token.clear();
        match self.config.save() {
            Ok(_) => self.log(tr("Abgemeldet.")),
            Err(e) => {
                log::warn!("Einstellungen konnten nicht gespeichert werden: {e}");
                self.config_error =
                    Some(tr("Die Einstellungen konnten nicht gespeichert werden.").to_string());
            }
        }
        // Deregister on the server when we still had an access token; never on
        // the UI thread.
        if let Some(access_token) = access_token {
            self.rt.spawn(async move {
                let client = reqwest::Client::new();
                if let Err(e) = auth::deregister(&client, &country, &language, &access_token).await {
                    log::warn!("Abmelden am Server fehlgeschlagen: {e}");
                }
            });
        }
        if let Err(e) = auth::KeyringTokenStore.clear() {
            log::warn!("Refresh-Token konnte nicht gelöscht werden: {e}");
        }
    }

    /// Whether a **refresh token** is available but the cookies have not been
    /// minted yet — a session restored from the keychain at start (S-C).
    ///
    /// The login screen is only shown when this is `false` too, so a restart
    /// with a stored token goes straight to the main UI while the worker fetches
    /// the access token and the cookies in the background (like the original,
    /// which only shows its auth portal without a refresh token).
    pub(crate) fn has_pending_session(&self) -> bool {
        self.session.lock().unwrap().refresh_token.is_some()
    }

    /// Session upkeep, called every UI frame (S-C, user decision 2026-10-15).
    ///
    /// This is the **only** place a session worker is spawned. It starts one as
    /// soon as a refresh token is available (after a device login, or at start
    /// with a token from the keychain) and no worker is running for the current
    /// generation. It also reacts to an invalidated session and picks up freshly
    /// minted cookies.
    pub(crate) fn poll_session(&mut self, ctx: &egui::Context) {
        // (1) The worker declared the session invalid (too many failed refresh
        // attempts): drop every token and return to the login screen — **no**
        // app restart (user decision 2026-10-15). This runs **before** the spawn
        // branch below: it bumps the generation and clears the refresh token, so
        // the spawn branch sees `refresh_token == None` and cannot start a worker
        // for the token we just discarded in the very same frame.
        if self.session.lock().unwrap().invalid {
            if let Err(e) = auth::KeyringTokenStore.clear() {
                log::warn!("Refresh-Token konnte nicht gelöscht werden: {e}");
            }
            {
                let mut s = self.session.lock().unwrap();
                s.refresh_token = None;
                s.access_token = None;
                s.next_refresh = None;
                s.invalid = false;
                s.generation += 1;
                s.started = false;
                s.cookies = SessionCookies::default();
                // Drop any cookies the worker minted but the UI has not consumed
                // yet: otherwise step (3) below would re-install exactly the
                // cookies we just discarded in this same frame.
                s.pending_cookies = None;
            }
            self.config.session_id = String::new();
            self.config.cookie_x_acb = String::new();
            self.config.cookie_at_acb = String::new();
            self.config.cookie_ubid_acb = String::new();
            self.config.cookie_x_amz_access_token = String::new();
            if let Err(e) = self.config.save() {
                log::warn!("Einstellungen konnten nicht gespeichert werden: {e}");
                self.config_error =
                    Some(tr("Die Einstellungen konnten nicht gespeichert werden.").to_string());
            }
            self.log(tr("Sitzung abgelaufen – bitte erneut anmelden."));
        }

        // (2) Start the refresh worker. `started` is cleared by a successful
        // login (`poll_login`), by `logout` and by an invalidated session, so
        // this branch covers both "app start with a stored token" and "just
        // signed in".
        let start_gen = {
            let mut s = self.session.lock().unwrap();
            if !s.started && s.refresh_token.is_some() {
                s.generation += 1;
                s.started = true;
                // A session restored from the keychain has no schedule yet: make
                // the worker due immediately, so the access token is renewed and
                // the web cookies are minted **at start**. After a login the
                // 80 %-schedule is already set by `poll_login`.
                if s.next_refresh.is_none() {
                    log::info!("Gespeicherte Sitzung gefunden – Access-Token wird erneuert.");
                    s.next_refresh = Some(Instant::now());
                }
                Some(s.generation)
            } else {
                None
            }
        };
        if let Some(my_gen) = start_gen {
            let country = self.config.country.clone();
            let language = crate::urls::portal_language(&country).to_string();
            let session = Arc::clone(&self.session);
            let ctx = ctx.clone();
            self.rt.spawn(async move {
                session_worker(session, country, language, ctx, my_gen).await;
            });
        }

        // (3) Freshly minted cookies from the worker: into the in-memory config
        // only, never persisted (`#[serde(skip)]`, see `config.rs`).
        let pending = self.session.lock().unwrap().pending_cookies.take();
        if let Some(cookies) = pending {
            self.apply_parsed(&cookies);
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
        // (1) The device redemption (background task) reported back.
        if let Some(result) = self.login.take_device_result() {
            // The user cancelled while the task ran: drop the result.
            if !self.login.running {
                return;
            }
            match result {
                Ok(dl) => {
                    // Cookies may be absent: the register succeeded but the
                    // follow-up cookie fetch failed. That is a **valid** login
                    // (the refresh token is stored) — the worker is made due
                    // immediately and mints the cookies on its next pass.
                    let have_cookies = dl.cookies.is_some();
                    if let Some(cookies) = &dl.cookies {
                        self.apply_parsed(cookies);
                    }
                    {
                        let mut s = self.session.lock().unwrap();
                        s.refresh_token = Some(dl.register.refresh_token.clone());
                        s.access_token = Some(dl.register.access_token.clone());
                        s.next_refresh = Some(if have_cookies {
                            Instant::now() + auth::next_refresh_delay(dl.register.expires_in)
                        } else {
                            // No cookies yet: refresh at once, not after 80 %.
                            Instant::now()
                        });
                        s.invalid = false;
                        s.started = false;
                        s.generation += 1;
                    }
                    match self.config.save() {
                        Ok(_) => self.log(tr("Login erfolgreich – Refresh-Token gespeichert.")),
                        Err(e) => {
                            log::warn!("Einstellungen konnten nicht gespeichert werden: {e}");
                            self.config_error = Some(
                                tr("Die Einstellungen konnten nicht gespeichert werden.")
                                    .to_string(),
                            );
                        }
                    }
                    self.login.cancel();
                    return;
                }
                Err(e) => {
                    // The device path was rejected: report it (never swallow it).
                    // There is no cookie fallback any more (user decision
                    // 2026-10-15) — the device path is the only sign-in.
                    self.login.error = Some(
                        tr("Die Anmeldung wurde von Amazon abgelehnt. Bitte erneut versuchen.")
                            .to_string(),
                    );
                    log::warn!("{e}");
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
                let my_gen = self.login.login_generation;
                self.rt.spawn(async move {
                    let client = reqwest::Client::new();
                    // The slot holds `(generation, Result<DeviceLogin, String>)`:
                    // build the inner result first, then tag it with the
                    // generation in one place.
                    //
                    // The redeem is retried only to **wait out a network
                    // failure** (user decision 2026-10-15): the request never
                    // reached the server, so the single-use
                    // `openid.oa2.authorization_code` is still valid. Up to
                    // `REGISTER_MAX_ATTEMPTS` offline passes, with
                    // `TICK_INTERVAL_SECONDS` between them (the login spinner
                    // stays visible). A **counted** error (HTTP status, JSON,
                    // empty token) means the server saw the code — a second try
                    // would find it already consumed (`InvalidValue`), so the
                    // loop stops at once instead of burning the code.
                    let mut offline_passes = 0u32;
                    let inner: Result<DeviceLogin, String> = loop {
                        match auth::register_device(
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
                                // The refresh token is persisted; a keychain
                                // failure is reported but not fatal — the
                                // cookies are still worth fetching.
                                if let Err(e) =
                                    auth::KeyringTokenStore.save(&outcome.refresh_token)
                                {
                                    log::warn!(
                                        "Refresh-Token konnte nicht im Schlüsselbund gespeichert werden: {e}"
                                    );
                                }
                                break match auth::fetch_auth_cookies(
                                    &client,
                                    &country,
                                    &language,
                                    &outcome.refresh_token,
                                )
                                .await
                                {
                                    Ok(cookies) => Ok(DeviceLogin {
                                        register: outcome,
                                        cookies: Some(cookies),
                                    }),
                                    // The register succeeded: the login is valid
                                    // and the refresh token is stored. Do **not**
                                    // turn this into a rejection — publish the
                                    // outcome without cookies so the worker mints
                                    // them on its next pass.
                                    Err(e) => {
                                        log::warn!(
                                            "Cookies konnten nach der Anmeldung nicht abgerufen werden: {e}"
                                        );
                                        Ok(DeviceLogin {
                                            register: outcome,
                                            cookies: None,
                                        })
                                    }
                                };
                            }
                            Err(e) if !auth::counts_as_attempt(&e) => {
                                offline_passes += 1;
                                if offline_passes >= auth::REGISTER_MAX_ATTEMPTS {
                                    break Err(e.to_string());
                                }
                                log::warn!(
                                    "Einlösen fehlgeschlagen (offline {offline_passes}/{}): {e}",
                                    auth::REGISTER_MAX_ATTEMPTS
                                );
                                tokio::time::sleep(Duration::from_secs(
                                    auth::TICK_INTERVAL_SECONDS,
                                ))
                                .await;
                            }
                            // A counted error means the server answered and the
                            // one-time authorization code is spent: retrying it
                            // cannot succeed, so fail immediately.
                            Err(e) => break Err(e.to_string()),
                        }
                    };
                    *slot.lock().unwrap() = Some((my_gen, inner));
                });
                return;
            }
        }
    }

    /// Copy a parsed cookie set into the config (the device path's
    /// `fetch_auth_cookies`).
    ///
    /// The config fields feed the upload engine; the same set is mirrored into
    /// the in-memory session so the **detached** background supervisor has the
    /// live cookies the on-disk `config.toml` no longer carries (`#[serde(skip)]`).
    pub(crate) fn apply_parsed(&mut self, parsed: &ParsedCookies) {
        self.config.session_id = parsed.session_id.clone();
        if !parsed.country.is_empty() {
            self.config.country = parsed.country.clone();
        }
        self.config.cookie_x_acb = parsed.cookie_x_acb.clone();
        self.config.cookie_at_acb = parsed.cookie_at_acb.clone();
        self.config.cookie_ubid_acb = parsed.cookie_ubid_acb.clone();
        self.config.cookie_x_amz_access_token = parsed.cookie_x_amz_access_token.clone();
        self.session.lock().unwrap().cookies = SessionCookies::from(parsed);
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
                    ui.horizontal_centered(|ui| {
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
            // A failed config save (e.g. right after signing out) must stay
            // visible on the sign-in screen — the sidebar that normally shows
            // `config_error` is not rendered here (user decision 2026-10-16).
            if let Some(e) = &self.config_error {
                ui.add_space(6.0);
                ui.colored_label(theme::RED, e);
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
