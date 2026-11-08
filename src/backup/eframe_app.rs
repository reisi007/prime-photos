//! Legacy eframe upload methods for the shared app state.

use super::*;
use crate::app::{Page, PrimePhotoApp};
use crate::backup_config::{BackupConfigForm, ConfigFormMode};
use crate::platform::app_shared::ONE_TIME_IDX;

impl PrimePhotoApp {
    // -- folders ------------------------------------------------------------

    pub(crate) fn apply_action(&mut self, action: FolderAction) {
        // Demo fixtures are read-only: no remove/toggle/upload/reveal/edit side
        // effect. The global pause button does not route through here, so the
        // paused state stays inspectable.
        if self.is_demo() {
            self.demo_skip("Ordner-Aktion");
            return;
        }
        match action {
            FolderAction::Upload(i) | FolderAction::Retry(i) => {
                self.start_folder_upload(vec![i]);
            }
            FolderAction::Remove(i) => {
                if i < self.store.folders.len() {
                    let f = self.store.folders.remove(i);
                    let _ = self.store.save();
                    self.log(tr1(
                        "Datensicherung gestoppt: {}",
                        crate::text::nfc(&f.name),
                    ));
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
                        content_filter: f.content_filter,
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
    /// The UI paths use it to route a new request while a run is busy; the
    /// background tick checks the same state through its shared handle. In the
    /// "add folder" path the guard no longer merely reports (user decision
    /// 2026-10-17): the folder is handed to the detached supervisor and taken
    /// into the live run (see [`Self::start_folder_upload`]), while
    /// `start_one_time_upload` still answers with "Es läuft bereits ein Upload.".
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
            // Never drop the add (user decision 2026-10-17): hand the folder to the
            // detached supervisor, which reads it in **parallel** and takes its files
            // into the running run. Without a supervisor (background sync off) there is
            // nothing to hand it to, so the old message stands. Automatically picked up
            // again only **with** background sync — the next app start's safety net
            // (`background_sync_pass`, spawned by `ensure_background_sync`); without it
            // only a manual run does.
            let paths: Vec<PathBuf> = indices
                .iter()
                .filter_map(|&i| self.store.folders.get(i).map(|f| f.path.clone()))
                .collect();
            // No valid index (a stale UI row): nothing to send and nothing to
            // report — `all()` would be vacuously true and the "wird eingelesen"
            // notice would lie.
            if paths.is_empty() {
                return;
            }
            let sent = self
                .scan_tx
                .as_ref()
                .map(|tx| paths.iter().all(|p| tx.send(p.clone()).is_ok()))
                .unwrap_or(false);
            if sent {
                self.log(tr("Ordner wird eingelesen und in den laufenden Upload aufgenommen."));
            } else {
                self.log(tr("Es läuft bereits ein Upload."));
            }
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
        let Some(run) = prepare_scheduled_upload_shared(
            &mut self.store,
            indices,
            &mut self.config,
            self.upload.clone(),
            self.paused.clone(),
            self.album_name.clone(),
            self.slow_mode,
            true,
        ) else {
            return;
        };
        // Keep eframe's original persistence-before-worker-start order.
        self.save_config();
        self.was_active = true;
        run.spawn(&self.rt);
    }

    // -- one-time upload (drag & drop) --------------------------------------

    /// Start a one-time ("Einmalige Sicherung") run for the dropped paths.
    ///
    /// Same guards, same worker spawn as the manual path; the difference is the
    /// target: the dropped files are an explicit list, not a scanned folder,
    /// and the run lives on `self.one_time` — never inserted into
    /// `self.store.folders` (a drop must not create a permanent backup target).
    /// Files are uploaded as-is: a dropped file passes through, a dropped
    /// directory is scanned recursively, unsupported entries are ignored
    /// silently (same contract as `scan_images`).
    ///
    /// The **resolved** queue is persisted to the shared SQLite before the run
    /// starts (user decision 2026-10-14), so a restart resumes the row; it is
    /// dropped again once the run finished clean (`finalize_upload`).
    pub(crate) fn start_one_time_upload(&mut self, paths: Vec<PathBuf>) {
        // Demo fixtures are read-only: no folder scan, no SQLite queue, no run.
        if self.is_demo() {
            self.demo_skip("Einmalige Sicherung starten");
            return;
        }
        if self.upload_busy() {
            self.log(tr("Es läuft bereits ein Upload."));
            return;
        }
        if !self.is_logged_in() {
            self.log(tr("Bitte zuerst anmelden (Einstellungen)."));
            self.page = Page::Settings;
            return;
        }

        // Collect the file list: file → as-is, directory → recursive scan.
        let mut files: Vec<PathBuf> = Vec::new();
        for p in &paths {
            if p.is_file() {
                if folders::is_supported_image(p) {
                    files.push(p.clone());
                }
            } else if p.is_dir() {
                files.extend(folders::scan_images(p));
            }
        }
        if files.is_empty() {
            self.log(tr("Keine Fotos in diesem Ordner gefunden."));
            return;
        }

        // Display path of the transient target: the single dropped path, else
        // the common parent of all of them (empty when there is none).
        let display_path = if paths.len() == 1 {
            paths[0].clone()
        } else {
            common_parent(&paths).unwrap_or_default()
        };

        // Persist the resolved queue (user decision 2026-10-14) *before* the run
        // starts: the file list, not the raw drop paths (so a restart never
        // walks a dropped directory again), plus the row's display path. The
        // guards and the empty check above already returned, so a rejected drop
        // writes nothing.
        DedupIndex::open().set_one_time_queue(&display_path, &files);

        self.begin_one_time_run(display_path, files);
    }

    /// Resume the persisted one-time queue after a restart (user decision
    /// 2026-10-14): the row was already restored in `new`, this starts its run
    /// as soon as the **same guards as the folder cold start** allow it — the
    /// `background_sync` switch, a live sign-in, no pause, no active run.
    ///
    /// Called every frame from `update`. It is a no-op until a queue was
    /// restored and **one-shot**: once the run is started the flag is cleared,
    /// so a run that fails afterwards is only retried by the next app start
    /// (per the decision, not by a timer).
    pub(crate) fn resume_one_time(&mut self) {
        if !self.one_time_resume {
            return;
        }
        // Defence in depth: a demo fixture never carries a restored queue, and
        // resuming one would open the SQLite index and start a real run.
        if self.is_demo() {
            self.demo_skip("Einmalige Sicherung fortsetzen");
            return;
        }
        if self.upload_busy()
            || !self.is_logged_in()
            || !self.config.background_sync
            || *self.paused.borrow()
        {
            return;
        }
        // Re-read the queue: it is the persisted truth and may already have
        // been cleared by a cleanly finished run.
        let Some((display_path, files)) = DedupIndex::open().one_time_queue() else {
            self.one_time_resume = false;
            return;
        };
        if files.is_empty() {
            self.one_time_resume = false;
            return;
        }
        self.one_time_resume = false;
        self.begin_one_time_run(display_path, files);
    }

    /// The shared tail of a one-time drop and of the resume-after-restart path:
    /// set the transient target, publish the plan and spawn the run. The queue
    /// is **not** written here — the caller owns persistence.
    fn begin_one_time_run(&mut self, display_path: PathBuf, files: Vec<PathBuf>) {
        // Dedup and conflict handling both follow their application default
        // (user decision 2026-10-15; before that `dedup: true` /
        // `ConflictResolution::default()` — same values unless the user changed
        // the app defaults in the settings). `default_options` is the same
        // source the create form uses, so a fresh drop and a new target agree.
        let (dedup, conflict_resolution) = folders::default_options(&self.config);
        // The transient target itself: the stored name stays the plain German
        // key, `tr` is applied at render time.
        self.one_time = Some(folders::BackupFolder {
            path: display_path,
            name: "Einmalige Sicherung".to_string(),
            enabled: true,
            dedup,
            conflict_resolution,
            content_filter: folders::ContentFilter::Photos,
            total_files: files.len(),
            uploaded_files: 0,
            last_updated: String::new(),
            status: FolderStatus::Uploading,
        });

        // One plan, one synthetic folder index: the counters, the live queue
        // and `folder_opts` look exactly like a single-folder run.
        let mut plan = UploadPlan::from_files(ONE_TIME_IDX, dedup, conflict_resolution, files);
        let concurrency = self.config.concurrency();
        let limit = if self.config.limit_enabled {
            self.config.limit_kbps
        } else {
            0
        };
        let count = plan.count();
        // `publish_state` (not `publish_plan`): the one-time target has no
        // persisted folder to mark, and nothing here may save the store.
        publish_state(
            &mut plan,
            &self.upload,
            tr1("{} Datei(en) werden hochgeladen …", count),
            HashMap::new(),
            Some(self.slow_mode),
        );
        if self.slow_mode {
            self.log(tr("Slow-Mode aktiv: 1 Datei gleichzeitig."));
        }
        self.paused.send_replace(false);
        // Persist the lifted pause (user decision 2026-10-14): the start paths
        // have no pause guard, so without this the runtime is unpaused while
        // `config.toml` still says otherwise and the next restart resumes it.
        self.config.paused = false;
        self.save_config();
        self.was_active = true;

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
                HashMap::new(),
            )
            .await;
        });
    }

    // -- background sync (PORT-5) -------------------------------------------

    /// Spawn the single supervising background task — once per process, as soon
    /// as the app is signed in and `config.background_sync` is on. Idempotent;
    /// the task itself re-checks every guard.
    ///
    /// Its first action is the start-time safety net (user decision 2026-10-14):
    /// a metadata-only walk per enabled folder, so a folder that did not change
    /// since the last run costs no hash and no DB query. Only then does it arm
    /// the OS filesystem watchers — and it does so from the same task, so the
    /// watchers can never race the pass above into a second run.
    pub(super) fn ensure_background_sync(&mut self) {
        // No supervisor, no filesystem watchers and no uploads from a demo run.
        if self.is_demo() {
            self.demo_skip("Hintergrund-Synchronisierung starten");
            return;
        }
        if !self.config.background_sync || !self.is_logged_in() {
            return;
        }
        if BG_SYNC_STARTED.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.upload.clone();
        let paused = self.paused.clone();
        // The session material is `#[serde(skip)]` and cannot be re-read from
        // disk; the detached supervisor gets the shared in-memory session.
        let session = self.session.clone();
        let cfg = self.config.clone();
        let concurrency = self.config.concurrency();
        let limit = if self.config.limit_enabled {
            self.config.limit_kbps
        } else {
            0
        };
        let album = self.album_name.clone();
        // The folder-add hand-off: a folder the user adds while a run is busy is
        // sent to the detached supervisor over this channel (Welle 5.31).
        let (scan_tx, scan_rx) = tokio::sync::mpsc::unbounded_channel::<PathBuf>();
        self.scan_tx = Some(scan_tx);
        self.rt.spawn(async move {
            background_sync_pass(
                shared.clone(),
                paused.clone(),
                session.clone(),
                cfg.clone(),
                concurrency,
                limit,
                album.clone(),
            )
            .await;
            // From then on the OS events decide.
            watch_loop(shared, paused, session, cfg, concurrency, limit, album, scan_rx).await;
        });
    }

    pub(crate) fn finalize_upload(&mut self) {
        // Defence in depth: the demo never starts a run, and finalizing one
        // would write the fixture store to disk and ask the SQLite for a queue.
        if self.is_demo() {
            self.demo_skip("Upload abschließen");
            return;
        }
        // A cancelled run (sign-out) is not a finished run: no folder status, no
        // counters, no `last_updated`, no store save, and the one-time queue is
        // kept so the next start resumes it (user decision 2026-10-16). The
        // already-uploaded part stays in the dedup index — nothing is uploaded
        // twice.
        if self.upload.lock().unwrap().cancelled {
            return;
        }
        // A detached background finalizer can persist the newer folder state
        // after the frame's reload check (which may have observed the run as
        // active). Rebase this UI finalization on that store before applying
        // counters, and never hold the shared upload lock across disk I/O.
        let reload_store = self.upload.lock().unwrap().store_dirty;
        let mut store_reload_succeeded = false;
        let mut can_save_store = true;
        if reload_store {
            if let Some(store) = load_store_logged() {
                self.store = store;
                self.upload.lock().unwrap().take_store_dirty();
                store_reload_succeeded = true;
            } else {
                // Keep the dirty marker for the next frame's reload attempt and
                // do not overwrite the newer persisted store with this stale
                // in-memory copy.
                can_save_store = false;
            }
        }
        let (folder_done, folder_failed, folder_total, folder_paths) = {
            let s = self.upload.lock().unwrap();
            (
                s.folder_done.clone(),
                s.folder_failed.clone(),
                s.folder_total.clone(),
                s.folder_paths.clone(),
            )
        };
        // Whether the run that just finished was the one-time run, plus its
        // global counters. `folder_total` gets its `ONE_TIME_IDX` key only from
        // a one-time plan and `publish_state` clears it at the start of every
        // run, so the key is a reliable "this finished run was a one-time run".
        let (run_total, run_done, run_failed, was_one_time) = {
            let s = self.upload.lock().unwrap();
            (
                s.total,
                s.done,
                s.failed,
                s.folder_total.contains_key(&ONE_TIME_IDX),
            )
        };
        let now = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
        if can_save_store {
            apply_folder_progress(
                &mut self.store,
                &folder_paths,
                &folder_total,
                &folder_done,
                &folder_failed,
                &now,
            );
            let _ = self.store.save();
        }
        // The transient one-time target is settled the same way, but from the
        // synthetic `ONE_TIME_IDX` counters and onto `self.one_time` — nothing
        // is written back to the store here. `folder_total` is the trigger
        // (Welle 5.11), exactly like the persisted folders above: a run whose
        // files **all** fail has no `folder_done` entry, so a `folder_done`
        // trigger left this row stuck on the `Uploading` it was born with
        // (`begin_one_time_run`). `folder_failed` decides the outcome.
        if let Some(one) = self.one_time.as_mut() {
            if let Some(&total) = folder_total.get(&ONE_TIME_IDX) {
                one.total_files = total;
                one.uploaded_files = folder_done.get(&ONE_TIME_IDX).copied().unwrap_or(0);
                one.last_updated = now.clone();
                one.status = if folder_failed.get(&ONE_TIME_IDX).copied().unwrap_or(0) > 0 {
                    FolderStatus::Error
                } else {
                    FolderStatus::Done
                };
            }
        }
        // A cleanly finished one-time run drops its persisted queue (user
        // decision 2026-10-14): every file settled (`done + failed = total`) and
        // no failure left over. Anything else — failures, an aborted/quit run,
        // a run that never covered all files — keeps the rows, so the next start
        // resumes. Only the SQLite rows go; the row itself stays for display.
        if was_one_time && run_failed == 0 && run_done + run_failed >= run_total {
            DedupIndex::open().clear_one_time_queue();
        }
        // Safety net for the successful reload path; normally this is a no-op
        // because the marker was consumed immediately after that reload.
        if store_reload_succeeded {
            let _ = self.upload.lock().unwrap().take_store_dirty();
        }
    }
}
