use crate::about;
use crate::app_activation;
use crate::app_menu;
use crate::auth::TokenStore;
use crate::backup_config::{BackupConfigForm, ConfigFormAction, ConfigFormMode};
use crate::config::Config;
use crate::countries::system_country;
use crate::dedup::DedupIndex;
use crate::folders::{BackupFolder, FolderStatus, FolderStore};
use crate::i18n::{self, tr, tr1, tr3};
use crate::icons::{app_logo, draw_flag_badge, nav_backup_icon, nav_settings_icon};
use crate::login;
use crate::notify;
use crate::single_instance;
use crate::theme;
use crate::tray;
use crate::upload::{eta_suffix, FileState, UploadShared};
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::watch;

/// Height of the slim branded header shown above the Amazon sign-in webview.
pub(crate) const HEADER_H: f32 = 60.0;

/// Synthetic folder index of the transient one-time (drag & drop) upload
/// target: it is never in `store.folders`, so no persisted index can collide
/// with it and every run reports its counters under this one key.
pub(crate) const ONE_TIME_IDX: usize = usize::MAX;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(PartialEq, Eq, Clone, Copy)]
pub(crate) enum Page {
    Backup,
    Settings,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub(crate) enum SettingsTab {
    General,
    Bandwidth,
}

// ---------------------------------------------------------------------------
// Autostart (login item)
// ---------------------------------------------------------------------------

/// App name used for the auto-launch (login item) registration.
const AUTOSTART_APP_NAME: &str = "Prime Photos";

/// Build the `auto-launch` registration for the current executable.
///
/// `None` means "autostart is unavailable here": the platform is not supported
/// or the executable path could not be determined. The app keeps running and
/// the toggle simply stays off.
///
/// The builder is used instead of the platform-specific `AutoLaunch::new`
/// constructors so this helper stays `#[cfg]`-free. `MacOSLaunchMode::SMAppService`
/// is spelled out (not the crate default `LaunchAgent`) because that is the mode
/// we want on macOS: `SMAppService.mainApp.register()` enters the app into the
/// **Anmeldeobjekte** (System Settings → General → Login Items) — a `LaunchAgent`
/// plist under `~/Library/LaunchAgents` never shows up there. Requires macOS 13+;
/// a freshly registered entry may first be `RequiresApproval` until the user
/// approves it in the login-items list.
fn build_autostart() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name(AUTOSTART_APP_NAME)
        .set_app_path(&exe.display().to_string())
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::SMAppService)
        .build()
        .ok()
}

/// Runtime-only autostart (login item) state.
///
/// `inner` is `None` when the platform is unsupported or the registration could
/// not be built. Every operation then answers "unsupported"/"disabled" rather
/// than panicking, so the settings toggle is safe on every platform.
pub(crate) struct Autostart {
    inner: Option<auto_launch::AutoLaunch>,
}

impl Autostart {
    pub(crate) fn new() -> Self {
        Self {
            inner: build_autostart(),
        }
    }

    pub(crate) fn enable(&self) -> auto_launch::Result<()> {
        match self.inner.as_ref() {
            Some(inner) => inner.enable(),
            None => Err(auto_launch::Error::UnsupportedOS),
        }
    }

    pub(crate) fn disable(&self) -> auto_launch::Result<()> {
        match self.inner.as_ref() {
            Some(inner) => inner.disable(),
            None => Err(auto_launch::Error::UnsupportedOS),
        }
    }

    /// Current OS state; `None` when unsupported or unreadable (treated as off).
    pub(crate) fn is_enabled(&self) -> Option<bool> {
        self.inner.as_ref().and_then(|inner| inner.is_enabled().ok())
    }
}

/// What the persisted autostart intent asks us to do at startup (PKG-3).
///
/// Pure decision, no OS access: derived from the `Config` intent and the
/// current OS state so it is cheap to unit-test.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum AutostartAction {
    /// Never decided before (`None`): turn the login item on and record
    /// `Some(true)`. The enable is a no-op when the OS state is already on.
    EnableAndRecord,
    /// Intent says "on" but the OS state is off: repair the login item
    /// (plist deleted, app moved).
    Enable,
    /// Nothing to do (already on, or the user vetoed with `Some(false)`).
    Nothing,
}

/// Derive the startup action from the persisted intent and the OS state.
///
/// `None` (never decided) ⇒ enable and record; `Some(true)` + OS off ⇒ repair;
/// `Some(false)` always ⇒ nothing (never fight a deliberate "off" — a login
/// item the user created by hand is not touched).
fn autostart_action(intent: Option<bool>, os_enabled: bool) -> AutostartAction {
    match intent {
        None => AutostartAction::EnableAndRecord,
        Some(true) if !os_enabled => AutostartAction::Enable,
        _ => AutostartAction::Nothing,
    }
}

/// What to persist after a successful enable/disable: the user's **intent**,
/// never the OS state that was read back (PKG-3 veto trap, user decision
/// 2026-10-17).
///
/// `is_smappservice_enabled()` is `status() == Enabled`, but a freshly
/// registered `SMAppService` entry is first `RequiresApproval` — so the
/// read-back would be `false` right after a successful `enable()`. Persisting
/// that read-back would silently turn a deliberate "on" into the `Some(false)`
/// veto, after which [`autostart_action`] returns `Nothing` forever and the app
/// never registers anything again. Pure and OS-free so it stays unit-testable.
fn autostart_intent_to_record(enabled: bool) -> Option<bool> {
    Some(enabled)
}

/// Whether the app should start **without a window** — silent, tray only.
///
/// `true` only for a login-item (autostart) start **with** a tray. A login-item
/// start without a tray must not go silent: the app would then run with no
/// window *and* no menu-bar icon, i.e. unreachable. Every other combination
/// starts visible, so a manual start (Finder/Dock/Spotlight/`open`/shell) always
/// shows the window. Pure and OS-free, so the start decision is unit-testable.
fn should_start_hidden(launched_as_log_in_item: bool, has_tray: bool) -> bool {
    launched_as_log_in_item && has_tray
}

/// Payload of the background account fetch: the display name plus, when it
/// could be obtained, the avatar JPEG bytes.
pub(crate) struct AvatarResult {
    pub(crate) name: String,
    pub(crate) jpeg: Option<Vec<u8>>,
}

/// Shared slot between the background avatar fetch and the UI thread. The UI
/// takes the value (leaving `None`) once per published intermediate/final
/// result, so a cached image shows immediately and a refreshed one replaces it
/// on a later frame.
#[derive(Default)]
pub(crate) struct AvatarFetch {
    pub(crate) result: Option<AvatarResult>,
    /// Set by the background fetch when an attempt ended **without** a usable
    /// result (neither name nor image). Consumed by `poll_avatar`; together with
    /// the attempt counter it drives the single allowed retry, see
    /// [`should_fetch_avatar`].
    pub(crate) failed: bool,
}

/// Which consumer the one in-flight asynchronous folder picker is answering for
/// (Welle 5.1). The slot carries the target so a late answer cannot land in the
/// wrong field: the Create form and the open "Datensicherungseinstellungen"
/// window both want a folder, but a different one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderPickerTarget {
    /// The "Datensicherung hinzufügen …" button: opens the Create form with the
    /// picked folder.
    NewBackup,
    /// The "Ändern …" button of the open "Datensicherungseinstellungen" window:
    /// replaces the form's source folder.
    ConfigForm,
}

/// Shared slot between the background folder picker and the UI thread.
///
/// `open` is the re-entry guard (one native dialog at a time) and doubles as the
/// answer's target; `result` is taken once by `poll_folder_picker`. A cancelled
/// dialog clears `open` without leaving a `result`.
#[derive(Default)]
pub(crate) struct FolderPicker {
    open: Option<FolderPickerTarget>,
    result: Option<(FolderPickerTarget, PathBuf)>,
}

/// Whether another account fetch (name + avatar) may be started. The fetch gets
/// **exactly one** retry after a failed attempt — at most 2 attempts in total:
///
/// * no attempt yet ⇒ start;
/// * one attempt done and it **failed** ⇒ one retry;
/// * one attempt done and it succeeded (name **or** image arrived) ⇒ done;
/// * two attempts done ⇒ give up until the next app start.
///
/// Pure, so the retry policy is unit-testable without any network.
pub(crate) fn should_fetch_avatar(attempts: u8, last_failed: bool) -> bool {
    match attempts {
        0 => true,
        1 => last_failed,
        _ => false,
    }
}

pub(crate) struct PrimePhotoApp {
    pub(crate) rt: Arc<Runtime>,
    pub(crate) config: Config,
    pub(crate) config_error: Option<String>,
    /// A damaged `folders.toml` was set aside at start (user decision
    /// 2026-10-16). While set, the app behaves like a logout: it shows the
    /// login screen and the message in `config_error`. Recoverable — a finished
    /// or cancelled login drops it again (see `update`). The keychain token is
    /// **not** touched, so the state is never destructive.
    pub(crate) folders_error: bool,
    pub(crate) login: login::WebviewLogin,
    /// In-memory session shared with the background refresh worker (S-C): the
    /// refresh token (mirrored from the OS keychain), the access token and the
    /// freshly minted cookies. Nothing here is persisted except the refresh
    /// token, which lives in the keychain.
    pub(crate) session: Arc<Mutex<login::Session>>,
    /// Set by the UI when the user asks to sign in; handled in `update` because
    /// creating the webview needs the frame (window handle).
    pub(crate) pending_login: bool,
    /// Last window title we pushed, so we only update it on change.
    pub(crate) window_title: Option<String>,
    /// The window is resized to the screen once, on the first frame.
    pub(crate) sized: bool,
    pub(crate) page: Page,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) store: FolderStore,
    pub(crate) upload: Arc<Mutex<UploadShared>>,
    pub(crate) paused: watch::Sender<bool>,
    pub(crate) slow_mode: bool,
    pub(crate) album_name: String,
    pub(crate) was_active: bool,
    /// Handed to the detached supervisor: a backup folder the user added while a
    /// run was busy. `None` until `ensure_background_sync` spawned the supervisor
    /// (and `None` forever when background sync is off).
    pub(crate) scan_tx: Option<tokio::sync::mpsc::UnboundedSender<PathBuf>>,
    /// Open "Datensicherungseinstellungen" window (Create or Edit), if any.
    pub(crate) config_form: Option<BackupConfigForm>,
    /// Open "Über Prime Photos" window (Welle 4b).
    pub(crate) about_open: bool,
    /// Open third-party license window (raised from the About window).
    pub(crate) license_open: bool,
    /// Selected tab of the license window.
    pub(crate) license_tab: about::LicenseTab,
    /// Tray / menu-bar icon (PORT-3); `None` if it could not be created.
    pub(crate) tray: Option<tray::Tray>,
    /// Set when the user really wants to quit (tray "Beenden" / ⌘Q), so the
    /// close-to-tray intercept below lets the close through.
    pub(crate) quitting: bool,
    /// The main window is hidden on purpose and has not been shown again since:
    /// set by close-to-tray (`ViewportCommand::Visible(false)`) and by a silent
    /// login-item start (the window starts hidden). egui offers no way to
    /// *query* window visibility, so this mirror is what lets the tray-drop path
    /// know whether it has to show the window again (otherwise the app would
    /// keep running with no window and no tray). The macOS Dock icon follows
    /// this flag: hidden ⇒ `Accessory`, shown ⇒ `Regular` (see
    /// `src/app_activation.rs`).
    pub(crate) window_hidden: bool,
    /// The first AppKit activation can be part of a silent Login-Item launch,
    /// not a request from the user to reopen the window. Consume that one
    /// notification without showing the window; later activations still do.
    pub(crate) ignore_initial_login_activation: bool,
    /// The last `tray::build` attempt failed (the OS has no tray backend —
    /// logged as an error in `src/tray.rs`). Blocks the per-frame reconcile
    /// from retrying the build until the next sign-in; see the reconcile in
    /// [`Self::update`].
    pub(crate) tray_build_failed: bool,
    /// Last pause state pushed to the tray label, to update it only on change.
    pub(crate) tray_paused: bool,
    /// Last tray glyph state (plus the OS theme it was drawn for), so the icon
    /// is only re-set on an actual change — see [`Self::update_tray_icon`].
    pub(crate) tray_icon_state: Option<(tray::TrayState, Option<egui::Theme>)>,
    /// Whether the last finished upload run had failures (drives the "error"
    /// glyph until the next run).
    pub(crate) tray_last_run_failed: bool,
    /// When the last run finished (drives the short-lived "done" glyph).
    pub(crate) tray_done_at: Option<Instant>,
    /// Auto-launch (login item) registration; `inner` is `None` when the
    /// platform does not support it.
    pub(crate) autostart: Autostart,
    /// UI state of the autostart checkbox, kept in sync with the OS state.
    /// The **intent** behind it is persisted in `Config.autostart_enabled`
    /// (see `set_autostart` / `autostart_action`).
    pub(crate) autostart_enabled: bool,
    /// A file drag is currently hovering over the window (drop overlay on,
    /// backup-page interactions off).
    pub(crate) dropping_files: bool,
    /// Cached grouped queue snapshot for the Backup page: `(revision, groups)`.
    /// Rebuilt only when `UploadShared::rev` changed — the queued file names
    /// otherwise get re-cloned on every frame for no visible change.
    pub(crate) queue_cache: (u64, Vec<(String, Vec<(String, FileState)>)>),
    /// The `revision` the cache was built from (`u64::MAX` = never built).
    pub(crate) queue_cache_rev: u64,
    /// Transient one-time (drag & drop) upload target: the row is never saved
    /// into `folders.toml`, lives only for the length of the run and then keeps
    /// its settled counters so the Backup page can show the finished row. Its
    /// **resolved file queue** is persisted separately (SQLite, see
    /// `DedupIndex::set_one_time_queue`) and restored here at start via
    /// `one_time_queue()`. `None` until the first drop (or when none restored).
    pub(crate) one_time: Option<BackupFolder>,
    /// A one-time queue was restored from the SQLite at start (user decision
    /// 2026-10-14) and its run has not been started yet. `resume_one_time`
    /// clears it as soon as it starts the run, so it is one-shot per app start.
    pub(crate) one_time_resume: bool,
    /// Account fetch (name + avatar) attempts started this session. Capped at
    /// two by [`should_fetch_avatar`]: one initial try plus at most one retry
    /// after a failure.
    pub(crate) avatar_attempts: u8,
    /// Set when the last account fetch attempt failed (no name and no image), so
    /// the single retry in [`should_fetch_avatar`] is allowed.
    pub(crate) avatar_failed: bool,
    /// Real display name from the account preferences, once fetched.
    pub(crate) avatar_name: Option<String>,
    /// Decoded avatar picture, once fetched; until then the sidebar keeps the
    /// drawn marketplace badge as the placeholder.
    pub(crate) avatar_texture: Option<egui::TextureHandle>,
    /// Shared slot written by the background account fetch, polled in `update`.
    pub(crate) avatar_fetch: Arc<Mutex<AvatarFetch>>,
    /// Shared slot written by the background folder picker, polled in `update`
    /// (Welle 5.1: the native dialog runs off the UI thread).
    pub(crate) folder_picker: Arc<Mutex<FolderPicker>>,
}

impl PrimePhotoApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        // A native main menu is required for ⌘V/⌘C/⌘X to work in the webview.
        // The context is handed in so the menu's About item can wake the UI.
        app_menu::install(&cc.egui_ctx);
        // App activation (relaunch / switch-back) and the single-instance
        // handshake both only flag + wake the UI, so they get the context here
        // (see `src/app_activation.rs`, `src/single_instance.rs`).
        app_activation::install(&cc.egui_ctx);
        single_instance::attach_ctx(&cc.egui_ctx);
        let rt = Arc::new(Runtime::new().expect("tokio runtime"));
        // `config_loaded_ok` is derived directly from the `Config::load()`
        // result: a broken/unreadable file yields `Config::default()` (with
        // empty cookies) in memory, and that fallback must never be written
        // over the still-present, hand-fixable `config.toml` (PKG-3 guard).
        let (mut config, load_error, config_loaded_ok) = match Config::load() {
            Ok(c) => (c, None, true),
            Err(e) => (Config::default(), Some(e.to_string()), false),
        };
        // Set the UI language before anything renders (or builds the tray).
        // `None` means "automatic": follow the system locale.
        let lang = config
            .language
            .as_deref()
            .and_then(i18n::Lang::from_code)
            .unwrap_or_else(i18n::system_lang);
        i18n::set_lang(lang);
        // Build the load failure in the UI language (the language is only known
        // now) and log the technical detail — it never reaches the UI (user
        // decision 2026-10-15: no technical messages for the end user).
        if let Some(e) = &load_error {
            log::warn!("Einstellungen konnten nicht geladen werden: {e}");
        }
        let mut config_error = load_error
            .map(|_| tr("Die Einstellungen konnten nicht geladen werden.").to_string());
        // Backup-target store (user decision 2026-10-16): a damaged `folders.toml`
        // is set aside by `FolderStore::load` and the app is treated like a
        // logout — the login screen is shown and a plain-language message is
        // surfaced (the technical detail only goes to the log). The keychain
        // token is left untouched; a finished or cancelled login drops the flag.
        let (store, folders_error) = match FolderStore::load() {
            Ok(store) => (store, false),
            Err(e) => {
                log::warn!("Sicherungsziele konnten nicht geladen werden: {e}");
                if config_error.is_none() {
                    config_error = Some(folders_load_message());
                }
                (FolderStore::default(), true)
            }
        };
        // On first run, preselect the marketplace from the system language.
        let first_run = Config::get_config_path()
            .map(|p| !p.exists())
            .unwrap_or(true);
        if first_run {
            if let Some(c) = system_country() {
                config.country = c;
            }
        }
        // Device identity (S-B): the serial is generated **once** and stored. It
        // is only persisted when the on-disk config loaded successfully — the
        // same PKG-3 guard as the autostart intent below: a broken but
        // hand-fixable `config.toml` must never be clobbered by the default
        // fallback. `begin_login` carries the same check as a safety net.
        if config.device_serial.is_empty() {
            config.device_serial = crate::auth::new_device_serial();
            if config_loaded_ok {
                if let Err(e) = config.save() {
                    log::warn!("Geräte-Serial konnte nicht gespeichert werden: {e}");
                }
            } else {
                log::warn!(
                    "Geräte-Serial konnte nicht gespeichert werden: config.toml wurde nicht geladen"
                );
            }
        }
        let autostart = Autostart::new();
        // Seed the checkbox from the OS state (is the login item already
        // registered?); `false` when unsupported or the state is unreadable.
        let os_enabled = autostart.is_enabled().unwrap_or(false);
        // Apply the persisted autostart intent (PKG-3, user decision
        // 2026-10-14: the login item defaults to **on** — without it no folder
        // watcher runs after a restart/logout). On an unsupported platform or a
        // failing OS call nothing is persisted; the `log` facade reports it and
        // the app keeps running.
        match autostart_action(config.autostart_enabled, os_enabled) {
            AutostartAction::EnableAndRecord => match autostart.enable() {
                Ok(()) => {
                    config.autostart_enabled = Some(true);
                    // Only persist the intent when the on-disk config was
                    // loaded successfully. Otherwise `config` is the default
                    // fallback (empty cookies); saving it would clobber a
                    // broken-but-hand-fixable `config.toml` and force a
                    // re-login. A failed load keeps `config_error` set and
                    // leaves the file untouched.
                    if config_loaded_ok {
                        if let Err(e) = config.save() {
                            log::warn!(
                                "Autostart (Login-Item) Absicht konnte nicht gespeichert werden: {e}"
                            );
                        }
                    }
                }
                Err(e) => log::warn!("Autostart (Login-Item) konnte nicht aktiviert werden: {e}"),
            },
            AutostartAction::Enable => {
                if let Err(e) = autostart.enable() {
                    log::warn!("Autostart (Login-Item) konnte nicht aktiviert werden: {e}");
                }
            }
            AutostartAction::Nothing => {}
        }
        // The checkbox mirrors reality: read the OS state back after the action.
        let autostart_enabled = autostart.is_enabled().unwrap_or(false);
        // The persisted pause state survives a restart (user decision
        // 2026-10-14): seed the sender from the config we just loaded instead
        // of always starting unpaused.
        let paused = watch::Sender::new(config.paused);
        // Restore a persisted one-time (drag & drop) queue (user decision
        // 2026-10-14): the "Einmalige Sicherung" row is visible right away with
        // `total_files` = the stored file count, status "läuft" and 0 done.
        // Already-uploaded files are skipped by the MD5 index anyway, and
        // `resume_one_time` starts the run once the usual guards allow it.
        // App defaults, exactly like a fresh drop (`begin_one_time_run`): a
        // restart-restored queue uses the same dedup/conflict settings the
        // user configured, not the enum default — otherwise a fresh drop and
        // a restored one could diverge (e.g. Replace vs. SaveAsNew).
        let (one_dedup, one_conflict) = crate::folders::default_options(&config);
        let (one_time, one_time_resume) = match DedupIndex::open().one_time_queue() {
            Some((display_path, files)) if !files.is_empty() => (
                Some(BackupFolder {
                    path: display_path,
                    name: "Einmalige Sicherung".to_string(),
                    enabled: true,
                    dedup: one_dedup,
                    conflict_resolution: one_conflict,
                    content_filter: crate::folders::ContentFilter::Photos,
                    total_files: files.len(),
                    uploaded_files: 0,
                    last_updated: String::new(),
                    // `Idle` is the only non-active, non-terminal variant here:
                    // at this point no run is live yet, so the row must not
                    // claim to be uploading. The effect is the muted look
                    // (`MUTED` instead of the active `TEAL`) — it asserts no
                    // activity, not a different label (`Idle` renders the same
                    // "{}% hochgeladen" text as `Uploading`). `resume_one_time`
                    // flips it as soon as the run starts.
                    status: FolderStatus::Idle,
                }),
                true,
            ),
            _ => (None, false),
        };
        let mut app = Self {
            rt,
            config,
            config_error,
            login: login::WebviewLogin::new(),
            session: Arc::new(Mutex::new(login::Session::default())),
            pending_login: false,
            window_title: None,
            sized: false,
            page: Page::Backup,
            settings_tab: SettingsTab::General,
            store,
            folders_error,
            upload: Arc::new(Mutex::new(UploadShared::default())),
            paused,
            slow_mode: false,
            album_name: String::new(),
            was_active: false,
            scan_tx: None,
            config_form: None,
            about_open: false,
            license_open: false,
            license_tab: about::LicenseTab::Components,
            // Built lazily: the tray exists exactly while a refresh token does
            // (reconciled every frame in `update`) — no icon before sign-in.
            tray: None,
            quitting: false,
            window_hidden: false,
            ignore_initial_login_activation: false,
            tray_build_failed: false,
            tray_paused: false,
            tray_icon_state: None,
            tray_last_run_failed: false,
            tray_done_at: None,
            autostart,
            autostart_enabled,
            dropping_files: false,
            one_time,
            one_time_resume,
            avatar_attempts: 0,
            avatar_failed: false,
            avatar_name: None,
            avatar_texture: None,
            avatar_fetch: Arc::new(Mutex::new(AvatarFetch::default())),
            folder_picker: Arc::new(Mutex::new(FolderPicker::default())),
            queue_cache: (0, Vec::new()),
            queue_cache_rev: u64::MAX,
        };
        // Session (S-C): only the **refresh token** is persisted, and it lives in
        // the OS keychain. Load it into the in-memory session; the refresh
        // worker then starts from `poll_session` on the first frame. A keychain
        // error (unavailable / locked) only logs a warning — the app simply
        // starts signed out.
        match crate::auth::KeyringTokenStore.load() {
            Ok(Some(refresh_token)) => {
                app.session.lock().unwrap().refresh_token = Some(refresh_token);
            }
            Ok(None) => {}
            Err(e) => log::warn!("Refresh-Token konnte nicht geladen werden: {e}"),
        }
        // Start decision (user decision 2026-10-18): a login-item (autostart)
        // start stays silent — no window, no Dock icon, tray only — while a
        // manual start (Finder/Dock/Spotlight/`open`/shell) shows the window.
        // `has_pending_session()` is only an **approximation** for "the tray
        // will be built": the tray is created lazily in `update` and `tray::build`
        // can fail (no tray backend). In that unlikely case the app is still
        // reachable via Finder / `open -a` / a second start (see
        // `src/single_instance.rs`), and `should_start_hidden` never goes silent
        // without a session, so it can never end up with no window *and* no
        // menu-bar icon on a manual start.
        let login_item = app_activation::launched_as_log_in_item();
        let has_tray = app.has_pending_session();
        let hidden = should_start_hidden(login_item, has_tray);
        log::debug!(
            "Login-Item-Startentscheidung: cached_login_item={login_item}, has_pending_session={has_tray}, hidden={hidden}"
        );
        app.window_hidden = hidden;
        app.ignore_initial_login_activation = hidden;
        if hidden {
            // No Dock icon may appear on the silent path.
            app_activation::become_accessory();
            // Hold the window hidden: `.with_visible(false)` in `main.rs` only
            // avoids the first-frame flash — eframe shows the window itself
            // (unconditionally) after the first frame, so the hidden state must
            // be (re)asserted with this command.
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else {
            // The window is created invisible (`main.rs`): `Regular` first,
            // then show + focus it — no flash on the manual path.
            app_activation::become_regular();
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(true));
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        app
    }

    pub(crate) fn log(&self, msg: impl Into<String>) {
        self.upload.lock().unwrap().log(msg);
    }

    pub(crate) fn is_logged_in(&self) -> bool {
        !self.config.session_id.is_empty() && !self.config.cookie_x_acb.is_empty()
    }

    /// Persist the configuration. **Silent** on success (user decision
    /// 2026-10-16: settings save immediately on every change now, so a
    /// per-write "saved" log line would flood the protocol; only failures are
    /// logged and surfaced through `config_error`).
    pub(crate) fn save_config(&mut self) {
        match self.config.save() {
            Ok(_) => {
                self.config_error = None;
            }
            Err(e) => {
                log::warn!("Einstellungen konnten nicht gespeichert werden: {e}");
                self.config_error =
                    Some(tr("Die Einstellungen konnten nicht gespeichert werden.").to_string());
            }
        }
    }

    /// Re-apply the tray menu labels in the current language (called after a
    /// language change so the menu bar does not stay in the old language).
    pub(crate) fn apply_tray_labels(&self) {
        if let Some(tray) = self.tray.as_ref() {
            tray.set_labels(*self.paused.borrow());
        }
    }

    /// Show and focus the main window — the one path every "bring the app back"
    /// trigger uses: the tray's "Fenster anzeigen", an app activation (a
    /// relaunch from Finder/Spotlight/`open`, see `src/app_activation.rs`), a
    /// second launch (see `src/single_instance.rs`) and the expired-session
    /// fallback. There is no Dock icon to click while the window is hidden
    /// (`Accessory`), so the Dock is not among these triggers.
    fn show_main_window(&mut self, ctx: &egui::Context) {
        self.window_hidden = false;
        // Restore the Dock icon / app menu (`Regular` first); unhide only as a
        // safeguard for an independently hidden state (e.g. ⌘H), then activate
        // the app so the restored window also comes to the front.
        app_activation::come_forward();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Open the About window and bring the main window to the front. The app
    /// menu can be chosen while the window is hidden in tray mode, so this goes
    /// through the shared show path.
    pub(crate) fn open_about(&mut self, ctx: &egui::Context) {
        self.about_open = true;
        self.show_main_window(ctx);
    }

    /// Derive the tray glyph state from the current app state and apply it —
    /// only when the state (or the OS theme it is drawn for) actually changed,
    /// so the menu-bar icon is not touched every frame. Also schedules the one
    /// repaint that turns the "done" glyph back into "idle" when its window
    /// expires (no polling/busy loop).
    pub(crate) fn update_tray_icon(&mut self, ctx: &egui::Context) {
        if self.tray.is_none() {
            return;
        }
        let paused = *self.paused.borrow();
        let busy = self.upload.lock().unwrap().active;
        let since_done = self.tray_done_at.map(|t| t.elapsed());
        let state = tray::tray_state(paused, busy, self.tray_last_run_failed, since_done);
        let theme = ctx.system_theme();
        if self.tray_icon_state != Some((state, theme)) {
            self.tray_icon_state = Some((state, theme));
            // `set_state` also re-applies the (now dynamic) tooltip, so the
            // status shows up there on every state change.
            if let Some(tray) = self.tray.as_mut() {
                tray.set_state(state, theme);
            }
        }
        // Wake up exactly when the "done" window ends so the glyph flips to
        // "idle"; a plain state change repaints from its own cause.
        if state == tray::TrayState::Done {
            if let Some(remaining) = self
                .tray_done_at
                .and_then(|t| tray::DONE_WINDOW.checked_sub(t.elapsed()))
            {
                ctx.request_repaint_after(remaining);
            }
        }
    }

    /// Turn the auto-launch (login item) registration on or off and re-sync the
    /// UI flag from the OS state.
    ///
    /// **Persisted intent (PKG-3, user decision 2026-10-14):** a successful
    /// change records the user's **intent** in `Config.autostart_enabled`
    /// (`Some(true)`/`Some(false)`) and saves it, so a deliberate "off" survives
    /// a restart. What is stored is the intent, **not** the OS state read back
    /// afterwards — see `autostart_intent_to_record` for why that distinction is
    /// load-bearing (SMAppService `RequiresApproval`). On an unsupported
    /// platform or a failing registration nothing is persisted.
    ///
    /// Never panics: on failure the cause is reported on the `log` facade and
    /// the flag snaps back to the real OS state.
    pub(crate) fn set_autostart(&mut self, enabled: bool) {
        let result = if enabled {
            self.autostart.enable()
        } else {
            self.autostart.disable()
        };
        // The checkbox mirrors the real OS state, not the intent: a failed
        // enable/disable must not leave it claiming something the OS does not
        // do. `autostart_enabled` is UI state only — never persisted (see
        // below).
        self.autostart_enabled = self.autostart.is_enabled().unwrap_or(false);
        match result {
            Ok(()) => {
                // Persist the user's **intent**, not the read-back OS state
                // (PKG-3 veto trap, user decision 2026-10-17). A freshly
                // registered SMAppService entry is first `RequiresApproval`,
                // so `is_enabled()` would report `false` right after a
                // successful `enable()`; persisting that would silently turn
                // the intent into the `Some(false)` veto and the app would
                // never register again. `Some(false)` therefore only ever
                // comes from an explicit user "off".
                self.config.autostart_enabled = autostart_intent_to_record(enabled);
                if let Err(e) = self.config.save() {
                    log::warn!("Einstellungen konnten nicht gespeichert werden: {e}");
                    self.config_error =
                        Some(tr("Die Einstellungen konnten nicht gespeichert werden.").to_string());
                }
            }
            Err(e) => {
                log::warn!("Autostart (Login-Item) konnte nicht geändert werden: {e}");
            }
        }
    }

    // -- account fetch (name + avatar) --------------------------------------

    /// Kick off the one-shot account fetch (name + avatar), unless it already
    /// ran this session. Triggered once after a successful sign-in, or at start
    /// when a session already exists. Never blocks a frame: the network + disk
    /// work runs on the tokio runtime and reports back through `avatar_fetch`.
    pub(crate) fn spawn_avatar_fetch(&self, ctx: &egui::Context) {
        let cfg = self.config.clone();
        let shared = self.avatar_fetch.clone();
        let ctx = ctx.clone();
        self.rt
            .spawn(async move { fetch_account_avatar(cfg, shared, ctx).await });
    }

    /// Pick up a published account-fetch result and turn the JPEG bytes into an
    /// egui texture. Called every frame; cheap while nothing was published.
    pub(crate) fn poll_avatar(&mut self, ctx: &egui::Context) {
        let (result, failed) = {
            let mut slot = self.avatar_fetch.lock().unwrap();
            (slot.result.take(), std::mem::take(&mut slot.failed))
        };
        if failed {
            // The attempt produced no name and no image: allow the one retry,
            // decided on a later frame via `should_fetch_avatar`.
            self.avatar_failed = true;
        }
        let Some(result) = result else {
            return;
        };
        if !result.name.is_empty() {
            self.avatar_name = Some(result.name);
        }
        if let Some(bytes) = result.jpeg {
            if let Some(tex) = decode_avatar_texture(ctx, &bytes) {
                self.avatar_texture = Some(tex);
            }
        }
    }

    // -- folder picker (Welle 5.1) ------------------------------------------

    /// Open the native folder picker **without blocking the UI**. The modal runs
    /// through `rfd::AsyncFileDialog` on the existing tokio runtime and reports
    /// back through `folder_picker`; `update` picks the result up on a later
    /// frame. Only one dialog at a time — a second request while one is open is
    /// dropped, so the answer can never be ambiguous.
    pub(crate) fn spawn_folder_picker(&self, target: FolderPickerTarget, ctx: &egui::Context) {
        {
            let mut slot = self.folder_picker.lock().unwrap();
            if slot.open.is_some() {
                return;
            }
            slot.open = Some(target);
        }
        let slot = self.folder_picker.clone();
        let ctx = ctx.clone();
        self.rt
            .spawn(async move { run_folder_picker(slot, target, ctx).await });
    }

    /// Pick up a finished folder-picker result and hand it to the matching
    /// consumer. Called every frame; cheap while nothing was published.
    pub(crate) fn poll_folder_picker(&mut self) {
        let Some((target, path)) = self.folder_picker.lock().unwrap().result.take() else {
            return;
        };
        match target {
            FolderPickerTarget::NewBackup => {
                // Open the shared "Datensicherungseinstellungen" window in Create
                // mode instead of adding + uploading right away.
                // Application defaults as the preselection (user decision
                // 2026-10-15), read through `default_options` so the create
                // form and the one-time queue cannot drift apart; a value
                // changed in the dialog belongs to the created target only,
                // it is not written back.
                let (dedup, conflict_resolution) =
                    crate::folders::default_options(&self.config);
                self.config_form = Some(BackupConfigForm {
                    path,
                    dedup,
                    conflict_resolution,
                    // Content filter: photos only today ("Videos sichern" is
                    // disabled); the effective filter stays `is_supported_image`.
                    content_filter: crate::folders::ContentFilter::Photos,
                    mode: ConfigFormMode::Create,
                    error: None,
                });
            }
            FolderPickerTarget::ConfigForm => {
                if let Some(f) = self.config_form.as_mut() {
                    f.path = path;
                    f.error = None;
                }
            }
        }
    }

    // -- rendering ----------------------------------------------------------

    pub(crate) fn nav_item(
        ui: &mut egui::Ui,
        current: &mut Page,
        value: Page,
        label: &str,
        icon: fn(&egui::Ui) -> egui::Image<'static>,
    ) {
        let selected = *current == value;
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::click());
        let hovered = resp.hovered();
        let color = if selected || hovered {
            theme::TEAL
        } else {
            theme::MUTED
        };
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, 6.0, Color32::from_rgb(0xD6, 0xEC, 0xF0));
        } else if hovered {
            painter.rect_filled(rect, 6.0, Color32::from_rgb(0xEF, 0xF3, 0xF3));
        }
        // Fixed icon column so every label starts at the same x.
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 17.0, rect.center().y),
            egui::vec2(18.0, 18.0),
        );
        // The raster is white, so the tint yields the navigation colour.
        icon(ui).tint(color).paint_at(ui, icon_rect);
        painter.text(
            egui::pos2(rect.left() + 38.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(15.0),
            theme::INK,
        );
        // Report the hand-drawn row as a selectable button (Welle 5.2c): without
        // this the row has no accessibility role and no label. Purely semantic —
        // nothing about the drawing above changes.
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label)
        });
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            *current = value;
        }
    }

    /// Full-window overlay while a file drag hovers over the window
    /// ("Einmalige Sicherung", docs/original/SPEC_ONE_TIME_UPLOAD.md §7): a
    /// semi-transparent frame with a visible border plus the drop instruction.
    ///
    /// Drawn last in `update` so it covers every panel, and `interactable
    /// (false)` so it lets clicks through to what is behind it instead of
    /// stealing them — a drag is not a click.
    fn drop_overlay(ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        egui::Area::new("one_time_drop_overlay".into())
            .order(egui::Order::Foreground)
            .interactable(false)
            .fixed_pos(screen.left_top())
            .default_size(screen.size())
            .show(ctx, |ui| {
                let painter = ui.painter();
                painter.rect(
                    screen,
                    8.0_f32,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 110),
                    egui::Stroke::new(2.0_f32, theme::TEAL),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    screen.center(),
                    egui::Align2::CENTER_CENTER,
                    tr("FOTOS PER DRAG & DROP VERSCHIEBEN, UM SIE HOCHZULADEN"),
                    egui::FontId::proportional(22.0),
                    Color32::WHITE,
                );
                painter.text(
                    screen.center() + egui::vec2(0.0, 34.0),
                    egui::Align2::CENTER_CENTER,
                    tr("Zur einmaligen Sicherung mit der Maus hierher ziehen"),
                    egui::FontId::proportional(15.0),
                    Color32::from_white_alpha(220),
                );
            });
    }

    pub(crate) fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 30.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add(app_logo(ui, 30.0));
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Prime Photos")).size(20.0).strong());
            },
        );
        ui.add_space(16.0);
        Self::nav_item(
            ui,
            &mut self.page,
            Page::Backup,
            tr("Datensicherung"),
            nav_backup_icon,
        );
        Self::nav_item(
            ui,
            &mut self.page,
            Page::Settings,
            tr("Einstellungen"),
            nav_settings_icon,
        );

        // Account block at the bottom of the sidebar. A fixed reserve plus a
        // plain top-down layout is far more predictable than `bottom_up`, which
        // mis-places a nested `vertical` and let the logout button collide with
        // the status line.
        const ACCOUNT_BLOCK_H: f32 = 104.0;
        let free = ui.available_height() - ACCOUNT_BLOCK_H;
        if free > 0.0 {
            ui.add_space(free);
        }
        if let Some(e) = &self.config_error {
            ui.label(RichText::new(e).color(theme::RED).size(11.0));
            ui.add_space(4.0);
        }
        let logged_in = self.is_logged_in();
        // Account block: the marketplace flag badge is the drawn placeholder
        // until the real avatar has been fetched (one-shot per session, see
        // `spawn_avatar_fetch`). Once it is there, the round picture replaces
        // the badge but keeps the same presence dot + tooltip. The real name
        // appears behind "Angemeldet" as soon as the preferences call answered;
        // until then (or on any failure) no name line is shown. The marketplace
        // line stays unchanged.
        ui.horizontal(|ui| {
            let tip = if logged_in {
                tr("Angemeldet")
            } else {
                tr("Nicht angemeldet")
            };
            match (logged_in, self.avatar_texture.as_ref()) {
                (true, Some(tex)) => draw_avatar_image(ui, 30.0, tex, tip),
                _ => draw_flag_badge(ui, 30.0, &self.config.country, logged_in, tip),
            }
            ui.add_space(10.0);
            ui.vertical(|ui| {
                if logged_in {
                    ui.label(
                        RichText::new(tr("Angemeldet als:"))
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                    if let Some(name) = &self.avatar_name {
                        ui.label(RichText::new(name).size(13.0).strong());
                    }
                }
                ui.label(
                    RichText::new(tr("Marktplatz"))
                        .size(11.0)
                        .color(theme::MUTED),
                );
                ui.label(
                    RichText::new(format!(
                        "amazon.{}",
                        crate::urls::marketplace_domain(&self.config.country)
                    ))
                    .size(13.0)
                    .strong(),
                );
            });
        });
        ui.add_space(10.0);
        if logged_in {
            let w = ui.available_width();
            if ui
                .add_sized([w, 26.0], theme::secondary_button(tr("Abmelden")))
                .clicked()
            {
                self.logout();
            }
        }
    }
}

impl eframe::App for PrimePhotoApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Drag & drop input ("Einmalige Sicherung"). `hovered_files` is
        // non-empty while a drag hovers over the window; `dropped_files` is
        // consumed by reading it, so a drop is handled exactly once. eframe
        // 0.31 ships this through by default (no `with_drag_and_drop` flag on
        // any platform — see the crate notes), so no viewport setup is needed.
        let drag_hover = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let dropped: Vec<std::path::PathBuf> = ctx
            .input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        self.dropping_files = drag_hover;
        if !dropped.is_empty() {
            self.start_one_time_upload(dropped);
        }

        // Tray / menu-bar events (PORT-3). Delivered through `tray`'s queue so
        // the handler can also wake the on-demand UI; see `src/tray.rs`.
        for ev in tray::drain_events() {
            let Some(tray) = self.tray.as_ref() else {
                break;
            };
            if ev.id == *tray.show_item.id() {
                self.show_main_window(ctx);
            } else if ev.id == *tray.pause_item.id() {
                let next = !*self.paused.borrow();
                self.paused.send_replace(next);
                // Persist the new state (user decision 2026-10-14): the pause
                // survives a restart. `save_config` is the existing path — it
                // reports a failure through `config_error` as usual.
                self.config.paused = next;
                self.save_config();
            } else if ev.id == *tray.about_item.id() {
                self.open_about(ctx);
            } else if ev.id == *tray.quit_item.id() {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // "Über Prime Photos" from the macOS app menu. The AppKit target object
        // only set a flag and woke us (see `src/app_menu.rs`); `false` on every
        // other platform.
        if app_menu::take_about_request() {
            self.open_about(ctx);
        }

        // App activation (macOS `NSApplicationDidBecomeActiveNotification`):
        // the user relaunched the app from Finder/Spotlight/`open` (while the
        // window is hidden the app is `Accessory`, so there is no Dock icon to
        // click). The observer only flagged + woke us (see
        // `src/app_activation.rs`); `false` on every other platform.
        if app_activation::take_become_active_request() {
            if self.ignore_initial_login_activation {
                self.ignore_initial_login_activation = false;
                log::debug!(
                    "App-Aktivierungsdiagnose: erste Login-Item-Startaktivierung ignoriert, window_hidden={}",
                    self.window_hidden
                );
            } else {
                log::debug!(
                    "App-Aktivierungsdiagnose: Aktivierungsanfrage verarbeitet, window_hidden={}",
                    self.window_hidden
                );
                self.show_main_window(ctx);
            }
        }

        // A second launch asked this instance to show its window and exited
        // (see `src/single_instance.rs`). `false` while nobody asked.
        if single_instance::take_show_request() {
            self.show_main_window(ctx);
        }

        // Keep the tray's pause label in sync with the actual pause state
        // (covers both the tray item itself and the in-window pause button).
        let paused_now = *self.paused.borrow();
        if paused_now != self.tray_paused {
            self.tray_paused = paused_now;
            if let Some(tray) = self.tray.as_ref() {
                tray.set_paused(paused_now);
            }
        }

        // Size the window to the screen once, on the first frame.
        if !self.sized {
            self.sized = true;
            if let Some(ms) = ctx.input(|i| i.viewport().monitor_size) {
                let w = (ms.x * 0.82).clamp(900.0, 1500.0);
                let h = (ms.y * 0.85).clamp(600.0, 1000.0);
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            }
        }

        // Remember whether a login attempt was in flight at the start of the
        // frame: a transition to "not running" below means it concluded
        // (success, rejection or the header's "Abbrechen") — used to drop the
        // `folders_error` flag again.
        let login_was_running = self.login.running;
        self.poll_login();
        // Session upkeep (S-C): starts/stops the background refresh worker and
        // reacts to an invalidated session or freshly minted cookies.
        self.poll_session(ctx);
        // Tray follows the session (user decision 2026-10-15: the menu-bar
        // icon appears only after sign-in). Reconciled every frame right where
        // the token states are applied, so all paths stay covered — keychain
        // token at start, successful login, logout and an invalidated session.
        if self.has_pending_session() {
            if self.tray.is_none() && !self.tray_build_failed {
                self.tray = tray::build(ctx.clone());
                if self.tray.is_none() {
                    // The OS gave us no tray (no backend, e.g. Linux without
                    // one — logged as an error in `src/tray.rs`). Latch it:
                    // without this flag `self.tray.is_none()` would still be
                    // true next frame and we would rebuild the whole tray
                    // (menu + items + global event handlers) and log the error
                    // again on **every** repaint, until the end of time. The
                    // flag is cleared when the session goes away (branch
                    // below), so the next sign-in gets exactly one new attempt
                    // — the same "once, not per frame" behaviour the app had
                    // when the tray was still built in `new`.
                    self.tray_build_failed = true;
                }
                // The glyph cache refers to a concrete tray: a fresh one has
                // never been drawn and its pause item starts at "Alle anhalten"
                // (the per-frame label sync above already ran this frame).
                self.tray_icon_state = None;
                let paused_now = *self.paused.borrow();
                self.tray_paused = paused_now;
                if let Some(tray) = self.tray.as_ref() {
                    tray.set_paused(paused_now);
                }
            }
        } else {
            // No session: a failed build may be retried on the next sign-in.
            self.tray_build_failed = false;
            if self.tray.is_some() {
                // Dropping the `Tray` removes the icon from the menu bar.
                self.tray = None;
                // …and forget the cached glyph so a later rebuild gets its
                // initial state again (`update_tray_icon` exits while no tray).
                self.tray_icon_state = None;
                // The tray may have been the only remaining handle to the app:
                // when the session died while the window was hidden in
                // tray-mode, nothing would be left to show the login screen
                // (or the "Sitzung abgelaufen" message) — the app would run
                // headless. Bring the window back so the user lands on the
                // login screen; the app is deliberately **not** quit (no
                // restart on an invalidated session, user decision
                // 2026-10-15).
                if self.window_hidden {
                    self.show_main_window(ctx);
                }
            }
        }

        // Close-to-tray: closing the window hides it and keeps the app running
        // in the background. Only the tray's "Beenden" (or a real quit) exits.
        // Runs **after** the tray reconcile so the decision uses this frame's
        // tray state: a logout followed by a close in the same frame must not
        // hide the window behind a tray that is dropped in that very frame
        // (app with no window and no menu-bar icon would be unkillable).
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.window_hidden = true;
            // Step the app back out of the foreground (macOS): reactivating an
            // already-frontmost app posts no activation notification, so
            // `step_back` changes to `Accessory` and deactivates without hiding
            // the app (see `src/app_activation.rs`). The Dock icon and app menu
            // disappear; the app keeps running. The tray calls
            // `show_main_window` directly; an app activation or second launch
            // also restores the window through that path.
            app_activation::step_back();
        }

        // Account fetch (name + avatar): first attempt after a successful
        // sign-in or at start when a session already exists, with **exactly one**
        // retry after a failed attempt (see `should_fetch_avatar`). A success is
        // final, so there is no loop and no endless retry. The actual work
        // happens on the tokio runtime (`spawn_avatar_fetch`); here we only
        // decide to start it and pick up its results.
        if self.is_logged_in() && should_fetch_avatar(self.avatar_attempts, self.avatar_failed) {
            self.avatar_attempts += 1;
            self.spawn_avatar_fetch(ctx);
        }
        self.poll_avatar(ctx);
        // Folder picker (Welle 5.1): hand a finished native-dialog answer to its
        // consumer before the pages that read it are drawn this frame.
        self.poll_folder_picker();

        // Onboarding: until the user is signed in (or while the sign-in page is
        // open) show the login flow in this same window. A stored refresh
        // token counts as a pending session: no login flash on restart — the
        // worker mints the cookies in the background (like the original).
        // A damaged `folders.toml` forces the login screen too (user decision
        // 2026-10-16, "like a logout"); `folders_error` is dropped again once a
        // login attempt concludes.
        if self.login.running
            || !(self.is_logged_in() || self.has_pending_session())
            || self.folders_error
        {
            self.login_ui(ctx);
        } else {
            // Start a one-time queue restored from the DB at app start (user
            // decision 2026-10-14) once the folder-cold-start guards allow it.
            // Cheap per frame: it is a no-op unless a queue was restored.
            self.resume_one_time();
            // Panels come **before** the `CentralPanel` (egui's standard order):
            // a bottom panel added after the central one overlaps the page,
            // because `CentralPanel` does not shrink the available rect (it is
            // allowed to be covered by windows), so the log panel would sit on
            // the page's last pixels. Shown first, it reserves its strip and the
            // page ends above it. The side panel stays first of all so the log
            // panel keeps its horizontal extent (right of the sidebar).
            egui::SidePanel::left("nav")
                .exact_width(220.0)
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(theme::SIDEBAR_BG)
                        .inner_margin(egui::Margin::symmetric(12, 16)),
                )
                .show(ctx, |ui| self.sidebar(ui));

            // Bottom log panel (collapsible) for transparency.
            egui::TopBottomPanel::bottom("log")
            .resizable(true)
            .default_height(24.0)
            .min_height(24.0)
            .show(ctx, |ui| {
                let s = self.upload.lock().unwrap();
                let summary = if s.active {
                    tr3(
                        "Upload läuft: {} · {} · {}",
                        format!("{}/{}", s.done, s.total),
                        &s.current,
                        eta_suffix(&s),
                    )
                } else if s.total > 0 {
                    tr1(
                        "Letzter Upload: {} erfolgreich",
                        format!("{}/{}", s.done, s.total - s.failed),
                    )
                } else {
                    tr("Bereit").to_string()
                };
                egui::CollapsingHeader::new(summary)
                    // Fixed id: `summary` changes with every counter / file / ETA
                    // update, so an id derived from the label would lose the
                    // open state on each update (docs/dev/DEV_NOTES.md §3).
                    .id_salt("log_panel")
                    .show(ui, |ui| {
                        // Definite body height given to the scroll area as a
                        // *floor*. With `max_height(available_height)` alone the
                        // loop settled on the scroll area's `min_scrolled_size`
                        // (64 px) and pinned the viewport to 64 px. With the
                        // floor the expanded panel is header + 200 px, and since
                        // the scroll area still fills whatever the panel hands
                        // it, a height enlarged by dragging persists; shrinking
                        // below the floor is not possible. Capped by the window
                        // so header + body always fit.
                        let body_h = 200.0_f32
                            .min(ui.ctx().screen_rect().height() - 120.0)
                            .max(64.0);
                        egui::ScrollArea::vertical()
                            .min_scrolled_height(body_h)
                            .show(ui, |ui| {
                                for line in &s.log {
                                    ui.label(line);
                                }
                            });
                    });
            });

            egui::CentralPanel::default().show(ctx, |ui| match self.page {
                Page::Backup => self.backup_ui(ui),
                Page::Settings => self.settings_ui(ui),
            });

            let active = self.upload.lock().unwrap().active;
            // Also latch background runs (PORT-5): the tick starts them without
            // `start_scheduled_upload`, so `was_active` would never become true
            // and their completion would stay silent (no notification, no
            // `finalize_upload`). Latching here makes both paths identical.
            if active {
                self.was_active = true;
            }
            if active || self.was_active {
                // Cheap while nothing is going on, responsive while it is: an
                // unfocused/hidden window (tray-mode background upload) only
                // needs the counter repaint, not 4 fps of full layout work.
                // `focused` is `None` on platforms that do not report it —
                // assume visible then (a repaint too many is harmless).
                let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
                ctx.request_repaint_after(if focused {
                    Duration::from_millis(250)
                } else {
                    Duration::from_secs(1)
                });
                if self.was_active && !active {
                    // A cancelled run (sign-out) is not a finished run: no
                    // finalize, no "done" notification, no tray "done" glyph
                    // (user decision 2026-10-16). The uploaded part stays in the
                    // dedup index and the rest is picked up on the next start.
                    let cancelled = self.upload.lock().unwrap().cancelled;
                    if !cancelled {
                        self.finalize_upload();
                        let (done, failed, total) = {
                            let s = self.upload.lock().unwrap();
                            (s.done, s.failed, s.total)
                        };
                        // Tray: remember how the run ended so the glyph can show
                        // "error" (sticky until the next run) or "done" (briefly).
                        self.tray_last_run_failed = failed > 0;
                        self.tray_done_at = Some(Instant::now());
                        if self.config.notifications {
                            notify::run_finished(done, failed, total);
                        }
                    }
                    self.was_active = false;
                }
            }

            // Shared "Datensicherungseinstellungen" window (Create/Edit).
            let mut form_action: Option<ConfigFormAction> = None;
            if let Some(form) = self.config_form.as_mut() {
                let editing = matches!(form.mode, ConfigFormMode::Edit(_));
                egui::Window::new(tr("Datensicherungseinstellungen"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        form_action = Self::render_backup_config(ui, form, editing);
                    });
            }
            if let Some(action) = form_action {
                self.handle_config_form_action(action, ctx);
            }
        }

        // A damaged `folders.toml` keeps the app on the login screen like a
        // logout (user decision 2026-10-16). The state is recoverable and never
        // destructive: as soon as a login attempt concludes — success, rejection
        // or the header's "Abbrechen" — the flag and its message are dropped, so
        // the app cannot stay stuck on the login screen. The keychain refresh
        // token is deliberately left untouched.
        if self.folders_error && login_was_running && !self.login.running {
            self.folders_error = false;
            let msg = folders_load_message();
            if self.config_error.as_deref() == Some(msg.as_str()) {
                self.config_error = None;
            }
        }

        // Tray glyph (idle/active/paused/done/error). Runs on every frame —
        // also before sign-in and while the window is hidden — but re-sets the
        // icon only on change.
        self.update_tray_icon(ctx);

        // Name-conflict prompt (`ConflictResolution::Ask`).
        self.conflict_dialog(ctx);

        // About window + the third-party license window it can open (Welle 4b).
        // Drawn outside the signed-in branch so the app menu can open them from
        // the login screen too.
        if self.about_open {
            about::about_window(ctx, &mut self.about_open, &mut self.license_open);
        }
        if self.license_open {
            about::license_window(ctx, &mut self.license_open, &mut self.license_tab);
        }

        // Deferred login start: creating the webview needs the window handle.
        if self.pending_login {
            self.pending_login = false;
            let _ = self.config.save();
            self.begin_login(frame, ctx);
        }
        if self.login.running {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        // Keep the window title clear about what this window is.
        let title = if self.login.running
            || !(self.is_logged_in() || self.has_pending_session())
            || self.folders_error
        {
            tr("Login – Prime Photos")
        } else {
            tr("Prime Photos")
        };
        if self.window_title.as_deref() != Some(title) {
            self.window_title = Some(title.to_owned());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_owned()));
        }

        // Drawn last, after every panel: the drop overlay covers the whole
        // window (and lets clicks through while it is visible).
        if self.dropping_files {
            Self::drop_overlay(ctx);
        }
    }
}

/// The plain-language message shown when `folders.toml` could not be parsed and
/// was set aside (user decision 2026-10-16). Contains no file name, path or
/// error code — the technical detail only goes to the log (user rule: no
/// technical messages in the end-user UI).
fn folders_load_message() -> String {
    tr("Die Sicherungsziele konnten nicht geladen werden. Die beschädigte Datei wurde gesichert.")
        .to_string()
}

/// Draw the fetched avatar as a round 30 px badge with the same presence dot as
/// the flag badge it replaces. The corners are rounded with `Image::corner_radius`
/// (half the side), matching the round marketplace badge's visual language.
fn draw_avatar_image(ui: &mut egui::Ui, diameter: f32, tex: &egui::TextureHandle, tooltip: &str) {
    let image = egui::Image::new(egui::load::SizedTexture::from_handle(tex))
        .fit_to_exact_size(egui::vec2(diameter, diameter))
        .corner_radius(egui::CornerRadius::same((diameter / 2.0).round() as u8));
    let resp = ui.add(image).on_hover_text(tooltip);
    let rect = resp.rect;
    // Presence dot in the lower-right, exactly like `draw_flag_badge`.
    let dot = diameter * 0.17;
    let c = egui::pos2(rect.right() - dot * 0.9, rect.bottom() - dot * 0.9);
    let painter = ui.painter();
    painter.circle_filled(c, dot + 1.6, Color32::WHITE);
    painter.circle_filled(c, dot, theme::GREEN);
}

/// Decode avatar JPEG/PNG bytes into an egui texture. `None` on any decode
/// error so the caller keeps the drawn placeholder (no panic, no empty image).
fn decode_avatar_texture(ctx: &egui::Context, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = img.into_raw();
    let color = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture("account_avatar", color, egui::TextureOptions::LINEAR))
}

/// Publish one intermediate/final account-fetch result and nudge the UI to
/// repaint so the value is picked up on the next frame.
fn publish_avatar(shared: &Arc<Mutex<AvatarFetch>>, ctx: &egui::Context, result: AvatarResult) {
    shared.lock().unwrap().result = Some(result);
    ctx.request_repaint();
}

/// Mark an account-fetch attempt as **failed** (neither name nor image arrived)
/// and nudge the UI to repaint, so it can decide on the single allowed retry
/// (`should_fetch_avatar`) on a later frame.
fn mark_avatar_failed(shared: &Arc<Mutex<AvatarFetch>>, ctx: &egui::Context) {
    shared.lock().unwrap().failed = true;
    ctx.request_repaint();
}

/// Background account fetch: preferences (name + avatar node id), then the
/// 60×60 thumbnail. A valid on-disk cache is preferred for the image and
/// published first; the fresh image is fetched afterwards and published as a
/// second result. Any failure only logs a warning **without values** and leaves
/// the UI on the drawn placeholder.
async fn fetch_account_avatar(cfg: Config, shared: Arc<Mutex<AvatarFetch>>, ctx: egui::Context) {
    let photo = crate::prime_photos::PrimePhotos::new(&cfg, false);
    let pair = match photo.fetch_account_name().await {
        Ok(Some(pair)) => pair,
        Ok(None) => {
            log::warn!("Kontoname/Profilbild: Antwort ohne verwertbare Felder.");
            mark_avatar_failed(&shared, &ctx);
            return;
        }
        Err(_) => {
            log::warn!("Kontoname/Profilbild: Abruf der Kontoeinstellungen fehlgeschlagen.");
            mark_avatar_failed(&shared, &ctx);
            return;
        }
    };
    let (name, node) = pair;

    // Read the cache (if any) up front: a present, valid file is used right
    // away, so the picture is on screen without waiting for the network.
    let cache = crate::prime_photos::avatar_cache_path(&node);
    let cached = cache
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .filter(|b| crate::prime_photos::looks_like_image(b));
    publish_avatar(
        &shared,
        &ctx,
        AvatarResult {
            name: name.clone(),
            jpeg: cached,
        },
    );

    // Refresh from the service and fill the cache. A response that is not a
    // valid image is discarded (the cached/placeholder picture stays).
    match photo.fetch_avatar_jpeg(&node).await {
        Ok(bytes) if crate::prime_photos::looks_like_image(&bytes) => {
            if let Some(path) = &cache {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Err(e) = std::fs::write(path, &bytes) {
                    log::warn!("Profilbild-Cache konnte nicht geschrieben werden: {e}");
                }
            }
            publish_avatar(
                &shared,
                &ctx,
                AvatarResult {
                    name,
                    jpeg: Some(bytes),
                },
            );
        }
        Ok(_) => log::warn!("Profilbild: Antwort ist kein gültiges Bild."),
        Err(_) => log::warn!("Profilbild: Abruf fehlgeschlagen."),
    }
}

/// Background folder picker (Welle 5.1): run the native modal off the UI thread
/// and publish the answer (if any) into the shared slot.
///
/// The macOS `rfd` backend starts the sheet inside
/// `AsyncFileDialog::pick_folder` itself (eagerly, via `run_on_main`) and wakes
/// the polling task from the sheet's completion handler — it needs no dedicated
/// executor and no main-thread polling, so the existing tokio runtime can drive
/// it. `request_repaint` makes the UI pick the result up on the next frame; a
/// cancelled dialog clears the guard without publishing a path.
async fn run_folder_picker(
    slot: Arc<Mutex<FolderPicker>>,
    target: FolderPickerTarget,
    ctx: egui::Context,
) {
    let picked = rfd::AsyncFileDialog::new().pick_folder().await;
    let path = picked.map(|handle| handle.path().to_path_buf());
    {
        let mut s = slot.lock().unwrap();
        s.open = None;
        if let Some(path) = path {
            s.result = Some((target, path));
        }
    }
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_action_never_decided_enables_and_records() {
        // `None` ⇒ enable + record, regardless of the current OS state.
        assert_eq!(
            autostart_action(None, false),
            AutostartAction::EnableAndRecord
        );
        assert_eq!(
            autostart_action(None, true),
            AutostartAction::EnableAndRecord
        );
    }

    #[test]
    fn autostart_action_intent_on_repairs_only_when_the_os_is_off() {
        assert_eq!(
            autostart_action(Some(true), false),
            AutostartAction::Enable
        );
        assert_eq!(
            autostart_action(Some(true), true),
            AutostartAction::Nothing
        );
    }

    #[test]
    fn autostart_action_deliberate_off_is_a_veto() {
        assert_eq!(
            autostart_action(Some(false), false),
            AutostartAction::Nothing
        );
        assert_eq!(
            autostart_action(Some(false), true),
            AutostartAction::Nothing
        );
    }

    #[test]
    fn autostart_intent_to_record_persists_the_intent_not_the_read_back() {
        // The veto trap: after a successful `enable()` the OS read-back can
        // still be `false` (SMAppService `RequiresApproval`). What gets
        // persisted is the user's intent — `Some(true)` — so the intent never
        // silently degrades into the `Some(false)` veto.
        assert_eq!(autostart_intent_to_record(true), Some(true));
        assert_eq!(autostart_intent_to_record(false), Some(false));
    }

    #[test]
    fn should_start_hidden_only_for_a_login_item_start_with_a_tray() {
        // Silent (window hidden, no Dock icon) **only** for a login-item start
        // that has a tray to come back through.
        assert!(should_start_hidden(true, true));
        // A login-item start without a tray must show the window: no window
        // *and* no menu-bar icon would be unreachable.
        assert!(!should_start_hidden(true, false));
        // A manual start always shows the window — with or without a tray.
        assert!(!should_start_hidden(false, true));
        assert!(!should_start_hidden(false, false));
    }

    #[test]
    fn should_fetch_avatar_allows_first_try_and_one_retry_after_failure() {
        // Nothing started yet: the first attempt always runs, whatever the
        // (irrelevant) previous outcome flag says.
        assert!(should_fetch_avatar(0, false));
        assert!(should_fetch_avatar(0, true));
        // One attempt done and it failed: exactly one retry.
        assert!(should_fetch_avatar(1, true));
        // One attempt done and it succeeded: done, no retry.
        assert!(!should_fetch_avatar(1, false));
    }

    #[test]
    fn should_fetch_avatar_gives_up_after_two_attempts() {
        // Two attempts (initial + retry) used up, regardless of the outcome.
        assert!(!should_fetch_avatar(2, false));
        assert!(!should_fetch_avatar(2, true));
        assert!(!should_fetch_avatar(3, true));
    }
}
