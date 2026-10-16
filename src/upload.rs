use crate::config::Config;
use crate::dedup::{DedupIndex, Origin};
use crate::folders::{ConflictResolution, FolderStatus};
use crate::i18n::{tr, tr1, tr2, tr3, tr_n};
use crate::prime_photos::{PrimePhotos, ReplaceBasis};
use crate::theme;
use eframe::egui::Color32;
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio::task::JoinSet;

// ---------------------------------------------------------------------------
// Shared upload state (written by the async workers, read by the UI)
// ---------------------------------------------------------------------------

/// Coarse per-file state of a queue entry. The upload path streams the body and
/// counts the bytes it hands to the transport, so an `Uploading` row also shows
/// a real per-file percentage (Befund 11c); `docs/SPEC_UPLOAD_PROGRESS.md` §4.2
/// described the older whole-file behaviour.
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
    /// File size in bytes, read once while the plan is built. `0` when the file
    /// could not be stat'ed — the queue's size column then shows `0 B`.
    pub(crate) size: u64,
    /// Full path, shown as the name column's hover tooltip
    /// (docs/SPEC_UPLOAD_PROGRESS.md §7.3).
    pub(crate) path: PathBuf,
    /// Bytes handed to the transport for this file so far (Befund 11c), copied
    /// from the upload's progress counter by a 100 ms polling task. `0` before a
    /// transfer starts; `size` once the file is settled without a transfer (a
    /// dedup skip) or has finished. Only meaningful for `Uploading` rows, where
    /// the Backup page derives the per-file percentage from it.
    pub(crate) bytes_sent: u64,
}

/// Per-file upload percentage for a queue row: sent bytes over the file size,
/// clamped to `0..=100`. `0` while either side is unknown (`size == 0`), so the
/// row falls back to the plain state text instead of showing a bogus `0%`.
pub(crate) fn calculate_pct(size: u64, bytes_sent: u64) -> u64 {
    if size == 0 {
        return 0;
    }
    (bytes_sent.saturating_mul(100) / size).min(100)
}

#[derive(Default)]
pub(crate) struct UploadShared {
    pub(crate) active: bool,
    pub(crate) total: usize,
    pub(crate) done: usize,
    pub(crate) failed: usize,
    /// Set by `logout` (user decision 2026-10-16): signing out must stop a
    /// running upload — the worker holds its own `Config` clone with the old
    /// cookies and would keep failing against a revoked session. Checked at
    /// every wait point; a request already in flight is allowed to finish.
    /// Cleared by `run_upload` before a new run's workers start.
    pub(crate) cancelled: bool,
    /// Wall-clock start of the current run; `None` when idle or finished. Basis
    /// for the ETA estimate (rate = done / elapsed).
    pub(crate) started: Option<Instant>,
    pub(crate) current: String,
    pub(crate) log: Vec<String>,
    pub(crate) folder_total: HashMap<usize, usize>,
    pub(crate) folder_done: HashMap<usize, usize>,
    pub(crate) folder_failed: HashMap<usize, usize>,
    /// Run outcome breakdown (user decision 2026-10-14): the single `done`
    /// counter used to mix three very different outcomes — a real upload, a 409
    /// "already in the account" and a local dedup/stat skip that never touched
    /// the network — so "480 fertig" was not readable as "480 new". These three
    /// counters keep them apart; `done` itself stays the sum (progress and
    /// persistence depend on it). Purely in-memory: no schema change.
    pub(crate) uploaded_new: usize,
    pub(crate) conflict_skipped: usize,
    pub(crate) unchanged_skipped: usize,
    /// Per-folder counterpart of the three counters above, so one folder row can
    /// show its own numbers (analogous to `folder_done`). `folder_failed` stays
    /// the failure side.
    pub(crate) folder_new: HashMap<usize, usize>,
    pub(crate) folder_conflict: HashMap<usize, usize>,
    pub(crate) folder_unchanged: HashMap<usize, usize>,
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
    /// Backup-folder index of the queue entry `pending_conflict` belongs to —
    /// the `idx` the worker carries per item, and therefore an index into
    /// `FolderStore::folders`. Lets the dialog persist a remembered answer into
    /// that folder's `conflict_resolution` (user decision 2026-10-15).
    /// `usize::MAX` (`ONE_TIME_IDX`) for the transient drag & drop target,
    /// which has no folder to write to.
    pub(crate) pending_conflict_idx: Option<usize>,
    /// Full path of the file being asked about. The safety check for the index
    /// above: a folder removed mid-run shifts `folders`, so the index alone is
    /// no proof that this folder is really the one the file lives in.
    pub(crate) pending_conflict_path: Option<PathBuf>,
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

/// The folder card's status word (Variant A, user decision 2026-10-16): our six
/// `FolderStatus` values mapped onto the original client's row vocabulary
/// (`docs/SPEC_UPLOAD_PROGRESS.md` §2.2, `docs/UI_FLOWS.md` §11.2), coloured by
/// the same alert-level logic with our palette
/// (Success→GREEN, Error→RED, Info→TEAL, Warning→AMBER, Normal→MUTED):
///
/// | Status | Word | Level → colour |
/// |---|---|---|
/// | `Uploading` | „Upload-Vorgang läuft" | Info → TEAL |
/// | `Done` | „Datensicherung abgeschlossen" | Success → GREEN |
/// | `Paused` | „ANGEHALTEN" | Warning → AMBER |
/// | `Error` | „Fehler" | Error → RED |
/// | `Scanning` | „Konfiguration wird durchgeführt" | Info → TEAL |
/// | `Idle`, `total > 0` | „Warten" (the original's cold-boot waiting word) | Info → TEAL |
/// | `Idle`, `total == 0` | — (empty) | — |
///
/// The empty arm is the never-scanned trap (user decision 2026-10-16): a
/// folder that was never scanned has `total == 0` just like a scanned folder
/// with nothing to upload, and only the run's finalizers know which is which —
/// they settle a scanned-empty folder as `Done` (see `finalize_upload`), so an
/// `Idle` folder with `total == 0` was never scanned and must not claim any
/// state. The percentage lives in the card's second line
/// ([`folder_progress_text`]), not here — the original's primary status carries
/// no number either.
pub(crate) fn status_text(status: FolderStatus, total: usize) -> (String, Color32) {
    match status {
        FolderStatus::Uploading => (tr("Upload-Vorgang läuft").to_string(), theme::TEAL),
        FolderStatus::Done => (tr("Datensicherung abgeschlossen").to_string(), theme::GREEN),
        FolderStatus::Paused => (tr("ANGEHALTEN").to_string(), theme::AMBER),
        FolderStatus::Error => (tr("Fehler").to_string(), theme::RED),
        FolderStatus::Scanning => (tr("Konfiguration wird durchgeführt").to_string(), theme::TEAL),
        FolderStatus::Idle if total > 0 => (tr("Warten").to_string(), theme::TEAL),
        FolderStatus::Idle => (String::new(), theme::MUTED),
    }
}

/// Progress fragment of the folder card's second line (Variant A): the
/// original's Fortschrittszeile (`docs/SPEC_UPLOAD_PROGRESS.md` §2.2) —
/// `{0}% hochgeladen` while running, the original's own completion word
/// `Vollständig` at 100 % (`docs/UI_FLOWS.md` §11.5), `Angehalten bei {0} %`
/// while paused. Empty when the target has no total: a never-scanned folder
/// must not claim any progress (same trap as [`status_text`]'s empty arm).
pub(crate) fn folder_progress_text(status: FolderStatus, done: usize, total: usize) -> String {
    if total == 0 {
        return String::new();
    }
    let pct = format!("{:.0}", done as f32 / total as f32 * 100.0);
    match status {
        FolderStatus::Paused => tr1("Angehalten bei {} %", pct),
        _ if done >= total => tr("Vollständig").to_string(),
        _ => tr1("{}% hochgeladen", pct),
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

/// The run's outcome breakdown as a single line: **only** the non-zero
/// categories are shown, joined by ` · `. An all-zero result yields an empty
/// string, so the caller can omit the line entirely (idle state, or a target
/// this run never touched). Pure and side-effect free — no lock, no UI — and
/// therefore cheap to test.
pub(crate) fn outcome_text(
    new: usize,
    existing: usize,
    unchanged: usize,
    failed: usize,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if new > 0 {
        parts.push(tr1("{} neu hochgeladen", new));
    }
    if existing > 0 {
        parts.push(tr1("{} bereits im Konto", existing));
    }
    if unchanged > 0 {
        parts.push(tr1("{} unverändert", unchanged));
    }
    if failed > 0 {
        parts.push(tr1("{} Fehler", failed));
    }
    parts.join(" · ")
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
///
/// Returns `None` when the run was cancelled (sign-out) while waiting: the
/// worker then stops instead of hanging up to the 60 s timeout.
///
/// `idx` and `file_path` describe the queue entry the prompt is about; they are
/// snapshot into the shared state with the name, so the dialog can persist a
/// "don't ask again" into that folder's own policy.
async fn resolve_conflict(
    shared: &Arc<Mutex<UploadShared>>,
    idx: usize,
    file_name: &str,
    file_path: &Path,
) -> Option<ConflictResolution> {
    const POLL: Duration = Duration::from_millis(150);
    const TIMEOUT: Duration = Duration::from_secs(60);
    loop {
        // A sign-out cancels the run: stop waiting for an answer instead of
        // holding the worker (and the pending prompt) for up to 60 s.
        if shared.lock().unwrap().cancelled {
            return None;
        }
        // A run-wide decision (or a previous answer) needs no prompt.
        let ask_now = {
            let mut s = shared.lock().unwrap();
            if let Some(o) = s.conflict_override {
                return Some(o);
            }
            if s.pending_conflict.is_none() {
                s.pending_conflict = Some(file_name.to_string());
                // Which folder the asked file belongs to — written together with
                // the name, cleared on every exit below, so the dialog never
                // persists an answer into a folder it was not asked about.
                s.pending_conflict_idx = Some(idx);
                s.pending_conflict_path = Some(file_path.to_path_buf());
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
            if s.cancelled {
                // Drop the prompt so no orphaned dialog survives the abort.
                s.pending_conflict = None;
                s.pending_conflict_idx = None;
                s.pending_conflict_path = None;
                s.touch_queue();
                return None;
            }
            if let Some(answer) = s.conflict_answer.take() {
                s.pending_conflict = None;
                s.pending_conflict_idx = None;
                s.pending_conflict_path = None;
                if s.conflict_remember {
                    s.conflict_override = Some(answer);
                }
                s.touch_queue();
                return Some(answer);
            }
            if Instant::now() >= deadline {
                s.pending_conflict = None;
                s.pending_conflict_idx = None;
                s.pending_conflict_path = None;
                s.touch_queue();
                s.log(tr("Keine Antwort auf den Konflikt — wird übersprungen."));
                return Some(ConflictResolution::Ignore);
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
            // A paused run parks here: without this check a sign-out could never
            // end it (the worker would wait forever for a resume).
            if shared.lock().unwrap().cancelled {
                return;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        // A sign-out cancels the run: stop before starting another file. A
        // request already in flight is allowed to finish — never abort a
        // transfer mid-way.
        if shared.lock().unwrap().cancelled {
            return;
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
        // never panic; the queue is only mutated while `shared` is held). A row
        // can be worked on more than once (token-renewal requeue), so the
        // progress counter is reset here, not just at plan build time.
        {
            let mut s = shared.lock().unwrap();
            if let Some(e) = s.queue.get_mut(row) {
                e.state = FileState::Uploading;
                e.bytes_sent = 0;
            }
            s.touch_queue();
        }

        let (dedup_on, conflict) = folder_opts
            .get(&idx)
            .copied()
            .unwrap_or((true, ConflictResolution::default()));

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
        // and the hash: done for this folder, no network, no throttle. The
        // recorded **origin** decides the counter and the wording: a path that
        // was uploaded itself stays "unverändert", a recorded content skip keeps
        // reporting "bereits im Konto" in every run.
        let stat_origin = if dedup_on {
            dedup.lock().unwrap().stat_matches(&path, size, mtime)
        } else {
            None
        };
        if let Some(origin) = stat_origin {
            let mut s = shared.lock().unwrap();
            s.done += 1;
            *s.folder_done.entry(idx).or_insert(0) += 1;
            match origin {
                Origin::ContentSkip => {
                    s.conflict_skipped += 1;
                    *s.folder_conflict.entry(idx).or_insert(0) += 1;
                }
                Origin::Uploaded => {
                    s.unchanged_skipped += 1;
                    *s.folder_unchanged.entry(idx).or_insert(0) += 1;
                }
            }
            if let Some(en) = s.queue.get_mut(row) {
                en.state = FileState::Skipped;
                // No transfer happened: the file counts as fully "sent" so the
                // row never reads as partially uploaded (Befund 11c).
                en.bytes_sent = size;
            }
            // Settled for progress, but not transferred: it must not raise the
            // measured byte rate.
            s.bytes_done = s.bytes_done.saturating_add(size);
            s.touch_queue();
            s.log(match origin {
                Origin::ContentSkip => tr1(
                    "Bereits im Konto, nur umbenannt: {}",
                    crate::text::nfc(&path.display().to_string()),
                ),
                Origin::Uploaded => tr1(
                    "UEBERSPRUNGEN {}: bereits hochgeladen",
                    crate::text::nfc(&path.display().to_string()),
                ),
            });
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
                s.log(tr2(
                    "FEHLER {}: {}",
                    crate::text::nfc(&path.display().to_string()),
                    crate::text::nfc(&e.to_string()),
                ));
                continue;
            }
        };
        let md5sum = format!("{:x}", md5::compute(&bytes));
        let size = bytes.len() as u64;

        {
            let mut s = shared.lock().unwrap();
            s.current = crate::text::nfc(&path.display().to_string());
        }

        // Local pre-upload check (path + md5). A skip is a finished item: it is
        // counted as done for this folder and consumes no network and no throttle.
        // The recorded **origin** distinguishes "unverändert" (this path was
        // uploaded) from "bereits im Konto" (this path only remembers a content
        // skip), so the wording stays stable across runs.
        let skip_origin = if dedup_on {
            dedup.lock().unwrap().should_skip(&path, &md5sum, size, mtime)
        } else {
            None
        };
        if let Some(origin) = skip_origin {
            let mut s = shared.lock().unwrap();
            s.done += 1;
            *s.folder_done.entry(idx).or_insert(0) += 1;
            match origin {
                Origin::ContentSkip => {
                    s.conflict_skipped += 1;
                    *s.folder_conflict.entry(idx).or_insert(0) += 1;
                }
                Origin::Uploaded => {
                    s.unchanged_skipped += 1;
                    *s.folder_unchanged.entry(idx).or_insert(0) += 1;
                }
            }
            if let Some(en) = s.queue.get_mut(row) {
                en.state = FileState::Skipped;
            }
            // Settled for progress, but not transferred: it must not raise the
            // measured byte rate.
            s.bytes_done = s.bytes_done.saturating_add(size);
            s.touch_queue();
            s.log(match origin {
                Origin::ContentSkip => tr1(
                    "Bereits im Konto, nur umbenannt: {}",
                    crate::text::nfc(&path.display().to_string()),
                ),
                Origin::Uploaded => tr1(
                    "UEBERSPRUNGEN {}: bereits hochgeladen",
                    crate::text::nfc(&path.display().to_string()),
                ),
            });
            continue;
        }

        // Cross-path content dedup (user decision 2026-10-14): the MD5 is the
        // file's identity, so a file whose content was already uploaded under a
        // *different* path (renamed/moved) is skipped locally, before the
        // network. The same-path case above stays "unverändert"; this branch
        // means the content is already in the account — counted like the
        // server's 409.
        if dedup_on && dedup.lock().unwrap().content_seen(&md5sum) {
            // Remember the skip so the next scan takes the cheap stat path and
            // keeps reporting "bereits im Konto" (user decision 2026-10-14) —
            // without this, the file would be read and hashed again every run.
            dedup
                .lock()
                .unwrap()
                .record_content_skip(&path, &md5sum, size, mtime);
            let mut s = shared.lock().unwrap();
            s.done += 1;
            s.conflict_skipped += 1;
            *s.folder_done.entry(idx).or_insert(0) += 1;
            *s.folder_conflict.entry(idx).or_insert(0) += 1;
            if let Some(en) = s.queue.get_mut(row) {
                en.state = FileState::Skipped;
            }
            // Settled for progress, but not transferred: it must not raise the
            // measured byte rate.
            s.bytes_done = s.bytes_done.saturating_add(size);
            s.touch_queue();
            s.log(tr1(
                "Bereits im Konto, nur umbenannt: {}",
                crate::text::nfc(&path.display().to_string()),
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
        // again" for the rest of the run). `SaveAsNew` (`RENAME`) still yields a
        // 409 for a content duplicate (`errorCode = "DuplicatesConflictError"`):
        // that is the intended safety net, not a name clash.
        let mut resolution = {
            let s = shared.lock().unwrap();
            s.conflict_override.unwrap_or(conflict)
        };
        let mut existing_conflict_id: Option<String> = None;
        // The 409 cause (server `errorCode`, from the marker), so the outcome
        // can tell a content duplicate ("bereits im Konto", nothing sent from
        // this path) from a name conflict ("vorhanden", different content).
        let mut conflict_cause: Option<String> = None;
        // The account node this path is bound to in the local index, if any — the
        // safe-replace rule's input (user decision 2026-10-16). Read once before
        // the loop: the guard is dropped immediately and never held across an
        // `await`.
        let indexed_node_id = dedup.lock().unwrap().node_id_for(&path);
        // Set when the user explicitly answers "Ersetzen" in the conflict dialog:
        // that consent for *this* file bypasses the binding check.
        let mut replace_confirmed = false;
        // Per-file byte progress (Befund 11c): the POST streams the file and
        // counts the bytes handed to the transport. A small polling task copies
        // that counter into the queue row every 100 ms, so the Backup page can
        // show a real percentage while the row is `Uploading`. The task stops as
        // soon as `file_done` is set; the worker writes the authoritative final
        // value after joining it.
        let current_bytes = Arc::new(AtomicU64::new(0));
        let file_done = Arc::new(AtomicBool::new(false));
        let mut progress_task = tokio::spawn({
            let current_bytes = current_bytes.clone();
            let file_done = file_done.clone();
            let shared = shared.clone();
            async move {
                loop {
                    {
                        let mut s = shared.lock().unwrap();
                        if let Some(en) = s.queue.get_mut(row) {
                            en.bytes_sent = current_bytes.load(Ordering::Relaxed);
                        }
                        s.touch_queue();
                    }
                    if file_done.load(Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
        let outcome = loop {
            let basis = if replace_confirmed {
                ReplaceBasis::Confirmed
            } else {
                ReplaceBasis::Automatic(indexed_node_id.as_deref())
            };
            match uploader
                .upload_picture(
                    &path,
                    Some(&bytes),
                    Some(&md5sum),
                    resolution,
                    basis,
                    Some(current_bytes.clone()),
                )
                .await
            {
                Ok(id) => break FileOutcome::Uploaded(id),
                Err(e) => {
                    let Some(existing) = crate::prime_photos::conflict_node_id(&e) else {
                        break FileOutcome::Failed(e);
                    };
                    conflict_cause = crate::prime_photos::conflict_cause(&e).map(str::to_owned);
                    // Name conflict under `Replace` that the index does not bind to
                    // this path: a foreign file with the same name must not be
                    // overwritten (user decision 2026-10-16) — rename once instead
                    // of replacing. A content duplicate stays "bereits im Konto"
                    // (the block below).
                    let content_duplicate = conflict_cause
                        .as_deref()
                        .is_some_and(crate::prime_photos::is_content_duplicate_code);
                    if resolution == ConflictResolution::Replace && !content_duplicate && !replace_confirmed {
                        resolution = ConflictResolution::SaveAsNew;
                        continue;
                    }
                    if resolution != ConflictResolution::Ask {
                        // "Ignorieren": the file is already on the server.
                        existing_conflict_id = Some(existing.to_string());
                        break FileOutcome::ConflictSkipped;
                    }
                    let name = crate::text::nfc(
                        &path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                    // `idx` is this item's backup-folder index and `path` the
                    // file itself — the two facts the dialog needs to persist a
                    // "don't ask again" into that folder's policy.
                    let answer = match resolve_conflict(&shared, idx, &name, &path).await {
                        // A sign-out cancelled the run while the prompt was open:
                        // stop the worker here (the run must not resume after a
                        // logout). The progress task is stopped first — a detached
                        // task would keep writing to the queue forever.
                        None => {
                            file_done.store(true, Ordering::Relaxed);
                            let _ = (&mut progress_task).await;
                            return;
                        }
                        // The prompt can only answer with a concrete policy;
                        // normalise defensively so `Ask` can never bounce the
                        // file through this loop a second time.
                        Some(ConflictResolution::Ask) => ConflictResolution::Ignore,
                        Some(a) => a,
                    };
                    if answer == ConflictResolution::Ignore {
                        existing_conflict_id = Some(existing.to_string());
                        break FileOutcome::ConflictSkipped;
                    }
                    if answer == ConflictResolution::Replace {
                        // The explicit "Ersetzen" is the user's consent for this
                        // file: it bypasses the safe-replace binding check.
                        replace_confirmed = true;
                    }
                    // Re-upload the same file with the chosen policy.
                    resolution = answer;
                }
            }
        };
        // The transfer (and any conflict retries) is over: stop the polling task
        // and pin the row's progress to the file's real size, so a settled row
        // never reads as partially sent (Befund 11c). The join waits for at most
        // one poll interval; the final value is written by the worker itself and
        // therefore wins over anything the task wrote.
        file_done.store(true, Ordering::Relaxed);
        let _ = (&mut progress_task).await;
        {
            let mut s = shared.lock().unwrap();
            if let Some(en) = s.queue.get_mut(row) {
                en.bytes_sent = size;
            }
            s.touch_queue();
        }
        let mut requeued = false;
        match outcome {
            FileOutcome::Uploaded(id) => {
                // Remember the upload so the next scan skips it, and bind the
                // path to the node it landed on — that binding is what later
                // allows an automatic `Replace` of *this* file (user decision
                // 2026-10-16).
                dedup
                    .lock()
                    .unwrap()
                    .record_upload(&path, &md5sum, size, mtime, Some(&id));
                let mut s = shared.lock().unwrap();
                s.done += 1;
                s.uploaded_new += 1;
                *s.folder_done.entry(idx).or_insert(0) += 1;
                *s.folder_new.entry(idx).or_insert(0) += 1;
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
                s.log(format!(
                    "OK    {}",
                    crate::text::nfc(&path.display().to_string())
                ));
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
                        crate::text::nfc(&path.display().to_string()),
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
                    s.log(tr2(
                        "FEHLER {}: {}",
                        crate::text::nfc(&path.display().to_string()),
                        crate::text::nfc(&e),
                    ));
                }
            }
            FileOutcome::ConflictSkipped => {
                // 409 with `Ignore` (or an `Ask` answered with "Ignorieren"):
                // the file is already on the server. Finished and dedup-friendly
                // — not an error, and no bytes were transferred.
                //
                // Which 409 it was decides *how* it enters the local MD5 index
                // (the worker still needs it there: without an entry, every
                // later run would read, hash and re-send the whole file just to
                // get the same 409 again — exactly the wasted bandwidth this
                // whole path exists to avoid):
                //
                // - `DuplicatesConflictError`/`MD5_DUPLICATE` = content
                //   duplicate: nothing was ever uploaded *from this path*, so it
                //   must keep reading "bereits im Konto" (`Origin::ContentSkip`).
                //   Recording it as a real upload was a bug — the next run then
                //   reported "unverändert" for a path that never uploaded
                //   anything.
                // - `NameAlreadyExists` (and anything unknown/empty) = name
                //   conflict with *different* content: the existing behaviour
                //   (`Origin::Uploaded`) stands — here the path itself was
                //   accepted as present.
                let content_duplicate = conflict_cause
                    .as_deref()
                    .is_some_and(crate::prime_photos::is_content_duplicate_code);
                {
                    let dedup = dedup.lock().unwrap();
                    if content_duplicate {
                        dedup.record_content_skip(&path, &md5sum, size, mtime);
                    } else {
                        // The 409 names the node of an existing file — possibly a
                        // *foreign* one that merely shares the name, so it must
                        // not be bound to this path (user decision 2026-10-16).
                        dedup.record_upload(&path, &md5sum, size, mtime, None);
                    }
                }
                let mut s = shared.lock().unwrap();
                s.done += 1;
                s.conflict_skipped += 1;
                *s.folder_done.entry(idx).or_insert(0) += 1;
                *s.folder_conflict.entry(idx).or_insert(0) += 1;
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
                if content_duplicate {
                    // Same cause and same wording as the local cross-path
                    // content dedup above.
                    s.log(tr1(
                        "Bereits im Konto, nur umbenannt: {}",
                        crate::text::nfc(&path.display().to_string()),
                    ));
                } else {
                    s.log(tr1(
                        "Vorhanden, übersprungen: {}",
                        crate::text::nfc(&path.display().to_string()),
                    ));
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
                        if shared.lock().unwrap().cancelled {
                            return;
                        }
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

    // A new run clears a previous cancellation: without this, a run started
    // after a sign-out-cancelled one would return at its first wait point.
    shared.lock().unwrap().cancelled = false;

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
            let up = PrimePhotos::new(&cfg, false);
            if let Ok(c) = up.get_album(&name).await {
                let _ = up.add_to_album(c.id, &ids).await;
            }
        }
    }

    let (done, failed, total, cancelled) = {
        let mut s = shared.lock().unwrap();
        s.active = false;
        s.started = None;
        s.current.clear();
        (s.done, s.failed, s.total, s.cancelled)
    };
    if cancelled {
        // Files a worker returned on are still marked `Uploading`; reset them so
        // the table does not show a stuck "running" row after the abort.
        {
            let mut s = shared.lock().unwrap();
            reset_in_flight(&mut s.queue);
            s.touch_queue();
        }
        let open = total.saturating_sub(done + failed);
        shared.lock().unwrap().log(tr3(
            "Abgebrochen: {} erfolgreich, {} fehlgeschlagen, {} offen.",
            done,
            failed,
            open,
        ));
    } else {
        shared
            .lock()
            .unwrap()
            .log(tr2("Fertig: {} erfolgreich, {} fehlgeschlagen.", done, failed));
    }
}

/// Reset queue entries still marked [`FileState::Uploading`] to
/// [`FileState::Waiting`] — a cancelled run leaves the file a worker returned on
/// in that state, which would otherwise render as a stuck "running" row.
fn reset_in_flight(queue: &mut [QueueEntry]) {
    for e in queue {
        if e.state == FileState::Uploading {
            e.state = FileState::Waiting;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The assertions compare against the same `tr*` helpers the code under test
    // uses, so they hold in either UI language without touching the global
    // language (other tests run in parallel) — the pattern `src/notify.rs`
    // already uses. Every branch and every substituted counter is pinned.

    #[test]
    fn outcome_is_empty_when_nothing_happened() {
        // All-zero: the caller omits the line entirely (idle / untouched target).
        assert_eq!(outcome_text(0, 0, 0, 0), "");
    }

    #[test]
    fn outcome_reports_a_clean_success_run() {
        // Only real uploads: exactly one category, no zero noise.
        assert_eq!(outcome_text(5, 0, 0, 0), tr1("{} neu hochgeladen", 5));
    }

    #[test]
    fn outcome_reports_a_mixed_run_with_every_category() {
        let text = outcome_text(3, 2, 1, 4);
        let expected = [
            tr1("{} neu hochgeladen", 3),
            tr1("{} bereits im Konto", 2),
            tr1("{} unverändert", 1),
            tr1("{} Fehler", 4),
        ]
        .join(" · ");
        assert_eq!(text, expected);
        for needle in ["3", "2", "1", "4"] {
            assert!(text.contains(needle), "counter {needle} missing: {text}");
        }
    }

    #[test]
    fn outcome_reports_a_failure_only_run() {
        assert_eq!(outcome_text(0, 0, 0, 2), tr1("{} Fehler", 2));
    }

    #[test]
    fn outcome_omits_zero_categories() {
        // A run that uploaded new files and skipped existing ones must not list
        // "0 unverändert" / "0 Fehler".
        let text = outcome_text(1, 1, 0, 0);
        assert!(!text.contains("0"), "zero category leaked: {text}");
        assert!(text.contains(tr1("{} neu hochgeladen", 1).as_str()));
        assert!(text.contains(tr1("{} bereits im Konto", 1).as_str()));
    }

    #[test]
    fn reset_in_flight_returns_uploading_entries_to_waiting() {
        // A cancelled run leaves the file a worker returned on in `Uploading`;
        // that row must not stay stuck as "running" after the abort. Settled
        // entries (Done/Failed/Skipped) are untouched.
        let mut queue = vec![
            QueueEntry {
                folder: 0,
                name: "a.jpg".into(),
                state: FileState::Uploading,
                size: 1,
                path: PathBuf::from("/a.jpg"),
                bytes_sent: 0,
            },
            QueueEntry {
                folder: 0,
                name: "b.jpg".into(),
                state: FileState::Done,
                size: 2,
                path: PathBuf::from("/b.jpg"),
                bytes_sent: 0,
            },
            QueueEntry {
                folder: 1,
                name: "c.jpg".into(),
                state: FileState::Uploading,
                size: 3,
                path: PathBuf::from("/c.jpg"),
                bytes_sent: 0,
            },
            QueueEntry {
                folder: 1,
                name: "d.jpg".into(),
                state: FileState::Failed,
                size: 4,
                path: PathBuf::from("/d.jpg"),
                bytes_sent: 0,
            },
        ];
        reset_in_flight(&mut queue);
        assert!(queue[0].state == FileState::Waiting);
        assert!(queue[2].state == FileState::Waiting);
        assert!(queue[1].state == FileState::Done);
        assert!(queue[3].state == FileState::Failed);
    }

    #[test]
    fn calculate_pct_is_clamped_and_needs_a_known_size() {
        // Befund 11c: the per-file percentage shown next to "In Bearbeitung".
        // An unknown size (0) must yield 0 — never a division or a bogus 100 %.
        assert_eq!(calculate_pct(0, 123), 0);
        assert_eq!(calculate_pct(0, 0), 0);
        // Nothing sent yet, half sent, fully sent.
        assert_eq!(calculate_pct(100, 0), 0);
        assert_eq!(calculate_pct(100, 42), 42);
        assert_eq!(calculate_pct(100, 100), 100);
        // Overshoot (a retry can briefly count more) clamps to 100, it never
        // reports more than "done".
        assert_eq!(calculate_pct(100, 250), 100);
        // Integer truncation, not rounding.
        assert_eq!(calculate_pct(3, 1), 33);
    }

    #[test]
    fn the_conflict_cause_decides_the_index_origin() {
        // Pins the `ConflictSkipped` branch without any network: a content
        // duplicate (nothing was ever uploaded from this path) must take the
        // content-skip marker, everything else the plain upload entry.
        let decode = |err: &str| {
            crate::prime_photos::conflict_cause(err)
                .is_some_and(crate::prime_photos::is_content_duplicate_code)
        };
        assert!(decode(&crate::prime_photos::conflict_error(
            "DuplicatesConflictError",
            "N1"
        )));
        assert!(decode(&crate::prime_photos::conflict_error(
            "MD5_DUPLICATE",
            "N1"
        )));
        assert!(!decode(&crate::prime_photos::conflict_error(
            "NameAlreadyExists",
            "N1"
        )));
        // Unknown or empty cause must behave like a name conflict.
        assert!(!decode(&crate::prime_photos::conflict_error("", "N1")));
        assert!(!decode(&crate::prime_photos::conflict_error(
            "SomethingElse",
            "N1"
        )));
    }

    // -- status word + progress fragment of the folder card (Variant A) -----

    #[test]
    fn status_words_map_our_statuses_onto_the_original_vocabulary() {
        // The mapping is the original's row vocabulary (UI_FLOWS.md §11.2);
        // each word is asserted exactly once, so a renamed key cannot drift.
        assert_eq!(status_text(FolderStatus::Uploading, 0).0, tr("Upload-Vorgang läuft"));
        assert_eq!(
            status_text(FolderStatus::Done, 0).0,
            tr("Datensicherung abgeschlossen")
        );
        assert_eq!(status_text(FolderStatus::Paused, 0).0, tr("ANGEHALTEN"));
        assert_eq!(status_text(FolderStatus::Error, 0).0, tr("Fehler"));
        assert_eq!(
            status_text(FolderStatus::Scanning, 0).0,
            tr("Konfiguration wird durchgeführt")
        );
    }

    #[test]
    fn idle_words_waiting_only_when_the_folder_knows_a_total() {
        // Cold boot with a known file count: the original's waiting word.
        assert_eq!(status_text(FolderStatus::Idle, 42).0, tr("Warten"));
        // The never-scanned trap: `total == 0` must claim nothing.
        let (empty, _) = status_text(FolderStatus::Idle, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn status_colours_follow_the_alert_levels() {
        use theme::{AMBER, GREEN, RED, TEAL};
        assert_eq!(status_text(FolderStatus::Uploading, 0).1, TEAL);
        assert_eq!(status_text(FolderStatus::Done, 0).1, GREEN);
        assert_eq!(status_text(FolderStatus::Paused, 0).1, AMBER);
        assert_eq!(status_text(FolderStatus::Error, 0).1, RED);
        assert_eq!(status_text(FolderStatus::Scanning, 0).1, TEAL);
    }

    #[test]
    fn progress_is_empty_without_a_total() {
        // Same trap as the status word: no total, no claim.
        assert_eq!(folder_progress_text(FolderStatus::Done, 0, 0), "");
        assert_eq!(folder_progress_text(FolderStatus::Uploading, 0, 0), "");
    }

    #[test]
    fn progress_reports_a_percentage_while_running() {
        assert_eq!(
            folder_progress_text(FolderStatus::Uploading, 1, 2),
            tr1("{}% hochgeladen", "50")
        );
    }

    #[test]
    fn progress_says_vollstaendig_at_hundred_percent() {
        // The original's own completion word, not "100% hochgeladen"
        // (UI_FLOWS.md §11.5).
        assert_eq!(folder_progress_text(FolderStatus::Done, 10, 10), tr("Vollständig"));
        // Reaching 100 % mid-run already counts as complete.
        assert_eq!(folder_progress_text(FolderStatus::Uploading, 7, 7), tr("Vollständig"));
    }

    #[test]
    fn progress_keeps_the_paused_wording() {
        assert_eq!(
            folder_progress_text(FolderStatus::Paused, 1, 4),
            tr1("Angehalten bei {} %", "25")
        );
    }
}

