mod amzn_photo;
mod config;
mod folders;
mod login;
mod theme;

use amzn_photo::AmznPhoto;
use config::Config;
use eframe::egui::{self, Color32, RichText};
use folders::{FolderStatus, FolderStore};
use rfd::FileDialog;
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use tokio::task::JoinSet;

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
    login: Arc<Mutex<login::WebviewLogin>>,
    page: Page,
    settings_tab: SettingsTab,
    store: FolderStore,
    upload: Arc<Mutex<UploadShared>>,
    paused: Arc<AtomicBool>,
    concurrency: usize,
    limit_enabled: bool,
    limit_kbps: u32,
    album_name: String,
    zone_override: String,
    was_active: bool,
}

impl PrimePhotoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
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
            login: Arc::new(Mutex::new(login::WebviewLogin::new())),
            page: Page::Backup,
            settings_tab: SettingsTab::General,
            store: FolderStore::load(),
            upload: Arc::new(Mutex::new(UploadShared::default())),
            paused: Arc::new(AtomicBool::new(false)),
            concurrency: 3,
            limit_enabled: false,
            limit_kbps: 100,
            album_name: String::new(),
            zone_override: String::new(),
            was_active: false,
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

    fn start_login(&mut self) {
        let url = login::login_url(&self.config.country);
        self.login.lock().unwrap().start(url);
    }

    fn cancel_login(&mut self) {
        self.login.lock().unwrap().cancel();
    }

    fn poll_login(&mut self) {
        {
            let mut l = self.login.lock().unwrap();
            l.poll();
        }
        let outcome = { self.login.lock().unwrap().outcome.take() };
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
            if self.store.add(folder.clone()) {
                let idx = self.store.folders.len() - 1;
                let _ = self.store.save();
                self.log(format!("Ordner hinzugefügt: {}", folder.display()));
                self.start_folder_upload(vec![idx]);
            } else {
                self.log("Ordner ist bereits in der Liste.");
            }
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
        for &i in &indices {
            if let Some(f) = self.store.folders.get(i) {
                let imgs = folders::scan_images(&f.path);
                per_folder.insert(i, imgs.len());
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
        self.paused.store(false, Ordering::Relaxed);
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
            run_upload(items, cfg, shared, paused, concurrency, limit, album).await;
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

    fn nav_item(ui: &mut egui::Ui, current: &mut Page, value: Page, label: &str) {
        let selected = *current == value;
        let text = RichText::new(label).size(15.0);
        let resp = ui.add_sized(
            [ui.available_width(), 34.0],
            egui::SelectableLabel::new(selected, text),
        );
        if resp.clicked() {
            *current = value;
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("amazon photos").size(20.0).strong());
        ui.add_space(16.0);
        Self::nav_item(ui, &mut self.page, Page::Backup, "☁   Datensicherung");
        Self::nav_item(ui, &mut self.page, Page::Settings, "⚙   Einstellungen");

        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(8.0);
            let logged_in = self.is_logged_in();
            let (txt, col) = if logged_in {
                ("● Angemeldet", theme::GREEN)
            } else {
                ("● Nicht angemeldet", theme::RED)
            };
            ui.label(RichText::new(txt).color(col).size(12.0));
            if let Some(e) = &self.config_error {
                ui.label(RichText::new(e).color(theme::RED).size(11.0));
            }
        });
    }

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(70.0);
            ui.label(RichText::new("amazon photos").size(30.0).strong());
            ui.add_space(6.0);
            ui.label(RichText::new("Prime Photo Uploader").size(16.0));
            ui.add_space(26.0);
            ui.label(
                RichText::new(
                    "Laden Sie Fotos und Videos von Ihrem Computer, Ihrer Kamera und Ihrer Speicherkarte hoch.",
                )
                .size(15.0),
            );
            ui.add_space(22.0);

            let mut login_started = false;
            let mut login_cancelled = false;

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
            ui.label(theme::muted(format!(
                "Anmeldung öffnet {}.",
                login::login_url(&self.config.country)
            )));
            ui.add_space(14.0);

            {
                let l = self.login.lock().unwrap();
                if ui
                    .add_enabled(!l.running, theme::primary_button("Mit Browser anmelden"))
                    .clicked()
                {
                    login_started = true;
                }
                if l.running {
                    ui.add_space(10.0);
                    ui.spinner();
                    if !l.message.is_empty() {
                        ui.label(&l.message);
                    }
                    ui.add_space(6.0);
                    if ui.add(theme::secondary_button("Abbrechen")).clicked() {
                        login_cancelled = true;
                    }
                }
                if let Some(e) = &l.error {
                    ui.add_space(6.0);
                    ui.colored_label(theme::RED, e);
                }
            }
            if login_started {
                let _ = self.config.save();
                self.start_login();
            }
            if login_cancelled {
                self.cancel_login();
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
            let all_paused = self.paused.load(Ordering::Relaxed);
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
                self.paused.store(!all_paused, Ordering::Relaxed);
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
        let mut login_started = false;
        let mut login_cancelled = false;
        {
            let l = self.login.lock().unwrap();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!l.running, theme::primary_button("Mit Browser anmelden"))
                    .clicked()
                {
                    login_started = true;
                }
                if ui
                    .add_enabled(l.running, theme::secondary_button("Abbrechen"))
                    .clicked()
                {
                    login_cancelled = true;
                }
                if l.running {
                    ui.spinner();
                }
            });
            if !l.message.is_empty() {
                ui.label(&l.message);
            }
            if let Some(e) = &l.error {
                ui.colored_label(theme::RED, e);
            }
        }
        if login_started {
            self.start_login();
        }
        if login_cancelled {
            self.cancel_login();
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_login();

        // Onboarding: until the user is signed in, show the login flow only.
        if !self.is_logged_in() {
            egui::CentralPanel::default().show(ctx, |ui| self.login_ui(ui));
            let running = self.login.lock().unwrap().running;
            if running {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
            return;
        }

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
    }
}

// ---------------------------------------------------------------------------
// Async upload machinery
// ---------------------------------------------------------------------------

async fn upload_worker(
    queue: Arc<Mutex<VecDeque<(usize, PathBuf)>>>,
    shared: Arc<Mutex<UploadShared>>,
    cfg: Config,
    paused: Arc<AtomicBool>,
    limit_kbps: u32,
) {
    let mut uploader = AmznPhoto::new(&cfg, false);
    loop {
        while paused.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        let item = { queue.lock().unwrap().pop_front() };
        let Some((idx, path)) = item else { break };

        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        {
            let mut s = shared.lock().unwrap();
            s.current = path.display().to_string();
        }
        let started = Instant::now();
        match uploader.upload_picture(&path, None, None).await {
            Ok(id) => {
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

        if limit_kbps > 0 && size > 0 {
            let target = Duration::from_secs_f64(size as f64 / (limit_kbps as f64 * 1024.0));
            let elapsed = started.elapsed();
            if target > elapsed {
                tokio::time::sleep(target - elapsed).await;
            }
        }
    }
}

async fn run_upload(
    items: Vec<(usize, PathBuf)>,
    cfg: Config,
    shared: Arc<Mutex<UploadShared>>,
    paused: Arc<AtomicBool>,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
) {
    let count = items.len();
    let queue = Arc::new(Mutex::new(VecDeque::from(items)));
    let workers = concurrency.max(1).min(count.max(1));

    let mut set = JoinSet::new();
    for _ in 0..workers {
        let q = queue.clone();
        let s = shared.clone();
        let c = cfg.clone();
        let p = paused.clone();
        set.spawn(async move {
            upload_worker(q, s, c, p, limit_kbps).await;
        });
    }
    while set.join_next().await.is_some() {}

    let name = album.trim().to_string();
    if !name.is_empty() {
        let ids = { shared.lock().unwrap().uploaded_ids.clone() };
        if !ids.is_empty() {
            let mut up = AmznPhoto::new(&cfg, false);
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
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Prime Photo Uploader",
        options,
        Box::new(|cc| Ok(Box::new(PrimePhotoApp::new(cc)))),
    )
}
