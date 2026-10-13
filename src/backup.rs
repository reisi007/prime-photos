use crate::app::{Page, PrimePhotoApp};
use crate::backup_config::{BackupConfigForm, ConfigFormMode};
use crate::folders::{self, ConflictResolution, FolderStatus};
use crate::i18n::{tr, tr1};
use crate::theme;
use crate::upload::{run_upload, status_text, upload_progress_text};
use eframe::egui::{self, RichText};
use rfd::FileDialog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

pub(crate) enum FolderAction {
    Upload(usize),
    Retry(usize),
    Remove(usize),
    OpenFinder(usize),
    ToggleEnabled(usize),
    Edit(usize),
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

impl PrimePhotoApp {
    // -- folders ------------------------------------------------------------

    pub(crate) fn add_backup_folder(&mut self) {
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

    pub(crate) fn apply_action(&mut self, action: FolderAction) {
        match action {
            FolderAction::Upload(i) | FolderAction::Retry(i) => {
                self.start_folder_upload(vec![i]);
            }
            FolderAction::Remove(i) => {
                if i < self.store.folders.len() {
                    let f = self.store.folders.remove(i);
                    let _ = self.store.save();
                    self.log(tr1("Datensicherung gestoppt: {}", &f.name));
                }
            }
            FolderAction::OpenFinder(i) => {
                if let Some(f) = self.store.folders.get(i) {
                    let _ = crate::platform::reveal_in_file_manager(&f.path);
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

    pub(crate) fn start_folder_upload(&mut self, indices: Vec<usize>) {
        if self.upload.lock().unwrap().active {
            self.log(tr("Es läuft bereits ein Upload."));
            return;
        }
        if !self.is_logged_in() {
            self.log(tr("Bitte zuerst anmelden (Einstellungen)."));
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
            self.log(tr("Keine Fotos oder Videos in diesem Ordner gefunden."));
            return;
        }

        {
            let mut s = self.upload.lock().unwrap();
            s.active = true;
            s.started = Some(Instant::now());
            s.done = 0;
            s.failed = 0;
            s.current.clear();
            s.log.clear();
            s.folder_total = per_folder.clone();
            s.folder_done.clear();
            s.folder_failed.clear();
            s.uploaded_ids.clear();
            s.total = items.len();
            s.log(tr1("{} Datei(en) werden hochgeladen …", items.len()));
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
        let concurrency = if self.slow_mode { 1 } else { self.concurrency };
        if self.slow_mode {
            self.log(tr("Slow-Mode aktiv: 1 Datei gleichzeitig."));
        }
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

    pub(crate) fn finalize_upload(&mut self) {
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

    pub(crate) fn backup_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add(theme::primary_button(tr("Datensicherung hinzufügen …")))
                .clicked()
            {
                self.add_backup_folder();
            }
            let all_paused = *self.paused.borrow();
            let label = if all_paused {
                tr("Alle Sicherungsvorgänge fortsetzen")
            } else {
                tr("Alle Sicherungsvorgänge anhalten")
            };
            let (running, done, total, started) = {
                let s = self.upload.lock().unwrap();
                (s.active, s.done, s.total, s.started)
            };
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
                let frac = if total > 0 {
                    (done as f32 / total as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(240.0)
                        .text(upload_progress_text(done, total, started)),
                );
            }
        });

        ui.add_space(12.0);
        ui.label(theme::muted(tr("Automatisch gesicherte Ordner:")));

        if self.store.folders.is_empty() {
            ui.add_space(8.0);
            ui.label(tr("Wählen Sie einen Ordner aus, um die Sicherung zu starten."));
            ui.add_space(6.0);
            ui.label(theme::muted(tr(
                "Tipp: Ordner werden dauerhaft gemerkt und bei jedem Start wieder gesichert.",
            )));
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
                                        RichText::new(tr1(
                                            "{}% hochgeladen",
                                            format!("{:.0}", pct * 100.0),
                                        ))
                                        .size(12.0),
                                    );
                                    if !r.last_updated.is_empty() {
                                        ui.label(
                                            theme::muted(tr1(
                                                "Zuletzt aktualisiert: {}",
                                                &r.last_updated,
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
                                            if ui.button(tr("Jetzt scannen und hochladen")).clicked() {
                                                actions.push(FolderAction::Upload(r.idx));
                                                ui.close_menu();
                                            }
                                            if ui.button(tr("Im Finder anzeigen")).clicked() {
                                                actions.push(FolderAction::OpenFinder(r.idx));
                                                ui.close_menu();
                                            }
                                            if ui.button(tr("Wiederholen")).clicked() {
                                                actions.push(FolderAction::Retry(r.idx));
                                                ui.close_menu();
                                            }
                                            if ui.button(tr("Bearbeiten")).clicked() {
                                                actions.push(FolderAction::Edit(r.idx));
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button(tr("Datensicherung stoppen")).clicked() {
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
}
