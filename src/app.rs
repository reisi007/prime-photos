use crate::app_menu;
use crate::backup_config::{BackupConfigForm, ConfigFormAction, ConfigFormMode};
use crate::config::Config;
use crate::countries::system_country;
use crate::folders::FolderStore;
use crate::i18n::{self, tr, tr1, tr3};
use crate::icons::{draw_avatar, nav_icon, status_dot, NavIcon};
use crate::login;
use crate::notify;
use crate::theme;
use crate::tray;
use crate::upload::{eta_suffix, UploadShared};
use eframe::egui::{self, Color32, RichText};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::watch;

/// Height of the slim branded header shown above the Amazon sign-in webview.
pub(crate) const HEADER_H: f32 = 60.0;

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
            paused: watch::Sender::new(false),
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

        // Account block pinned to the bottom of the sidebar.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(10.0);
            if let Some(e) = &self.config_error {
                ui.label(RichText::new(e).color(theme::RED).size(11.0));
            }
            let logged_in = self.is_logged_in();
            if logged_in && ui.add(theme::secondary_button(tr("Abmelden"))).clicked() {
                self.logout();
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                status_dot(ui, if logged_in { theme::GREEN } else { theme::RED });
                ui.label(
                    RichText::new(if logged_in {
                        tr("Angemeldet")
                    } else {
                        tr("Nicht angemeldet")
                    })
                    .size(12.0)
                    .color(if logged_in { theme::GREEN } else { theme::RED }),
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                draw_avatar(ui, 30.0, theme::TEAL);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(tr("Angemeldet als:"))
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                    ui.label(RichText::new(tr("Amazon Photos")).size(13.0).strong());
                    ui.label(theme::muted(format!(
                        "amazon.{}",
                        login::amazon_domain(&self.config.country)
                    )));
                });
            });
            ui.add_space(8.0);
        });
    }
}

impl eframe::App for PrimePhotoApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
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
                self.paused.send_replace(!*self.paused.borrow());
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
                        eta_suffix(s.done, s.total, s.started),
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
            if active || self.was_active {
                ctx.request_repaint_after(Duration::from_millis(250));
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
    }
}
