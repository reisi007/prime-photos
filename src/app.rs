use crate::app_menu;
use crate::auth::TokenStore;
use crate::backup_config::{BackupConfigForm, ConfigFormAction, ConfigFormMode};
use crate::config::Config;
use crate::countries::system_country;
use crate::dedup::DedupIndex;
use crate::folders::{BackupFolder, FolderStatus, FolderStore};
use crate::i18n::{self, tr, tr1, tr3};
use crate::icons::{draw_flag_badge, nav_icon, NavIcon};
use crate::login;
use crate::notify;
use crate::theme;
use crate::tray;
use crate::upload::{eta_suffix, FileState, UploadShared};
use eframe::egui::{self, Color32, RichText};
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
/// constructors so this helper stays `#[cfg]`-free. `MacOSLaunchMode::LaunchAgent`
/// is also the crate default; it is spelled out because that is the mode we
/// want on macOS (a plist in `~/Library/LaunchAgents`, no AppleScript).
fn build_autostart() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name(AUTOSTART_APP_NAME)
        .set_app_path(&exe.display().to_string())
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
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
    pub(crate) config_path: String,
    pub(crate) config_error: Option<String>,
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
    pub(crate) concurrency: usize,
    pub(crate) slow_mode: bool,
    pub(crate) limit_enabled: bool,
    pub(crate) limit_kbps: u32,
    pub(crate) album_name: String,
    pub(crate) zone_override: String,
    pub(crate) was_active: bool,
    /// Open "Datensicherungseinstellungen" window (Create or Edit), if any.
    pub(crate) config_form: Option<BackupConfigForm>,
    /// Tray / menu-bar icon (PORT-3); `None` if it could not be created.
    pub(crate) tray: Option<tray::Tray>,
    /// Set when the user really wants to quit (tray "Beenden" / ⌘Q), so the
    /// close-to-tray intercept below lets the close through.
    pub(crate) quitting: bool,
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
}

impl PrimePhotoApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        // A native main menu is required for ⌘V/⌘C/⌘X to work in the webview.
        app_menu::install();
        let rt = Arc::new(Runtime::new().expect("tokio runtime"));
        // `config_loaded_ok` is derived directly from the `Config::load()`
        // result: a broken/unreadable file yields `Config::default()` (with
        // empty cookies) in memory, and that fallback must never be written
        // over the still-present, hand-fixable `config.toml` (PKG-3 guard).
        let (mut config, config_error, config_loaded_ok) = match Config::load() {
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
        let config_path = Config::get_config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| e.to_string());
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
        let (one_time, one_time_resume) = match DedupIndex::open().one_time_queue() {
            Some((display_path, files)) if !files.is_empty() => (
                Some(BackupFolder {
                    path: display_path,
                    name: "Einmalige Sicherung".to_string(),
                    enabled: true,
                    // App defaults, exactly like a fresh drop
                    // (`begin_one_time_run`): a restart-restored queue uses the
                    // same dedup/conflict settings the user configured, not the
                    // enum default — otherwise a fresh drop and a restored one
                    // could diverge (e.g. Replace vs. SaveAsNew).
                    dedup: config.dedup_default,
                    conflict_resolution: config.conflict_resolution_default,
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
        let app = Self {
            rt,
            config,
            config_path,
            config_error,
            login: login::WebviewLogin::new(),
            session: Arc::new(Mutex::new(login::Session::default())),
            pending_login: false,
            window_title: None,
            sized: false,
            page: Page::Backup,
            settings_tab: SettingsTab::General,
            store: FolderStore::load(),
            upload: Arc::new(Mutex::new(UploadShared::default())),
            paused,
            concurrency: 3,
            slow_mode: false,
            limit_enabled: false,
            limit_kbps: 100,
            album_name: String::new(),
            zone_override: String::new(),
            was_active: false,
            config_form: None,
            tray: tray::build(cc.egui_ctx.clone()),
            quitting: false,
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
        app
    }

    pub(crate) fn log(&self, msg: impl Into<String>) {
        self.upload.lock().unwrap().log(msg);
    }

    pub(crate) fn is_logged_in(&self) -> bool {
        !self.config.session_id.is_empty() && !self.config.cookie_x_acb.is_empty()
    }

    pub(crate) fn save_config(&mut self) {
        if !self.zone_override.trim().is_empty() {
            self.config.zone = self.zone_override.trim().to_string();
        }
        match self.config.save() {
            Ok(_) => {
                self.config_error = None;
                self.log(tr("Einstellungen gespeichert."));
            }
            Err(e) => self.config_error = Some(e.to_string()),
        }
    }

    /// Re-apply the tray menu labels in the current language (called after a
    /// language change so the menu bar does not stay in the old language).
    pub(crate) fn apply_tray_labels(&self) {
        if let Some(tray) = self.tray.as_ref() {
            tray.set_labels(*self.paused.borrow());
        }
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
            if let Some(tray) = self.tray.as_ref() {
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
    /// change is recorded in `Config.autostart_enabled` (`Some(true)`/`Some(false)`)
    /// and saved, so a deliberate "off" survives a restart. On an unsupported
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
        // Report the real state, not the intent: a failed enable/disable must
        // not leave the checkbox claiming something the OS does not do.
        self.autostart_enabled = self.autostart.is_enabled().unwrap_or(false);
        match result {
            Ok(()) => {
                // Record the achieved intent so it is re-applied on the next
                // start (including a deliberate "off").
                self.config.autostart_enabled = Some(self.autostart_enabled);
                if let Err(e) = self.config.save() {
                    self.config_error = Some(e.to_string());
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

    // -- rendering ----------------------------------------------------------

    pub(crate) fn nav_item(ui: &mut egui::Ui, current: &mut Page, value: Page, label: &str, icon: NavIcon) {
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
        nav_icon(painter, icon, icon_rect, color);
        painter.text(
            egui::pos2(rect.left() + 38.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(15.0),
            theme::INK,
        );
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            *current = value;
        }
    }

    /// Full-window overlay while a file drag hovers over the window
    /// ("Einmalige Sicherung", docs/SPEC_ONE_TIME_UPLOAD.md §7): a
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
        ui.label(RichText::new(tr("Prime Photos")).size(20.0).strong());
        ui.add_space(16.0);
        Self::nav_item(
            ui,
            &mut self.page,
            Page::Backup,
            tr("Datensicherung"),
            NavIcon::Backup,
        );
        Self::nav_item(
            ui,
            &mut self.page,
            Page::Settings,
            tr("Einstellungen"),
            NavIcon::Settings,
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
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            } else if ev.id == *tray.pause_item.id() {
                let next = !*self.paused.borrow();
                self.paused.send_replace(next);
                // Persist the new state (user decision 2026-10-14): the pause
                // survives a restart. `save_config` is the existing path — it
                // reports a failure through `config_error` as usual.
                self.config.paused = next;
                self.save_config();
            } else if ev.id == *tray.quit_item.id() {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
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

        // Close-to-tray: closing the window hides it and keeps the app running
        // in the background. Only the tray's "Beenden" (or a real quit) exits.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
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

        self.poll_login();
        // Session upkeep (S-C): starts/stops the background refresh worker and
        // reacts to an invalidated session or freshly minted cookies.
        self.poll_session(ctx);

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

        // Onboarding: until the user is signed in (or while the sign-in page is
        // open) show the login flow in this same window. A stored refresh
        // token counts as a pending session: no login flash on restart — the
        // worker mints the cookies in the background (like the original).
        if self.login.running || !(self.is_logged_in() || self.has_pending_session()) {
            self.login_ui(ctx);
        } else {
            // Start a one-time queue restored from the DB at app start (user
            // decision 2026-10-14) once the folder-cold-start guards allow it.
            // Cheap per frame: it is a no-op unless a queue was restored.
            self.resume_one_time();
            egui::SidePanel::left("nav")
                .exact_width(220.0)
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(theme::SIDEBAR_BG)
                        .inner_margin(egui::Margin::symmetric(12, 16)),
                )
                .show(ctx, |ui| self.sidebar(ui));

            egui::CentralPanel::default().show(ctx, |ui| match self.page {
                Page::Backup => self.backup_ui(ui),
                Page::Settings => self.settings_ui(ui),
            });

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
                egui::CollapsingHeader::new(summary).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for line in &s.log {
                                ui.label(line);
                            }
                        });
                });
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
                self.handle_config_form_action(action);
            }
        }

        // Tray glyph (idle/active/paused/done/error). Runs on every frame —
        // also before sign-in and while the window is hidden — but re-sets the
        // icon only on change.
        self.update_tray_icon(ctx);

        // Name-conflict prompt (`ConflictResolution::Ask`).
        self.conflict_dialog(ctx);

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
        let title = if self.login.running || !(self.is_logged_in() || self.has_pending_session()) {
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
