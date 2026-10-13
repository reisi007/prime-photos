use crate::app::{Page, PrimePhotoApp};
use crate::backup_config::{BackupConfigForm, ConfigFormMode};
use crate::config::Config;
use crate::folders::{self, ConflictResolution, FolderStatus, FolderStore};
use crate::i18n::{tr, tr1};
use crate::theme;
use crate::upload::{run_upload, status_text, upload_progress_text, FileState, QueueEntry, UploadShared};
use eframe::egui::{self, RichText};
use rfd::FileDialog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{watch, Notify};

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

// ---------------------------------------------------------------------------
// Upload-run preparation (shared by the manual path and the background sync)
// ---------------------------------------------------------------------------

/// A prepared upload run: the worker items, the live queue rows for the Backup
/// page and the per-folder metadata `run_upload` needs. Pure scan of a folder
/// selection — no side effects, no logging, no runtime.
struct UploadPlan {
    /// `(row, folder index, path)`; `row` is this entry's position in `queue`.
    items: Vec<(usize, usize, PathBuf)>,
    queue: Vec<QueueEntry>,
    per_folder: HashMap<usize, usize>,
    folder_opts: HashMap<usize, (bool, ConflictResolution)>,
}

impl UploadPlan {
    /// Recursively scan the supported images of the folders at `indices`
    /// (indices that no longer exist are skipped, not an error). `None` when
    /// nothing at all was found: the manual path then logs it, the background
    /// tick stays silent.
    fn build(store: &FolderStore, indices: &[usize]) -> Option<Self> {
        let mut plan = UploadPlan {
            items: Vec::new(),
            queue: Vec::new(),
            per_folder: HashMap::new(),
            folder_opts: HashMap::new(),
        };
        for &i in indices {
            let Some(f) = store.folders.get(i) else {
                continue;
            };
            let imgs = folders::scan_images(&f.path);
            plan.per_folder.insert(i, imgs.len());
            plan.folder_opts.insert(i, (f.dedup, f.conflict_resolution));
            for p in imgs {
                // Same order as `items`: the row is this entry's position in
                // the queue, the folder is the backup-folder index `i`.
                let row = plan.items.len();
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                plan.queue.push(QueueEntry {
                    folder: i,
                    name,
                    state: FileState::Waiting,
                });
                plan.items.push((row, i, p));
            }
        }
        if plan.items.is_empty() {
            None
        } else {
            Some(plan)
        }
    }

    /// Number of files this run would upload (before per-file dedup skips).
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// Publish a scanned plan on the shared upload state (active, counters, live
/// queue and `msg` as the run-start log line), mark its folders as uploading
/// and persist that. Both start paths go through here, so a manual run and a
/// background run look and behave identically.
///
/// Takes the live queue out of `plan`; the caller keeps `items`/`folder_opts`
/// for the worker spawn.
fn publish_plan(
    plan: &mut UploadPlan,
    store: &mut FolderStore,
    shared: &Arc<Mutex<UploadShared>>,
    msg: String,
) {
    {
        let mut s = shared.lock().unwrap();
        s.active = true;
        s.started = Some(Instant::now());
        s.done = 0;
        s.failed = 0;
        s.current.clear();
        s.log.clear();
        s.folder_total = plan.per_folder.clone();
        s.folder_done.clear();
        s.folder_failed.clear();
        s.uploaded_ids.clear();
        s.total = plan.items.len();
        s.queue = std::mem::take(&mut plan.queue);
        s.log(msg);
    }
    for (&i, &count) in &plan.per_folder {
        if let Some(f) = store.folders.get_mut(i) {
            f.status = FolderStatus::Uploading;
            f.total_files = count;
        }
    }
    let _ = store.save();
}

// ---------------------------------------------------------------------------
// Background sync (PORT-5): one supervising timer task
// ---------------------------------------------------------------------------

/// Cadence of the background sync. Both values are hard-coded on purpose:
/// whether the user may change them is an open decision (AGENTS.todo.md,
/// "Zurückgestellt" #3), so nothing is wired to a setting.
/// First automatic run — deliberately conservative, so right after signing in
/// the user has room for a manual run before anything starts on its own.
const BG_SYNC_FIRST_TICK: Duration = Duration::from_secs(15 * 60);
/// After that: one re-scan every 90 minutes.
const BG_SYNC_INTERVAL: Duration = Duration::from_secs(90 * 60);

/// Set once the supervising background-sync task has been spawned for this
/// process — `ensure_background_sync` is idempotent through it.
static BG_SYNC_STARTED: AtomicBool = AtomicBool::new(false);

/// Reset signal for the background-sync interval: every manual upload bumps it,
/// so the periodic run does not start a second time right behind a manual one.
/// `Notify` keeps one permit, so a bump that lands between two ticks is never
/// lost.
static BG_SYNC_RESET: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Persist the counters of a finished **background** run. `finalize_upload` is
/// only reached through the UI hook for manual runs (`was_active` in `update`),
/// so the background chain writes the same folder state itself.
fn finalize_background_store(shared: &Arc<Mutex<UploadShared>>) {
    let (folder_done, folder_failed) = {
        let s = shared.lock().unwrap();
        (s.folder_done.clone(), s.folder_failed.clone())
    };
    let mut store = FolderStore::load();
    let now = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
    for (i, f) in store.folders.iter_mut().enumerate() {
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
    let _ = store.save();
}

/// One background-sync tick: re-scan every enabled folder and start a run for
/// the ones with files. Runs detached from the UI struct (the supervisor owns
/// its own clones), so every guard is re-checked here — it never panics into
/// the app and never touches the UI thread.
async fn background_sync_tick(
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    last_config: Config,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
) {
    // Freshest settings first: the supervisor is detached from the UI struct, so
    // the persisted config is the only source that reflects changes made after
    // it was spawned (login/logout, the `background_sync` toggle).
    let cfg = Config::load().unwrap_or(last_config);
    if !cfg.background_sync {
        return;
    }
    // Same predicate as `PrimePhotoApp::is_logged_in` (session + acb cookie).
    if cfg.session_id.is_empty() || cfg.cookie_x_acb.is_empty() {
        return;
    }
    if *paused.borrow() {
        return;
    }
    if shared.lock().unwrap().active {
        return;
    }

    // Enabled folders come from the persisted store (saved on every change).
    let mut store = FolderStore::load();
    let indices = store.enabled_indices();
    if indices.is_empty() {
        return;
    }
    let Some(mut plan) = UploadPlan::build(&store, &indices) else {
        return;
    };
    let count = plan.count();
    publish_plan(
        &mut plan,
        &mut store,
        &shared,
        format!("Hintergrund-Sync: {count} Datei(en) werden hochgeladen …"),
    );

    // Spawned, not awaited: the supervisor must keep ticking while the run
    // uploads (the `active` guard above already prevents a second run).
    tokio::spawn(async move {
        let after = shared.clone();
        run_upload(
            plan.items,
            cfg,
            shared,
            paused,
            concurrency,
            limit_kbps,
            album,
            plan.folder_opts,
        )
        .await;
        finalize_background_store(&after);
    });
}

/// The supervising task: one conservative first tick, then one run every 90
/// minutes, for the rest of the process. A manual upload resets the interval.
async fn background_sync_loop(
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    last_config: Config,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
) {
    let mut interval = tokio::time::interval_at(
        tokio::time::Instant::now() + BG_SYNC_FIRST_TICK,
        BG_SYNC_INTERVAL,
    );
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Run the tick as its own task and wait for it: if it ever
                // panicked, the supervisor — the only thing keeping the sync
                // alive in this session — must survive.
                let (shared, paused, cfg, album) = (
                    shared.clone(),
                    paused.clone(),
                    last_config.clone(),
                    album.clone(),
                );
                if let Err(e) = tokio::spawn(background_sync_tick(
                    shared, paused, cfg, concurrency, limit_kbps, album,
                ))
                .await
                {
                    log::warn!("Hintergrund-Sync: Tick fehlgeschlagen: {e}");
                }
            }
            // A manual upload just started: restart the 90-minute cadence so
            // the periodic run does not double-run behind it.
            _ = BG_SYNC_RESET.notified() => interval.reset(),
        }
    }
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

    /// Whether an upload run is currently active.
    ///
    /// The UI paths use it for the "Es läuft bereits ein Upload." guard; the
    /// background tick checks the same state through its shared handle.
    pub(crate) fn upload_busy(&self) -> bool {
        self.upload.lock().unwrap().active
    }

    /// UI path for a manual run (row menu "Jetzt scannen und hochladen" /
    /// "Wiederholen", and the create-backup dialog). Keeps both UI guards: the
    /// already-running check and the redirect to the settings page when nobody
    /// is signed in. The background tick checks its own guards (paused, busy,
    /// logged in) and must never navigate the UI.
    pub(crate) fn start_folder_upload(&mut self, indices: Vec<usize>) {
        if self.upload_busy() {
            self.log(tr("Es läuft bereits ein Upload."));
            return;
        }
        if !self.is_logged_in() {
            self.log(tr("Bitte zuerst anmelden (Einstellungen)."));
            self.page = Page::Settings;
            return;
        }
        self.start_scheduled_upload(indices);
    }

    /// Scan `indices` and start an upload run for them: the shared start path
    /// **without** the UI guards above (no busy check, no login redirect), so a
    /// run is not tied to a UI click.
    ///
    /// The background-sync tick cannot call this (it has no `&mut self`), but it
    /// starts its runs through the very same steps — `UploadPlan::build` for the
    /// scan and `publish_plan` for the state/folder side effects.
    pub(crate) fn start_scheduled_upload(&mut self, indices: Vec<usize>) {
        let Some(mut plan) = UploadPlan::build(&self.store, &indices) else {
            self.log(tr("Keine Fotos oder Videos in diesem Ordner gefunden."));
            return;
        };
        let concurrency = if self.slow_mode { 1 } else { self.concurrency };
        let limit = if self.limit_enabled {
            self.limit_kbps
        } else {
            0
        };
        let count = plan.count();
        publish_plan(
            &mut plan,
            &mut self.store,
            &self.upload,
            tr1("{} Datei(en) werden hochgeladen …", count),
        );
        if self.slow_mode {
            self.log(tr("Slow-Mode aktiv: 1 Datei gleichzeitig."));
        }
        self.paused.send_replace(false);
        self.was_active = true;
        // Manual run: restart the background-sync cadence, so its next tick does
        // not start a second run right behind this one.
        BG_SYNC_RESET.notify_one();

        let cfg = self.config.clone();
        let shared = self.upload.clone();
        let paused = self.paused.clone();
        let album = self.album_name.clone();
        self.rt.spawn(async move {
            run_upload(
                plan.items,
                cfg,
                shared,
                paused,
                concurrency,
                limit,
                album,
                plan.folder_opts,
            )
            .await;
        });
    }

    // -- background sync (PORT-5) -------------------------------------------

    /// Spawn the single supervising background-sync task — once per process,
    /// as soon as the app is signed in and `config.background_sync` is on.
    /// Idempotent; the task itself re-checks every guard on every tick.
    fn ensure_background_sync(&mut self) {
        if !self.config.background_sync || !self.is_logged_in() {
            return;
        }
        if BG_SYNC_STARTED.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.upload.clone();
        let paused = self.paused.clone();
        let cfg = self.config.clone();
        let concurrency = if self.slow_mode { 1 } else { self.concurrency };
        let limit = if self.limit_enabled {
            self.limit_kbps
        } else {
            0
        };
        let album = self.album_name.clone();
        self.rt.spawn(async move {
            background_sync_loop(shared, paused, cfg, concurrency, limit, album).await;
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
        // Background sync (PORT-5): starts the supervising timer once we are
        // signed in; from then on this is a cheap no-op.
        self.ensure_background_sync();

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

        // Live per-file queue, grouped by backup folder
        // (docs/SPEC_UPLOAD_PROGRESS.md §4.1/§4.3). One snapshot under the upload
        // lock; no lock is held across `.show`.
        let queue_groups: Vec<(String, Vec<(String, FileState)>)> = {
            let s = self.upload.lock().unwrap();
            let mut groups: Vec<(String, Vec<(String, FileState)>)> = Vec::new();
            // Folder index -> position in `groups` (keeps first-seen order).
            let mut slot: HashMap<usize, usize> = HashMap::new();
            for e in &s.queue {
                let folder = self
                    .store
                    .folders
                    .get(e.folder)
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                let idx = match slot.get(&e.folder).copied() {
                    Some(i) => i,
                    None => {
                        groups.push((folder, Vec::new()));
                        slot.insert(e.folder, groups.len() - 1);
                        groups.len() - 1
                    }
                };
                groups[idx].1.push((e.name.clone(), e.state));
            }
            groups
        };
        if !queue_groups.is_empty() {
            let remaining: usize = queue_groups
                .iter()
                .map(|(_, files)| {
                    files
                        .iter()
                        .filter(|(_, st)| matches!(st, FileState::Waiting | FileState::Uploading))
                        .count()
                })
                .sum();
            egui::CollapsingHeader::new(tr1("Warteschlange ({})", remaining)).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        // Cap the rendered rows so a very large run stays cheap.
                        let mut shown = 0usize;
                        for (folder, files) in &queue_groups {
                            if shown >= 300 {
                                break;
                            }
                            if !folder.is_empty() {
                                let open = files
                                    .iter()
                                    .filter(|(_, st)| {
                                        matches!(st, FileState::Waiting | FileState::Uploading)
                                    })
                                    .count();
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(folder.as_str()).strong().size(12.0));
                                    ui.label(theme::muted(tr1("{} offen", open)).size(11.0));
                                });
                            }
                            for (name, state) in files.iter().take(300 - shown) {
                                shown += 1;
                                let (txt, col) = match state {
                                    FileState::Waiting => (tr("Warten"), theme::MUTED),
                                    FileState::Uploading => (tr("In Bearbeitung"), theme::TEAL),
                                    FileState::Done => (tr("Abgeschlossen"), theme::GREEN),
                                    FileState::Failed => (tr("Fehlgeschlagen"), theme::RED),
                                    FileState::Skipped => (tr("Übersprungen"), theme::MUTED),
                                };
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(txt).color(col).size(12.0));
                                    ui.label(RichText::new(name).size(12.0));
                                });
                            }
                        }
                    });
            });
            ui.add_space(8.0);
        }

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
