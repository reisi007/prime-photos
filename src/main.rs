mod amzn_photo;
mod config;
mod login_helper;
mod theme;

use amzn_photo::AmznPhoto;
use config::Config;
use eframe::egui;
use rfd::FileDialog;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use tokio::runtime::Runtime;

#[derive(Clone)]
struct UploadProgress {
    total: usize,
    done: usize,
    current: String,
    finished: bool,
    lines: Vec<String>,
}

impl UploadProgress {
    fn new() -> Self {
        Self {
            total: 0,
            done: 0,
            current: String::new(),
            finished: false,
            lines: Vec::new(),
        }
    }
    fn log(&mut self, msg: String) {
        self.lines.push(msg);
        if self.lines.len() > 200 {
            self.lines.drain(0..1);
        }
    }
}

struct PrimePhotoApp {
    rt: std::sync::Arc<Runtime>,
    config: Config,
    config_path: String,
    config_error: Option<String>,
    album_name: String,
    zone_override: String,
    cookie_paste: String,
    files: Vec<PathBuf>,
    progress: Arc<Mutex<UploadProgress>>,
    uploading: Arc<Mutex<bool>>,
    login: Arc<Mutex<login_helper::BrowserLogin>>,
}

impl PrimePhotoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&_cc.egui_ctx);
        let rt = std::sync::Arc::new(Runtime::new().expect("tokio runtime"));
        let (config, config_error) = match Config::load() {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e.to_string())),
        };
        let config_path = Config::get_config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| e.to_string());
        let zone_override = String::new();
        let cookie_paste = String::new();
        Self {
            rt,
            config,
            config_path,
            config_error,
            album_name: String::new(),
            zone_override,
            cookie_paste,
            files: Vec::new(),
            progress: Arc::new(Mutex::new(UploadProgress::new())),
            uploading: Arc::new(Mutex::new(false)),
            login: Arc::new(Mutex::new(login_helper::BrowserLogin::new())),
        }
    }

    fn save_config(&mut self) {
        // If user gave zone override, use it
        if !self.zone_override.trim().is_empty() {
            self.config.zone = self.zone_override.trim().to_string();
        }
        match self.config.save() {
            Ok(_) => {
                self.config_error = None;
                let mut p = self.progress.lock().unwrap();
                p.log("Config saved.".to_string());
            }
            Err(e) => {
                self.config_error = Some(e.to_string());
            }
        }
    }

    fn start_login(&mut self) {
        {
            let l = self.login.lock().unwrap();
            if l.running {
                return;
            }
        }
        let country = self.config.country.clone();
        {
            let mut l = self.login.lock().unwrap();
            l.running = true;
            l.message.clear();
            l.outcome = None;
            l.error = None;
        }
        let login = self.login.clone();
        let rt = self.rt.clone();
        rt.spawn(async move {
            login_helper::run_browser_login(country, login).await;
        });
    }

    fn cancel_login(&mut self) {
        let mut l = self.login.lock().unwrap();
        if l.running {
            l.running = false;
            l.stop_browser();
        }
    }

    fn add_files(&mut self) {
        if let Some(paths) = FileDialog::new()
            .add_filter("Images", &["jpg", "jpeg", "png", "heic", "heif", "tif", "tiff", "gif", "webp", "bmp", "cr2", "cr3", "arw", "nef", "dng", "raf", "rw2", "orf", "srw", "pef", "3fr", "iiq", "erf", "mos", "kdc", "dcr", "mrw"])
            .pick_files()
        {
            for p in paths {
                if is_supported_image(&p) && !self.files.contains(&p) {
                    self.files.push(p);
                }
            }
        }
    }

    fn add_folder(&mut self) {
        if let Some(folder) = FileDialog::new().pick_folder() {
            for entry in walkdir::WalkDir::new(folder)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
            {
                let p = entry.path().to_path_buf();
                if is_supported_image(&p) && !self.files.contains(&p) {
                    self.files.push(p);
                }
            }
        }
    }

    fn start_upload(&mut self) {
        if *self.uploading.lock().unwrap() {
            return;
        }
        if self.files.is_empty() {
            let mut p = self.progress.lock().unwrap();
            p.log("No files selected.".to_string());
            return;
        }
        // reset progress
        {
            let mut p = self.progress.lock().unwrap();
            *p = UploadProgress::new();
            p.total = self.files.len();
            p.log(format!(
                "Starting upload of {} file(s)...",
                self.files.len()
            ));
        }
        *self.uploading.lock().unwrap() = true;

        let config = self.config.clone();
        let files = self.files.clone();
        let album_name = self.album_name.clone();
        let progress = self.progress.clone();
        let uploading = self.uploading.clone();
        let rt = &self.rt;

        rt.spawn(async move {
            let mut uploader = AmznPhoto::new(&config, false);
            let mut album_id = String::new();

            let name = album_name.trim().to_string();
            if !name.is_empty() {
                {
                    let mut p = progress.lock().unwrap();
                    p.log(format!("Resolving album '{}'...", name));
                }
                match uploader.get_album(&name).await {
                    Ok(c) => {
                        album_id = c.id.clone();
                        let mut p = progress.lock().unwrap();
                        p.log(format!("Album ready: {} ({})", c.name, c.id));
                    }
                    Err(e) => {
                        let mut p = progress.lock().unwrap();
                        p.log(format!("Album error: {:?}", e));
                        *uploading.lock().unwrap() = false;
                        return;
                    }
                }
            }

            let mut uploaded_ids: Vec<String> = Vec::new();
            for (idx, path) in files.iter().enumerate() {
                {
                    let mut p = progress.lock().unwrap();
                    p.current = path.display().to_string();
                    p.log(format!(
                        "[{}/{}] Uploading {}...",
                        idx + 1,
                        files.len(),
                        path.display()
                    ));
                }
                match uploader.upload_picture(path, None, None).await {
                    Ok(id) => {
                        uploaded_ids.push(id.clone());
                        {
                            let mut p = progress.lock().unwrap();
                            p.done += 1;
                            p.log(format!("OK -> {}", id));
                        }
                    }
                    Err(e) => {
                        let mut p = progress.lock().unwrap();
                        p.log(format!("FAILED: {}", e));
                    }
                }
            }

            if !uploaded_ids.is_empty() && !album_id.is_empty() {
                {
                    let mut p = progress.lock().unwrap();
                    p.log(format!("Adding {} file(s) to album...", uploaded_ids.len()));
                }
                let _ = uploader.add_to_album(album_id.clone(), &uploaded_ids).await;
                // ignore error logging
            }

            {
                let mut p = progress.lock().unwrap();
                p.finished = true;
                p.current.clear();
                p.log("Done.".to_string());
            }
            *uploading.lock().unwrap() = false;
        });
    }
}

impl eframe::App for PrimePhotoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Prime Photo Uploader");
            ui.label(format!("Config: {}", self.config_path));
            if let Some(e) = &self.config_error {
                ui.colored_label(egui::Color32::RED, format!("Config error: {}", e));
            }

            ui.separator();
            ui.label("Album name (optional):");
            ui.text_edit_singleline(&mut self.album_name);

            ui.separator();
            ui.label("Config fields (edit then Save):");
            egui::Grid::new("cfg").num_columns(2).show(ui, |ui| {
                ui.label("country");
                ui.text_edit_singleline(&mut self.config.country);
                ui.end_row();
                ui.label("zone");
                ui.text_edit_singleline(&mut self.config.zone);
                ui.end_row();
                ui.label("zone override");
                ui.text_edit_singleline(&mut self.zone_override);
                ui.end_row();
                ui.label("user_agent");
                ui.text_edit_singleline(&mut self.config.user_agent);
                ui.end_row();
                ui.label("session_id");
                ui.text_edit_singleline(&mut self.config.session_id);
                ui.end_row();
                ui.label("cookie_x_acb");
                ui.text_edit_singleline(&mut self.config.cookie_x_acb);
                ui.end_row();
                ui.label("cookie_at_acb");
                ui.text_edit_singleline(&mut self.config.cookie_at_acb);
                ui.end_row();
                ui.label("cookie_ubid_acb");
                ui.text_edit_singleline(&mut self.config.cookie_ubid_acb);
                ui.end_row();
                ui.label("cookie_x_amz_access_token");
                ui.text_edit_singleline(&mut self.config.cookie_x_amz_access_token);
                ui.end_row();
            });
            if ui.button("Save Config").clicked() {
                self.save_config();
            }

            ui.separator();
            ui.label("Automatischer Browser-Login:");
            let mut login_started = false;
            let mut login_cancelled = false;
            {
                let l = self.login.lock().unwrap();
                ui.horizontal(|ui| {
                    let enabled = !l.running;
                    if ui.add_enabled(enabled, egui::Button::new("Mit Browser anmelden")).clicked() {
                        login_started = true;
                    }
                    if ui.add_enabled(l.running, egui::Button::new("Abbrechen")).clicked() {
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
                    ui.colored_label(egui::Color32::RED, e);
                }
            }
            if login_started {
                self.start_login();
            }
            if login_cancelled {
                self.cancel_login();
            }

            // Handle a completed browser login.
            let outcome = {
                let mut l = self.login.lock().unwrap();
                l.outcome.take()
            };
            if let Some(outcome) = outcome {
                let cookie_string = outcome
                    .cookies
                    .iter()
                    .map(|(n, v)| format!("{}={}", n, v))
                    .collect::<Vec<_>>()
                    .join("; ");
                let parsed = parse_cookie_string(&cookie_string);
                self.config.session_id = parsed.session_id;
                if !parsed.country.is_empty() {
                    self.config.country = parsed.country;
                }
                self.config.cookie_x_acb = parsed.cookie_x_acb;
                self.config.cookie_at_acb = parsed.cookie_at_acb;
                self.config.cookie_ubid_acb = parsed.cookie_ubid_acb;
                self.config.cookie_x_amz_access_token = parsed.cookie_x_amz_access_token;
                match self.config.save() {
                    Ok(_) => {
                        let mut p = self.progress.lock().unwrap();
                        p.log(format!(
                            "Login erfolgreich ({}). Cookies übernommen und gespeichert.",
                            outcome.browser_name
                        ));
                    }
                    Err(e) => {
                        self.config_error = Some(e.to_string());
                    }
                }
            }

            ui.separator();
            ui.label("Manueller Login-Helper (Browser-Cookie-String einfügen):");
            ui.add(egui::TextEdit::multiline(&mut self.cookie_paste).desired_rows(3));
            ui.horizontal(|ui| {
                if ui.button("Parse cookies").clicked() {
                    let parsed = parse_cookie_string(&self.cookie_paste);
                    self.config.session_id = parsed.session_id;
                    if !parsed.country.is_empty() {
                        self.config.country = parsed.country;
                    }
                    self.config.cookie_x_acb = parsed.cookie_x_acb;
                    self.config.cookie_at_acb = parsed.cookie_at_acb;
                    self.config.cookie_ubid_acb = parsed.cookie_ubid_acb;
                    self.config.cookie_x_amz_access_token = parsed.cookie_x_amz_access_token;
                    let mut p = self.progress.lock().unwrap();
                    p.log("Cookies parsed into config fields. Press Save Config.".to_string());
                }
                if ui.button("Open Amazon Photos in browser").clicked() {
                    let url = "https://www.amazon.com/photos".to_string();
                    let _ = std::process::Command::new("open").arg(url).spawn();
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Add Files").clicked() {
                    self.add_files();
                }
                if ui.button("Add Folder").clicked() {
                    self.add_folder();
                }
                if ui.button("Clear").clicked() {
                    self.files.clear();
                }
            });
            ui.label(format!("Selected files: {}", self.files.len()));
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .show(ui, |ui| {
                    for f in &self.files {
                        ui.label(f.display().to_string());
                    }
                });

            let uploading = *self.uploading.lock().unwrap();
            ui.separator();
            ui.horizontal(|ui| {
                let enabled = !uploading && !self.files.is_empty();
                if ui
                    .add_enabled(enabled, egui::Button::new("Upload"))
                    .clicked()
                {
                    self.start_upload();
                }
                if uploading {
                    ui.spinner();
                }
            });

            // progress
            let prog = self.progress.lock().unwrap();
            let total = prog.total;
            let done = prog.done;
            let fraction = if total > 0 {
                done as f32 / total as f32
            } else {
                0.0
            };
            ui.add(egui::ProgressBar::new(fraction).text(format!("{}/{}", done, total)));
            if !prog.current.is_empty() {
                ui.label(format!("Current: {}", prog.current));
            }
            ui.separator();
            ui.label("Log:");
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for line in &prog.lines {
                        ui.label(line);
                    }
                });
        });
        if *self.uploading.lock().unwrap() {
            ctx.request_repaint();
        }
    }
}

fn is_supported_image(path: &std::path::Path) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };
    // Explicitly never treat XMP sidecars (or other non-image assets) as images.
    if ext == "xmp" {
        return false;
    }
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "heic" | "heif" | "tif" | "tiff" | "gif" | "webp" | "bmp"
            | "cr2" | "cr3" | "arw" | "nef" | "dng" | "raf" | "rw2" | "orf" | "srw"
            | "pef" | "3fr" | "iiq" | "erf" | "mos" | "kdc" | "dcr" | "mrw" | "raw"
    )
}

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
                    let suffix = &n["x-acb".len()..];
                    if result.cookie_x_acb.is_empty() {
                        result.cookie_x_acb = value.to_string();
                    }
                    if detected_country.is_empty() && !suffix.is_empty() {
                        detected_country = suffix.to_lowercase();
                    }
                }
                n if n.starts_with("at-acb") => {
                    if result.cookie_at_acb.is_empty() {
                        result.cookie_at_acb = value.to_string();
                    }
                    let suffix = &n["at-acb".len()..];
                    if detected_country.is_empty() && !suffix.is_empty() {
                        detected_country = suffix.to_lowercase();
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

fn main() -> eframe::Result<()> {
    // Build a multi-threaded tokio runtime held by the app
    let _rt = Runtime::new().expect("failed to build tokio runtime");

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Prime Photo Uploader",
        options,
        Box::new(|cc| Ok(Box::new(PrimePhotoApp::new(cc)))),
    )
}

// helper to avoid unused
#[allow(dead_code)]
fn _use_systemtime(_t: SystemTime) {}
