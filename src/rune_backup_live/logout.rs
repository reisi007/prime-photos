use super::{LivePage, LiveState};
use crate::auth::TokenStore;
use crate::config::Config;
use crate::i18n::tr;
use crate::login::{self, Session};
use crate::upload::UploadShared;
use rune::Tx;

#[derive(Debug, PartialEq, Eq)]
struct Deregistration {
    access_token: String,
    country: String,
    language: String,
}

/// Clear all local session state and retain only the values needed for the
/// best-effort server call. The token store is injected so this policy stays
/// deterministic in tests and never needs the user's keychain.
fn clear_local_session(
    config: &mut Config,
    session: &mut Session,
    upload: &mut UploadShared,
    token_store: &dyn TokenStore,
) -> (Option<Deregistration>, Result<(), String>) {
    upload.cancelled = true;
    let country = config.country.clone();
    let access_token = session
        .access_token
        .clone()
        .filter(|token| !token.trim().is_empty());
    login::clear_invalidated_session(config, session);
    let clear_result = token_store.clear();
    let deregistration = access_token.map(|access_token| Deregistration {
        language: crate::urls::portal_language(&country).to_owned(),
        country,
        access_token,
    });
    (deregistration, clear_result)
}

impl LiveState {
    pub(super) fn logout(&mut self, tx: &mut Tx) {
        let (deregistration, token_clear_result) = {
            let mut session = self.session.lock().unwrap();
            let mut upload = self.upload.lock().unwrap();
            clear_local_session(
                &mut self.config,
                &mut session,
                &mut upload,
                self.token_store.as_ref(),
            )
        };

        self.login.cancel();
        self.account_identity.reset_if_signed_out();
        self.page = LivePage::Backup;
        self.form = None;
        self.child_bounds = None;
        self.status = Some(tr("Abgemeldet.").to_owned());
        self.persist_config_if_allowed();
        tx.set_title("Prime Photos");

        if let Err(error) = token_clear_result {
            log::warn!("Rune-Sicherungsroute: Refresh-Token konnte nicht gelöscht werden: {error}");
            self.status = Some(
                tr("Der gespeicherte Anmeldeschlüssel konnte nicht gelöscht werden.").to_owned(),
            );
        }

        if let Some(deregistration) = deregistration {
            drop(self.runtime.spawn(async move {
                let client = reqwest::Client::new();
                if let Err(error) = crate::auth::deregister(
                    &client,
                    &deregistration.country,
                    &deregistration.language,
                    &deregistration.access_token,
                )
                .await
                {
                    log::warn!("Rune-Sicherungsroute: Abmelden am Server fehlgeschlagen: {error}");
                }
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Deregistration, clear_local_session};
    use crate::auth::TokenStore;
    use crate::config::Config;
    use crate::countries::parse_cookie_string;
    use crate::login::{Session, SessionCookies};
    use crate::upload::UploadShared;
    use std::sync::Mutex;
    use std::time::Instant;

    #[derive(Default)]
    struct MemoryTokenStore {
        token: Mutex<Option<String>>,
        clears: Mutex<usize>,
    }

    impl TokenStore for MemoryTokenStore {
        fn save(&self, refresh_token: &str) -> Result<(), String> {
            *self.token.lock().unwrap() = Some(refresh_token.to_owned());
            Ok(())
        }

        fn load(&self) -> Result<Option<String>, String> {
            Ok(self.token.lock().unwrap().clone())
        }

        fn clear(&self) -> Result<(), String> {
            *self.clears.lock().unwrap() += 1;
            *self.token.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn logout_clears_local_credentials_and_preserves_deregistration_context() {
        let token_store = MemoryTokenStore::default();
        token_store.save("synthetic-refresh-token").unwrap();
        let mut config = Config {
            country: "de".to_owned(),
            zone: "eu".to_owned(),
            session_id: "session".to_owned(),
            cookie_x_acb: "x-acb".to_owned(),
            cookie_at_acb: "at-acb".to_owned(),
            cookie_ubid_acb: "ubid".to_owned(),
            cookie_x_amz_access_token: "access-cookie".to_owned(),
            ..Config::default()
        };
        let mut session = Session {
            refresh_token: Some("synthetic-refresh-token".to_owned()),
            access_token: Some("synthetic-access-token".to_owned()),
            next_refresh: Some(Instant::now()),
            cookie_retry_at: Some(Instant::now()),
            generation: 8,
            started: true,
            pending_cookies: Some(parse_cookie_string("session-id=pending")),
            cookies: SessionCookies {
                session_id: "session".to_owned(),
                cookie_x_acb: "x-acb".to_owned(),
                cookie_at_acb: "at-acb".to_owned(),
                cookie_ubid_acb: "ubid".to_owned(),
                cookie_x_amz_access_token: "access-cookie".to_owned(),
            },
            ..Session::default()
        };
        let mut upload = UploadShared {
            active: true,
            ..UploadShared::default()
        };

        let (deregistration, clear_result) =
            clear_local_session(&mut config, &mut session, &mut upload, &token_store);

        assert_eq!(
            deregistration,
            Some(Deregistration {
                access_token: "synthetic-access-token".to_owned(),
                country: "de".to_owned(),
                language: "de_DE".to_owned(),
            })
        );
        assert!(clear_result.is_ok());
        assert_eq!(session.generation, 9);
        assert!(!session.started);
        assert!(!session.invalid);
        assert!(session.refresh_token.is_none());
        assert!(session.access_token.is_none());
        assert!(session.next_refresh.is_none());
        assert!(session.cookie_retry_at.is_none());
        assert!(session.pending_cookies.is_none());
        assert!(session.cookies.session_id.is_empty());
        assert!(session.cookies.cookie_x_acb.is_empty());
        assert!(session.cookies.cookie_at_acb.is_empty());
        assert!(session.cookies.cookie_ubid_acb.is_empty());
        assert!(session.cookies.cookie_x_amz_access_token.is_empty());
        assert!(config.session_id.is_empty());
        assert!(config.cookie_x_acb.is_empty());
        assert!(config.cookie_at_acb.is_empty());
        assert!(config.cookie_ubid_acb.is_empty());
        assert!(config.cookie_x_amz_access_token.is_empty());
        assert_eq!(config.country, "de");
        assert_eq!(config.zone, "eu");
        assert!(upload.cancelled);
        assert!(upload.active);
        assert!(token_store.load().unwrap().is_none());
        assert_eq!(*token_store.clears.lock().unwrap(), 1);
    }

    #[test]
    fn repeated_logout_without_an_access_token_is_safe_and_does_not_deregister() {
        let token_store = MemoryTokenStore::default();
        token_store.save("stale-refresh-token").unwrap();
        let mut config = Config {
            session_id: "stale-session".to_owned(),
            cookie_x_acb: "stale-cookie".to_owned(),
            ..Config::default()
        };
        let mut session = Session {
            refresh_token: Some("stale-refresh-token".to_owned()),
            access_token: None,
            generation: 2,
            ..Session::default()
        };
        let mut upload = UploadShared::default();

        for generation in [3, 4] {
            let (deregistration, clear_result) =
                clear_local_session(&mut config, &mut session, &mut upload, &token_store);
            assert!(deregistration.is_none());
            assert!(clear_result.is_ok());
            assert_eq!(session.generation, generation);
            assert!(session.refresh_token.is_none());
            assert!(session.access_token.is_none());
            assert!(config.session_id.is_empty());
            assert!(config.cookie_x_acb.is_empty());
            assert!(upload.cancelled);
        }

        assert!(token_store.load().unwrap().is_none());
        assert_eq!(*token_store.clears.lock().unwrap(), 2);
    }
}
