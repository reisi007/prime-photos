use crate::app_menu;
use crate::backup_config::{BackupConfigForm, ConfigFormAction, ConfigFormMode};
use crate::config::Config;
use crate::countries::system_country;
use crate::folders::{BackupFolder, FolderStore};
use crate::i18n::{self, tr, tr1, tr3};
use crate::icons::{draw_flag_badge, nav_icon, NavIcon};
use crate::login;
use crate::notify;
use crate::theme;
use crate::tray;
use crate::upload::{eta_suffix, FileState, UploadShared};
use eframe::egui::{self, Color32, RichText};
use std::sync::{Arc, Mutex};
use std::time::Duration;
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

pub(crate) struct PrimePhotoApp {
    pub(crate) rt: Arc<Runtime>,
    pub(crate) config: Config,
    pub(crate) config_path: String,
    pub(crate) config_error: Option<String>,
    pub(crate) login: login::WebviewLogin,
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
    /// Auto-launch (login item) registration; `inner` is `None` when the
    /// platform does not support it.
    pub(crate) autostart: Autostart,
    /// UI state of the autostart checkbox, kept in sync with the OS state.
    /// Runtime-only: not persisted in `Config` (see `set_autostart`).
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
    /// Transient one-time (drag & drop) upload target: never persisted, lives
    /// only for the length of the run and then keeps its settled counters so
    /// the Backup page can show the finished row. `None` until the first drop.
    pub(crate) one_time: Option<BackupFolder>,
}

impl PrimePhotoApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        // A native main menu is required for ⌘V/⌘C/⌘X to work in the webview.
        app_menu::install();
        let rt = Arc::new(Runtime::new().expect("tokio runtime"));
        let (mut config, config_error) = match Config::load() {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e.to_string())),
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
        let config_path = Config::get_config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| e.to_string());
        let autostart = Autostart::new();
        // Seed the checkbox from the OS state (is the login item already
        // registered?); `false` when unsupported or the state is unreadable.
        let autostart_enabled = autostart.is_enabled().unwrap_or(false);
        // The persisted pause state survives a restart (user decision
        // 2026-10-14): seed the sender from the config we just loaded instead
        // of always starting unpaused.
        let paused = watch::Sender::new(config.paused);
        Self {
            rt,
            config,
            config_path,
            config_error,
            login: login::WebviewLogin::new(),
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
            autostart,
            autostart_enabled,
            dropping_files: false,
            one_time: None,
            queue_cache: (0, Vec::new()),
            queue_cache_rev: u64::MAX,
        }
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

    /// Turn the auto-launch (login item) registration on or off and re-sync the
    /// UI flag from the OS state.
    ///
    /// **Runtime-only / not persisted:** the flag lives in `PrimePhotoApp`, not
    /// in `Config` — this package may not touch `config.rs` (another agent owns
    /// it), so there is no serialized field to write. On startup the checkbox is
    /// therefore seeded from the OS state, and a manual change survives until
    /// the registration itself is changed (e.g. from the OS or another tool).
    ///
    /// Never panics: on an unsupported platform (or a failing registration) the
    /// failure is reported on the `log` facade and the flag snaps back to `false`.
    pub(crate) fn set_autostart(&mut self, enabled: bool) {
        let result = if enabled {
            self.autostart.enable()
        } else {
            self.autostart.disable()
        };
        // Report the real state, not the intent: a failed enable/disable must
        // not leave the checkbox claiming something the OS does not do.
        self.autostart_enabled = self.autostart.is_enabled().unwrap_or(false);
        if let Err(e) = result {
            log::warn!("Autostart (Login-Item) konnte nicht geändert werden: {e}");
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
        // Flag badge (with the presence dot) + the marketplace. Deliberately no
        // account name: we cannot fetch the real one yet, and a stand-in would be
        // wrong (AGENTS.todo.md „Zurückgestellt" #8). The sign-in state is the
        // dot on the badge, not a coloured word — this view is normally only
        // reached while signed in.
        ui.horizontal(|ui| {
            let tip = if logged_in {
                tr("Angemeldet")
            } else {
                tr("Nicht angemeldet")
            };
            draw_flag_badge(ui, 30.0, &self.config.country, logged_in, tip);
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(tr("Marktplatz"))
                        .size(11.0)
                        .color(theme::MUTED),
                );
                ui.label(
                    RichText::new(format!(
                        "amazon.{}",
                        login::amazon_domain(&self.config.country)
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

        // Onboarding: until the user is signed in (or while the sign-in page is
        // open) show the login flow in this same window.
        if self.login.running || !self.is_logged_in() {
            self.login_ui(ctx);
        } else {
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
        let title = if self.login.running || !self.is_logged_in() {
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
