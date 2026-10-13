mod prime_photos;
mod app_menu;
mod config;
mod dedup;
mod folders;
mod login;
mod theme;

use prime_photos::PrimePhotos;
use config::Config;
use dedup::DedupIndex;
use eframe::egui::{self, Color32, RichText};
use folders::{ConflictResolution, FolderStatus, FolderStore};
use rfd::FileDialog;
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use tokio::sync::watch;
use tokio::task::JoinSet;

/// Height of the slim branded header shown above the Amazon sign-in webview.
const HEADER_H: f32 = 60.0;

// ---------------------------------------------------------------------------
// Shared upload state (written by the async workers, read by the UI)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct UploadShared {
    active: bool,
    total: usize,
    done: usize,
    failed: usize,
    current: String,
    log: Vec<String>,
    folder_total: HashMap<usize, usize>,
    folder_done: HashMap<usize, usize>,
    folder_failed: HashMap<usize, usize>,
    uploaded_ids: Vec<String>,
}

impl UploadShared {
    fn log(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 500 {
            self.log.drain(0..100);
        }
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(PartialEq, Eq, Clone, Copy)]
enum Page {
    Backup,
    Settings,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum SettingsTab {
    General,
    Bandwidth,
}

enum FolderAction {
    Upload(usize),
    Retry(usize),
    Remove(usize),
    OpenFinder(usize),
    ToggleEnabled(usize),
    Edit(usize),
}

/// Whether the "Datensicherungseinstellungen" window is configuring a new
/// backup or one that already exists. Edit mode stores the folder **path** (not
/// a row index) so a folder removed while the window is open cannot make Save
/// write to the wrong backup.
#[derive(Clone, PartialEq, Eq)]
enum ConfigFormMode {
    Create,
    Edit(PathBuf),
}

/// Result of one rendered frame of the shared backup-configuration form body.
enum ConfigFormAction {
    Save,
    Cancel,
}

/// State of the "Datensicherungseinstellungen" window, shared by Create and Edit.
struct BackupConfigForm {
    path: PathBuf,
    dedup: bool,
    conflict_resolution: ConflictResolution,
    mode: ConfigFormMode,
    /// Inline validation message; keeps the window open after an invalid Save.
    error: Option<String>,
}

struct Row {
    idx: usize,
    name: String,
    path: PathBuf,
    enabled: bool,
    status: FolderStatus,
    total: usize,
    done: usize,
    last_updated: String,
}

struct PrimePhotoApp {
    rt: Arc<Runtime>,
    config: Config,
    config_path: String,
    config_error: Option<String>,
    login: login::WebviewLogin,
    /// Set by the UI when the user asks to sign in; handled in `update` because
    /// creating the webview needs the frame (window handle).
    pending_login: bool,
    /// Last window title we pushed, so we only update it on change.
    window_title: Option<String>,
    /// The window is resized to the screen once, on the first frame.
    sized: bool,
    page: Page,
    settings_tab: SettingsTab,
    store: FolderStore,
    upload: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    concurrency: usize,
    limit_enabled: bool,
    limit_kbps: u32,
    album_name: String,
    zone_override: String,
    was_active: bool,
    /// Open "Datensicherungseinstellungen" window (Create or Edit), if any.
    config_form: Option<BackupConfigForm>,
}

impl PrimePhotoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        // A native main menu is required for ⌘V/⌘C/⌘X to work in the webview.
        app_menu::install();
        let rt = Arc::new(Runtime::new().expect("tokio runtime"));
        let (mut config, config_error) = match Config::load() {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e.to_string())),
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
            limit_enabled: false,
            limit_kbps: 100,
            album_name: String::new(),
            zone_override: String::new(),
            was_active: false,
            config_form: None,
        }
    }

    fn log(&self, msg: impl Into<String>) {
        self.upload.lock().unwrap().log(msg);
    }

    fn is_logged_in(&self) -> bool {
        !self.config.session_id.is_empty() && !self.config.cookie_x_acb.is_empty()
    }

    fn save_config(&mut self) {
        if !self.zone_override.trim().is_empty() {
            self.config.zone = self.zone_override.trim().to_string();
        }
        match self.config.save() {
            Ok(_) => {
                self.config_error = None;
                self.log("Einstellungen gespeichert.");
            }
            Err(e) => self.config_error = Some(e.to_string()),
        }
    }

    // -- login --------------------------------------------------------------

    fn cancel_login(&mut self) {
        self.login.cancel();
    }

    /// Sign out: drop the stored cookies and return to the login screen.
    fn logout(&mut self) {
        self.config_form = None;
        self.config.session_id.clear();
        self.config.cookie_x_acb.clear();
        self.config.cookie_at_acb.clear();
        self.config.cookie_ubid_acb.clear();
        self.config.cookie_x_amz_access_token.clear();
        match self.config.save() {
            Ok(_) => self.log("Abgemeldet."),
            Err(e) => self.config_error = Some(e.to_string()),
        }
    }

    /// Create the login webview. Needs the frame for the window handle.
    fn begin_login(&mut self, frame: &eframe::Frame, ctx: &egui::Context) {
        if self.login.running {
            return;
        }
        let url = login::login_url(&self.config.country);
        let screen = ctx.screen_rect();
        // Start just below an estimated header; the exact bounds are corrected
        // on the next frame once the header has been measured.
        let bounds = login::rect(0.0, HEADER_H, screen.width(), screen.height() - HEADER_H);
        self.login.start(frame, &url, bounds);
    }

    fn poll_login(&mut self) {
        self.login.poll();
        let outcome = self.login.outcome.take();
        if let Some(cookie_string) = outcome {
            self.apply_cookie_string(&cookie_string);
            match self.config.save() {
                Ok(_) => self.log("Login erfolgreich – Cookies übernommen."),
                Err(e) => self.config_error = Some(e.to_string()),
            }
        }
    }

    fn apply_cookie_string(&mut self, cookie_string: &str) {
        let parsed = parse_cookie_string(cookie_string);
        self.config.session_id = parsed.session_id;
        if !parsed.country.is_empty() {
            self.config.country = parsed.country;
        }
        self.config.cookie_x_acb = parsed.cookie_x_acb;
        self.config.cookie_at_acb = parsed.cookie_at_acb;
        self.config.cookie_ubid_acb = parsed.cookie_ubid_acb;
        self.config.cookie_x_amz_access_token = parsed.cookie_x_amz_access_token;
    }

    // -- folders ------------------------------------------------------------

    fn add_backup_folder(&mut self) {
        if let Some(folder) = FileDialog::new().pick_folder() {
            // Open the shared "Datensicherungseinstellungen" window in Create
            // mode instead of adding + uploading right away.
            self.config_form = Some(BackupConfigForm {
                path: folder,
                dedup: true,
                conflict_resolution: ConflictResolution::Ignore,
                mode: ConfigFormMode::Create,
                error: None,
            });
        }
    }

    fn apply_action(&mut self, action: FolderAction) {
        match action {
            FolderAction::Upload(i) | FolderAction::Retry(i) => {
                self.start_folder_upload(vec![i]);
            }
            FolderAction::Remove(i) => {
                if i < self.store.folders.len() {
                    let f = self.store.folders.remove(i);
                    let _ = self.store.save();
                    self.log(format!("Datensicherung gestoppt: {}", f.name));
                }
            }
            FolderAction::OpenFinder(i) => {
                if let Some(f) = self.store.folders.get(i) {
                    let _ = std::process::Command::new("open")
                        .arg("-R")
                        .arg(&f.path)
                        .spawn();
                }
            }
            FolderAction::ToggleEnabled(i) => {
                if let Some(f) = self.store.folders.get_mut(i) {
                    f.enabled = !f.enabled;
                }
                let _ = self.store.save();
            }
            FolderAction::Edit(i) => {
                if let Some(f) = self.store.folders.get(i) {
                    let form = BackupConfigForm {
                        path: f.path.clone(),
                        dedup: f.dedup,
                        conflict_resolution: f.conflict_resolution,
                        mode: ConfigFormMode::Edit(f.path.clone()),
                        error: None,
                    };
                    self.config_form = Some(form);
                }
            }
        }
    }

    fn start_folder_upload(&mut self, indices: Vec<usize>) {
        if self.upload.lock().unwrap().active {
            self.log("Es läuft bereits ein Upload.");
            return;
        }
        if !self.is_logged_in() {
            self.log("Bitte zuerst anmelden (Einstellungen).");
            self.page = Page::Settings;
            return;
        }

        let mut items: Vec<(usize, PathBuf)> = Vec::new();
        let mut per_folder: HashMap<usize, usize> = HashMap::new();
        let mut folder_opts: HashMap<usize, (bool, ConflictResolution)> = HashMap::new();
        for &i in &indices {
            if let Some(f) = self.store.folders.get(i) {
                let imgs = folders::scan_images(&f.path);
                per_folder.insert(i, imgs.len());
                folder_opts.insert(i, (f.dedup, f.conflict_resolution));
                for p in imgs {
                    items.push((i, p));
                }
            }
        }
        if items.is_empty() {
            self.log("Keine Fotos oder Videos in diesem Ordner gefunden.");
            return;
        }

        {
            let mut s = self.upload.lock().unwrap();
            s.active = true;
            s.done = 0;
            s.failed = 0;
            s.current.clear();
            s.log.clear();
            s.folder_total = per_folder.clone();
            s.folder_done.clear();
            s.folder_failed.clear();
            s.uploaded_ids.clear();
            s.total = items.len();
            s.log(format!("{} Datei(en) werden hochgeladen …", items.len()));
        }
        self.paused.send_replace(false);
        self.was_active = true;

        for (&i, &count) in &per_folder {
            if let Some(f) = self.store.folders.get_mut(i) {
                f.status = FolderStatus::Uploading;
                f.total_files = count;
            }
        }
        let _ = self.store.save();

        let cfg = self.config.clone();
        let shared = self.upload.clone();
        let paused = self.paused.clone();
        let concurrency = self.concurrency;
        let limit = if self.limit_enabled {
            self.limit_kbps
        } else {
            0
        };
        let album = self.album_name.clone();
        self.rt.spawn(async move {
            run_upload(items, cfg, shared, paused, concurrency, limit, album, folder_opts).await;
        });
    }

    fn finalize_upload(&mut self) {
        let (folder_done, folder_failed) = {
            let s = self.upload.lock().unwrap();
            (s.folder_done.clone(), s.folder_failed.clone())
        };
        let now = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
        for (i, f) in self.store.folders.iter_mut().enumerate() {
            if let Some(&done) = folder_done.get(&i) {
                f.uploaded_files = done;
                f.last_updated = now.clone();
                f.status = if folder_failed.get(&i).copied().unwrap_or(0) > 0 {
                    FolderStatus::Error
                } else {
                    FolderStatus::Done
                };
            }
        }
        let _ = self.store.save();
    }

    // -- rendering ----------------------------------------------------------

    fn nav_item(ui: &mut egui::Ui, current: &mut Page, value: Page, label: &str, icon: NavIcon) {
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

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("Prime Photos").size(20.0).strong());
        ui.add_space(16.0);
        Self::nav_item(
            ui,
            &mut self.page,
            Page::Backup,
            "Datensicherung",
            NavIcon::Backup,
        );
        Self::nav_item(
            ui,
            &mut self.page,
            Page::Settings,
            "Einstellungen",
            NavIcon::Settings,
        );

        // Account block pinned to the bottom of the sidebar.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(10.0);
            if let Some(e) = &self.config_error {
                ui.label(RichText::new(e).color(theme::RED).size(11.0));
            }
            let logged_in = self.is_logged_in();
            if logged_in && ui.add(theme::secondary_button("Abmelden")).clicked() {
                self.logout();
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                status_dot(ui, if logged_in { theme::GREEN } else { theme::RED });
                ui.label(
                    RichText::new(if logged_in {
                        "Angemeldet"
                    } else {
                        "Nicht angemeldet"
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
                    ui.label(RichText::new("Angemeldet als:").size(11.0).color(theme::MUTED));
                    ui.label(RichText::new("Amazon Photos").size(13.0).strong());
                    ui.label(theme::muted(format!(
                        "amazon.{}",
                        login::amazon_domain(&self.config.country)
                    )));
                });
            });
            ui.add_space(8.0);
        });
    }

    /// Login phase (same window as the app, like the original).
    ///
    /// While Amazon's page is open we show a slim branded header and let the
    /// webview fill the rest of the window; before that we show our own
    /// sign-in screen with the marketplace picker.
    fn login_ui(&mut self, ctx: &egui::Context) {
        if self.login.running {
            let mut cancel = false;
            let header = egui::TopBottomPanel::top("login_header")
                .exact_height(HEADER_H)
                .frame(
                    egui::Frame::new()
                        .fill(theme::SIDEBAR_BG)
                        .inner_margin(egui::Margin::symmetric(16, 8)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        upload_logo(ui, 30.0);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Prime Photos").strong().size(15.0));
                            ui.label(theme::muted("Bei Amazon anmelden"));
                        });
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.add(theme::secondary_button("Abbrechen")).clicked() {
                                    cancel = true;
                                }
                                if let Some(e) = &self.login.error {
                                    ui.colored_label(theme::RED, e);
                                } else {
                                    ui.spinner();
                                }
                            },
                        );
                    });
                });
            let top_h = header.response.rect.height();
            let screen = ctx.screen_rect();
            self.login.set_bounds(login::rect(
                0.0,
                top_h,
                screen.width(),
                screen.height() - top_h,
            ));
            if cancel {
                self.cancel_login();
            }
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| self.login_screen_ui(ui));
    }

    /// Our own sign-in screen, shown before the Amazon page is opened.
    fn login_screen_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(56.0);
            upload_logo(ui, 84.0);
            ui.add_space(12.0);
            ui.label(RichText::new("Prime Photos").size(28.0).strong());
            ui.add_space(4.0);
            ui.label(theme::muted("Fotos & Videos zu Amazon Photos hochladen"));
            ui.add_space(26.0);

            // Marketplace selection before signing in.
            ui.horizontal(|ui| {
                ui.label("Land / Amazon-Marktplatz:");
                let current = self.config.country.clone();
                egui::ComboBox::from_id_salt("login-country")
                    .selected_text(country_label(&current))
                    .show_ui(ui, |ui| {
                        for (code, label) in COUNTRIES {
                            ui.selectable_value(&mut self.config.country, code.to_string(), *label);
                        }
                    });
            });
            ui.add_space(14.0);

            if ui.add(theme::primary_button("Mit Amazon anmelden")).clicked() {
                self.pending_login = true;
            }
            if let Some(e) = &self.login.error {
                ui.add_space(6.0);
                ui.colored_label(theme::RED, e);
            }

            ui.add_space(16.0);
            if ui.link("Amazon Photos im Browser öffnen").clicked() {
                let url = login::login_url(&self.config.country);
                let _ = std::process::Command::new("open").arg(url).spawn();
            }
        });
    }

    fn backup_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add(theme::primary_button("Datensicherung hinzufügen …"))
                .clicked()
            {
                self.add_backup_folder();
            }
            let all_paused = *self.paused.borrow();
            let label = if all_paused {
                "Alle Sicherungsvorgänge fortsetzen"
            } else {
                "Alle Sicherungsvorgänge anhalten"
            };
            let running = self.upload.lock().unwrap().active;
            if ui
                .add_enabled(
                    !self.store.folders.is_empty() && running,
                    theme::secondary_button(label),
                )
                .clicked()
            {
                self.paused.send_replace(!all_paused);
            }
            if running {
                ui.spinner();
            }
        });

        ui.add_space(12.0);
        ui.label(theme::muted("Automatisch gesicherte Ordner:"));

        if self.store.folders.is_empty() {
            ui.add_space(8.0);
            ui.label("Wählen Sie einen Ordner aus, um die Sicherung zu starten.");
            ui.add_space(6.0);
            ui.label(theme::muted(
                "Tipp: Ordner werden dauerhaft gemerkt und bei jedem Start wieder gesichert.",
            ));
        }

        // Snapshot rows for read-only rendering.
        let rows: Vec<Row> = {
            let s = self.upload.lock().unwrap();
            self.store
                .folders
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let total = s.folder_total.get(&i).copied().unwrap_or(f.total_files);
                    let done = s.folder_done.get(&i).copied().unwrap_or(f.uploaded_files);
                    Row {
                        idx: i,
                        name: f.name.clone(),
                        path: f.path.clone(),
                        enabled: f.enabled,
                        status: f.status,
                        total,
                        done,
                        last_updated: f.last_updated.clone(),
                    }
                })
                .collect()
        };

        let mut actions: Vec<FolderAction> = Vec::new();
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for r in &rows {
                    let pct = if r.total > 0 {
                        r.done as f32 / r.total as f32
                    } else {
                        0.0
                    };
                    let (status_txt, status_col) = status_text(r.status, pct, r.total);
                    egui::Frame::new()
                        .fill(theme::CARD_BG)
                        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .outer_margin(egui::Margin::symmetric(0, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&r.name).strong().size(15.0));
                                    ui.label(
                                        RichText::new(format!("{:.0}% hochgeladen", pct * 100.0))
                                            .size(12.0),
                                    );
                                    if !r.last_updated.is_empty() {
                                        ui.label(
                                            theme::muted(format!(
                                                "Zuletzt aktualisiert: {}",
                                                r.last_updated
                                            ))
                                            .size(11.0),
                                        );
                                    } else {
                                        ui.label(
                                            theme::muted(&r.path.display().to_string()).size(11.0),
                                        );
                                    }
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.menu_button("⋯", |ui| {
                                            if ui.button("Jetzt scannen und hochladen").clicked() {
                                                actions.push(FolderAction::Upload(r.idx));
                                                ui.close_menu();
                                            }
                                            if ui.button("Im Finder anzeigen").clicked() {
                                                actions.push(FolderAction::OpenFinder(r.idx));
                                                ui.close_menu();
                                            }
                                            if ui.button("Wiederholen").clicked() {
                                                actions.push(FolderAction::Retry(r.idx));
                                                ui.close_menu();
                                            }
                                            if ui.button("Bearbeiten").clicked() {
                                                actions.push(FolderAction::Edit(r.idx));
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button("Datensicherung stoppen").clicked() {
                                                actions.push(FolderAction::Remove(r.idx));
                                                ui.close_menu();
                                            }
                                        });
                                        let running = self.upload.lock().unwrap().active;
                                        if ui
                                            .add_enabled(
                                                running,
                                                egui::Button::new(
                                                    RichText::new(if r.enabled {
                                                        "⏸"
                                                    } else {
                                                        "▶"
                                                    })
                                                    .size(16.0),
                                                )
                                                .frame(false),
                                            )
                                            .clicked()
                                        {
                                            actions.push(FolderAction::ToggleEnabled(r.idx));
                                        }
                                        ui.label(
                                            RichText::new(status_txt).color(status_col).size(12.0),
                                        );
                                    },
                                );
                            });
                        });
                }
            });

        ui.add_space(8.0);

        for a in actions {
            self.apply_action(a);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.settings_tab, SettingsTab::General, "Allgemeines");
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Bandwidth, "Bandbreite");
        });
        ui.separator();

        match self.settings_tab {
            SettingsTab::General => self.general_settings(ui),
            SettingsTab::Bandwidth => self.bandwidth_settings(ui),
        }
    }

    fn general_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Anmeldung");
        ui.label(theme::muted(
            "Melde dich bei Amazon Photos an. Die Cookies werden automatisch übernommen.",
        ));
        ui.horizontal(|ui| {
            if ui.add(theme::primary_button("Mit Amazon anmelden")).clicked() {
                self.pending_login = true;
            }
            if self.login.running {
                ui.spinner();
            }
        });
        if let Some(e) = &self.login.error {
            ui.colored_label(theme::RED, e);
        }

        ui.separator();
        ui.heading("Upload");
        ui.horizontal(|ui| {
            ui.label("Album (optional):");
            ui.text_edit_singleline(&mut self.album_name);
        });

        ui.separator();
        ui.collapsing("Erweitert (Konto)", |ui| {
            ui.label(theme::muted(format!("Konfiguration: {}", self.config_path)));
            egui::Grid::new("cfg").num_columns(2).show(ui, |ui| {
                ui.label("Amazon-Marktplatz");
                let current = self.config.country.clone();
                egui::ComboBox::from_id_salt("settings-country")
                    .selected_text(country_label(&current))
                    .show_ui(ui, |ui| {
                        for (code, label) in COUNTRIES {
                            ui.selectable_value(&mut self.config.country, code.to_string(), *label);
                        }
                    });
                ui.end_row();
                ui.label("Zone");
                ui.text_edit_singleline(&mut self.config.zone);
                ui.end_row();
                ui.label("User-Agent");
                ui.text_edit_singleline(&mut self.config.user_agent);
                ui.end_row();
            });
        });

        ui.add_space(10.0);
        if ui.add(theme::primary_button("Speichern")).clicked() {
            self.save_config();
        }
    }

    fn bandwidth_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Uploads");
        ui.add(
            egui::Slider::new(&mut self.concurrency, 1..=8)
                .text("Gleichzeitige Uploads")
                .integer(),
        );
        ui.add_space(8.0);
        ui.label("Upload-Rate pro Datei");
        ui.radio_value(&mut self.limit_enabled, false, "Nicht begrenzen");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.limit_enabled, true, "Begrenzen auf");
            ui.add_enabled(
                self.limit_enabled,
                egui::DragValue::new(&mut self.limit_kbps).speed(10.0),
            );
            ui.label("KB/Sekunde");
        });
        ui.add_space(6.0);
        ui.label(theme::muted(
            "Hinweis: Die Begrenzung wird pro Datei nach dem Upload angewendet.",
        ));

        ui.separator();
        ui.heading("Downloads");
        ui.label(theme::muted(
            "Downloads sind in diesem Port noch nicht implementiert.",
        ));

        ui.add_space(10.0);
        if ui.add(theme::primary_button("Speichern")).clicked() {
            self.save_config();
        }
    }

    /// The single shared form body for the "Datensicherungseinstellungen"
    /// window. Drawn once for both Create and Edit; returns the user's action.
    /// Functional fields: source folder (read-only when `editing`), dedup and the
    /// name-conflict policy. Destination, content filter and upload interval have
    /// no backend yet, so they are shown disabled with a hover hint.
    fn render_backup_config(
        ui: &mut egui::Ui,
        form: &mut BackupConfigForm,
        editing: bool,
    ) -> Option<ConfigFormAction> {
        const UNAVAILABLE: &str = "Noch nicht verfügbar";
        let mut action = None;

        ui.label(RichText::new("Datensicherung").size(16.0).strong());
        ui.add_space(10.0);

        // Source folder. Read-only when editing an existing backup
        // (original `CanChangeSource`); "Ändern …" only in Create mode.
        ui.label("Gesicherter Ordner:");
        ui.horizontal(|ui| {
            ui.label(theme::muted(form.path.display().to_string()));
            let change = ui.add_enabled(!editing, theme::secondary_button("Ändern …"));
            if change.clicked() {
                if let Some(p) = FileDialog::new().pick_folder() {
                    form.path = p;
                    form.error = None;
                }
            }
        });
        ui.add_space(10.0);

        // Destination (not implemented yet).
        ui.label("Ziel für Datensicherung:");
        ui.horizontal(|ui| {
            ui.add_enabled(false, theme::secondary_button("Ändern …"))
                .on_disabled_hover_text(UNAVAILABLE);
        });
        ui.add_space(10.0);

        // Content filter (not implemented yet).
        ui.label("Was soll gesichert werden:");
        ui.horizontal(|ui| {
            for (i, label) in ["Alles", "Fotos und Videos", "Fotos"].iter().enumerate() {
                ui.add_enabled(false, egui::Button::new(*label).selected(i == 0))
                    .on_disabled_hover_text(UNAVAILABLE);
            }
        });
        ui.add_space(10.0);

        // Upload interval (not implemented yet).
        ui.label("Änderungen hochladen:");
        ui.horizontal(|ui| {
            ui.add_enabled_ui(false, |ui| {
                egui::ComboBox::from_id_salt("backup-config-interval")
                    .selected_text("Sofort")
                    .show_ui(ui, |ui| {
                        ui.label("Sofort");
                    });
            })
            .response
            .on_disabled_hover_text(UNAVAILABLE);
        });
        ui.add_space(10.0);

        // Weekly schedule (inert; no backend yet) — §4.5 "… aktiviert" + "um".
        ui.horizontal(|ui| {
            let mut weekly = true;
            let mut time = "14:00".to_string();
            ui.add_enabled(
                false,
                egui::Checkbox::new(&mut weekly, "Wöchentlich aktiviert"),
            )
            .on_disabled_hover_text(UNAVAILABLE);
            ui.label("um");
            ui.add_enabled(
                false,
                egui::TextEdit::singleline(&mut time).desired_width(64.0),
            )
            .on_disabled_hover_text(UNAVAILABLE);
        });
        ui.add_space(10.0);

        // Functional: dedup.
        ui.checkbox(
            &mut form.dedup,
            "Duplikate vermeiden: Keine Fotos und Videos sichern, die sich bereits in meinem Konto befinden",
        );
        ui.add_space(10.0);

        // Functional: name-conflict policy.
        ui.label("Umgang mit Dateinamenkonflikten:");
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Ask,
            "Jedes Mal nachfragen",
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Ignore,
            "Ignorieren",
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::SaveAsNew,
            "Als neue Dateien speichern",
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Replace,
            "Ersetzen",
        );

        if let Some(err) = &form.error {
            ui.add_space(6.0);
            ui.colored_label(theme::RED, err);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.add(theme::primary_button("Speichern")).clicked() {
                action = Some(ConfigFormAction::Save);
            }
            if ui.add(theme::secondary_button("Abbrechen")).clicked() {
                action = Some(ConfigFormAction::Cancel);
            }
        });

        action
    }

    /// Applies a Save/Cancel from the configuration window. Validation failures
    /// keep the window open with an inline error; a valid Save closes it.
    fn handle_config_form_action(&mut self, action: ConfigFormAction) {
        match action {
            ConfigFormAction::Cancel => self.config_form = None,
            ConfigFormAction::Save => {
                let Some((mode, path, dedup, conflict)) =
                    self.config_form.as_ref().map(|f| {
                        (f.mode.clone(), f.path.clone(), f.dedup, f.conflict_resolution)
                    })
                else {
                    return;
                };

                if !path.exists() {
                    if let Some(f) = self.config_form.as_mut() {
                        f.error = Some("Lokaler Pfad ist nicht verfügbar".to_string());
                    }
                    return;
                }

                match mode {
                    ConfigFormMode::Create => {
                        if self.store.folders.iter().any(|f| f.path == path) {
                            if let Some(f) = self.config_form.as_mut() {
                                f.error = Some(
                                    "Datensicherung für lokalen Pfad existiert bereits"
                                        .to_string(),
                                );
                            }
                            return;
                        }
                        self.store.add(path.clone());
                        let idx = self.store.folders.len() - 1;
                        if let Some(f) = self.store.folders.get_mut(idx) {
                            f.dedup = dedup;
                            f.conflict_resolution = conflict;
                        }
                        let _ = self.store.save();
                        self.log(format!("Ordner hinzugefügt: {}", path.display()));
                        self.config_form = None;
                        self.start_folder_upload(vec![idx]);
                    }
                    ConfigFormMode::Edit(edit_path) => {
                        match self
                            .store
                            .folders
                            .iter()
                            .position(|f| f.path == edit_path)
                        {
                            Some(idx) => {
                                if let Some(f) = self.store.folders.get_mut(idx) {
                                    f.dedup = dedup;
                                    f.conflict_resolution = conflict;
                                }
                                let _ = self.store.save();
                                self.config_form = None;
                            }
                            None => {
                                if let Some(f) = self.config_form.as_mut() {
                                    f.error =
                                        Some("Ordner ist nicht mehr vorhanden".to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn status_text(status: FolderStatus, pct: f32, total: usize) -> (String, Color32) {
    match status {
        FolderStatus::Uploading => (format!("{:.0}% hochgeladen", pct * 100.0), theme::TEAL),
        FolderStatus::Done => ("100% hochgeladen".to_string(), theme::GREEN),
        FolderStatus::Paused => (format!("Angehalten ({:.0}%)", pct * 100.0), theme::AMBER),
        FolderStatus::Error => ("Fehler".to_string(), theme::RED),
        FolderStatus::Scanning => ("Wird vorbereitet …".to_string(), theme::MUTED),
        FolderStatus::Idle => {
            if total > 0 {
                (format!("{:.0}% hochgeladen", pct * 100.0), theme::MUTED)
            } else {
                (String::new(), theme::MUTED)
            }
        }
    }
}

impl eframe::App for PrimePhotoApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
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
                    format!("Upload läuft: {}/{} · {}", s.done, s.total, s.current)
                } else if s.total > 0 {
                    format!(
                        "Letzter Upload: {}/{} erfolgreich",
                        s.done,
                        s.total - s.failed
                    )
                } else {
                    "Bereit".to_string()
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
                    self.was_active = false;
                }
            }

            // Shared "Datensicherungseinstellungen" window (Create/Edit).
            let mut form_action: Option<ConfigFormAction> = None;
            if let Some(form) = self.config_form.as_mut() {
                let editing = matches!(form.mode, ConfigFormMode::Edit(_));
                egui::Window::new("Datensicherungseinstellungen")
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
            "Login – Prime Photos"
        } else {
            "Prime Photos"
        };
        if self.window_title.as_deref() != Some(title) {
            self.window_title = Some(title.to_owned());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_owned()));
        }
    }
}

// ---------------------------------------------------------------------------
// Async upload machinery
// ---------------------------------------------------------------------------

async fn upload_worker(
    queue: Arc<Mutex<VecDeque<(usize, PathBuf)>>>,
    shared: Arc<Mutex<UploadShared>>,
    cfg: Config,
    mut pause_rx: watch::Receiver<bool>,
    limit_kbps: u32,
    dedup: Arc<Mutex<DedupIndex>>,
    folder_opts: Arc<HashMap<usize, (bool, ConflictResolution)>>,
) {
    let mut uploader = PrimePhotos::new(&cfg, false);
    loop {
        while *pause_rx.borrow_and_update() {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        let item = { queue.lock().unwrap().pop_front() };
        let Some((idx, path)) = item else { break };

        let (dedup_on, conflict) = folder_opts
            .get(&idx)
            .copied()
            .unwrap_or((true, ConflictResolution::Ignore));

        // Read the file once and compute the MD5 up front: the MD5 is the
        // duplicate identity (docs/SPEC_DEDUP.md §1) and the bytes are handed to
        // upload_picture so the file is never read twice.
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                let mut s = shared.lock().unwrap();
                s.failed += 1;
                *s.folder_failed.entry(idx).or_insert(0) += 1;
                s.log(format!("FEHLER {}: {}", path.display(), e));
                continue;
            }
        };
        let md5sum = format!("{:x}", md5::compute(&bytes));
        let size = bytes.len() as u64;
        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        {
            let mut s = shared.lock().unwrap();
            s.current = path.display().to_string();
        }

        // Local pre-upload check (path + md5). A skip is a finished item: it is
        // counted as done for this folder and consumes no network and no throttle.
        if dedup_on && dedup.lock().unwrap().should_skip(&path, &md5sum, size, mtime) {
            let mut s = shared.lock().unwrap();
            s.done += 1;
            *s.folder_done.entry(idx).or_insert(0) += 1;
            s.log(format!("UEBERSPRUNGEN {}: bereits hochgeladen", path.display()));
            continue;
        }

        let started = Instant::now();

        let upload_fut = uploader.upload_picture(&path, Some(&bytes), Some(&md5sum), conflict);
        tokio::pin!(upload_fut);
        let mut requeued = false;
        tokio::select! {
            biased;
            changed = pause_rx.changed() => {
                let _ = changed;
                queue.lock().unwrap().push_front((idx, path.clone()));
                requeued = true;
            }
            res = &mut upload_fut => {
                match res {
                    Ok(id) => {
                        // Remember the upload (also for the server-side 409 case,
                        // which upload_picture returns as Ok) so the next scan skips it.
                        dedup.lock().unwrap().record_upload(&path, &md5sum, size, mtime);
                        let mut s = shared.lock().unwrap();
                        s.done += 1;
                        *s.folder_done.entry(idx).or_insert(0) += 1;
                        s.uploaded_ids.push(id);
                        s.log(format!("OK    {}", path.display()));
                    }
                    Err(e) => {
                        let mut s = shared.lock().unwrap();
                        s.failed += 1;
                        *s.folder_failed.entry(idx).or_insert(0) += 1;
                        s.log(format!("FEHLER {}: {}", path.display(), e));
                    }
                }
            }
        }
        if requeued {
            continue;
        }

        if limit_kbps > 0 && size > 0 {
            let target = Duration::from_secs_f64(size as f64 / (limit_kbps as f64 * 1024.0));
            let elapsed = started.elapsed();
            if target > elapsed {
                // F-2 fix (AGENTS.todo.md, P1 residue): enforce the per-file
                // pacing *across* a pause/resume boundary. Previously this was
                // one big sleep raced against pause_rx.changed(), so a pause
                // dropped the outstanding throttle and the next file uploaded
                // immediately (effective limit exceeded). Instead sleep in
                // short slices: park while paused, otherwise consume each
                // slice's actually-elapsed time from the remaining budget, and
                // stop only once it is fully spent — so resuming continues the
                // throttle rather than skipping it.
                let slice = Duration::from_millis(200);
                let mut remaining = target - elapsed;
                while remaining > Duration::ZERO {
                    while *pause_rx.borrow_and_update() {
                        tokio::time::sleep(slice).await;
                    }
                    let step = remaining.min(slice);
                    let before = Instant::now();
                    tokio::time::sleep(step).await;
                    remaining = remaining.saturating_sub(before.elapsed());
                }
            }
        }
    }
}

async fn run_upload(
    items: Vec<(usize, PathBuf)>,
    cfg: Config,
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
    folder_opts: HashMap<usize, (bool, ConflictResolution)>,
) {
    let count = items.len();
    let queue = Arc::new(Mutex::new(VecDeque::from(items)));
    let workers = concurrency.max(1).min(count.max(1));

    // One shared persistent MD5 index for the whole run (opened here, lives in
    // ~/Library/Application Support/prime_photos/dedup.sqlite3).
    let dedup = Arc::new(Mutex::new(DedupIndex::open()));
    let folder_opts = Arc::new(folder_opts);

    let mut set = JoinSet::new();
    for _ in 0..workers {
        let q = queue.clone();
        let s = shared.clone();
        let c = cfg.clone();
        let p = paused.subscribe();
        let d = dedup.clone();
        let o = folder_opts.clone();
        set.spawn(async move {
            upload_worker(q, s, c, p, limit_kbps, d, o).await;
        });
    }
    while set.join_next().await.is_some() {}

    let name = album.trim().to_string();
    if !name.is_empty() {
        let ids = { shared.lock().unwrap().uploaded_ids.clone() };
        if !ids.is_empty() {
            let mut up = PrimePhotos::new(&cfg, false);
            if let Ok(c) = up.get_album(&name).await {
                let _ = up.add_to_album(c.id, &ids).await;
            }
        }
    }

    let (done, failed) = {
        let mut s = shared.lock().unwrap();
        s.active = false;
        s.current.clear();
        (s.done, s.failed)
    };
    shared.lock().unwrap().log(format!(
        "Fertig: {} erfolgreich, {} fehlgeschlagen.",
        done, failed
    ));
}

// ---------------------------------------------------------------------------
// Cookie parsing (manual fallback)
// ---------------------------------------------------------------------------

struct ParsedCookies {
    session_id: String,
    country: String,
    cookie_x_acb: String,
    cookie_at_acb: String,
    cookie_ubid_acb: String,
    cookie_x_amz_access_token: String,
}

fn parse_cookie_string(input: &str) -> ParsedCookies {
    let mut result = ParsedCookies {
        session_id: String::new(),
        country: String::new(),
        cookie_x_acb: String::new(),
        cookie_at_acb: String::new(),
        cookie_ubid_acb: String::new(),
        cookie_x_amz_access_token: String::new(),
    };
    let mut detected_country = String::new();
    for part in input.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((name, value)) = part.split_once('=') {
            let name = name.trim();
            let value = value.trim().trim_matches('"');
            match name {
                "session-id" => result.session_id = value.to_string(),
                n if n.starts_with("x-acb") && n.len() > 5 => {
                    if result.cookie_x_acb.is_empty() {
                        result.cookie_x_acb = value.to_string();
                    }
                    if detected_country.is_empty() {
                        detected_country = n["x-acb".len()..].to_lowercase();
                    }
                }
                n if n.starts_with("at-acb") => {
                    if result.cookie_at_acb.is_empty() {
                        result.cookie_at_acb = value.to_string();
                    }
                    if detected_country.is_empty() {
                        detected_country = n["at-acb".len()..].to_lowercase();
                    }
                }
                n if n.starts_with("ubid-acb") => {
                    if result.cookie_ubid_acb.is_empty() {
                        result.cookie_ubid_acb = value.to_string();
                    }
                }
                n if n.starts_with("sess-at-acb") => {
                    if result.cookie_x_amz_access_token.is_empty() {
                        result.cookie_x_amz_access_token = value.to_string();
                    }
                }
                _ => {}
            }
        }
    }
    result.country = detected_country;
    result
}

// ---------------------------------------------------------------------------
// Country / marketplace selection
// ---------------------------------------------------------------------------

const COUNTRIES: &[(&str, &str)] = &[
    ("de", "Deutschland · amazon.de"),
    ("at", "Österreich · amazon.de"),
    ("ch", "Schweiz · amazon.de"),
    ("us", "USA · amazon.com"),
    ("uk", "Vereinigtes Königreich · amazon.co.uk"),
    ("fr", "Frankreich · amazon.fr"),
    ("it", "Italien · amazon.it"),
    ("es", "Spanien · amazon.es"),
    ("ca", "Kanada · amazon.ca"),
    ("jp", "Japan · amazon.co.jp"),
    ("in", "Indien · amazon.in"),
    ("au", "Australien · amazon.com.au"),
    ("br", "Brasilien · amazon.com.br"),
    ("mx", "Mexiko · amazon.com.mx"),
];

fn country_label(code: &str) -> String {
    COUNTRIES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, l)| l.to_string())
        .unwrap_or_else(|| code.to_string())
}

/// Best-effort country from the system locale (macOS `AppleLocale`, then `LANG`).
fn system_country() -> Option<String> {
    if let Ok(out) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(region) = s.split(['_', '-']).nth(1) {
                let code = region.to_lowercase();
                if COUNTRIES.iter().any(|(c, _)| *c == code) {
                    return Some(code);
                }
            }
        }
    }
    if let Ok(lang) = std::env::var("LANG") {
        if let Some(region) = lang.split(['_', '.', '-']).nth(1) {
            let code = region.to_lowercase();
            if COUNTRIES.iter().any(|(c, _)| *c == code) {
                return Some(code);
            }
        }
    }
    None
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Prime Photos",
        options,
        Box::new(|cc| Ok(Box::new(PrimePhotoApp::new(cc)))),
    )
}

/// Small line icons for the sidebar navigation.
#[derive(Clone, Copy)]
enum NavIcon {
    Backup,
    Settings,
}

/// Draws a small, crisp line icon that aligns to the fixed icon column.
fn nav_icon(painter: &egui::Painter, kind: NavIcon, rect: egui::Rect, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let stroke = egui::Stroke::new(1.6, color);
    match kind {
        NavIcon::Backup => {
            // Upload arrow into a tray.
            let top = c.y - s * 0.30;
            let mid = c.y + s * 0.02;
            painter.line_segment([egui::pos2(c.x, mid), egui::pos2(c.x, top)], stroke);
            painter.line_segment(
                [egui::pos2(c.x, top), egui::pos2(c.x - s * 0.17, top + s * 0.17)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x, top), egui::pos2(c.x + s * 0.17, top + s * 0.17)],
                stroke,
            );
            let ty = c.y + s * 0.34;
            painter.line_segment(
                [egui::pos2(c.x - s * 0.32, ty - s * 0.14), egui::pos2(c.x - s * 0.32, ty)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x + s * 0.32, ty - s * 0.14), egui::pos2(c.x + s * 0.32, ty)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - s * 0.32, ty), egui::pos2(c.x + s * 0.32, ty)],
                stroke,
            );
        }
        NavIcon::Settings => {
            // Three sliders.
            for (i, kx) in [-0.18f32, 0.12, -0.06].iter().enumerate() {
                let y = c.y + (i as f32 - 1.0) * s * 0.30;
                painter.line_segment(
                    [egui::pos2(c.x - s * 0.34, y), egui::pos2(c.x + s * 0.34, y)],
                    stroke,
                );
                painter.circle_filled(egui::pos2(c.x + kx * s, y), s * 0.10, color);
            }
        }
    }
}

/// A filled status dot (replaces the "●" glyph, which rendered as a tofu box).
fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// A simple avatar placeholder (head + shoulders inside a filled circle).
fn draw_avatar(ui: &mut egui::Ui, diameter: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(diameter, diameter), egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    let c = rect.center();
    painter.circle_filled(c, diameter / 2.0, color);
    let white = Color32::WHITE;
    painter.circle_filled(egui::pos2(c.x, c.y - diameter * 0.14), diameter * 0.16, white);
    painter.circle_filled(egui::pos2(c.x, c.y + diameter * 0.32), diameter * 0.28, white);
}

/// Our own upload logo: a teal circle with a white upload arrow and tray.
fn upload_logo(ui: &mut egui::Ui, diameter: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(diameter, diameter), egui::Sense::hover());
    let painter = ui.painter();
    let c = rect.center();
    painter.circle_filled(c, diameter / 2.0, theme::TEAL);
    let white = Color32::WHITE;
    // Geometry scaled from the 84 pt reference size.
    let s = diameter / 84.0;
    let w = 4.0 * s;
    let cx = c.x;
    let arrow_top = c.y - 16.0 * s;
    let arrow_bottom = c.y + 6.0 * s;
    painter.line_segment(
        [egui::pos2(cx, arrow_bottom), egui::pos2(cx, arrow_top)],
        egui::Stroke::new(w, white),
    );
    painter.line_segment(
        [
            egui::pos2(cx, arrow_top),
            egui::pos2(cx - 11.0 * s, arrow_top + 11.0 * s),
        ],
        egui::Stroke::new(w, white),
    );
    painter.line_segment(
        [
            egui::pos2(cx, arrow_top),
            egui::pos2(cx + 11.0 * s, arrow_top + 11.0 * s),
        ],
        egui::Stroke::new(w, white),
    );
    painter.line_segment(
        [
            egui::pos2(cx - 18.0 * s, c.y + 19.0 * s),
            egui::pos2(cx + 18.0 * s, c.y + 19.0 * s),
        ],
        egui::Stroke::new(w, white),
    );
}
