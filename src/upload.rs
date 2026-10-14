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
    /// Used by the Backup page to group the queue per folder.
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
    /// Revision of the run's *file states*. Bumped on every queue mutation, so
    /// the UI cache can skip rebuilding its (potentially thousands of entries)
    /// snapshot while only the counters moved.
    pub(crate) rev: u64,
    /// Byte totals for the byte-based ETA: known size of the run, how much of it
    /// is settled (uploaded **or** dedup-skipped) and how much went over the wire.
    /// 0 while unknown — the ETA then falls back to the file-count rate.
    pub(crate) bytes_total: u64,
    pub(crate) bytes_done: u64,
    pub(crate) net_bytes: u64,
    /// Wall-clock instant of the first completed transfer; the byte rate is
    /// measured from here (`upload_secs`), so the ETA does not depend on how
    /// many workers ran in parallel.
    pub(crate) net_started: Option<Instant>,
    /// Seconds since `net_started` (wall clock). 0 while nothing has finished.
    pub(crate) upload_secs: f64,
    /// Interactive name-conflict prompt (docs/SPEC_DEDUP.md §4.1): the file name
    /// the user must decide on. `Some` while the run is waiting for the answer.
    pub(crate) pending_conflict: Option<String>,
    /// The user's answer to `pending_conflict` (`Ignore` / `SaveAsNew` /
    /// `Replace`; never `Ask`). The worker takes it and clears it.
    pub(crate) conflict_answer: Option<ConflictResolution>,
    /// "Nicht noch einmal fragen": apply the answer to every further conflict of
    /// this run without asking again.
    pub(crate) conflict_remember: bool,
    /// Per-run override set by `conflict_remember`; wins over the folder policy.
    pub(crate) conflict_override: Option<ConflictResolution>,
}

impl UploadShared {
    /// Append to the (bounded) run log. Old lines are dropped oldest-first, the
    /// panel stays a scrolling tail.
    pub(crate) fn log(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 500 {
            self.log.drain(0..100);
        }
    }

    /// Mark the queue as changed (the UI cache rebuilds its snapshot).
    /// Mark the queue as changed (the UI cache rebuilds its snapshot). Also
    /// call it when the queue is *replaced* wholesale (`publish_state`): the
    /// revision is the cache's only signal, and a new run must never inherit
    /// the previous run's rendered entries.
    pub(crate) fn touch_queue(&mut self) {
        self.rev = self.rev.wrapping_add(1);
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

/// ETA of the running upload in seconds. Byte-based when the run's sizes are
/// known (much more accurate than counting files: dedup skips are free and the
/// files differ wildly in size), otherwise from the file-completion rate
/// (`done / elapsed`). Returns `None` while the estimate is unreliable: no
/// completed file yet, under a second elapsed, or a non-finite rate/result —
/// so the UI never shows a bogus or NaN/inf figure.
pub(crate) fn eta_secs(
    done: usize,
    total: usize,
    started: Option<Instant>,
    bytes_total: u64,
    bytes_done: u64,
    net_bytes: u64,
    upload_secs: f64,
) -> Option<f64> {
    let started = started?;
    if done == 0 {
        return None;
    }

    // Bytes: remaining unsettled bytes over the measured transfer rate.
    if bytes_total > 0 && upload_secs >= 1.0 {
        let bps = net_bytes as f64 / upload_secs;
        if bps > 0.0 && bps.is_finite() {
            let remaining = bytes_total.saturating_sub(bytes_done) as f64;
            let eta = remaining / bps;
            if eta.is_finite() && eta >= 0.0 {
                return Some(eta);
            }
        }
    }

    // Fallback: completed files per wall-clock second.
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
pub(crate) fn eta_suffix(shared: &UploadShared) -> String {
    let s = shared;
    match eta_secs(
        s.done,
        s.total,
        s.started,
        s.bytes_total,
        s.bytes_done,
        s.net_bytes,
        s.upload_secs,
    ) {
        Some(eta) => tr1("ca. {}", format_eta(eta)),
        None => tr("Schätzung läuft …").to_string(),
    }
}

/// Status text for the progress bar, e.g. `"42% · noch 7 Dateien · ca. 1:20"`.
pub(crate) fn upload_progress_text(shared: &UploadShared) -> String {
    let (done, total) = (shared.done, shared.total);
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
        eta_suffix(shared),
    )
}

// ---------------------------------------------------------------------------
// Async upload machinery
// ---------------------------------------------------------------------------

/// Outcome of one file's upload attempt, after the conflict policy has been
/// applied (`Ignore`/`Ask→Ignore` become `ConflictSkipped`, everything else is
/// an upload or a real failure).
enum FileOutcome {
    Uploaded(String),
    /// A name conflict the policy resolved as "keep the existing file".
    ConflictSkipped,
    Failed(String),
}

/// Ask the user about one name conflict (`Ask` policy) and return the decision.
///
/// Prompts are serialized: only one is on screen at a time. Other workers that
/// hit a conflict wait here and pick up the run-wide override if the user chose
/// "nicht noch einmal fragen". A hard timeout guarantees a hidden window can
/// never stall the whole run — the dedup-friendly `Ignore` is the fallback.
async fn resolve_conflict(
    shared: &Arc<Mutex<UploadShared>>,
    file_name: &str,
) -> ConflictResolution {
    const POLL: Duration = Duration::from_millis(150);
    const TIMEOUT: Duration = Duration::from_secs(60);
    loop {
        // A run-wide decision (or a previous answer) needs no prompt.
        let ask_now = {
            let mut s = shared.lock().unwrap();
            if let Some(o) = s.conflict_override {
                return o;
            }
            if s.pending_conflict.is_none() {
                s.pending_conflict = Some(file_name.to_string());
                s.conflict_answer = None;
                s.touch_queue();
                s.log(tr1("Dateiname-Konflikt: {}", file_name));
                true
            } else {
                false
            }
        };
        if !ask_now {
            // Another worker is already asking; wait until it is answered and
            // then re-check for a run-wide override.
            tokio::time::sleep(POLL).await;
            continue;
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            tokio::time::sleep(POLL).await;
            let mut s = shared.lock().unwrap();
            if let Some(answer) = s.conflict_answer.take() {
                s.pending_conflict = None;
                if s.conflict_remember {
                    s.conflict_override = Some(answer);
                }
                s.touch_queue();
                return answer;
            }
            if Instant::now() >= deadline {
                s.pending_conflict = None;
                s.touch_queue();
                s.log(tr("Keine Antwort auf den Konflikt — wird übersprungen."));
                return ConflictResolution::Ignore;
            }
        }
    }
}

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
    // A 401 mid-run means the session cookies expired, not that the file is
    // broken: renew the token once, then retry the file. `renew_auth_token`
    // needs `&mut uploader`, which is only free at the top of the loop (the
    // pinned upload future below borrows it), so the renewal is deferred to the
    // next iteration and the file is pushed back onto the queue.
    let mut need_token_renew = false;
    let mut token_renewed_once = false;
    loop {
        while *pause_rx.borrow_and_update() {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        if need_token_renew {
            need_token_renew = false;
            token_renewed_once = true;
            match uploader.renew_auth_token().await {
                Ok(_) => {
                    let mut s = shared.lock().unwrap();
                    s.log(tr("Auth-Token wurde erneuert."));
                }
                Err(e) => {
                    let mut s = shared.lock().unwrap();
                    s.log(tr1("Auth-Token-Erneuerung fehlgeschlagen: {}", e));
                }
            }
        }
        let item = { queue.lock().unwrap().pop_front() };
        let Some((row, idx, path)) = item else { break };

        // Mark the file as being worked on (guarded so an out-of-range row can
        // never panic; the queue is only mutated while `shared` is held).
        {
            let mut s = shared.lock().unwrap();
            if let Some(e) = s.queue.get_mut(row) {
                e.state = FileState::Uploading;
            }
            s.touch_queue();
        }

        let (dedup_on, conflict) = folder_opts
            .get(&idx)
            .copied()
            .unwrap_or((true, ConflictResolution::Ignore));

        // Stat before reading (user decision 2026-10-14): the recorded
        // `size`/`mtime` decide the cheap skip path, so an already-uploaded,
        // unchanged file is never read or hashed at all. The MD5 stays the
        // duplicate identity (docs/SPEC_DEDUP.md §1) — a stat hit only
        // short-circuits files whose metadata matches exactly what was
        // recorded for this path. An unknown mtime (0) never hits.
        let meta = std::fs::metadata(&path);
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = meta
            .as_ref()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Local pre-upload check (path + stat): same path, same size, same
        // mtime as recorded — the file is provably the one that was uploaded.
        // Treated exactly like the `should_skip` branch below, minus the read
        // and the hash: done for this folder, no network, no throttle.
        if dedup_on && dedup.lock().unwrap().stat_matches(&path, size, mtime) {
            let mut s = shared.lock().unwrap();
            s.done += 1;
            *s.folder_done.entry(idx).or_insert(0) += 1;
            if let Some(en) = s.queue.get_mut(row) {
                en.state = FileState::Skipped;
            }
            // Settled for progress, but not transferred: it must not raise the
            // measured byte rate.
            s.bytes_done = s.bytes_done.saturating_add(size);
            s.touch_queue();
            s.log(tr1(
                "UEBERSPRUNGEN {}: bereits hochgeladen",
                path.display(),
            ));
            continue;
        }

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
                s.touch_queue();
                s.log(tr2("FEHLER {}: {}", path.display(), e));
                continue;
            }
        };
        let md5sum = format!("{:x}", md5::compute(&bytes));
        let size = bytes.len() as u64;

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
            // Settled for progress, but not transferred: it must not raise the
            // measured byte rate.
            s.bytes_done = s.bytes_done.saturating_add(size);
            s.touch_queue();
            s.log(tr1(
                "UEBERSPRUNGEN {}: bereits hochgeladen",
                path.display(),
            ));
            continue;
        }

        let started = Instant::now();

        // Pause policy (F-3 decision): the *current* file always finishes, only
        // the next one waits. Aborting mid-request would discard the bytes
        // already sent and could leave a `RENAME` duplicate on the server behind
        // — both contradict "never waste bandwidth". The pause check therefore
        // happens at the top of the loop only, not by racing this future.
        //
        // Conflict policy (docs/SPEC_DEDUP.md §4.1): a 409 is reported by the
        // engine. `Ignore` maps it to "already there" (finished, no bytes); `Ask`
        // waits for one user decision per file (with an optional "don't ask
        // again" for the rest of the run); `SaveAsNew`/`Replace` are plain wire
        // values and never produce a 409.
        let mut resolution = {
            let s = shared.lock().unwrap();
            s.conflict_override.unwrap_or(conflict)
        };
        let mut existing_conflict_id: Option<String> = None;
        let outcome = loop {
            match uploader
                .upload_picture(&path, Some(&bytes), Some(&md5sum), resolution)
                .await
            {
                Ok(id) => break FileOutcome::Uploaded(id),
                Err(e) => {
                    let Some(existing) = crate::prime_photos::conflict_node_id(&e) else {
                        break FileOutcome::Failed(e);
                    };
                    if resolution != ConflictResolution::Ask {
                        // "Ignorieren": the file is already on the server.
                        existing_conflict_id = Some(existing.to_string());
                        break FileOutcome::ConflictSkipped;
                    }
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let answer = match resolve_conflict(&shared, &name).await {
                        // The prompt can only answer with a concrete policy;
                        // normalise defensively so `Ask` can never bounce the
                        // file through this loop a second time.
                        ConflictResolution::Ask => ConflictResolution::Ignore,
                        a => a,
                    };
                    if answer == ConflictResolution::Ignore {
                        existing_conflict_id = Some(existing.to_string());
                        break FileOutcome::ConflictSkipped;
                    }
                    // Re-upload the same file with the chosen policy.
                    resolution = answer;
                }
            }
        };
        let mut requeued = false;
        match outcome {
            FileOutcome::Uploaded(id) => {
                // Remember the upload so the next scan skips it.
                dedup.lock().unwrap().record_upload(&path, &md5sum, size, mtime);
                let mut s = shared.lock().unwrap();
                s.done += 1;
                *s.folder_done.entry(idx).or_insert(0) += 1;
                s.uploaded_ids.push(id);
                if let Some(en) = s.queue.get_mut(row) {
                    en.state = FileState::Done;
                }
                s.bytes_done = s.bytes_done.saturating_add(size);
                s.net_bytes = s.net_bytes.saturating_add(size);
                // Throughput is measured against wall clock, not by summing
                // per-file durations: with several workers that sum is a
                // multiple of the real elapsed time and the ETA would be
                // pessimistic by roughly the worker count.
                let first = *s.net_started.get_or_insert_with(Instant::now);
                s.upload_secs = first.elapsed().as_secs_f64();
                s.touch_queue();
                s.log(format!("OK    {}", path.display()));
            }
            FileOutcome::Failed(e) => {
                // Expired session: retry this one file once after the token
                // renewal (never counted as a failure). Only once per run, so a
                // persistently bad session cannot spin.
                if !token_renewed_once && e.contains("Authentication failed") {
                    need_token_renew = true;
                    queue.lock().unwrap().push_front((row, idx, path.clone()));
                    let mut s = shared.lock().unwrap();
                    if let Some(en) = s.queue.get_mut(row) {
                        en.state = FileState::Waiting;
                    }
                    s.touch_queue();
                    s.log(tr1(
                        "Anmeldung abgelaufen, {} wird erneut versucht.",
                        path.display(),
                    ));
                    requeued = true;
                } else {
                    let mut s = shared.lock().unwrap();
                    s.failed += 1;
                    *s.folder_failed.entry(idx).or_insert(0) += 1;
                    if let Some(en) = s.queue.get_mut(row) {
                        en.state = FileState::Failed;
                    }
                    s.touch_queue();
                    s.log(tr2("FEHLER {}: {}", path.display(), e));
                }
            }
            FileOutcome::ConflictSkipped => {
                // 409 with `Ignore` (or an `Ask` answered with "Ignorieren"):
                // the file is already on the server. Finished and dedup-friendly
                // — not an error, and no bytes were transferred.
                //
                // Record it in the local MD5 index like a real upload: without
                // this, every later run would read, hash and re-send the whole
                // file just to get the same 409 again — exactly the wasted
                // bandwidth this whole path exists to avoid.
                dedup.lock().unwrap().record_upload(&path, &md5sum, size, mtime);
                let mut s = shared.lock().unwrap();
                s.done += 1;
                *s.folder_done.entry(idx).or_insert(0) += 1;
                if let Some(id) = existing_conflict_id.take() {
                    if !id.is_empty() {
                        // Keep the existing node in the album list, exactly as
                        // the previous 409-as-Ok path did.
                        s.uploaded_ids.push(id);
                    }
                }
                if let Some(en) = s.queue.get_mut(row) {
                    en.state = FileState::Skipped;
                }
                s.bytes_done = s.bytes_done.saturating_add(size);
                s.touch_queue();
                s.log(tr1("Vorhanden, übersprungen: {}", path.display()));
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
