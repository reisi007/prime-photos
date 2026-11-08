//! Explicit Rune production backup host plus its debug-only synthetic visual
//! smoke route. The existing headless fixture stays separate.

use crate::backup;
use crate::config::Config;
use crate::folders::{ConflictResolution, ContentFilter, FolderStore};
use crate::i18n::tr;
use crate::login::{self, LoginPoll, Session, WebviewLogin};
use crate::platform::app_shared::Autostart;
use crate::upload::UploadShared;
use lifecycle::deliver_pending_show_request;
use rune::{App, AppHandle, AppSpec, TextEdit, Tx, Waker, WindowHandle, WindowOptions};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

mod language_change;

const WINDOW_SIZE: (f32, f32) = (1180.0, 820.0);
const MIN_WINDOW_SIZE: (f32, f32) = (760.0, 560.0);
const HEADER_HEIGHT: f32 = 74.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LivePage {
    #[default]
    Backup,
    Settings,
    About,
    Licenses,
}

struct TargetForm {
    path: PathBuf,
    dedup: bool,
    conflict: ConflictResolution,
    content_filter: ContentFilter,
    edit_path: Option<PathBuf>,
    error: Option<String>,
}

struct LiveState {
    config: Config,
    page: LivePage,
    license_tab: crate::about::LicenseTab,
    settings_tab: crate::rune_settings::SettingsTab,
    autostart: Autostart,
    autostart_enabled: bool,
    settings_limit_edit: TextEdit,
    settings_album_edit: TextEdit,
    settings_slider_origin: usize,
    store: FolderStore,
    config_error: Option<String>,
    store_error: Option<String>,
    persist_config: bool,
    persist_store: bool,
    scan_tx: Option<tokio::sync::mpsc::UnboundedSender<PathBuf>>,
    upload: Arc<Mutex<UploadShared>>,
    paused: tokio::sync::watch::Sender<bool>,
    runtime: Arc<tokio::runtime::Runtime>,
    login: WebviewLogin,
    session: Arc<Mutex<Session>>,
    token_store: Arc<dyn crate::auth::TokenStore>,
    session_wakeup: Option<login::SessionWakeup>,
    account_identity: account_identity::AccountIdentity,
    native_window: Option<WindowHandle>,
    app_handle: Option<AppHandle<LiveState>>,
    lifecycle_route: lifecycle_route::LifecycleRoute,
    tray_route: tray_route::TrayRoute,
    ignore_initial_login_activation: bool,
    pending_show_request: bool,
    child_bounds: Option<[f32; 4]>,
    form: Option<TargetForm>,
    status: Option<String>,
    was_active: bool,
    one_time: Option<crate::folders::BackupFolder>,
    one_time_resume: bool,
    drop_hovered: bool,
    marketplaces_open: bool,
}

impl LiveState {
    fn logged_in(&self) -> bool {
        !self.config.session_id.is_empty() && !self.config.cookie_x_acb.is_empty()
    }

    fn persist_config_if_allowed(&mut self) {
        if !self.persist_config {
            return;
        }
        if let Err(error) = self.config.save() {
            log::warn!("Rune-Sicherungsroute: Einstellungen konnten nicht gespeichert werden: {error}");
            self.config_error = Some(tr("Die Einstellungen konnten nicht gespeichert werden.").to_owned());
        } else {
            self.config_error = None;
        }
    }

    fn persist_store_if_allowed(&mut self) {
        if !self.persist_store {
            return;
        }
        if let Err(error) = self.store.save() {
            log::warn!("Rune-Sicherungsroute: Sicherungsziele konnten nicht gespeichert werden: {error}");
            self.store_error = Some(tr("Sicherungsziele konnten nicht gespeichert werden.").to_owned());
        } else {
            self.store_error = None;
        }
    }

    fn start_upload(&mut self, indices: Vec<usize>) {
        if self.upload.lock().unwrap().active {
            self.status = Some(tr("Es läuft bereits ein Upload.").to_owned());
            return;
        }
        if !self.logged_in() {
            self.status = Some(tr("Bitte zuerst anmelden, bevor ein Upload gestartet wird.").to_owned());
            return;
        }

        let album_name = self.config.album_name.clone();
        let slow_mode = { self.upload.lock().unwrap().slow_mode };
        let Some(run) = backup::prepare_scheduled_upload_shared(
            &mut self.store,
            indices,
            &mut self.config,
            self.upload.clone(),
            self.paused.clone(),
            album_name,
            slow_mode,
            self.persist_store,
        ) else {
            return;
        };
        self.persist_config_if_allowed();
        self.was_active = true;
        self.status = Some(tr("Upload-Vorgang gestartet.").to_owned());
        run.spawn(&self.runtime);
    }

    fn ensure_background_sync(&mut self) {
        let session = self.session.lock().unwrap();
        if self.scan_tx.is_some()
            || !session_route::background_sync_ready(&self.config, &session)
        {
            return;
        }
        self.scan_tx = backup::spawn_background_sync(
            &self.runtime,
            self.upload.clone(),
            self.paused.clone(),
            self.session.clone(),
            self.config.clone(),
            self.config.album_name.clone(),
        );
    }

    fn reload_store_if_dirty(&mut self) -> bool {
        let should_reload = {
            let upload = self.upload.lock().unwrap();
            !upload.active && upload.store_dirty
        };
        if !should_reload {
            return false;
        }
        let Some(store) = backup::load_store_logged() else {
            return false;
        };
        self.store = store;
        self.upload.lock().unwrap().take_store_dirty();
        true
    }

    fn finish_upload_if_needed(&mut self) -> bool {
        let active = self.upload.lock().unwrap().active;
        if self.was_active && !active {
            self.was_active = false;
            let (paths, totals, done, failed, cancelled, run_done, run_failed, run_total) = {
                let shared = self.upload.lock().unwrap();
                (
                    shared.folder_paths.clone(),
                    shared.folder_total.clone(),
                    shared.folder_done.clone(),
                    shared.folder_failed.clone(),
                    shared.cancelled,
                    shared.done,
                    shared.failed,
                    shared.total,
                )
            };
            if one_time::finish_if_one_time(
                self,
                &totals,
                &done,
                &failed,
                run_done,
                run_failed,
                run_total,
                cancelled,
            ) {
                if !cancelled && self.config.notifications {
                    crate::notify::run_finished(run_done, run_failed, run_total);
                }
                return true;
            }
            if !cancelled {
                let now = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
                backup::apply_folder_progress(
                    &mut self.store,
                    &paths,
                    &totals,
                    &done,
                    &failed,
                    &now,
                );
                self.persist_store_if_allowed();
                if self.config.notifications {
                    crate::notify::run_finished(run_done, run_failed, run_total);
                }
            }
            return true;
        }
        false
    }
}

impl crate::rune_settings::SettingsHost for LiveState {
    fn config(&self) -> &Config {
        &self.config
    }

    fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    fn settings_tab(&self) -> crate::rune_settings::SettingsTab {
        self.settings_tab
    }

    fn set_settings_tab(&mut self, tab: crate::rune_settings::SettingsTab) {
        self.settings_tab = tab;
    }

    fn autostart_enabled(&self) -> bool {
        self.autostart_enabled
    }

    fn set_autostart_enabled(&mut self, enabled: bool) {
        // This host method is reached only by an explicit switch action.
        let result = if enabled {
            self.autostart.enable()
        } else {
            self.autostart.disable()
        };
        self.autostart_enabled = self.autostart.is_enabled().unwrap_or(false);
        match result {
            Ok(()) => {
                self.config.autostart_enabled = Some(enabled);
                self.persist_config_if_allowed();
            }
            Err(error) => {
                log::warn!("Autostart (Login-Item) konnte nicht geändert werden: {error}");
            }
        }
    }

    fn slow_mode(&self) -> bool {
        self.upload.lock().unwrap().slow_mode
    }

    fn set_slow_mode(&mut self, enabled: bool) {
        self.upload.lock().unwrap().slow_mode = enabled;
    }

    fn limit_edit(&self) -> &TextEdit {
        &self.settings_limit_edit
    }

    fn limit_edit_mut(&mut self) -> &mut TextEdit {
        &mut self.settings_limit_edit
    }

    fn album_edit(&self) -> &TextEdit {
        &self.settings_album_edit
    }

    fn album_edit_mut(&mut self) -> &mut TextEdit {
        &mut self.settings_album_edit
    }

    fn slider_origin(&self) -> usize {
        self.settings_slider_origin
    }

    fn set_slider_origin(&mut self, value: usize) {
        self.settings_slider_origin = value;
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    fn save_error(&self) -> Option<&str> {
        self.config_error.as_deref()
    }

    fn set_language(&mut self, language: Option<&'static str>) {
        language_change::set_config_language(&mut self.config, language);
        let lang = self
            .config
            .language
            .as_deref()
            .and_then(crate::i18n::Lang::from_code)
            .unwrap_or_else(crate::i18n::system_lang);
        crate::i18n::set_lang(lang);
        language_change::apply_effects(|effect| match effect {
            language_change::Effect::RefreshMenu => crate::app_menu::refresh_labels(),
            language_change::Effect::RefreshTray => self.tray_route.request_label_refresh(),
            language_change::Effect::Persist => self.persist_settings(),
        });
    }

    fn persist_settings(&mut self) {
        self.persist_config_if_allowed();
        self.ensure_background_sync();
    }

    fn album_name_changed(&mut self) {
        let album_name = self.settings_album_edit.text.clone();
        if self.config.album_name != album_name {
            self.config.album_name = album_name;
            self.persist_settings();
        }
    }
}

pub(crate) fn run_production(args: Vec<String>) -> Result<(), String> {
    if rune::tooling::Args::parse_from(args.clone()).headless() {
        return Err(
            "--rune-backup is an interactive route, not a headless fixture".to_owned(),
        );
    }
    if !crate::single_instance::acquire() {
        log::info!("Prime Photos läuft bereits — Rune-Fenster der laufenden Instanz angefordert");
        return Ok(());
    }
    // Cache the transient login-item Apple event before Rune starts AppKit. The
    // production host consumes the same classification as the legacy host.
    crate::app_activation::install_launch_diagnostic();
    crate::notify::init();
    let state = LiveState::production()?;
    let app = App::new(AppSpec {
        min_size: Some(MIN_WINDOW_SIZE),
        setup: Some(setup),
        poll: Some(poll),
        ..AppSpec::new("Prime Photos", WINDOW_SIZE, empty_live_state, view::live_view)
    })
    .args(args)
    .standard_menus()
    .keep_running();
    // AppKit reports Login Item classification only after Rune has selected
    // its launch activation policy. Start this production host as an
    // accessory unconditionally; setup promotes every non-silent start to
    // Regular after validating its final visibility policy.
    #[cfg(target_os = "macos")]
    let app = app.accessory();
    // Keep the first native window hidden until setup has received AppKit's
    // classification and, for a silent candidate, verified the tray.
    let app = app.window_options(WindowOptions {
        visible: false,
        ..WindowOptions::default()
    });
    let app = app
        .on_close(lifecycle_route::close_live)
        .on_reopen(lifecycle_route::reopen_live);
    let state_handle = app.handle();
    let app = app.init(move || {
        let mut state = state;
        state.app_handle = Some(state_handle);
        state
    });
    let handle = app.handle();
    app.on_window(move |window| {
        let native_window = window.clone();
        handle.update(move |state, _| {
            state.native_window = Some(native_window);
            deliver_pending_show_request(
                &mut state.pending_show_request,
                state.native_window.as_ref(),
                |native_window| native_window.open(),
            );
        });
    })
    .try_run()
}

fn empty_live_state() -> LiveState {
    unreachable!("Rune production backup route initializes after explicit selection")
}

fn setup(state: &mut LiveState, wakeup: Waker) {
    let activation_wakeup = wakeup.clone();
    crate::app_activation::install(Arc::new(move || activation_wakeup()));
    let about_wakeup = wakeup.clone();
    crate::app_menu::install_rune_about(Arc::new(move || about_wakeup()));
    let single_instance_wakeup = wakeup.clone();
    crate::single_instance::attach_wakeup(Arc::new(move || single_instance_wakeup()));
    state.tray_route.set_wakeup(wakeup.clone());
    state.login.set_wakeup(wakeup.clone());
    state.account_identity.set_wakeup(wakeup.clone());
    let session_wakeup = wakeup.clone();
    state.session_wakeup = Some(Arc::new(move || session_wakeup()));
    state.upload.lock().unwrap().wakeup = Some(Arc::new(move || wakeup()));
    startup::apply_window_policy(state);
}

fn poll(state: &mut LiveState, _: f64, tx: &mut Tx) -> bool {
    // Rune materializes the standard About item when its accessory app menu is
    // promoted; setup can run before that queued activation-policy change.
    crate::app_menu::ensure_rune_about_installed();
    let about_requested = crate::app_menu::take_about_request();
    let single_instance_show = crate::single_instance::take_show_request();
    let activation_show = lifecycle::activation_should_show(
        &mut state.ignore_initial_login_activation,
        crate::app_activation::take_become_active_request(),
    );
    if about_requested {
        state.page = LivePage::About;
    }
    let mut changed = if single_instance_show || activation_show || about_requested {
        state.show_window(tx)
    } else {
        false
    };
    changed |= about_requested;
    changed |= deliver_pending_show_request(
        &mut state.pending_show_request,
        state.native_window.as_ref(),
        |native_window| native_window.open(),
    );
    if state.upload.lock().unwrap().active {
        state.was_active = true;
    }
    changed |= state.finish_upload_if_needed();
    changed |= state.reload_store_if_dirty();
    match login::poll_login_flow(
        &mut state.login,
        &mut state.config,
        &state.session,
        &state.runtime,
        &mut state.config_error,
        state.persist_config,
    ) {
        LoginPoll::Idle => {}
        LoginPoll::Redeeming => {
            state.status = Some(tr("Anmeldecode empfangen; Anmeldung wird abgeschlossen.").to_owned());
            changed = true;
        }
        LoginPoll::Succeeded => {
            state.status = Some(tr("Anmeldung erfolgreich – Sitzung eingerichtet.").to_owned());
            tx.set_title("Prime Photos");
            state.ensure_background_sync();
            changed = true;
        }
        LoginPoll::Failed => {
            state.status = Some(tr("Anmeldung fehlgeschlagen. Bitte erneut versuchen.").to_owned());
            tx.set_title("Prime Photos");
            changed = true;
        }
    }
    changed |= session_route::poll(state);
    changed |= account_identity::poll(state);
    changed |= state.resume_one_time();
    changed |= tray_route::poll(state, tx);
    // Polling observes login, session, and upload state. Successful sign-in or
    // renewed cookies can start the configured background supervisor; its
    // initial pass scans enabled targets before the filesystem watcher takes over.
    state.upload.lock().unwrap().active || changed
}

mod conflict;
mod about;
mod account_identity;
mod login_panel;
mod lifecycle;
mod lifecycle_route;
mod logout;
mod session_route;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod smoke;
mod tray_route;
mod targets;
mod one_time;
mod startup;
mod view;
mod widgets;

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
pub(crate) fn run_smoke(args: Vec<String>) -> Result<(), String> {
    smoke::run_smoke(args)
}
