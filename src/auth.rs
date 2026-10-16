//! The **device** login flow (PKCE → authorization code → tokens), rebuilt from
//! the original's observed behaviour (user decision 2026-10-15,
//! `docs/original/SPEC_LOGIN.md` § 3.8 / `docs/protocol/PROTOCOL.md` § 2.1).
//!
//! This module contains the **pure, testable building blocks only**: PKCE, the
//! portal URL (in [`crate::urls`]), the request bodies and response parsers, the
//! keychain-backed refresh-token store and the 80 %-refresh schedule. Since S-B
//! the **login path is wired in**: `begin_login`/`poll_login` use [`Pkce`],
//! [`client_id`], [`register_device`], [`KeyringTokenStore`] and
//! [`fetch_auth_cookies`]. Since S-C the **session is wired in** too:
//! `login::session_worker` consumes [`refresh_access_token`],
//! [`fetch_auth_cookies`], [`next_refresh_delay`], [`counts_as_attempt`] and
//! the refresh constants, and [`deregister`] backs `logout`.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use keyring::Entry;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::countries::ParsedCookies;
use crate::urls;

// ---------------------------------------------------------------------------
// Device identity & protocol defaults
// ---------------------------------------------------------------------------

/// Application name the device flow announces to Amazon (recon 2026-10-15).
///
/// The three identity values below are the ones the **original** currently
/// sends (probed 2026-10-15, `docs/protocol/PROTOCOL.md` § 2.1). They only ever appear in
/// the request **body** of the device path — never in our UI, window title,
/// bundle name or branding: we speak the original client's protocol on the
/// wire, we do not present ourselves as an Amazon app (project rule "Name &
/// Branding", `AGENTS.md`).
pub(crate) const APP_NAME: &str = "Amazon Photos";

/// Application version the device flow announces (original, probed 2026-10-15).
pub(crate) const APP_VERSION: &str = "11.3.0.1d7941b5";

/// Device type id the original uses (probed 2026-10-15). The original's bundle
/// carries exactly **one** device type, so there is no second choice.
pub(crate) const DEVICE_TYPE: &str = "A2UXDCSFQXA1M7";

/// Keychain service name for **our** refresh token.
///
/// Deliberately our own, and **never** the original's
/// `com.amazon.clouddrive.mac` / account `amzp-rt`: writing there would collide
/// with a real Amazon Photos install. Only [`KeyringTokenStore`] reads this.
pub(crate) const KEYRING_SERVICE: &str = "prime-photos";

/// Keychain account name for **our** refresh token (see [`KEYRING_SERVICE`]).
pub(crate) const KEYRING_ACCOUNT: &str = "refresh-token";

/// Seconds subtracted from `expires_in` so a token is refreshed slightly before
/// it actually expires (documented default, `AGENTS.md` 2026-10-15).
pub(crate) const ACCESS_TOKEN_EXPIRY_CUSHION_SECONDS: i64 = 60;

/// Fraction of the **remaining** lifetime after which the next refresh is due
/// (80 %, `AGENTS.md` 2026-10-15).
pub(crate) const ACCESS_TOKEN_REFRESH_FRACTION: f64 = 0.8;

/// Number of refresh attempts before the session is declared invalid. This is
/// the original's own count (observed in `AppSettings`).
pub(crate) const ACCESS_TOKEN_MAX_RETRIES: u32 = 5;

/// Seconds between two refresh attempts. This is the original's own spacing
/// (observed in `AppSettings`).
pub(crate) const ACCESS_TOKEN_RETRY_DELAY_SECONDS: u64 = 15;

/// Tick interval of the background refresh worker (user decision 2026-10-15):
/// the worker wakes up this often to check whether a refresh is due. Modeled
/// after `TickTockTimer` in the original.
pub(crate) const TICK_INTERVAL_SECONDS: u64 = 3;

/// Attempts of the **redeem** (`/auth/register`) step. This is the original's
/// own count (observed in `AppSettings`).
///
/// Used by the device-login background task in `login::poll_login`
/// (`src/login.rs`): the redeem is retried up to this many **offline** passes,
/// with [`TICK_INTERVAL_SECONDS`] between them. Only a network error is retried
/// (the request never reached the server, so the one-time authorization code is
/// still valid); a counted error stops the loop at once, because the server has
/// already consumed the code.
pub(crate) const REGISTER_MAX_ATTEMPTS: u32 = 3;

/// OS name reported as the device model. There is **no subprocess** and no
/// probe of the real machine: the original's exact value is unproven, so the
/// compile-time target OS is used as a stable, honest stand-in.
pub(crate) fn os_model() -> String {
    std::env::consts::OS.to_string()
}

/// OS version reported to the device flow. Like [`os_model`], the original's
/// value is unproven; the target architecture is the closest constant we have
/// without spawning a subprocess.
pub(crate) fn os_version() -> String {
    std::env::consts::ARCH.to_string()
}

// ---------------------------------------------------------------------------
// PKCE & device identity
// ---------------------------------------------------------------------------

/// A PKCE pair. `verifier` is the random secret kept for the token exchange,
/// `challenge` is its SHA-256, base64url-encoded, and goes into the portal URL.
pub(crate) struct Pkce {
    pub(crate) verifier: String,
    pub(crate) challenge: String,
}

impl Pkce {
    /// Generate a fresh pair: 32 random bytes as the verifier, the S256
    /// challenge derived from it.
    pub(crate) fn generate() -> Self {
        let verifier = random_urlsafe(32);
        let challenge = code_challenge(&verifier);
        Self {
            verifier,
            challenge,
        }
    }
}

/// S256 `code_challenge`: url-safe base64 (no padding) of `SHA-256(verifier)`.
pub(crate) fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A fresh device serial: 32 lowercase hex characters (16 random bytes). This
/// is the **one** identity value that is ours (user decision 2026-10-15); it is
/// generated once and stored in `config.toml`.
pub(crate) fn new_device_serial() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex(&bytes)
}

/// `client_id` of the device flow: `"device:"` + the **hex dump of the UTF-8
/// bytes** of `"<serial>#<device_type>"`, lowercased. This is *not* a hash — the
/// original concatenates and hex-encodes the string (recon 2026-10-15).
pub(crate) fn client_id(device_serial: &str, device_type: &str) -> String {
    let raw = format!("{device_serial}#{device_type}");
    format!("device:{}", hex(raw.as_bytes())).to_lowercase()
}

/// Random `len` bytes, url-safe base64 without `=` padding (the PKCE alphabet).
fn random_urlsafe(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Lowercase hex of a byte slice (`61` → `"61"`).
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Refresh schedule
// ---------------------------------------------------------------------------

/// Delay until the access token is refreshed: 80 % of the lifetime that is left
/// after the [`ACCESS_TOKEN_EXPIRY_CUSHION_SECONDS`] cushion. A non-positive
/// remainder yields a zero delay (refresh immediately).
pub(crate) fn next_refresh_delay(expires_in_seconds: i64) -> Duration {
    let remaining = (expires_in_seconds - ACCESS_TOKEN_EXPIRY_CUSHION_SECONDS).max(0);
    let seconds = (remaining as f64 * ACCESS_TOKEN_REFRESH_FRACTION) as u64;
    Duration::from_secs(seconds)
}

/// Whether an error counts as a refresh **attempt** (user decision 2026-10-15).
///
/// A connection error means "offline" — it must **not** consume one of the
/// [`ACCESS_TOKEN_MAX_RETRIES`] attempts; the schedule stays and the next tick
/// tries again. Offline never signs the app out. Only real server responses
/// (HTTP status, empty token, JSON error) count towards the retry budget.
pub(crate) fn counts_as_attempt(e: &AuthError) -> bool {
    !matches!(e, AuthError::Network(_))
}

// ---------------------------------------------------------------------------
// Request bodies (device flow)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct RegistrationData {
    app_name: String,
    app_version: String,
    device_type: String,
    device_serial: String,
    device_model: String,
    os_version: String,
    domain: String,
}

#[derive(Debug, Serialize)]
struct AuthData {
    use_global_authentication: bool,
    authorization_code: String,
    code_verifier: String,
    code_algorithm: String,
    client_id: String,
    client_domain: String,
}

/// Body of `POST <base>/auth/register` (snake_case, exactly the original's
/// shape). `auth_data.client_id` carries **no** `device:` prefix — the original
/// drops it here even though the portal URL sends it (recon 2026-10-15).
#[derive(Debug, Serialize)]
pub(crate) struct RegisterRequest {
    registration_data: RegistrationData,
    auth_data: AuthData,
    requested_extensions: Vec<String>,
    requested_token_type: Vec<String>,
    user_context_map: serde_json::Value,
}

impl RegisterRequest {
    /// Build the register body from the PKCE exchange inputs. `client_id` is the
    /// full `device:…` value from [`client_id`]; the prefix is stripped for the
    /// body.
    pub(crate) fn new(
        device_serial: &str,
        authorization_code: &str,
        code_verifier: &str,
        client_id: &str,
    ) -> Self {
        Self {
            registration_data: RegistrationData {
                app_name: APP_NAME.to_string(),
                app_version: APP_VERSION.to_string(),
                device_type: DEVICE_TYPE.to_string(),
                device_serial: device_serial.to_string(),
                device_model: os_model(),
                os_version: os_version(),
                domain: "Device".to_string(),
            },
            auth_data: AuthData {
                use_global_authentication: false,
                authorization_code: authorization_code.to_string(),
                code_verifier: code_verifier.to_string(),
                code_algorithm: "SHA-256".to_string(),
                client_id: client_id
                    .strip_prefix("device:")
                    .unwrap_or(client_id)
                    .to_string(),
                client_domain: "DeviceLegacy".to_string(),
            },
            requested_extensions: vec!["customer_info".to_string(), "device_info".to_string()],
            requested_token_type: vec!["bearer".to_string(), "mac_dms".to_string()],
            user_context_map: serde_json::json!({}),
        }
    }
}

/// Body of `POST <base>/auth/token` in the refresh-token mode.
#[derive(Debug, Serialize)]
pub(crate) struct RefreshRequest {
    source_token: String,
    source_token_type: String,
    requested_token_type: String,
    app_name: String,
    app_version: String,
}

impl RefreshRequest {
    /// Build the refresh body for the stored refresh token.
    pub(crate) fn new(source_token: &str) -> Self {
        Self {
            source_token: source_token.to_string(),
            source_token_type: "refresh_token".to_string(),
            requested_token_type: "access_token".to_string(),
            app_name: APP_NAME.to_string(),
            app_version: APP_VERSION.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

/// `expires_in` arrives as a JSON **string** in the register response and as a
/// number elsewhere; accept both.
fn de_expires_in<'de, D>(d: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(i64),
        Str(String),
    }
    match NumOrStr::deserialize(d)? {
        NumOrStr::Num(n) => Ok(n),
        NumOrStr::Str(s) => s.trim().parse::<i64>().map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Deserialize)]
struct RegisterResponseBody {
    response: RegisterResponse,
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    success: RegisterSuccess,
}

#[derive(Debug, Deserialize)]
struct RegisterSuccess {
    tokens: RegisterTokens,
}

#[derive(Debug, Deserialize)]
struct RegisterTokens {
    bearer: BearerTokens,
}

#[derive(Debug, Deserialize)]
struct BearerTokens {
    #[serde(default)]
    access_token: String,
    #[serde(default, deserialize_with = "de_expires_in")]
    expires_in: i64,
    #[serde(default)]
    refresh_token: String,
}

/// The bearer tokens extracted from a successful register response. The
/// **refresh token lives here**; the access token is memory-only.
pub(crate) struct RegisterOutcome {
    pub(crate) access_token: String,
    pub(crate) expires_in: i64,
    pub(crate) refresh_token: String,
}

/// Parse `response.success.tokens.bearer` of `/auth/register`. An HTTP success
/// with an **empty** access token is still a failure
/// ([`AuthError::EmptyAccessToken`]).
pub(crate) fn parse_register_response(body: &str) -> Result<RegisterOutcome, AuthError> {
    let parsed: RegisterResponseBody =
        serde_json::from_str(body).map_err(|e| AuthError::Json(e.to_string()))?;
    let bearer = parsed.response.success.tokens.bearer;
    if bearer.access_token.is_empty() {
        return Err(AuthError::EmptyAccessToken);
    }
    Ok(RegisterOutcome {
        access_token: bearer.access_token,
        expires_in: bearer.expires_in,
        refresh_token: bearer.refresh_token,
    })
}

#[derive(Debug, Deserialize)]
struct RefreshResponseBody {
    #[serde(default)]
    access_token: String,
    #[serde(default, deserialize_with = "de_expires_in")]
    expires_in: i64,
}

/// The renewed access token from `/auth/token`.
pub(crate) struct RefreshOutcome {
    pub(crate) access_token: String,
    pub(crate) expires_in: i64,
}

/// Parse the refresh response. An empty access token is a failure here too.
pub(crate) fn parse_refresh_response(body: &str) -> Result<RefreshOutcome, AuthError> {
    let parsed: RefreshResponseBody =
        serde_json::from_str(body).map_err(|e| AuthError::Json(e.to_string()))?;
    if parsed.access_token.is_empty() {
        return Err(AuthError::EmptyAccessToken);
    }
    Ok(RefreshOutcome {
        access_token: parsed.access_token,
        expires_in: parsed.expires_in,
    })
}

/// Parse the auth-cookies answer of `POST <base>/auth/token` (device login step
/// 6) into the same [`ParsedCookies`] the webview path produces.
///
/// The response is `response.tokens.cookies`: a map of **cookie domain** →
/// array of cookie objects. Field names are read **case-insensitively** (the
/// original does the same), only `Name`/`Value` are needed. The entries are
/// joined into a `name=value; …` string and handed to the **existing**
/// [`crate::countries::parse_cookie_string`] — the parsing is not duplicated.
pub(crate) fn parse_auth_cookies(body: &str) -> Result<ParsedCookies, AuthError> {
    let root: serde_json::Value =
        serde_json::from_str(body).map_err(|e| AuthError::Json(e.to_string()))?;
    let cookies = root
        .get("response")
        .and_then(|r| r.get("tokens"))
        .and_then(|t| t.get("cookies"))
        .and_then(|c| c.as_object())
        .ok_or_else(|| AuthError::Json("response.tokens.cookies missing".to_string()))?;

    let mut parts: Vec<String> = Vec::new();
    for list in cookies.values() {
        let Some(list) = list.as_array() else { continue };
        for cookie in list {
            let Some(obj) = cookie.as_object() else { continue };
            if let (Some(name), Some(value)) = (get_ci(obj, "Name"), get_ci(obj, "Value")) {
                parts.push(format!("{name}={value}"));
            }
        }
    }
    Ok(crate::countries::parse_cookie_string(&parts.join("; ")))
}

/// First value of `key` in `obj`, matched **case-insensitively** (the original
/// reads the cookie fields that way). Returns `None` unless it is a string.
fn get_ci<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    obj.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Refresh-token storage (OS keychain)
// ---------------------------------------------------------------------------

/// Where the refresh token is kept. Only the refresh token is persisted; the
/// access token stays in memory.
pub(crate) trait TokenStore {
    fn save(&self, refresh_token: &str) -> Result<(), String>;
    fn load(&self) -> Result<Option<String>, String>;
    fn clear(&self) -> Result<(), String>;
}

/// [`TokenStore`] backed by the OS keychain (macOS Keychain / Windows Credential
/// Manager / Linux Secret Service) under **our own** service/account
/// ([`KEYRING_SERVICE`]/[`KEYRING_ACCOUNT`]).
pub(crate) struct KeyringTokenStore;

impl TokenStore for KeyringTokenStore {
    fn save(&self, refresh_token: &str) -> Result<(), String> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| e.to_string())?;
        entry
            .set_password(refresh_token)
            .map_err(|e| e.to_string())
    }

    fn load(&self) -> Result<Option<String>, String> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            // No stored credential is not an error: the user is simply signed out.
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn clear(&self) -> Result<(), String> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Already gone — clearing is idempotent.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Technical failure of a device-flow step. Carries no user-facing wording —
/// the later wiring (S-B/S-C) turns these into translated messages.
#[derive(Debug)]
pub(crate) enum AuthError {
    /// The request could not be sent / the connection failed.
    Network(String),
    /// The server answered with a non-success status (and its body).
    HttpStatus { status: u16, body: String },
    /// The response parsed but carried no access token — treated as a failure.
    EmptyAccessToken,
    /// The response body was not the expected JSON.
    Json(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Network(msg) => write!(f, "network error: {msg}"),
            AuthError::HttpStatus { status, body } => write!(f, "HTTP {status}: {body}"),
            AuthError::EmptyAccessToken => write!(f, "empty access token"),
            AuthError::Json(msg) => write!(f, "JSON error: {msg}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<reqwest::Error> for AuthError {
    fn from(e: reqwest::Error) -> Self {
        AuthError::Network(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Device-flow HTTP calls
// ---------------------------------------------------------------------------

/// Common headers of the Panda (`api.amazon.<tld>`) calls, as the original
/// sends them: JSON accept, the language and a `CloudDriveMac/<version>` user
/// agent.
fn panda_headers(language: &str) -> reqwest::header::HeaderMap {
    use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, HeaderValue, USER_AGENT};
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&format!("CloudDriveMac/{APP_VERSION}"))
            .expect("static user agent is a valid header value"),
    );
    if let Ok(value) = HeaderValue::from_str(language) {
        headers.insert(ACCEPT_LANGUAGE, value);
    }
    headers
}

/// `POST /auth/register`: redeem the PKCE authorization code for bearer tokens.
pub(crate) async fn register_device(
    client: &reqwest::Client,
    country: &str,
    language: &str,
    device_serial: &str,
    authorization_code: &str,
    code_verifier: &str,
    client_id: &str,
) -> Result<RegisterOutcome, AuthError> {
    let request = RegisterRequest::new(device_serial, authorization_code, code_verifier, client_id);
    let response = client
        .post(urls::panda_register_url(country))
        .headers(panda_headers(language))
        .json(&request)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(AuthError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }
    parse_register_response(&body)
}

/// `POST /auth/token`: renew the access token from the refresh token.
pub(crate) async fn refresh_access_token(
    client: &reqwest::Client,
    country: &str,
    language: &str,
    refresh_token: &str,
) -> Result<RefreshOutcome, AuthError> {
    let request = RefreshRequest::new(refresh_token);
    let response = client
        .post(urls::panda_token_url(country))
        .headers(panda_headers(language))
        .json(&request)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(AuthError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }
    parse_refresh_response(&body)
}

/// `POST /auth/token` with the auth-cookies query: turn the refresh token into
/// the cookie set the (unchanged) cookie upload chain needs.
pub(crate) async fn fetch_auth_cookies(
    client: &reqwest::Client,
    country: &str,
    language: &str,
    refresh_token: &str,
) -> Result<ParsedCookies, AuthError> {
    let url = urls::panda_auth_cookies_url(country, refresh_token, APP_NAME, APP_VERSION);
    let response = client
        .post(url)
        .headers(panda_headers(language))
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(AuthError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }
    parse_auth_cookies(&body)
}

/// `POST /auth/deregister`: sign out on the server (the local token is cleared
/// separately).
pub(crate) async fn deregister(
    client: &reqwest::Client,
    country: &str,
    language: &str,
    access_token: &str,
) -> Result<(), AuthError> {
    use reqwest::header::{AUTHORIZATION, HeaderValue};
    let mut headers = panda_headers(language);
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("bearer {access_token}"))
            .map_err(|e| AuthError::Network(e.to_string()))?,
    );
    let body = serde_json::json!({
        "request_metadata": { "app_name": APP_NAME, "app_version": APP_VERSION }
    });
    let response = client
        .post(urls::panda_deregister_url(country))
        .headers(headers)
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(AuthError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_challenge_matches_the_rfc7636_vector() {
        // RFC 7636, Appendix B: verifier → S256 challenge.
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        // SHA-256("abc"), url-safe base64 without padding.
        assert_eq!(
            code_challenge("abc"),
            URL_SAFE_NO_PAD.encode(Sha256::digest(b"abc"))
        );
    }

    #[test]
    fn client_id_is_the_hex_dump_of_serial_hash_type() {
        // "abc#T1" → bytes 61 62 63 23 54 31 → "616263235431".
        assert_eq!(client_id("abc", "T1"), "device:616263235431");
        // The real device type yields a lowercase, `device:`-prefixed value.
        let id = client_id("0123456789abcdef0123456789abcdef", DEVICE_TYPE);
        assert!(id.starts_with("device:"));
        assert_eq!(id, id.to_lowercase());
        assert!(!id.contains('#'));
    }

    #[test]
    fn new_device_serial_is_32_lowercase_hex_chars() {
        let serial = new_device_serial();
        assert_eq!(serial.len(), 32);
        assert!(serial.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(serial, serial.to_lowercase());
    }

    #[test]
    fn pkce_generate_is_random_and_urlsafe() {
        let a = Pkce::generate();
        let b = Pkce::generate();
        assert_ne!(a.verifier, b.verifier);
        // 32 bytes → 43 url-safe base64 chars, no padding.
        assert_eq!(a.verifier.len(), 43);
        for pair in [&a, &b] {
            assert_eq!(pair.challenge, code_challenge(&pair.verifier));
            assert!(!pair.verifier.contains(&['=', '+', '/']));
            assert!(!pair.challenge.contains(&['=', '+', '/']));
        }
    }

    #[test]
    fn next_refresh_delay_uses_80_percent_after_the_cushion() {
        assert_eq!(next_refresh_delay(3600), Duration::from_secs(2832));
        // At or below the 60 s cushion there is nothing left: refresh now.
        assert_eq!(next_refresh_delay(60), Duration::from_secs(0));
        assert_eq!(next_refresh_delay(30), Duration::from_secs(0));
    }

    #[test]
    fn register_response_reads_bearer_tokens() {
        let body = r#"{
            "response": {
                "success": {
                    "tokens": {
                        "bearer": {
                            "access_token": "AT",
                            "expires_in": 3600,
                            "refresh_token": "RT"
                        }
                    }
                }
            }
        }"#;
        let outcome = parse_register_response(body).expect("valid register response");
        assert_eq!(outcome.access_token, "AT");
        assert_eq!(outcome.expires_in, 3600);
        assert_eq!(outcome.refresh_token, "RT");
    }

    #[test]
    fn register_response_accepts_expires_in_as_a_string() {
        // The live `/auth/register` answer sends `"expires_in": "3600"`.
        let body = r#"{"response":{"success":{"tokens":{"bearer":{"access_token":"AT","expires_in":"3600","refresh_token":"RT"}}}}}"#;
        let outcome = parse_register_response(body).expect("valid register response");
        assert_eq!(outcome.access_token, "AT");
        assert_eq!(outcome.expires_in, 3600);
        assert_eq!(outcome.refresh_token, "RT");
    }

    #[test]
    fn register_response_accepts_expires_in_as_a_number() {
        let body = r#"{"response":{"success":{"tokens":{"bearer":{"access_token":"AT","expires_in":3600,"refresh_token":"RT"}}}}}"#;
        let outcome = parse_register_response(body).expect("valid register response");
        assert_eq!(outcome.expires_in, 3600);
    }

    #[test]
    fn register_response_rejects_an_empty_access_token() {
        let body = r#"{"response":{"success":{"tokens":{"bearer":{"access_token":"","expires_in":3600,"refresh_token":"RT"}}}}}"#;
        assert!(matches!(
            parse_register_response(body),
            Err(AuthError::EmptyAccessToken)
        ));
    }

    #[test]
    fn register_response_reports_json_errors() {
        assert!(matches!(
            parse_register_response("not json"),
            Err(AuthError::Json(_))
        ));
        // A well-formed JSON without the bearer tokens is a JSON error too.
        assert!(matches!(
            parse_register_response(r#"{"response":{"success":{}}}"#),
            Err(AuthError::Json(_))
        ));
    }

    #[test]
    fn refresh_response_reads_the_access_token() {
        let outcome = parse_refresh_response(
            r#"{"access_token":"AT2","token_type":"bearer","expires_in":3600}"#,
        )
        .expect("valid refresh response");
        assert_eq!(outcome.access_token, "AT2");
        assert_eq!(outcome.expires_in, 3600);
        assert!(matches!(
            parse_refresh_response(r#"{"access_token":"","expires_in":1}"#),
            Err(AuthError::EmptyAccessToken)
        ));
    }

    #[test]
    fn refresh_response_accepts_expires_in_as_a_string() {
        let outcome = parse_refresh_response(r#"{"access_token":"AT2","expires_in":"3600"}"#)
            .expect("valid refresh response");
        assert_eq!(outcome.access_token, "AT2");
        assert_eq!(outcome.expires_in, 3600);
    }

    #[test]
    fn auth_cookies_are_read_case_insensitively_and_set_the_marketplace() {
        // PascalCase (the original's observed casing).
        let pascal = r#"{
            "response": {
                "tokens": {
                    "cookies": {
                        ".amazon.de": [
                            {"Name":"session-id","Value":"SID","Path":"/","HttpOnly":true,"Secure":true},
                            {"Name":"x-acbde","Value":"ACB"},
                            {"Name":"at-acbde","Value":"AT"},
                            {"Name":"ubid-acbde","Value":"UBID"},
                            {"Name":"sess-at-acbde","Value":"SESSAT"}
                        ]
                    }
                }
            }
        }"#;
        // camelCase, same content.
        let camel = r#"{
            "response": {
                "tokens": {
                    "cookies": {
                        ".amazon.de": [
                            {"name":"session-id","value":"SID","path":"/","httpOnly":true,"secure":true},
                            {"name":"x-acbde","value":"ACB"},
                            {"name":"at-acbde","value":"AT"},
                            {"name":"ubid-acbde","value":"UBID"},
                            {"name":"sess-at-acbde","value":"SESSAT"}
                        ]
                    }
                }
            }
        }"#;
        for body in [pascal, camel] {
            let parsed = parse_auth_cookies(body).expect("valid auth cookies");
            assert_eq!(parsed.session_id, "SID");
            // `x-acb<cc>` sets the marketplace code.
            assert_eq!(parsed.country, "de");
            assert_eq!(parsed.cookie_x_acb, "ACB");
            assert_eq!(parsed.cookie_at_acb, "AT");
            assert_eq!(parsed.cookie_ubid_acb, "UBID");
            assert_eq!(parsed.cookie_x_amz_access_token, "SESSAT");
        }
        // A body without the cookies map is a JSON error.
        assert!(matches!(
            parse_auth_cookies(r#"{"response":{}}"#),
            Err(AuthError::Json(_))
        ));
    }

    #[test]
    fn register_request_serialises_the_documented_body() {
        let request = RegisterRequest::new("SERIAL", "CODE", "VERIFIER", "device:abc123");
        let value = serde_json::to_value(&request).expect("serialises");

        assert_eq!(value["registration_data"]["app_name"], APP_NAME);
        assert_eq!(value["registration_data"]["app_version"], APP_VERSION);
        assert_eq!(value["registration_data"]["device_type"], DEVICE_TYPE);
        assert_eq!(value["registration_data"]["device_serial"], "SERIAL");
        assert_eq!(value["registration_data"]["domain"], "Device");
        assert_eq!(value["registration_data"]["device_model"], os_model());
        assert_eq!(value["registration_data"]["os_version"], os_version());

        assert_eq!(value["auth_data"]["use_global_authentication"], false);
        assert_eq!(value["auth_data"]["authorization_code"], "CODE");
        assert_eq!(value["auth_data"]["code_verifier"], "VERIFIER");
        assert_eq!(value["auth_data"]["code_algorithm"], "SHA-256");
        assert_eq!(value["auth_data"]["client_domain"], "DeviceLegacy");
        // The `device:` prefix is dropped in the body.
        assert_eq!(value["auth_data"]["client_id"], "abc123");

        assert_eq!(value["requested_extensions"][0], "customer_info");
        assert_eq!(value["requested_extensions"][1], "device_info");
        assert_eq!(value["requested_token_type"][0], "bearer");
        assert_eq!(value["requested_token_type"][1], "mac_dms");
        assert_eq!(value["user_context_map"], serde_json::json!({}));
    }

    #[test]
    fn refresh_request_serialises_the_documented_body() {
        let request = RefreshRequest::new("RT");
        let value = serde_json::to_value(&request).expect("serialises");
        assert_eq!(value["source_token"], "RT");
        assert_eq!(value["source_token_type"], "refresh_token");
        assert_eq!(value["requested_token_type"], "access_token");
        assert_eq!(value["app_name"], APP_NAME);
        assert_eq!(value["app_version"], APP_VERSION);
    }

    #[test]
    fn device_identity_constants_match_the_probed_values() {
        assert_eq!(APP_NAME, "Amazon Photos");
        assert_eq!(APP_VERSION, "11.3.0.1d7941b5");
        assert_eq!(DEVICE_TYPE, "A2UXDCSFQXA1M7");
    }

    #[test]
    fn access_token_constants_match_the_documented_defaults() {
        assert_eq!(ACCESS_TOKEN_EXPIRY_CUSHION_SECONDS, 60);
        assert_eq!(ACCESS_TOKEN_REFRESH_FRACTION, 0.8);
        assert_eq!(ACCESS_TOKEN_MAX_RETRIES, 5);
        assert_eq!(ACCESS_TOKEN_RETRY_DELAY_SECONDS, 15);
        assert_eq!(REGISTER_MAX_ATTEMPTS, 3);
        assert_eq!(TICK_INTERVAL_SECONDS, 3);
    }

    #[test]
    fn counts_as_attempt_treats_network_errors_as_offline() {
        // A connection error means "offline": it must not consume a retry.
        assert!(!counts_as_attempt(&AuthError::Network("down".to_string())));
        // Every real server response counts towards the retry budget.
        assert!(counts_as_attempt(&AuthError::HttpStatus {
            status: 403,
            body: "nope".to_string()
        }));
        assert!(counts_as_attempt(&AuthError::EmptyAccessToken));
        assert!(counts_as_attempt(&AuthError::Json("bad".to_string())));
    }

    #[test]
    fn keyring_identity_is_our_own_and_never_the_originals() {
        assert_eq!(KEYRING_SERVICE, "prime-photos");
        assert_eq!(KEYRING_ACCOUNT, "refresh-token");
        assert!(!KEYRING_SERVICE.is_empty());
        assert_ne!(KEYRING_SERVICE, "com.amazon.clouddrive.mac");
        assert_ne!(KEYRING_ACCOUNT, "amzp-rt");
    }

    #[test]
    fn keyring_store_implements_the_token_store_trait() {
        // Compile-time check only: the real keychain is never touched in tests.
        fn assert_token_store<T: TokenStore>() {}
        assert_token_store::<KeyringTokenStore>();
    }

    #[test]
    fn auth_error_display_is_technical_not_user_facing() {
        assert_eq!(
            AuthError::EmptyAccessToken.to_string(),
            "empty access token"
        );
        assert_eq!(
            AuthError::HttpStatus {
                status: 403,
                body: "nope".to_string()
            }
            .to_string(),
            "HTTP 403: nope"
        );
        assert_eq!(
            AuthError::Json("bad".to_string()).to_string(),
            "JSON error: bad"
        );
        assert_eq!(
            AuthError::Network("down".to_string()).to_string(),
            "network error: down"
        );
    }
}
