mod amzn_photo;
mod config;

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
    files: Vec<PathBuf>,
    progress: Arc<Mutex<UploadProgress>>,
    uploading: Arc<Mutex<bool>>,
}

impl PrimePhotoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let rt = std::sync::Arc::new(Runtime::new().expect("tokio runtime"));
        let (config, config_error) = match Config::load() {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e.to_string())),
        };
        let config_path = Config::get_config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| e.to_string());
        let zone_override = String::new();
        Self {
            rt,
            config,
            config_path,
            config_error,
            album_name: String::new(),
            zone_override,
            files: Vec::new(),
            progress: Arc::new(Mutex::new(UploadProgress::new())),
            uploading: Arc::new(Mutex::new(false)),
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

    fn add_files(&mut self) {
        if let Some(paths) = FileDialog::new()
            .add_filter("Images", &["jpg", "jpeg", "png", "heic", "heif", "tif", "tiff", "gif", "webp", "bmp"])
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
    )
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
