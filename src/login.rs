//! Embedded login. Amazon's real sign-in page is shown in a WKWebView that is a
//! **child view of the main application window** — no extra window, no external
//! browser, no Chrome DevTools Protocol.
//!
//! The loaded page is the **device** portal URL ([`crate::urls::auth_portal_url`],
//! PKCE); a navigation handler captures the `openid.oa2.authorization_code` when
//! Amazon redirects to an `/ap/…` URL and **ignores** that navigation (the
//! original does the same, `docs/original/SPEC_LOGIN.md` § 3.4). The code is redeemed
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
use crate::login;
use crate::theme;
use eframe::egui::{self, RichText};

thread_local! {
    /// The login webview lives on the main thread only (a WebKit requirement).
    static WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

/// The **sign-in completion signal** of the device flow: the authorization code
/// carried as `openid.oa2.authorization_code` on a `/ap/` navigation (original
/// `LoginWebPolicyDelegate`, `docs/original/SPEC_LOGIN.md` § 3.4).
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
    /// (`docs/original/SPEC_LOGIN.md` § 3.4). Every other navigation is used.
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
                // behaviour, docs/original/SPEC_LOGIN.md § 3.3).
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
    /// When a failed cookie mint should be retried, independently of token refresh.
    pub(crate) cookie_retry_at: Option<Instant>,
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

struct DueSessionWork {
    refresh_token: Option<String>,
    refresh_access_token: bool,
    mint_cookies: bool,
}

fn next_cookie_retry_at(now: Instant) -> Instant {
    now + Duration::from_secs(auth::TICK_INTERVAL_SECONDS)
}

fn schedule_cookie_retry(session: &mut Session, now: Instant) {
    session.cookie_retry_at = Some(next_cookie_retry_at(now));
}

fn cookie_retry_is_due(session: &Session, now: Instant) -> bool {
    session.cookie_retry_at.is_some_and(|retry_at| retry_at <= now)
}

fn next_login_refresh_at(now: Instant, expires_in: i64) -> Instant {
    now + auth::next_refresh_delay(expires_in)
}

fn next_refresh_after_success(now: Instant, expires_in: i64) -> Instant {
    now + auth::next_refresh_delay(expires_in)
        .max(Duration::from_secs(auth::TICK_INTERVAL_SECONDS))
}

fn record_access_refresh_success(
    session: &mut Session,
    access_token: String,
    expires_in: i64,
    now: Instant,
) {
    session.access_token = Some(access_token);
    session.next_refresh = Some(next_refresh_after_success(now, expires_in));
}

fn record_cookie_mint_result(
    session: &mut Session,
    result: Result<ParsedCookies, auth::AuthError>,
    now: Instant,
) -> Result<(), auth::AuthError> {
    match result {
        Ok(cookies) => {
            session.cookie_retry_at = None;
            session.pending_cookies = Some(cookies);
            Ok(())
        }
        Err(error) => {
            schedule_cookie_retry(session, now);
            Err(error)
        }
    }
}

fn install_login_session(
    session: &mut Session,
    refresh_token: String,
    access_token: String,
    expires_in: i64,
    cookies_minted: bool,
    now: Instant,
) {
    session.refresh_token = Some(refresh_token);
    session.access_token = Some(access_token);
    session.next_refresh = Some(next_login_refresh_at(now, expires_in));
    session.cookie_retry_at = if cookies_minted {
        None
    } else {
        Some(next_cookie_retry_at(now))
    };
    session.invalid = false;
    session.started = false;
    session.pending_cookies = None;
    session.generation += 1;
}

fn due_session_work(
    session: &Session,
    now: Instant,
    next_token_attempt_at: Option<Instant>,
) -> DueSessionWork {
    let has_refresh_token = session.refresh_token.is_some();
    let refresh_access_token = has_refresh_token
        && session.next_refresh.is_some_and(|refresh_at| refresh_at <= now)
        && next_token_attempt_at.is_none_or(|retry_at| retry_at <= now);
    let mint_cookies = has_refresh_token && cookie_retry_is_due(session, now);
    let refresh_token = if refresh_access_token || mint_cookies {
        session.refresh_token.clone()
    } else {
        None
    };

    DueSessionWork {
        refresh_token,
        refresh_access_token,
        mint_cookies,
    }
}

/// Background refresh worker (user decision 2026-10-15). Ticks every
/// [`auth::TICK_INTERVAL_SECONDS`] seconds; refreshes the access token when
/// [`Session::next_refresh`] is due, and mints fresh cookies independently when
/// their retry is due. Offline errors do **not** count as access-token attempts
/// and never sign out. After [`auth::ACCESS_TOKEN_MAX_RETRIES`] failed access
/// token attempts the session is declared invalid.
pub(crate) async fn session_worker(
    session: Arc<Mutex<Session>>,
    country: String,
    language: String,
    ctx: egui::Context,
    my_gen: u64,
) {
    let client = reqwest::Client::new();
    let mut attempts = 0u32;
    let mut next_token_attempt_at = None;
    loop {
        // Check both independent schedules before sleeping. A restored session
        // has a token refresh due immediately; a failed cookie mint is due on
        // the next 3-second tick even while its access-token appointment is in
        // the future. Whenever nothing is due, sleep a full tick — never spin.
        let work = {
            let s = session.lock().unwrap();
            if s.generation != my_gen {
                return;
            }
            due_session_work(&s, Instant::now(), next_token_attempt_at)
        };
        let Some(refresh_token) = work.refresh_token else {
            tokio::time::sleep(Duration::from_secs(auth::TICK_INTERVAL_SECONDS)).await;
            continue;
        };

        if work.refresh_access_token {
            // Re-check the generation before every access-token attempt.
            if session.lock().unwrap().generation != my_gen {
                return;
            }
            match auth::refresh_access_token(&client, &country, &language, &refresh_token).await
            {
                Ok(outcome) => {
                    log::info!("Access-Token aufgefrischt ({} s gültig).", outcome.expires_in);
                    let refreshed_at = Instant::now();
                    {
                        let mut s = session.lock().unwrap();
                        // Re-check after the await: a logout or new login must
                        // not be overwritten by this worker generation.
                        if s.generation != my_gen {
                            return;
                        }
                        record_access_refresh_success(
                            &mut s,
                            outcome.access_token,
                            outcome.expires_in,
                            refreshed_at,
                        );
                    }
                    attempts = 0;
                    next_token_attempt_at = None;

                    let cookies = auth::fetch_auth_cookies(
                        &client,
                        &country,
                        &language,
                        &refresh_token,
                    )
                    .await;
                    let cookie_result = {
                        let mut s = session.lock().unwrap();
                        // Re-check the generation *inside* the lock, after the
                        // awaits: a "log out" during the refresh bumped it, so
                        // writing now would resurrect the discarded session.
                        if s.generation != my_gen {
                            return;
                        }
                        record_cookie_mint_result(&mut s, cookies, Instant::now())
                    };
                    match cookie_result {
                        Ok(()) => log::info!("Web-Cookies erneuert."),
                        Err(e) => log::warn!("Cookies konnten nicht erneuert werden: {e}"),
                    }
                    ctx.request_repaint();
                    // This pass already minted cookies after the successful
                    // access-token refresh, whether or not a retry was pending.
                    continue;
                }
                Err(e) if !auth::counts_as_attempt(&e) => {
                    // Offline access-token failures keep their existing policy:
                    // retry on the next worker tick without consuming budget.
                    next_token_attempt_at = Some(next_cookie_retry_at(Instant::now()));
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
                        s.cookie_retry_at = None;
                        s.started = false;
                        ctx.request_repaint();
                        return;
                    }
                    log::warn!(
                        "Auffrischung fehlgeschlagen ({attempts}/{}): {e}",
                        auth::ACCESS_TOKEN_MAX_RETRIES
                    );
                    next_token_attempt_at = Some(
                        Instant::now()
                            + Duration::from_secs(auth::ACCESS_TOKEN_RETRY_DELAY_SECONDS),
                    );
                }
            }
        }

        // A cookie retry is independent of access-token failures and its budget.
        // If access refresh succeeded above, its cookie call already happened;
        // otherwise this is the cookie-only retry path.
        if work.mint_cookies {
            if session.lock().unwrap().generation != my_gen {
                return;
            }
            let cookies = auth::fetch_auth_cookies(&client, &country, &language, &refresh_token)
                .await;
            let result = {
                let mut s = session.lock().unwrap();
                if s.generation != my_gen {
                    return;
                }
                record_cookie_mint_result(&mut s, cookies, Instant::now())
            };
            match result {
                Ok(()) => {
                    log::info!("Web-Cookies erneut gemintet.");
                    ctx.request_repaint();
                }
                Err(e) => log::warn!("Cookies konnten nicht erneuert werden: {e}"),
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
            s.cookie_retry_at = None;
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
                s.refresh_token = None;
                s.access_token = None;
                s.next_refresh = None;
                s.cookie_retry_at = None;
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

        // (2) Start the session worker. It must run even when only cookie retry
        // work is due and the access-token appointment is still in the future.
        // `started` is cleared by login, logout and invalidation, so this covers
        // both "app start with a stored token" and "just signed in".
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
    /// (`docs/original/SPEC_LOGIN.md` § 3.3).
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
                    // follow-up cookie fetch failed. That is a **valid** login;
                    // the cookie retry is scheduled separately from the
                    // access-token appointment derived from `expires_in`.
                    let cookies_minted = dl.cookies.is_some();
                    if let Some(cookies) = &dl.cookies {
                        self.apply_parsed(cookies);
                    }
                    {
                        let mut s = self.session.lock().unwrap();
                        install_login_session(
                            &mut s,
                            dl.register.refresh_token.clone(),
                            dl.register.access_token.clone(),
                            dl.register.expires_in,
                            cookies_minted,
                            Instant::now(),
                        );
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
                        ui.add(crate::icons::app_logo(ui, 30.0));
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
            ui.add(crate::icons::app_logo(ui, 84.0));
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

    fn parsed_cookies() -> ParsedCookies {
        ParsedCookies {
            session_id: "session".to_string(),
            country: "de".to_string(),
            cookie_x_acb: "x-acb".to_string(),
            cookie_at_acb: "at-acb".to_string(),
            cookie_ubid_acb: "ubid-acb".to_string(),
            cookie_x_amz_access_token: "access".to_string(),
        }
    }

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

    #[test]
    fn failed_cookie_mint_retries_next_tick_without_moving_refresh_appointment() {
        let now = Instant::now();
        let mut session = Session {
            refresh_token: Some("refresh".to_string()),
            ..Session::default()
        };

        record_access_refresh_success(&mut session, "access".to_string(), 3600, now);
        let access_appointment = session.next_refresh;
        assert!(record_cookie_mint_result(
            &mut session,
            Err(auth::AuthError::Network("offline".to_string())),
            now,
        )
        .is_err());

        let next_tick = now + Duration::from_secs(auth::TICK_INTERVAL_SECONDS);
        let work = due_session_work(&session, next_tick, None);
        assert!(work.mint_cookies);
        assert!(!work.refresh_access_token);
        assert_eq!(work.refresh_token.as_deref(), Some("refresh"));
        assert_eq!(session.next_refresh, access_appointment);
    }

    #[test]
    fn login_cookie_failure_schedules_retry_without_advancing_access_refresh() {
        let now = Instant::now();
        let mut session = Session::default();

        install_login_session(
            &mut session,
            "refresh".to_string(),
            "access".to_string(),
            3600,
            false,
            now,
        );

        let expected_access_appointment = Some(now + auth::next_refresh_delay(3600));
        assert_eq!(session.next_refresh, expected_access_appointment);
        assert_eq!(session.cookie_retry_at, Some(next_cookie_retry_at(now)));

        let work = due_session_work(
            &session,
            now + Duration::from_secs(auth::TICK_INTERVAL_SECONDS),
            None,
        );
        assert!(work.mint_cookies);
        assert!(!work.refresh_access_token);
    }

    #[test]
    fn cookie_only_retry_preserves_session_and_refresh_schedule() {
        let now = Instant::now();
        let access_appointment = now + Duration::from_secs(600);
        let mut session = Session {
            refresh_token: Some("refresh".to_string()),
            access_token: Some("access".to_string()),
            next_refresh: Some(access_appointment),
            cookie_retry_at: Some(now),
            ..Session::default()
        };

        let work = due_session_work(&session, now, None);
        assert!(work.mint_cookies);
        assert!(!work.refresh_access_token);
        assert!(record_cookie_mint_result(
            &mut session,
            Err(auth::AuthError::HttpStatus {
                status: 503,
                body: "unavailable".to_string(),
            }),
            now,
        )
        .is_err());
        assert!(!session.invalid);
        assert_eq!(session.next_refresh, Some(access_appointment));

        let retry_at = session.cookie_retry_at.unwrap();
        let retry_work = due_session_work(&session, retry_at, None);
        assert!(retry_work.mint_cookies);
        assert!(!retry_work.refresh_access_token);
        assert!(record_cookie_mint_result(&mut session, Ok(parsed_cookies()), retry_at).is_ok());
        assert!(!session.invalid);
        assert_eq!(session.next_refresh, Some(access_appointment));
        assert!(session.cookie_retry_at.is_none());
        assert!(session.pending_cookies.is_some());
    }
}
