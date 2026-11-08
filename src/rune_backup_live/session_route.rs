use super::{LivePage, LiveState, TargetForm};
use crate::auth::TokenStore;
use crate::config::Config;
use crate::login::{self, Session};
use crate::upload::UploadShared;
use std::sync::{Arc, Mutex};

pub(super) fn poll(state: &mut LiveState) -> bool {
    let mut changed = false;
    if let Some(clear_result) = clear_if_invalidated(
        &mut state.config,
        &state.session,
        &state.upload,
        state.token_store.as_ref(),
    ) {
        if let Err(error) = clear_result {
            log::warn!("Rune-Sicherungsroute: Refresh-Token konnte nicht gelöscht werden: {error}");
        }
        state.login.cancel();
        crate::wry_host::close();
        reset_expired_route(
            &mut state.page,
            &mut state.form,
            &mut state.child_bounds,
            &mut state.status,
            crate::i18n::tr("Sitzung abgelaufen – bitte erneut anmelden.").to_owned(),
        );
        state.persist_config_if_allowed();
        changed = true;
    }

    let start_gen = if state.session_wakeup.is_some() {
        login::prepare_session_worker(&mut state.session.lock().unwrap())
    } else {
        None
    };
    if let (Some(my_gen), Some(wakeup)) = (start_gen, state.session_wakeup.clone()) {
        let country = state.config.country.clone();
        let language = crate::urls::portal_language(&country).to_string();
        let session = Arc::clone(&state.session);
        state.runtime.spawn(async move {
            login::session_worker(session, country, language, wakeup, my_gen).await;
        });
    }

    let cookies_applied = {
        let mut session = state.session.lock().unwrap();
        login::apply_pending_cookies(&mut state.config, &mut session)
    };
    if cookies_applied {
        state.ensure_background_sync();
        changed = true;
    }
    changed
}

fn clear_if_invalidated(
    config: &mut Config,
    session: &Arc<Mutex<Session>>,
    upload: &Arc<Mutex<UploadShared>>,
    token_store: &dyn TokenStore,
) -> Option<Result<(), String>> {
    if !session.lock().unwrap().invalid {
        return None;
    }
    upload.lock().unwrap().cancelled = true;
    let clear_result = token_store.clear();
    login::clear_invalidated_session(config, &mut session.lock().unwrap());
    Some(clear_result)
}

fn reset_expired_route(
    page: &mut LivePage,
    form: &mut Option<TargetForm>,
    child_bounds: &mut Option<[f32; 4]>,
    status: &mut Option<String>,
    expired_status: String,
) {
    *page = LivePage::Backup;
    *form = None;
    *child_bounds = None;
    *status = Some(expired_status);
}

fn sign_in_available(config: &Config, session: &Session) -> bool {
    let logged_in = !config.session_id.is_empty() && !config.cookie_x_acb.is_empty();
    !logged_in && session.refresh_token.is_none()
}

pub(super) fn background_sync_ready(config: &Config, session: &Session) -> bool {
    config.background_sync
        && !session.invalid
        && !config.session_id.is_empty()
        && !config.cookie_x_acb.is_empty()
}

impl LiveState {
    pub(super) fn has_pending_session(&self) -> bool {
        self.session.lock().unwrap().refresh_token.is_some()
    }

    pub(super) fn sign_in_available(&self) -> bool {
        sign_in_available(&self.config, &self.session.lock().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LivePage, TargetForm};
    use super::{
        background_sync_ready, clear_if_invalidated, reset_expired_route, sign_in_available,
    };
    use crate::auth::TokenStore;
    use crate::config::Config;
    use crate::countries::parse_cookie_string;
    use crate::folders::ConflictResolution;
    use crate::login::{self, Session, SessionCookies};
    use crate::upload::UploadShared;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    #[derive(Default)]
    struct MemoryTokenStore {
        token: Mutex<Option<String>>,
        clears: AtomicUsize,
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
            self.clears.fetch_add(1, Ordering::SeqCst);
            *self.token.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn rune_session_restores_refresh_token_from_injected_store() {
        let token_store = MemoryTokenStore::default();
        token_store.save("synthetic-refresh-token").unwrap();
        let mut session = Session::default();

        assert!(login::restore_refresh_token(&mut session, &token_store).unwrap());
        assert_eq!(
            session.refresh_token.as_deref(),
            Some("synthetic-refresh-token")
        );
        assert!(session.access_token.is_none());
        assert!(session.cookies.session_id.is_empty());
        assert_eq!(token_store.clears.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn rune_session_restore_plans_a_worker_with_immediate_access_refresh() {
        let token_store = MemoryTokenStore::default();
        token_store.save("synthetic-refresh-token").unwrap();
        let mut session = Session::default();
        login::restore_refresh_token(&mut session, &token_store).unwrap();

        let generation = login::prepare_session_worker(&mut session).unwrap();
        assert_eq!(generation, 1);
        assert!(session.started);
        assert!(
            session
                .next_refresh
                .is_some_and(|due| due <= Instant::now())
        );
        assert!(login::prepare_session_worker(&mut session).is_none());
    }

    #[test]
    fn rune_session_applies_synthetic_pending_cookies_to_config_and_session() {
        let parsed = parse_cookie_string(
            "session-id=session; x-acb-us=x-acb; at-acb-us=at-acb; ubid-acb-us=ubid; sess-at-acb=access-cookie",
        );
        let mut session = Session {
            pending_cookies: Some(parsed),
            ..Session::default()
        };
        let mut config = Config::default();

        assert!(login::apply_pending_cookies(&mut config, &mut session));
        assert_eq!(config.session_id, "session");
        assert_eq!(config.cookie_x_acb, "x-acb");
        assert_eq!(config.cookie_at_acb, "at-acb");
        assert_eq!(config.cookie_ubid_acb, "ubid");
        assert_eq!(config.cookie_x_amz_access_token, "access-cookie");
        assert_eq!(session.cookies.session_id, config.session_id);
        assert_eq!(session.cookies.cookie_x_acb, config.cookie_x_acb);
        assert_eq!(session.cookies.cookie_at_acb, config.cookie_at_acb);
        assert_eq!(session.cookies.cookie_ubid_acb, config.cookie_ubid_acb);
        assert_eq!(
            session.cookies.cookie_x_amz_access_token,
            config.cookie_x_amz_access_token
        );
        assert!(session.pending_cookies.is_none());
        assert!(!login::apply_pending_cookies(&mut config, &mut session));
    }

    #[test]
    fn rune_session_invalidation_clears_tokens_cookies_upload_and_store() {
        let token_store = MemoryTokenStore::default();
        token_store.save("synthetic-refresh-token").unwrap();
        let mut config = Config {
            session_id: "session".to_owned(),
            cookie_x_acb: "x-acb".to_owned(),
            cookie_at_acb: "at-acb".to_owned(),
            cookie_ubid_acb: "ubid".to_owned(),
            cookie_x_amz_access_token: "access-cookie".to_owned(),
            ..Config::default()
        };
        let session = Arc::new(Mutex::new(Session {
            refresh_token: Some("synthetic-refresh-token".to_owned()),
            access_token: Some("synthetic-access-token".to_owned()),
            next_refresh: Some(Instant::now()),
            cookie_retry_at: Some(Instant::now()),
            generation: 4,
            started: true,
            invalid: true,
            pending_cookies: Some(parse_cookie_string("session-id=stale")),
            cookies: SessionCookies {
                session_id: "session".to_owned(),
                cookie_x_acb: "x-acb".to_owned(),
                cookie_at_acb: "at-acb".to_owned(),
                cookie_ubid_acb: "ubid".to_owned(),
                cookie_x_amz_access_token: "access-cookie".to_owned(),
            },
        }));
        let mut upload_state = UploadShared::default();
        upload_state.active = true;
        upload_state.cancelled = false;
        let upload = Arc::new(Mutex::new(upload_state));

        assert!(
            clear_if_invalidated(&mut config, &session, &upload, &token_store)
                .unwrap()
                .is_ok()
        );
        let mut session = session.lock().unwrap();
        assert!(session.refresh_token.is_none());
        assert!(session.access_token.is_none());
        assert!(session.next_refresh.is_none());
        assert!(session.cookie_retry_at.is_none());
        assert!(!session.invalid);
        assert!(!session.started);
        assert_eq!(session.generation, 5);
        assert!(session.pending_cookies.is_none());
        assert_eq!(session.cookies.session_id, "");
        assert_eq!(session.cookies.cookie_x_acb, "");
        assert_eq!(session.cookies.cookie_at_acb, "");
        assert_eq!(session.cookies.cookie_ubid_acb, "");
        assert_eq!(session.cookies.cookie_x_amz_access_token, "");
        assert!(config.session_id.is_empty());
        assert!(config.cookie_x_acb.is_empty());
        assert!(config.cookie_at_acb.is_empty());
        assert!(config.cookie_ubid_acb.is_empty());
        assert!(config.cookie_x_amz_access_token.is_empty());
        assert!(upload.lock().unwrap().cancelled);
        assert_eq!(token_store.clears.load(Ordering::SeqCst), 1);
        assert!(token_store.load().unwrap().is_none());
        assert!(!login::apply_pending_cookies(&mut config, &mut session));
        assert!(sign_in_available(&config, &session));
    }

    #[test]
    fn rune_session_expiry_returns_from_target_form_to_backup_login() {
        let mut page = LivePage::Settings;
        let mut form = Some(TargetForm {
            path: PathBuf::from("synthetic-target"),
            dedup: true,
            conflict: ConflictResolution::Ask,
            edit_path: None,
            error: Some("synthetic form error".to_owned()),
        });
        let mut child_bounds = Some([1.0, 2.0, 300.0, 400.0]);
        let mut status = Some("stale status".to_owned());

        reset_expired_route(
            &mut page,
            &mut form,
            &mut child_bounds,
            &mut status,
            "Sitzung abgelaufen – bitte erneut anmelden.".to_owned(),
        );

        assert_eq!(page, LivePage::Backup);
        assert!(form.is_none());
        assert!(child_bounds.is_none());
        assert_eq!(
            status.as_deref(),
            Some("Sitzung abgelaufen – bitte erneut anmelden.")
        );
    }

    #[test]
    fn rune_session_pending_invalidation_blocks_worker_start_plan() {
        let mut session = Session {
            refresh_token: Some("synthetic-refresh-token".to_owned()),
            generation: 7,
            invalid: true,
            ..Session::default()
        };

        assert!(login::prepare_session_worker(&mut session).is_none());
        assert_eq!(session.generation, 7);
        assert!(!session.started);
        assert!(session.next_refresh.is_none());
    }

    #[test]
    fn rune_session_pending_invalidation_preserves_unapplied_cookies() {
        let mut session = Session {
            invalid: true,
            pending_cookies: Some(parse_cookie_string(
                "session-id=stale; x-acb-us=stale-cookie",
            )),
            ..Session::default()
        };
        let mut config = Config::default();

        assert!(!login::apply_pending_cookies(&mut config, &mut session));
        assert!(session.pending_cookies.is_some());
        assert!(session.cookies.session_id.is_empty());
        assert!(session.cookies.cookie_x_acb.is_empty());
        assert!(config.session_id.is_empty());
        assert!(config.cookie_x_acb.is_empty());
    }

    #[test]
    fn rune_session_pending_invalidation_blocks_background_sync_readiness() {
        let config = Config {
            background_sync: true,
            session_id: "synthetic-session".to_owned(),
            cookie_x_acb: "synthetic-cookie".to_owned(),
            ..Config::default()
        };
        let session = Session {
            invalid: true,
            ..Session::default()
        };

        assert!(!background_sync_ready(&config, &session));
    }
}
