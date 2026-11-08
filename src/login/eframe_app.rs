use crate::app::PrimePhotoApp;
use crate::auth::{self, TokenStore};
use crate::i18n::tr;

use super::{poll_login_flow, LoginPoll, SessionCookies};

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
        // Nothing to sign out of in the demo: the fixture session lives in
        // memory only, and no keychain/config write may happen.
        if self.is_demo() {
            self.demo_skip("Abmelden");
            return;
        }
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

    pub(crate) fn poll_login(&mut self) {
        // Defence in depth: no device login ever runs in the demo (no webview),
        // and a result must never install a real session or save the fixture
        // config over the developer's `config.toml`.
        if self.is_demo() {
            self.demo_skip("Anmelde-Status abfragen");
            return;
        }
        if matches!(
            poll_login_flow(
                &mut self.login,
                &mut self.config,
                &self.session,
                &self.rt,
                &mut self.config_error,
                true,
            ),
            LoginPoll::Succeeded
        ) {
            self.log(tr("Login erfolgreich – Refresh-Token gespeichert."));
        }
    }
}
