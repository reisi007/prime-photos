use super::{LivePage, LiveState};
use crate::auth::TokenStore;
use crate::config::Config;
use crate::folders::FolderStore;
use crate::login::{Session, WebviewLogin};
use crate::upload::UploadShared;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Default)]
struct NoopTokenStore;

impl TokenStore for NoopTokenStore {
    fn save(&self, _: &str) -> Result<(), String> {
        Ok(())
    }

    fn load(&self) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn clear(&self) -> Result<(), String> {
        Ok(())
    }
}

pub(super) fn signed_out_state() -> LiveState {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let (paused, _) = watch::channel(false);
    LiveState {
        config: Config::default(),
        page: LivePage::Backup,
        license_tab: crate::about::LicenseTab::Components,
        settings_tab: crate::rune_settings::SettingsTab::General,
        autostart: crate::platform::app_shared::Autostart::unavailable_for_test(),
        autostart_enabled: false,
        settings_limit_edit: rune::TextEdit::new("0"),
        settings_album_edit: rune::TextEdit::new(""),
        settings_slider_origin: 1,
        store: FolderStore::default(),
        config_error: None,
        store_error: None,
        persist_config: false,
        persist_store: false,
        scan_tx: None,
        upload: Arc::new(Mutex::new(UploadShared::default())),
        paused,
        runtime,
        login: WebviewLogin::new(),
        session: Arc::new(Mutex::new(Session::default())),
        token_store: Arc::new(NoopTokenStore),
        session_wakeup: None,
        account_identity: super::account_identity::AccountIdentity::default(),
        native_window: None,
        app_handle: None,
        lifecycle_route: super::lifecycle_route::LifecycleRoute::default(),
        tray_route: super::tray_route::TrayRoute::default(),
        pending_show_request: false,
        ignore_initial_login_activation: false,
        child_bounds: None,
        form: None,
        status: None,
        was_active: false,
        one_time: None,
        one_time_resume: false,
        drop_hovered: false,
        marketplaces_open: false,
    }
}
