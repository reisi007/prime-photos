use crate::config::Config;
use crate::dedup::DedupIndex;
use crate::folders::{ConflictResolution, FolderStatus};
use crate::i18n::{tr, tr1, tr2, tr3, tr_n};
use crate::prime_photos::PrimePhotos;
use crate::theme;
use eframe::egui::Color32;
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio::task::JoinSet;

// ---------------------------------------------------------------------------
// Shared upload state (written by the async workers, read by the UI)
// ---------------------------------------------------------------------------

/// Coarse per-file state of a queue entry. The cookie upload path reads the
/// file as a whole, so there is no byte-level progress (docs/SPEC_UPLOAD_PROGRESS.md §4.2).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileState {
    Waiting,
    Uploading,
    Done,
    Failed,
    Skipped,
}

/// One entry of the live per-file queue rendered on the Backup page.
pub(crate) struct QueueEntry {
    /// Backup-folder index this file belongs to (the tuple's folder element).
    /// Kept for later per-folder grouping; not read by the current panel.
    #[allow(dead_code)]
    pub(crate) folder: usize,
    pub(crate) name: String,
    pub(crate) state: FileState,
}

#[derive(Default)]
pub(crate) struct UploadShared {
    pub(crate) active: bool,
    pub(crate) total: usize,
    pub(crate) done: usize,
    pub(crate) failed: usize,
    /// Wall-clock start of the current run; `None` when idle or finished. Basis
    /// for the ETA estimate (rate = done / elapsed).
    pub(crate) started: Option<Instant>,
    pub(crate) current: String,
    pub(crate) log: Vec<String>,
    pub(crate) folder_total: HashMap<usize, usize>,
    pub(crate) folder_done: HashMap<usize, usize>,
    pub(crate) folder_failed: HashMap<usize, usize>,
    pub(crate) uploaded_ids: Vec<String>,
    /// Live per-file queue for the current run, in `items` order. `QueueEntry.folder`
    /// is the folder index, the vector position is the row handed to the worker.
    pub(crate) queue: Vec<QueueEntry>,
}

impl UploadShared {
    pub(crate) fn log(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 500 {
            self.log.drain(0..100);
        }
    }
}

pub(crate) fn status_text(status: FolderStatus, pct: f32, total: usize) -> (String, Color32) {
    match status {
        FolderStatus::Uploading => (
            tr1("{}% hochgeladen", format!("{:.0}", pct * 100.0)),
            theme::TEAL,
        ),
        FolderStatus::Done => (tr("100% hochgeladen").to_string(), theme::GREEN),
        FolderStatus::Paused => (
            tr1("Angehalten ({}%)", format!("{:.0}", pct * 100.0)),
            theme::AMBER,
        ),
        FolderStatus::Error => (tr("Fehler").to_string(), theme::RED),
        FolderStatus::Scanning => (tr("Wird vorbereitet …").to_string(), theme::MUTED),
        FolderStatus::Idle => {
            if total > 0 {
                (
                    tr1("{}% hochgeladen", format!("{:.0}", pct * 100.0)),
                    theme::MUTED,
                )
            } else {
                (String::new(), theme::MUTED)
            }
        }
    }
}

/// ETA of the running upload in seconds, from the file-completion rate
/// (`done / elapsed`). Returns `None` while the estimate is unreliable: no
/// completed file yet, under a second elapsed, or a non-finite rate/result —
/// so the UI never shows a bogus or NaN/inf figure.
pub(crate) fn eta_secs(done: usize, total: usize, started: Option<Instant>) -> Option<f64> {
    let started = started?;
    if done == 0 {
        return None;
    }
    let elapsed = started.elapsed().as_secs_f64();
    if elapsed < 1.0 {
        return None;
    }
    let rate = done as f64 / elapsed;
    if !(rate > 0.0) || !rate.is_finite() {
        return None;
    }
    let remaining = total.saturating_sub(done) as f64;
    let eta = remaining / rate;
    if !eta.is_finite() || eta < 0.0 {
        return None;
    }
    Some(eta)
}

/// `M:SS`, or `H:MM:SS` from one hour up.
pub(crate) fn format_eta(secs: f64) -> String {
    let total = secs.round().max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// ETA fragment for the log summary, e.g. `"ca. 1:20"` — or `"Schätzung läuft …"`
/// while the estimate is not yet reliable.
pub(crate) fn eta_suffix(done: usize, total: usize, started: Option<Instant>) -> String {
    match eta_secs(done, total, started) {
        Some(eta) => tr1("ca. {}", format_eta(eta)),
        None => tr("Schätzung läuft …").to_string(),
    }
}

/// Status text for the progress bar, e.g. `"42% · noch 7 Dateien · ca. 1:20"`.
pub(crate) fn upload_progress_text(done: usize, total: usize, started: Option<Instant>) -> String {
    let pct = if total > 0 {
        (done as f32 / total as f32 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let remaining = total.saturating_sub(done);
    let files = tr_n(remaining, "1 Datei", "{n} Dateien");
    tr3(
        "{}% · noch {} · {}",
        format!("{pct:.0}"),
        files,
        eta_suffix(done, total, started),
    )
}

// ---------------------------------------------------------------------------
// Async upload machinery
// ---------------------------------------------------------------------------

pub(crate) async fn upload_worker(
    queue: Arc<Mutex<VecDeque<(usize, usize, PathBuf)>>>,
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
        let Some((row, idx, path)) = item else { break };

        // Mark the file as being worked on (guarded so an out-of-range row can
        // never panic; the queue is only mutated while `shared` is held).
        if let Some(e) = shared.lock().unwrap().queue.get_mut(row) {
            e.state = FileState::Uploading;
        }

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
                if let Some(en) = s.queue.get_mut(row) {
                    en.state = FileState::Failed;
                }
                s.log(tr2("FEHLER {}: {}", path.display(), e));
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
            if let Some(en) = s.queue.get_mut(row) {
                en.state = FileState::Skipped;
            }
            s.log(tr1(
                "UEBERSPRUNGEN {}: bereits hochgeladen",
                path.display(),
            ));
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
                queue.lock().unwrap().push_front((row, idx, path.clone()));
                if let Some(e) = shared.lock().unwrap().queue.get_mut(row) {
                    e.state = FileState::Waiting;
                }
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
                        if let Some(en) = s.queue.get_mut(row) {
                            en.state = FileState::Done;
                        }
                        s.log(format!("OK    {}", path.display()));
                    }
                    Err(e) => {
                        let mut s = shared.lock().unwrap();
                        s.failed += 1;
                        *s.folder_failed.entry(idx).or_insert(0) += 1;
                        if let Some(en) = s.queue.get_mut(row) {
                            en.state = FileState::Failed;
                        }
                        s.log(tr2("FEHLER {}: {}", path.display(), e));
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

pub(crate) async fn run_upload(
    items: Vec<(usize, usize, PathBuf)>,
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
        s.started = None;
        s.current.clear();
        (s.done, s.failed)
    };
    shared
        .lock()
        .unwrap()
        .log(tr2("Fertig: {} erfolgreich, {} fehlgeschlagen.", done, failed));
}
