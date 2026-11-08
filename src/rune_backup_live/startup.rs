use super::{LivePage, LiveState};
use crate::config::Config;
use crate::folders::FolderStore;
use crate::i18n::{self, Lang};
use crate::login::{self, Session, WebviewLogin};
use crate::platform::app_shared::{Autostart, AutostartAction, autostart_action};
use crate::upload::UploadShared;
use rune::TextEdit;
use std::sync::{Arc, Mutex};

/// Whether a classified Login Item with a restored session needs a tray probe.
/// Tray construction needs Rune's live main-thread event loop, so this only
/// controls the probe; [`should_start_hidden`] makes the final decision after
/// setup confirms that the tray was actually created.
pub(super) fn should_attempt_silent_start(
    is_macos: bool,
    login_item: bool,
    restored_session: bool,
) -> bool {
    is_macos && login_item && restored_session
}

/// Silent startup is safe only when every way back exists: macOS Login Item,
/// a restored refresh-token session, and a successfully-created tray.
pub(super) fn should_start_hidden(
    is_macos: bool,
    login_item: bool,
    restored_session: bool,
    working_tray: bool,
) -> bool {
    should_attempt_silent_start(is_macos, login_item, restored_session) && working_tray
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowStartupPolicy {
    SilentAccessory,
    RegularVisible,
}

fn select_window_startup_policy(
    is_macos: bool,
    login_item: bool,
    restored_session: bool,
    working_tray: bool,
) -> WindowStartupPolicy {
    if should_start_hidden(is_macos, login_item, restored_session, working_tray) {
        WindowStartupPolicy::SilentAccessory
    } else {
        WindowStartupPolicy::RegularVisible
    }
}

/// Resolve startup visibility after Rune has entered its native event loop and
/// the tray builder can report a real result. The Rune production builder has
/// already selected accessory mode on macOS and kept its first window hidden.
pub(super) fn apply_window_policy(state: &mut LiveState) {
    let login_item = crate::app_activation::launched_as_log_in_item();
    let restored_session = state.has_pending_session();
    let probe_tray = should_attempt_silent_start(
        cfg!(target_os = "macos"),
        login_item,
        restored_session,
    );
    let tray_ready = if probe_tray {
        let paused = *state.paused.borrow();
        let busy = state.upload.lock().unwrap().active;
        state.tray_route.prepare(
            restored_session,
            paused,
            busy,
            i18n::lang(),
            state.runtime.as_ref(),
        )
    } else {
        false
    };
    let policy = select_window_startup_policy(
        cfg!(target_os = "macos"),
        login_item,
        restored_session,
        tray_ready,
    );
    let start_hidden = policy == WindowStartupPolicy::SilentAccessory;
    state.ignore_initial_login_activation = start_hidden;
    log::debug!(
        "Rune-Login-Item-Startentscheidung: login_item={}, restored_session={}, tray_ready={tray_ready}, hidden={start_hidden}",
        login_item,
        restored_session
    );

    // The window is invisible in its native creation attributes on every path.
    // Apply the final activation policy before any queued open: a validated
    // silent start stays accessory, while every other start becomes Regular
    // before its first visible app window is queued.
    if let Some(app) = &state.app_handle {
        app.set_dock_icon(!start_hidden);
        if !start_hidden {
            app.open();
        }
    }
}

/// Apply the shared startup policy for the explicit Rune production host.
/// Callbacks keep the OS operation and persistence boundary deterministic in
/// tests; fixtures and smoke routes do not call this helper.
fn reconcile_autostart_startup<E, S>(
    config: &mut Config,
    config_loaded_ok: bool,
    os_enabled: bool,
    mut enable: impl FnMut() -> Result<(), E>,
    mut save_config: impl FnMut(&Config) -> Result<(), S>,
    readback: impl FnOnce() -> bool,
) -> bool
where
    E: std::fmt::Display,
    S: std::fmt::Display,
{
    match autostart_action(config.autostart_enabled, os_enabled) {
        AutostartAction::EnableAndRecord => match enable() {
            Ok(()) => {
                config.autostart_enabled = Some(true);
                // Never replace a damaged config with the default fallback.
                if config_loaded_ok {
                    if let Err(error) = save_config(config) {
                        log::warn!("Rune-Sicherungsroute: Autostart-Absicht konnte nicht gespeichert werden: {error}");
                    }
                }
            }
            Err(error) => {
                log::warn!("Rune-Sicherungsroute: Autostart (Login-Item) konnte nicht aktiviert werden: {error}");
            }
        },
        AutostartAction::Enable => {
            if let Err(error) = enable() {
                log::warn!("Rune-Sicherungsroute: Autostart (Login-Item) konnte nicht aktiviert werden: {error}");
            }
        }
        AutostartAction::Nothing => {}
    }

    // The checkbox mirrors the OS state after any attempted reconciliation.
    readback()
}

impl LiveState {
    pub(super) fn production() -> Result<Self, String> {
        // These reads happen only after --rune-backup has selected this route.
        // Failed reads keep the fallback in memory and close the corresponding
        // persistence gate, so damaged files are never replaced by defaults.
        let (mut config, config_error, persist_config) = match Config::load() {
            Ok(config) => (config, None, true),
            Err(error) => (Config::default(), Some(error.to_string()), false),
        };
        let language = config
            .language
            .as_deref()
            .and_then(Lang::from_code)
            .unwrap_or_else(i18n::system_lang);
        i18n::set_lang(language);
        let settings_limit_edit = TextEdit::new(config.limit_kbps.to_string());
        let settings_album_edit = TextEdit::new(config.album_name.clone());
        // The explicit production route applies the same startup intent policy
        // as the normal eframe host, then seeds the switch from OS readback.
        let autostart = Autostart::new();
        let autostart_enabled = autostart.is_enabled().unwrap_or(false);
        let autostart_enabled = reconcile_autostart_startup(
            &mut config,
            persist_config,
            autostart_enabled,
            || autostart.enable(),
            |config| config.save(),
            || autostart.is_enabled().unwrap_or(false),
        );
        if let Some(error) = &config_error {
            log::warn!("Rune-Sicherungsroute: Einstellungen konnten nicht geladen werden: {error}");
        }

        let (store, store_error, persist_store) = match FolderStore::load() {
            Ok(store) => (store, None, true),
            Err(error) => (FolderStore::default(), Some(error.to_string()), false),
        };
        if let Some(error) = &store_error {
            log::error!("Rune-Sicherungsroute: Sicherungsziele konnten nicht geladen werden: {error}");
        }

        let runtime = Arc::new(
            tokio::runtime::Runtime::new()
                .map_err(|error| format!("Tokio runtime could not start: {error}"))?,
        );
        let session = Arc::new(Mutex::new(Session::default()));
        let token_store: Arc<dyn crate::auth::TokenStore> =
            Arc::new(crate::auth::KeyringTokenStore);
        if let Err(error) = login::restore_refresh_token(
            &mut session.lock().unwrap(),
            token_store.as_ref(),
        ) {
            log::warn!("Rune-Sicherungsroute: Refresh-Token konnte nicht geladen werden: {error}");
        }
        // Only the explicitly selected production host restores the resolved
        // ad-hoc queue. Fixtures and the visual smoke route never open SQLite.
        let (one_time, one_time_resume) = super::one_time::restore_persisted(&config);
        let (paused, _) = tokio::sync::watch::channel(config.paused);
        Ok(Self {
            config,
            page: LivePage::Backup,
            license_tab: crate::about::LicenseTab::Components,
            settings_tab: crate::rune_settings::SettingsTab::General,
            autostart,
            autostart_enabled,
            settings_limit_edit,
            settings_album_edit,
            settings_slider_origin: 1,
            store,
            config_error,
            store_error,
            persist_config,
            persist_store,
            scan_tx: None,
            upload: Arc::new(Mutex::new(UploadShared::default())),
            paused,
            runtime,
            login: WebviewLogin::new(),
            session,
            token_store,
            session_wakeup: None,
            account_identity: super::account_identity::AccountIdentity::default(),
            native_window: None,
            app_handle: None,
            lifecycle_route: super::lifecycle_route::LifecycleRoute::default(),
            tray_route: super::tray_route::TrayRoute::default(),
            ignore_initial_login_activation: false,
            pending_show_request: false,
            child_bounds: None,
            form: None,
            status: None,
            was_active: false,
            one_time,
            one_time_resume,
            drop_hovered: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct ResultState {
        checkbox: bool,
        intent: Option<bool>,
        enable_calls: usize,
        saves: usize,
        readbacks: usize,
    }

    fn reconcile_with_fake_os(
        intent: Option<bool>,
        initial_os_enabled: bool,
        config_loaded_ok: bool,
        enable_succeeds: bool,
        save_succeeds: bool,
    ) -> ResultState {
        let mut config = Config::default();
        config.autostart_enabled = intent;
        let os_enabled = Cell::new(initial_os_enabled);
        let enable_calls = Cell::new(0);
        let saves = Cell::new(0);
        let readbacks = Cell::new(0);

        let checkbox = reconcile_autostart_startup(
            &mut config,
            config_loaded_ok,
            initial_os_enabled,
            || {
                enable_calls.set(enable_calls.get() + 1);
                if enable_succeeds {
                    os_enabled.set(true);
                    Ok::<(), &'static str>(())
                } else {
                    Err("synthetic enable failure")
                }
            },
            |_| {
                saves.set(saves.get() + 1);
                if save_succeeds {
                    Ok::<(), &'static str>(())
                } else {
                    Err("synthetic save failure")
                }
            },
            || {
                readbacks.set(readbacks.get() + 1);
                os_enabled.get()
            },
        );

        ResultState {
            checkbox,
            intent: config.autostart_enabled,
            enable_calls: enable_calls.get(),
            saves: saves.get(),
            readbacks: readbacks.get(),
        }
    }

    #[test]
    fn startup_reconciliation_covers_every_intent_and_os_state() {
        let cases = [
            (None, false, 1, Some(true), 1),
            (None, true, 1, Some(true), 1),
            (Some(true), false, 1, Some(true), 0),
            (Some(true), true, 0, Some(true), 0),
            (Some(false), false, 0, Some(false), 0),
            (Some(false), true, 0, Some(false), 0),
        ];

        for (intent, os_enabled, expected_enable_calls, expected_intent, expected_saves) in cases {
            let result = reconcile_with_fake_os(intent, os_enabled, true, true, true);
            assert_eq!(result.enable_calls, expected_enable_calls);
            assert_eq!(result.intent, expected_intent);
            assert_eq!(result.saves, expected_saves);
            assert_eq!(result.checkbox, os_enabled || expected_enable_calls > 0);
            assert_eq!(result.readbacks, 1);
        }
    }

    #[test]
    fn failed_enable_does_not_record_or_persist_intent() {
        for (intent, os_enabled) in [(None, false), (None, true), (Some(true), false)] {
            let result = reconcile_with_fake_os(intent, os_enabled, true, false, true);
            assert_eq!(result.enable_calls, 1);
            assert_eq!(result.intent, intent);
            assert_eq!(result.saves, 0);
            assert_eq!(result.checkbox, os_enabled);
            assert_eq!(result.readbacks, 1);
        }
    }

    #[test]
    fn corrupt_config_fallback_is_never_saved_but_checkbox_uses_readback() {
        let result = reconcile_with_fake_os(None, false, false, true, true);
        assert_eq!(result.intent, Some(true));
        assert_eq!(result.saves, 0);
        assert!(result.checkbox);
        assert_eq!(result.readbacks, 1);
    }

    #[test]
    fn save_failure_keeps_successful_intent_and_os_readback() {
        let result = reconcile_with_fake_os(None, false, true, true, false);
        assert_eq!(result.intent, Some(true));
        assert_eq!(result.saves, 1);
        assert!(result.checkbox);
        assert_eq!(result.readbacks, 1);
    }

    #[test]
    fn startup_policy_is_silent_only_for_macos_login_item_session_and_working_tray() {
        assert_eq!(
            select_window_startup_policy(true, true, true, true),
            WindowStartupPolicy::SilentAccessory
        );
        // These all select Regular + visible at setup, including a manual
        // launch even when a session and tray happen to be available.
        for (is_macos, login_item, restored_session, working_tray) in [
            (false, true, true, true),
            (true, false, true, true),
            (true, true, false, true),
            (true, true, true, false),
        ] {
            assert_eq!(
                select_window_startup_policy(
                    is_macos,
                    login_item,
                    restored_session,
                    working_tray,
                ),
                WindowStartupPolicy::RegularVisible
            );
        }
    }

    #[test]
    fn startup_tray_probe_requires_macos_login_item_and_restored_session() {
        assert!(should_attempt_silent_start(true, true, true));
        assert!(!should_attempt_silent_start(false, true, true));
        assert!(!should_attempt_silent_start(true, false, true));
        assert!(!should_attempt_silent_start(true, true, false));
    }
}
