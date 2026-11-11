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
//! cannot be read from the UI frame loop — the device path never needs it.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wry::dpi::{LogicalPosition, LogicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{Rect, WebViewBuilder};

use crate::auth::{self, Pkce, TokenStore};
use crate::countries::ParsedCookies;
use crate::i18n::tr;

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

/// A rectangle in logical points with a **top-left** origin, as wry wants.
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
    /// Optional host wakeup; some hosts drive their own frame loop.
    wakeup: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
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
            wakeup: Arc::new(Mutex::new(None)),
            login_generation: 0,
        }
    }

    pub(crate) fn set_wakeup(&mut self, wakeup: Arc<dyn Fn() + Send + Sync>) {
        *self.wakeup.lock().unwrap() = Some(wakeup);
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
        let wakeup = Arc::clone(&self.wakeup);
        match WebViewBuilder::new()
            .with_bounds(bounds)
            .with_navigation_handler(move |url: String| match extract_authorization_code(&url) {
                Some(code) => {
                    *auth_code.lock().unwrap() = Some(code);
                    if let Some(wakeup) = wakeup.lock().unwrap().clone() {
                        wakeup();
                    }
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
                        match crate::wry_host::install(wv) {
                            Ok(()) => {
                                self.running = true;
                                self.error = None;
                            }
                            Err(e) => {
                                log::warn!("Anmelde-WebView konnte nicht gehostet werden: {e}");
                                self.error = Some(
                                    tr("Anmeldeseite konnte nicht geöffnet werden.").to_string(),
                                );
                            }
                        }
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
        crate::wry_host::set_bounds(bounds);
    }

    pub fn cancel(&mut self) {
        crate::wry_host::close();
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
    /// failed refresh attempts); consumed by
    /// [`crate::rune_backup_live::session_route`].
    pub(crate) invalid: bool,
    /// Freshly minted cookies, picked up by `poll_session` every frame and
    /// copied into `Config` (memory only, never persisted).
    pub(crate) pending_cookies: Option<ParsedCookies>,
    /// The live cookie set, mirrored from `Config` by `apply_parsed` so the
    /// detached background supervisor has the session material that the
    /// on-disk `config.toml` no longer carries (`#[serde(skip)]`).
    pub(crate) cookies: SessionCookies,
}

/// UI-neutral notification for session-worker state changes.
pub(crate) type SessionWakeup = Arc<dyn Fn() + Send + Sync>;

/// Restore only the persisted refresh token. Access tokens and cookies are
/// intentionally transient and are recreated by the session worker.
pub(crate) fn restore_refresh_token(
    session: &mut Session,
    token_store: &dyn auth::TokenStore,
) -> Result<bool, String> {
    match token_store.load()? {
        Some(refresh_token) => {
            session.refresh_token = Some(refresh_token);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Claim the next session-worker generation. A restored token has no access
/// token schedule, so it is due now; a signed-in session keeps its existing
/// 80%-of-lifetime appointment.
pub(crate) fn prepare_session_worker(session: &mut Session) -> Option<u64> {
    if session.invalid || session.started || session.refresh_token.is_none() {
        return None;
    }
    session.generation += 1;
    session.started = true;
    if session.next_refresh.is_none() {
        log::info!("Gespeicherte Sitzung gefunden – Access-Token wird erneuert.");
        session.next_refresh = Some(Instant::now());
    }
    Some(session.generation)
}

/// Clear all session and in-memory cookie state after the worker declares the
/// refresh token invalid. The host owns persistence and user-facing status.
pub(crate) fn clear_invalidated_session(
    config: &mut crate::config::Config,
    session: &mut Session,
) {
    session.refresh_token = None;
    session.access_token = None;
    session.next_refresh = None;
    session.cookie_retry_at = None;
    session.invalid = false;
    session.generation += 1;
    session.started = false;
    session.cookies = SessionCookies::default();
    session.pending_cookies = None;
    clear_config_cookies(config);
}

fn clear_config_cookies(config: &mut crate::config::Config) {
    config.session_id.clear();
    config.cookie_x_acb.clear();
    config.cookie_at_acb.clear();
    config.cookie_ubid_acb.clear();
    config.cookie_x_amz_access_token.clear();
}

/// Consume worker-minted cookies into both the host Config and the shared
/// in-memory session. These fields are serde-skipped and are never persisted.
pub(crate) fn apply_pending_cookies(
    config: &mut crate::config::Config,
    session: &mut Session,
) -> bool {
    if session.invalid {
        return false;
    }
    let Some(cookies) = session.pending_cookies.take() else {
        return false;
    };
    apply_parsed_cookies(config, &mut session.cookies, &cookies);
    true
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
    wakeup: SessionWakeup,
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
            if s.generation != my_gen || s.invalid {
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
                    wakeup();
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
                        wakeup();
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
                    wakeup();
                }
                Err(e) => log::warn!("Cookies konnten nicht erneuert werden: {e}"),
            }
        }
    }
}

// -- login --------------------------------------------------------------

pub(crate) enum LoginPoll {
    Idle,
    Redeeming,
    Succeeded,
    Failed,
}

/// Start the production PKCE portal against whichever native window owns it.
/// Both the desktop UI and developer-only Rune host use this exact URL builder and
/// `WebviewLogin` navigation handler.
pub(crate) fn start_device_login<W: HasWindowHandle>(
    login: &mut WebviewLogin,
    config: &mut crate::config::Config,
    window: &W,
    bounds: Rect,
) {
    if login.running {
        return;
    }
    if config.device_serial.is_empty() {
        config.device_serial = auth::new_device_serial();
    }
    let pkce = Pkce::generate();
    let client_id = auth::client_id(&config.device_serial, auth::DEVICE_TYPE);
    let url = crate::urls::auth_portal_url(
        &config.country,
        &client_id,
        &pkce.challenge,
        crate::urls::portal_language(&config.country),
    );
    login.pkce = Some(pkce);
    login.start(window, &url, bounds);
}

/// Shared completion/redeem path for the desktop app and Rune's explicit
/// developer login window. No request is started until the navigation handler
/// has captured a real authorization code from the production portal.
pub(crate) fn poll_login_flow(
    login: &mut WebviewLogin,
    config: &mut crate::config::Config,
    session: &Arc<Mutex<Session>>,
    runtime: &tokio::runtime::Runtime,
    config_error: &mut Option<String>,
    persist_config: bool,
) -> LoginPoll {
    if let Some(result) = login.take_device_result() {
        if !login.running {
            return LoginPoll::Idle;
        }
        match result {
            Ok(device_login) => {
                let cookies_minted = device_login.cookies.is_some();
                {
                    let mut session = session.lock().unwrap();
                    if let Some(cookies) = &device_login.cookies {
                        apply_parsed_cookies(config, &mut session.cookies, cookies);
                    }
                    install_login_session(
                        &mut session,
                        device_login.register.refresh_token.clone(),
                        device_login.register.access_token.clone(),
                        device_login.register.expires_in,
                        cookies_minted,
                        Instant::now(),
                    );
                }
                if persist_config {
                    match config.save() {
                        Ok(_) => {}
                        Err(e) => {
                            log::warn!("Einstellungen konnten nicht gespeichert werden: {e}");
                            *config_error = Some(
                                tr("Die Einstellungen konnten nicht gespeichert werden.")
                                    .to_string(),
                            );
                        }
                    }
                }
                login.cancel();
                return LoginPoll::Succeeded;
            }
            Err(e) => {
                login.error = Some(
                    tr("Die Anmeldung wurde von Amazon abgelehnt. Bitte erneut versuchen.")
                        .to_string(),
                );
                log::warn!("{e}");
                login.cancel();
                return LoginPoll::Failed;
            }
        }
    }

    if login.running {
        if let Some(code) = login.take_auth_code() {
            let Some(pkce) = login.take_pkce() else {
                log::warn!("Anmeldecode ohne PKCE-Paket – verworfen");
                login.cancel();
                return LoginPoll::Failed;
            };
            let country = config.country.clone();
            let serial = config.device_serial.clone();
            let client_id = auth::client_id(&serial, auth::DEVICE_TYPE);
            let language = crate::urls::portal_language(&country).to_string();
            let slot = Arc::clone(&login.device_result);
            let wakeup = Arc::clone(&login.wakeup);
            let my_generation = login.login_generation;
            runtime.spawn(async move {
                let client = reqwest::Client::new();
                // A single-use code is retried only while the request never
                // reached the server; a counted server response spends it.
                let mut offline_passes = 0u32;
                let result: Result<DeviceLogin, String> = loop {
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
                            if let Err(e) = auth::KeyringTokenStore.save(&outcome.refresh_token) {
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
                        Err(e) => break Err(e.to_string()),
                    }
                };
                *slot.lock().unwrap() = Some((my_generation, result));
                if let Some(wakeup) = wakeup.lock().unwrap().clone() {
                    wakeup();
                }
            });
            return LoginPoll::Redeeming;
        }
    }

    LoginPoll::Idle
}

/// Copy cookie/session data without changing the user's selected marketplace.
/// Cookie-name suffixes identify which cookie value is being copied; they are
/// not a source of marketplace selection.
fn apply_parsed_cookies(
    config: &mut crate::config::Config,
    session_cookies: &mut SessionCookies,
    parsed: &ParsedCookies,
) {
    config.session_id = parsed.session_id.clone();
    config.cookie_x_acb = parsed.cookie_x_acb.clone();
    config.cookie_at_acb = parsed.cookie_at_acb.clone();
    config.cookie_ubid_acb = parsed.cookie_ubid_acb.clone();
    config.cookie_x_amz_access_token = parsed.cookie_x_amz_access_token.clone();
    *session_cookies = SessionCookies::from(parsed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_cookies() -> ParsedCookies {
        ParsedCookies {
            session_id: "session".to_string(),
            cookie_x_acb: "x-acb".to_string(),
            cookie_at_acb: "at-acb".to_string(),
            cookie_ubid_acb: "ubid-acb".to_string(),
            cookie_x_amz_access_token: "access".to_string(),
        }
    }

    #[test]
    fn applying_de_suffixed_cookies_keeps_the_selected_at_marketplace() {
        let mut config = crate::config::Config::default();
        config.country = "at".to_string();
        let mut session_cookies = SessionCookies::default();
        let parsed = crate::countries::parse_cookie_string(
            "session-id=SID; x-acbde=ACB; at-acbde=AT; ubid-acbde=UBID; sess-at-acbde=SESSAT",
        );

        apply_parsed_cookies(&mut config, &mut session_cookies, &parsed);

        assert_eq!(config.country, "at");
        assert_eq!(config.session_id, "SID");
        assert_eq!(config.cookie_x_acb, "ACB");
        assert_eq!(config.cookie_at_acb, "AT");
        assert_eq!(config.cookie_ubid_acb, "UBID");
        assert_eq!(config.cookie_x_amz_access_token, "SESSAT");
        assert_eq!(session_cookies.session_id, "SID");
        assert_eq!(session_cookies.cookie_x_acb, "ACB");
        assert_eq!(session_cookies.cookie_at_acb, "AT");
        assert_eq!(session_cookies.cookie_ubid_acb, "UBID");
        assert_eq!(session_cookies.cookie_x_amz_access_token, "SESSAT");
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
