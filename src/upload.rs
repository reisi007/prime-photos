use crate::config::Config;
use crate::dedup::{DedupIndex, Origin};
use crate::destination::{machine_name, mirrored_path_components};
use crate::folders::{ConflictResolution, FolderStatus};
use crate::i18n::{tr, tr1, tr2, tr3, tr_n};
use crate::prime_photos::PrimePhotos;
use crate::theme;
use eframe::egui::{self, Color32};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::task::JoinSet;

// ---------------------------------------------------------------------------
// Shared upload state (written by the async workers, read by the UI)
// ---------------------------------------------------------------------------

/// Coarse per-file state of a queue entry. The upload path streams the body and
/// counts the bytes it hands to the transport, so an `Uploading` row also shows
/// a real per-file percentage (Befund 11c); `docs/original/SPEC_UPLOAD_PROGRESS.md` §4.2
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
    /// Used by the Backup page for the per-row folder column — the queue is
    /// ordered globally by status, not grouped by folder.
    pub(crate) folder: usize,
    pub(crate) name: String,
    pub(crate) state: FileState,
    /// File size in bytes, read once while the plan is built. `0` when the file
    /// could not be stat'ed — the queue's size column then shows `0 B`.
    pub(crate) size: u64,
    /// Full path, shown as the name column's hover tooltip
    /// (docs/original/SPEC_UPLOAD_PROGRESS.md §7.3).
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
    /// The persisted store was updated outside the UI; the UI must reload its copy.
    pub(crate) store_dirty: bool,
    /// UI context used to wake an on-demand UI after a background store update.
    pub(crate) ctx: Option<egui::Context>,
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
    /// Plan-time index → folder path, so finalizers still find the right row
    /// if the persisted store's indices shift during the run. `ONE_TIME_IDX`
    /// is synthetic and has no path.
    pub(crate) folder_paths: HashMap<usize, PathBuf>,
    /// Configured source roots whose mirrored cloud folder must be resolved,
    /// including empty backups and folders injected into a live run.
    pub(crate) destination_roots: HashMap<usize, PathBuf>,
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
    /// The live worker queue of the current run, in `items` order. `None` while no
    /// run exists; the run's own `Arc` so an injection from outside the run can
    /// append to the very same queue the workers drain.
    pub(crate) item_queue: Option<Arc<Mutex<VecDeque<(usize, usize, PathBuf)>>>>,
    /// Files whose **first** file-level attempt failed (upload error or local
    /// read error). Filled by the initial workers and drained by `run_upload`
    /// before the run's one finite retry pass. Deliberately a separate list, not
    /// another trip through `item_queue`: without an attempt guard a persistent
    /// failure would requeue itself forever. Successes and resolved
    /// conflicts/dedup skips never land here.
    pub(crate) retry_items: Vec<(usize, usize, PathBuf)>,
    /// Live per-folder options of the current run. Was an immutable `Arc<HashMap>`
    /// handed to the workers at start; an injected folder must be able to add its
    /// own entry while the run already uploads.
    pub(crate) folder_opts: HashMap<usize, (bool, ConflictResolution)>,
    /// Number of folder scans that were accepted and are still in flight (an
    /// injection may land at any moment). While > 0 a worker that finds the queue
    /// empty keeps the run open instead of ending it — otherwise an injection could
    /// land in a queue nobody drains any more.
    pub(crate) scans_in_flight: usize,
    /// The run stopped accepting work: set **under the shared lock** by the worker
    /// that finds the queue empty with no scan in flight. Together with
    /// `scans_in_flight` this is what makes an injection either land before the last
    /// worker exits or be refused — never to land in a dead queue.
    pub(crate) sealed: bool,
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
    /// Interactive name-conflict prompt (docs/original/SPEC_DEDUP.md §4.1): the file name
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
    /// Consume the marker that a background path persisted a new store state.
    pub(crate) fn take_store_dirty(&mut self) -> bool {
        std::mem::take(&mut self.store_dirty)
    }

    /// Consume the marker only after a successful reload, and only while idle.
    pub(crate) fn consume_store_dirty_after_reload(&mut self, loaded: bool) -> bool {
        loaded && !self.active && self.take_store_dirty()
    }

    /// Append to the (bounded) run log. Old lines are dropped oldest-first, the
    /// panel stays a scrolling tail.
    pub(crate) fn log(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 500 {
            self.log.drain(0..100);
        }
    }
}

/// A persisted store reload is safe only once no upload run is active.
pub(crate) fn should_reload_store(active: bool, dirty: bool) -> bool {
    !active && dirty
}

/// The folder card's status word (Variant A, user decision 2026-10-16): our six
/// `FolderStatus` values mapped onto the original client's row vocabulary
/// (`docs/original/SPEC_UPLOAD_PROGRESS.md` §2.2, `docs/original/UI_FLOWS.md` §11.2), coloured by
/// the same alert-level logic with our palette
/// (Success→GREEN, Error→RED, Info→TEAL, Warning→AMBER, Normal→MUTED):
///
/// | Status | Word | Level → colour |
/// |---|---|---|
/// | `Uploading` | „Upload-Vorgang läuft" | Info → TEAL |
/// | `Done` | „Datensicherung abgeschlossen" | Success → GREEN |
/// | `Paused` | „ANGEHALTEN" | Warning → AMBER |
/// | `NotFound` | „Ordner nicht gefunden" | Warning → AMBER |
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
        FolderStatus::NotFound => (tr("Ordner nicht gefunden").to_string(), theme::AMBER),
        FolderStatus::Error => (tr("Fehler").to_string(), theme::RED),
        FolderStatus::Scanning => (tr("Konfiguration wird durchgeführt").to_string(), theme::TEAL),
        FolderStatus::Idle if total > 0 => (tr("Warten").to_string(), theme::TEAL),
        FolderStatus::Idle => (String::new(), theme::MUTED),
    }
}

/// Progress fragment of the folder card's second line (Variant A): the
/// original's Fortschrittszeile (`docs/original/SPEC_UPLOAD_PROGRESS.md` §2.2) —
/// `{0}% hochgeladen` while running, the original's own completion word
/// `Vollständig` at 100 % (`docs/original/UI_FLOWS.md` §11.5), `Angehalten bei {0} %`
/// while paused. Empty when the target has no total: a never-scanned folder
/// must not claim any progress (same trap as [`status_text`]'s empty arm) —
/// and empty for `NotFound`: a missing source must not claim any progress
/// either, regardless of the last known total.
pub(crate) fn folder_progress_text(status: FolderStatus, done: usize, total: usize) -> String {
    if total == 0 || status == FolderStatus::NotFound {
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

/// Summary for the active upload shown in the collapsible log header.
/// An active scan with no transfer candidates uses the same wording as the
/// main progress area and its start log, without an irrelevant ETA.
pub(crate) fn active_log_summary(
    shared: &UploadShared,
    with_candidates: impl FnOnce() -> String,
) -> String {
    if shared.total == 0 {
        return tr("Alles aktuell").to_string();
    }
    with_candidates()
}

/// Status text for a live run, e.g. `"42% · noch 7 Dateien · ca. 1:20"`, or
/// `"Alles aktuell"` when it has no transfer candidates.
pub(crate) fn upload_progress_text(shared: &UploadShared) -> String {
    if shared.active && shared.total == 0 {
        return tr("Alles aktuell").to_string();
    }
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

/// Poll interval of an idle run that is still waiting for a scan in flight
/// (`scans_in_flight > 0`) to be folded in.
const QUEUE_IDLE_POLL: Duration = Duration::from_millis(150);

/// Which file-level attempt of the run a worker is performing. A run makes at
/// most two: the initial pass over the queue, then one retry pass over exactly
/// the files whose initial attempt failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttemptStage {
    Initial,
    Retry,
}

/// Outcome of one file's upload attempt, after the conflict policy has been
/// applied (`Ignore`/`Ask→Ignore` become `ConflictSkipped`, everything else is
/// an upload or a real failure).
enum FileOutcome {
    Uploaded(String),
    /// A name conflict the policy resolved as "keep the existing file".
    ConflictSkipped,
    Failed(String),
}

/// The run's one-retry policy as a pure function: only a **first-pass failure**
/// goes into the retry pass. A successful upload or a conflict/dedup skip is
/// final immediately and must never be retried, and a failure of the retry
/// attempt is final too. Keeping this decision pure makes the selection and the
/// "final failure counted once" rule testable without the network.
fn retry_after(outcome: &FileOutcome, stage: AttemptStage) -> bool {
    matches!(outcome, FileOutcome::Failed(_)) && stage == AttemptStage::Initial
}

/// Settle one failed file-level attempt on the shared state: defer it to the
/// run's retry pass (no counter change yet, row back to `Waiting`) or count the
/// one final failure. Called from both failure paths — upload errors and local
/// read errors — so the retry rule cannot drift between them.
fn settle_failed_attempt(
    s: &mut UploadShared,
    stage: AttemptStage,
    row: usize,
    idx: usize,
    path: &Path,
    reason: &str,
) {
    let path_text = crate::text::nfc(&path.display().to_string());
    let reason = crate::text::nfc(reason);
    // A local read error is a file-level failure like an upload error, so it
    // goes through the same policy.
    let failure = FileOutcome::Failed(reason.clone());
    if retry_after(&failure, stage) {
        if let Some(en) = s.queue.get_mut(row) {
            en.state = FileState::Waiting;
            en.bytes_sent = 0;
        }
        s.retry_items.push((row, idx, path.to_path_buf()));
        s.log(tr2(
            "FEHLER {}: {} — wird erneut versucht.",
            path_text,
            reason,
        ));
    } else {
        s.failed += 1;
        *s.folder_failed.entry(idx).or_insert(0) += 1;
        if let Some(en) = s.queue.get_mut(row) {
            en.state = FileState::Failed;
        }
        s.log(tr2("FEHLER {}: {}", path_text, reason));
    }
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
                return None;
            }
            if let Some(answer) = s.conflict_answer.take() {
                s.pending_conflict = None;
                s.pending_conflict_idx = None;
                s.pending_conflict_path = None;
                if s.conflict_remember {
                    s.conflict_override = Some(answer);
                }
                return Some(answer);
            }
            if Instant::now() >= deadline {
                s.pending_conflict = None;
                s.pending_conflict_idx = None;
                s.pending_conflict_path = None;
                s.log(tr("Keine Antwort auf den Konflikt — wird übersprungen."));
                return Some(ConflictResolution::Ignore);
            }
        }
    }
}

/// Whether a worker that just found the queue empty may end the run.
///
/// The decision runs **under the shared lock** and re-checks the queue: between
/// a worker's empty pop and this call an injection may have appended rows
/// (`inject_into_live_run`) — sealing then would leave them in a queue nobody
/// drains (`workers == 1`), the exact failure class this wave removes. `sealed`
/// is set only when the queue is still empty and no scan is in flight; together
/// with `inject_into_live_run`'s `sealed` check under the same lock, an injection
/// either lands before the last worker exits or is refused — never in a dead
/// queue.
fn seal_now(shared: &Arc<Mutex<UploadShared>>) -> bool {
    let mut s = shared.lock().unwrap();
    // Lock order `shared` → `item_queue`, same as the pop above.
    let queue_empty = s
        .item_queue
        .as_ref()
        .map_or(true, |q| q.lock().unwrap().is_empty());
    if s.scans_in_flight > 0 || !queue_empty {
        false
    } else {
        s.sealed = true;
        true
    }
}

async fn upload_worker(
    shared: Arc<Mutex<UploadShared>>,
    cfg: Config,
    destination_resolver: Arc<AsyncMutex<PrimePhotos>>,
    machine_name: String,
    mut pause_rx: watch::Receiver<bool>,
    limit_kbps: u32,
    dedup: Arc<Mutex<DedupIndex>>,
    stage: AttemptStage,
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
        let item = {
            let s = shared.lock().unwrap();
            s.item_queue
                .as_ref()
                .and_then(|q| q.lock().unwrap().pop_front())
        };
        let Some((row, idx, path)) = item else {
            if let Some((root_idx, root_path)) = take_pending_destination_root(&shared) {
                let result = mirrored_path_components(&root_path, &machine_name)
                    .map_err(|error| format!("{}: {error}", root_path.display()));
                let result = match result {
                    Ok(components) => {
                        destination_resolver
                            .lock()
                            .await
                            .resolve_folder_path(&components)
                            .await
                    }
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    shared.lock().unwrap().log(tr1(
                        "Upload-Ziel konnte nicht aufgelöst werden: {}",
                        error,
                    ));
                    let mut s = shared.lock().unwrap();
                    *s.folder_failed.entry(root_idx).or_insert(0) += 1;
                }
                continue;
            }
            // Keine Arbeit *gerade jetzt*: den Lauf nur beenden, wenn kein Scan mehr
            // unterwegs ist **und** die Queue immer noch leer ist. `seal_now`
            // entscheidet und versiegelt unter derselben Sperre wie die Injektion,
            // damit eine späte Einhängung niemals in eine tote Queue laufen kann.
            if seal_now(&shared) {
                break;
            }
            tokio::time::sleep(QUEUE_IDLE_POLL).await;
            continue;
        };

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
        }

        let (dedup_on, conflict) = {
            shared
                .lock()
                .unwrap()
                .folder_opts
                .get(&idx)
                .copied()
                .unwrap_or((true, ConflictResolution::default()))
        };

        // Resolve the actual cloud parent before stat/dedupe checks. This also
        // covers files injected into a live run after its initial preflight.
        let parent_node_id = match path.parent() {
            Some(parent) => match mirrored_path_components(parent, &machine_name) {
                Ok(components) => {
                    destination_resolver
                        .lock()
                        .await
                        .resolve_folder_path(&components)
                        .await
                }
                Err(error) => Err(error),
            },
            None => Err("Die Upload-Datei hat keinen lokalen Elternordner.".to_string()),
        };
        let parent_node_id = match parent_node_id {
            Ok(parent_node_id) => parent_node_id,
            Err(error) => {
                let reason = tr1("Upload-Ziel konnte nicht aufgelöst werden: {}", error);
                let mut s = shared.lock().unwrap();
                settle_failed_attempt(&mut s, stage, row, idx, &path, &reason);
                continue;
            }
        };

        // Stat before reading (user decision 2026-10-14): the recorded
        // `size`/`mtime` decide the cheap skip path, so an already-uploaded,
        // unchanged file is never read or hashed at all. The MD5 stays the
        // duplicate identity (docs/original/SPEC_DEDUP.md §1) — a stat hit only
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
        // duplicate identity (docs/original/SPEC_DEDUP.md §1) and the bytes are handed to
        // upload_picture so the file is never read twice.
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                let mut s = shared.lock().unwrap();
                let reason = e.to_string();
                settle_failed_attempt(&mut s, stage, row, idx, &path, &reason);
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
        // Conflict policy (docs/original/SPEC_DEDUP.md §4.1): a 409 is reported by the
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
                    }
                    if file_done.load(Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
        let outcome = loop {
            match uploader
                .upload_picture(
                    &path,
                    &parent_node_id,
                    Some(&bytes),
                    Some(&md5sum),
                    resolution,
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
        }
        let mut requeued = false;
        match outcome {
            FileOutcome::Uploaded(id) => {
                // Remember the upload so the next scan skips it, and retain the
                // successful node id in the local index. Replace targets are
                // selected from the server's 409 response, not gated by this
                // persisted binding.
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
                    {
                        let s = shared.lock().unwrap();
                        if let Some(q) = s.item_queue.as_ref() {
                            q.lock().unwrap().push_front((row, idx, path.clone()));
                        }
                    }
                    let mut s = shared.lock().unwrap();
                    if let Some(en) = s.queue.get_mut(row) {
                        en.state = FileState::Waiting;
                    }
                    s.log(tr1(
                        "Anmeldung abgelaufen, {} wird erneut versucht.",
                        crate::text::nfc(&path.display().to_string()),
                    ));
                    requeued = true;
                } else {
                    let mut s = shared.lock().unwrap();
                    settle_failed_attempt(&mut s, stage, row, idx, &path, &e);
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

fn take_pending_destination_root(
    shared: &Arc<Mutex<UploadShared>>,
) -> Option<(usize, PathBuf)> {
    let mut s = shared.lock().unwrap();
    let idx = *s.destination_roots.keys().min()?;
    s.destination_roots
        .remove(&idx)
        .map(|path| (idx, path))
}

/// The retry pass's file list: exactly the initial pass's failures, or nothing
/// at all for a cancelled/sign-out run. A cancelled run must not turn deferred
/// files into final failures — they stay open for the next start like every
/// other abandoned item (user decision 2026-10-16). Pure so the cancellation
/// branch is unit-testable without a network or a real run.
fn retry_pass_items(shared: &mut UploadShared) -> Vec<(usize, usize, PathBuf)> {
    if shared.cancelled {
        Vec::new()
    } else {
        std::mem::take(&mut shared.retry_items)
    }
}

/// One bounded worker generation: spawn `workers` workers for `stage` and wait
/// until every one has left. The retry pass uses the same function, so
/// concurrency, pause handling and the throttle stay identical to the initial
/// pass. The pass is finite by construction: only `run_upload` starts the retry
/// generation, and only once.
async fn run_worker_pass(
    shared: &Arc<Mutex<UploadShared>>,
    cfg: &Config,
    paused: &watch::Sender<bool>,
    limit_kbps: u32,
    dedup: &Arc<Mutex<DedupIndex>>,
    destination_resolver: &Arc<AsyncMutex<PrimePhotos>>,
    machine_name: &str,
    stage: AttemptStage,
    workers: usize,
) {
    let mut set = JoinSet::new();
    for _ in 0..workers {
        let s = shared.clone();
        let c = cfg.clone();
        let p = paused.subscribe();
        let d = dedup.clone();
        let resolver = destination_resolver.clone();
        let machine_name = machine_name.to_string();
        set.spawn(async move {
            upload_worker(s, c, resolver, machine_name, p, limit_kbps, d, stage).await;
        });
    }
    while set.join_next().await.is_some() {}
}

/// Mark all queued files failed when a run-level destination could not be
/// resolved. Sealing under the same lock used by injection prevents new work
/// from slipping into a run that will never start its workers.
///
/// A configured root with **no** queued files (an empty backup or a fully
/// deduplicated plan) would otherwise end the run without a single marker:
/// `apply_folder_progress` derives the row status from `folder_failed` and
/// would mark it `Done`. Every such root gets exactly one marker — roots whose
/// drained files already counted a failure are left as they are, so no folder
/// is counted twice. Runs without configured roots (the one-time drop) keep the
/// purely item-based behaviour above.
fn fail_destination_preflight(
    shared: &Arc<Mutex<UploadShared>>,
    queue: &Arc<Mutex<VecDeque<(usize, usize, PathBuf)>>>,
    reason: &str,
) {
    let mut s = shared.lock().unwrap();
    s.sealed = true;
    let configured_roots: Vec<usize> = s.destination_roots.keys().copied().collect();
    s.destination_roots.clear();
    s.log(tr1("Upload-Ziel konnte nicht aufgelöst werden: {}", reason));
    let pending: Vec<_> = queue.lock().unwrap().drain(..).collect();
    for (row, idx, path) in pending {
        s.failed += 1;
        *s.folder_failed.entry(idx).or_insert(0) += 1;
        if let Some(entry) = s.queue.get_mut(row) {
            entry.state = FileState::Failed;
        }
        s.log(tr1(
            "FEHLER {}: Upload wegen fehlendem Ziel abgebrochen.",
            crate::text::nfc(&path.display().to_string()),
        ));
    }
    // Run-level marker, not a file failure: `failed` stays untouched, exactly
    // like the per-root failure a worker records when a late injection's
    // destination cannot be resolved.
    for idx in configured_roots {
        let failures = s.folder_failed.entry(idx).or_insert(0);
        if *failures == 0 {
            *failures = 1;
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
    destination_roots: HashMap<usize, PathBuf>,
) {
    let count = items.len();
    let preflight_items = items.clone();
    let queue = Arc::new(Mutex::new(VecDeque::from(items)));
    let workers = concurrency.max(1).min(count.max(1));

    // A new run clears a previous cancellation: without this, a run started
    // after a sign-out-cancelled one would return at its first wait point.
    shared.lock().unwrap().cancelled = false;

    // One shared persistent MD5 index for the whole run (opened here, lives in
    // ~/Library/Application Support/prime_photos/dedup.sqlite3).
    let dedup = Arc::new(Mutex::new(DedupIndex::open()));

    // Publish the run's live fields (Welle 5.31): the very queue the workers
    // drain, the per-folder options and the scan-in-flight/sealed handshake — set
    // together, under one lock, before any worker starts, so an injection from
    // outside the run (`inject_into_live_run`) appends to the same queue the
    // workers read and cannot land in a dead one. The retry list belongs to the
    // run too: a previous run's deferred failures must never leak into this one.
    {
        let mut s = shared.lock().unwrap();
        s.item_queue = Some(queue.clone());
        s.retry_items.clear();
        s.folder_opts = folder_opts;
        s.sealed = false;
        s.destination_roots = destination_roots.clone();
    }

    // Resolve selected backup roots even if the scan found no files. Resolve
    // every initial file parent as well, before any worker (and therefore any
    // file upload) starts. BTreeSet collapses duplicate destinations and keeps
    // path creation deterministic; the shared resolver handles later injections.
    let destination_preflight = async {
        let machine_name = machine_name()?;
        let mut local_targets: Vec<PathBuf> = destination_roots.values().cloned().collect();
        for (_, _, path) in &preflight_items {
            let parent = path
                .parent()
                .ok_or_else(|| "Die Upload-Datei hat keinen lokalen Elternordner.".to_string())?;
            local_targets.push(parent.to_path_buf());
        }
        let mut cloud_paths = BTreeSet::new();
        for path in local_targets {
            let components = mirrored_path_components(&path, &machine_name).map_err(|error| {
                format!("{}: {error}", path.display())
            })?;
            cloud_paths.insert(components);
        }
        let resolver = Arc::new(AsyncMutex::new(PrimePhotos::new(&cfg, false)));
        for components in cloud_paths {
            resolver
                .lock()
                .await
                .resolve_folder_path(&components)
                .await?;
        }
        {
            let mut s = shared.lock().unwrap();
            for idx in destination_roots.keys() {
                s.destination_roots.remove(idx);
            }
        }
        Ok::<_, String>((resolver, machine_name))
    }
    .await;

    let destination_preflight = match destination_preflight {
        Ok((resolver, machine_name)) => Some((resolver, machine_name)),
        Err(error) => {
            fail_destination_preflight(&shared, &queue, &error);
            // No worker starts after a path or cloud-target failure: never fall
            // back to the former Pictures/Web node. Continue through the common
            // run finalizer so the UI sees a completed failed run.
            None
        }
    };

    if let Some((destination_resolver, machine_name)) = &destination_preflight {
        run_worker_pass(
            &shared,
            &cfg,
            &paused,
            limit_kbps,
            &dedup,
            destination_resolver,
            machine_name,
            AttemptStage::Initial,
            workers,
        )
        .await;
    }

    // One finite second pass over exactly the files whose initial attempt
    // failed (upload error or local read error). The run stays active, so the
    // UI, counters, tray outcome and notification only settle afterwards. A
    // cancelled/sign-out run gets no retry pass: its deferred files stay open
    // and are picked up by the next start, like every other abandoned item —
    // they must not turn into final failures. `sealed` stays set, so a folder
    // scan cannot inject new first-attempt work into the retry generation.
    let retry_items = {
        let mut s = shared.lock().unwrap();
        retry_pass_items(&mut s)
    };
    if !retry_items.is_empty() && destination_preflight.is_some() {
        let n = retry_items.len();
        {
            let mut s = shared.lock().unwrap();
            s.log(tr1("{} Datei(en) werden erneut versucht …", n));
            s.sealed = true;
            let queue = s
                .item_queue
                .get_or_insert_with(|| Arc::new(Mutex::new(VecDeque::new())))
                .clone();
            queue.lock().unwrap().extend(retry_items);
        }
        let retry_workers = concurrency.max(1).min(n);
        if let Some((destination_resolver, machine_name)) = &destination_preflight {
            run_worker_pass(
                &shared,
                &cfg,
                &paused,
                limit_kbps,
                &dedup,
                destination_resolver,
                machine_name,
                AttemptStage::Retry,
                retry_workers,
            )
            .await;
        }
    }

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
        // The run is over: no injection may land in this queue any more. The last
        // worker already set `sealed`; clearing the queue handle closes the door
        // for good.
        s.item_queue = None;
        s.retry_items.clear();
        s.folder_opts.clear();
        s.destination_roots.clear();
        (s.done, s.failed, s.total, s.cancelled)
    };
    if cancelled {
        // Files a worker returned on are still marked `Uploading`; reset them so
        // the table does not show a stuck "running" row after the abort.
        {
            let mut s = shared.lock().unwrap();
            reset_in_flight(&mut s.queue);
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
    fn take_store_dirty_returns_true_only_once() {
        let mut shared = UploadShared {
            store_dirty: true,
            ..UploadShared::default()
        };

        assert!(shared.take_store_dirty());
        assert!(!shared.take_store_dirty());
        assert!(!shared.store_dirty);
    }

    #[test]
    fn should_reload_store_only_when_idle_and_dirty() {
        for (active, dirty, expected) in [
            (false, false, false),
            (false, true, true),
            (true, false, false),
            (true, true, false),
        ] {
            assert_eq!(should_reload_store(active, dirty), expected);
        }
    }

    #[test]
    fn failed_store_reload_keeps_dirty_marker() {
        let mut shared = UploadShared {
            store_dirty: true,
            ..UploadShared::default()
        };
        let dirty = shared.store_dirty;

        assert!(should_reload_store(shared.active, dirty));
        assert!(!shared.consume_store_dirty_after_reload(false));
        assert!(shared.store_dirty);
    }

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
    fn active_empty_run_reports_all_current_without_progress_or_eta() {
        let shared = UploadShared {
            active: true,
            done: 0,
            total: 0,
            ..UploadShared::default()
        };

        let text = upload_progress_text(&shared);

        assert_eq!(text, tr("Alles aktuell"));
        assert!(!text.contains("0 von 0"));
        assert!(!text.contains("0%"));
        assert!(!text.contains(tr("Schätzung läuft …")));
    }

    #[test]
    fn active_empty_log_summary_reports_all_current_without_progress_or_eta() {
        let shared = UploadShared {
            active: true,
            done: 0,
            total: 0,
            ..UploadShared::default()
        };

        let text = active_log_summary(&shared, || {
            tr3(
                "Upload läuft: {} · {} · {}",
                format!("{}/{}", shared.done, shared.total),
                &shared.current,
                eta_suffix(&shared),
            )
        });

        assert_eq!(text, tr("Alles aktuell"));
        assert!(!text.contains("0/0"));
        assert!(!text.contains(tr("Schätzung läuft …")));
    }

    #[test]
    fn active_log_summary_keeps_progress_text_when_candidates_remain() {
        let shared = UploadShared {
            active: true,
            done: 2,
            total: 5,
            current: "photo.jpg".to_string(),
            ..UploadShared::default()
        };

        let summary = active_log_summary(&shared, || {
            tr3(
                "Upload läuft: {} · {} · {}",
                format!("{}/{}", shared.done, shared.total),
                &shared.current,
                eta_suffix(&shared),
            )
        });

        assert_eq!(
            summary,
            tr3(
                "Upload läuft: {} · {} · {}",
                "2/5",
                "photo.jpg",
                eta_suffix(&shared),
            )
        );
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

    /// The run-level counter effects the retry tests assert on: exactly the
    /// part of the worker's settlement the retry policy controls. A success or
    /// resolved skip is one `done`; a failure is either deferred to the retry
    /// pass (`retry`) or the one final `failed`.
    #[derive(Default, Debug, PartialEq, Eq)]
    struct SettledCounters {
        retry: usize,
        done: usize,
        failed: usize,
    }

    impl SettledCounters {
        fn settle(&mut self, outcome: &FileOutcome, stage: AttemptStage) {
            if retry_after(outcome, stage) {
                self.retry += 1;
            } else if matches!(outcome, FileOutcome::Failed(_)) {
                self.failed += 1;
            } else {
                self.done += 1;
            }
        }
    }

    #[test]
    fn retry_pass_turns_one_initial_failure_into_one_final_failure() {
        // First attempt fails: held for the retry pass, no final error yet.
        let mut counters = SettledCounters::default();
        counters.settle(
            &FileOutcome::Failed("first attempt".into()),
            AttemptStage::Initial,
        );
        assert_eq!(
            counters,
            SettledCounters {
                retry: 1,
                done: 0,
                failed: 0,
            }
        );

        // The retry also fails: exactly one final error, never the first too.
        counters.settle(
            &FileOutcome::Failed("retry attempt".into()),
            AttemptStage::Retry,
        );
        assert_eq!(
            counters,
            SettledCounters {
                retry: 1,
                done: 0,
                failed: 1,
            }
        );
    }

    #[test]
    fn successful_retry_counts_one_done_and_no_failure() {
        let mut counters = SettledCounters::default();
        counters.settle(
            &FileOutcome::Failed("first attempt".into()),
            AttemptStage::Initial,
        );
        counters.settle(
            &FileOutcome::Uploaded("node-id".into()),
            AttemptStage::Retry,
        );

        assert_eq!(
            counters,
            SettledCounters {
                retry: 1,
                done: 1,
                failed: 0,
            }
        );
    }

    #[test]
    fn conflict_and_dedup_successes_are_never_retried() {
        // Both success kinds settle on the initial attempt. A 409 resolved as
        // "keep the existing file" and an already-uploaded file are done, not
        // failure candidates.
        let mut counters = SettledCounters::default();
        counters.settle(&FileOutcome::ConflictSkipped, AttemptStage::Initial);
        counters.settle(
            &FileOutcome::Uploaded("node-id".into()),
            AttemptStage::Initial,
        );
        assert_eq!(
            counters,
            SettledCounters {
                retry: 0,
                done: 2,
                failed: 0,
            }
        );

        // Even a retry-stage success stays a success and never triggers another
        // pass.
        counters.settle(&FileOutcome::ConflictSkipped, AttemptStage::Retry);
        assert_eq!(
            counters,
            SettledCounters {
                retry: 0,
                done: 3,
                failed: 0,
            }
        );
    }

    #[test]
    fn cancelled_run_starts_no_retry_pass() {
        // Sign-out during the initial pass: deferred files stay open (next
        // start), and the cancelled finalization must not turn them into
        // failures.
        let mut shared = UploadShared {
            cancelled: true,
            retry_items: vec![(0, 1, PathBuf::from("/a.jpg"))],
            ..UploadShared::default()
        };

        assert!(retry_pass_items(&mut shared).is_empty());
    }

    #[test]
    fn retry_pass_takes_the_deferred_failures_once() {
        let items = vec![
            (0, 1, PathBuf::from("/a.jpg")),
            (2, 3, PathBuf::from("/b.jpg")),
        ];
        let mut shared = UploadShared {
            retry_items: items.clone(),
            ..UploadShared::default()
        };

        assert_eq!(retry_pass_items(&mut shared), items);
        assert!(
            retry_pass_items(&mut shared).is_empty(),
            "the retry list must be finite"
        );
    }

    #[test]
    fn failed_attempt_counts_once_and_only_after_the_retry() {
        // Exercises the production settlement helper used by both failure paths
        // (upload error and local read error): the first attempt only queues the
        // file, the second increments the one final failure.
        let mut shared = UploadShared {
            queue: vec![QueueEntry {
                folder: 0,
                name: "a.jpg".into(),
                state: FileState::Uploading,
                size: 1,
                path: PathBuf::from("/a.jpg"),
                bytes_sent: 7,
            }],
            ..UploadShared::default()
        };

        settle_failed_attempt(
            &mut shared,
            AttemptStage::Initial,
            0,
            0,
            Path::new("/a.jpg"),
            "lokal nicht lesbar",
        );
        assert_eq!(shared.failed, 0, "the first failure must not be final");
        assert_eq!(shared.folder_failed.get(&0), None);
        assert_eq!(shared.retry_items.len(), 1);
        assert!(shared.queue[0].state == FileState::Waiting);

        // `run_upload` drains the list before the retry pass.
        shared.retry_items.clear();
        settle_failed_attempt(
            &mut shared,
            AttemptStage::Retry,
            0,
            0,
            Path::new("/a.jpg"),
            "lokal nicht lesbar",
        );
        assert_eq!(shared.failed, 1, "exactly one final failure");
        assert_eq!(shared.folder_failed.get(&0), Some(&1));
        assert!(shared.queue[0].state == FileState::Failed);
        assert!(shared.retry_items.is_empty(), "no third attempt");
    }

    /// A root with no queued files (empty backup / fully deduplicated plan)
    /// still failed as a whole: without its own `folder_failed` marker,
    /// `apply_folder_progress` would report the row as `Done`.
    #[test]
    fn preflight_failure_marks_a_root_without_queued_files() {
        let shared = Arc::new(Mutex::new(UploadShared {
            destination_roots: HashMap::from([(2, PathBuf::from("/fotos/leer"))]),
            folder_total: HashMap::from([(2, 0)]),
            ..UploadShared::default()
        }));
        let queue = Arc::new(Mutex::new(VecDeque::new()));

        fail_destination_preflight(&shared, &queue, "kein Ziel");

        let s = shared.lock().unwrap();
        assert_eq!(
            s.folder_failed.get(&2),
            Some(&1),
            "zero-item root must be marked failed (status Error, not Done)"
        );
        assert_eq!(s.failed, 0, "no file failed, so the global counter stays");
        assert!(s.destination_roots.is_empty(), "roots are consumed");
        assert!(s.sealed, "the run must not accept new work");
    }

    /// A root whose queued files were drained as failures already carries its
    /// per-file count; it must not get an additional run-level marker.
    #[test]
    fn preflight_failure_does_not_double_count_a_root_with_drained_files() {
        let shared = Arc::new(Mutex::new(UploadShared {
            destination_roots: HashMap::from([
                (0, PathBuf::from("/fotos/a")),
                (1, PathBuf::from("/fotos/leer")),
            ]),
            folder_total: HashMap::from([(0, 2), (1, 0)]),
            queue: vec![
                QueueEntry {
                    folder: 0,
                    name: "a.jpg".into(),
                    state: FileState::Waiting,
                    size: 1,
                    path: PathBuf::from("/fotos/a/a.jpg"),
                    bytes_sent: 0,
                },
                QueueEntry {
                    folder: 0,
                    name: "b.jpg".into(),
                    state: FileState::Waiting,
                    size: 1,
                    path: PathBuf::from("/fotos/a/b.jpg"),
                    bytes_sent: 0,
                },
            ],
            ..UploadShared::default()
        }));
        let queue = Arc::new(Mutex::new(VecDeque::from(vec![
            (0, 0, PathBuf::from("/fotos/a/a.jpg")),
            (1, 0, PathBuf::from("/fotos/a/b.jpg")),
        ])));

        fail_destination_preflight(&shared, &queue, "kein Ziel");

        let s = shared.lock().unwrap();
        assert_eq!(
            s.folder_failed.get(&0),
            Some(&2),
            "the drained file failures stay the root's only count"
        );
        assert_eq!(
            s.folder_failed.get(&1),
            Some(&1),
            "the empty sibling root gets one marker"
        );
        assert_eq!(s.failed, 2, "exactly the two drained files count globally");
        assert!(s.queue.iter().all(|e| e.state == FileState::Failed));
        assert!(queue.lock().unwrap().is_empty());
    }

    /// The one-time drop has no configured roots: only real file failures are
    /// recorded, no extra marker appears.
    #[test]
    fn preflight_failure_without_roots_stays_item_based() {
        let one_time = crate::app::ONE_TIME_IDX;
        let shared = Arc::new(Mutex::new(UploadShared {
            queue: vec![QueueEntry {
                folder: one_time,
                name: "a.jpg".into(),
                state: FileState::Waiting,
                size: 1,
                path: PathBuf::from("/a.jpg"),
                bytes_sent: 0,
            }],
            ..UploadShared::default()
        }));
        let queue = Arc::new(Mutex::new(VecDeque::from(vec![(
            0,
            one_time,
            PathBuf::from("/a.jpg"),
        )])));

        fail_destination_preflight(&shared, &queue, "kein Ziel");

        let s = shared.lock().unwrap();
        assert_eq!(s.folder_failed.len(), 1, "no marker without a root");
        assert_eq!(s.folder_failed.get(&one_time), Some(&1));
        assert_eq!(s.failed, 1);
    }

    /// A shared state with a live (empty) worker queue — the shape the worker's
    /// idle path sees. `scans_in_flight` and the queue are set by the caller.
    fn shared_with_queue(queue: VecDeque<(usize, usize, PathBuf)>) -> Arc<Mutex<UploadShared>> {
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        shared.lock().unwrap().item_queue = Some(Arc::new(Mutex::new(queue)));
        shared
    }

    #[test]
    fn seal_now_seals_an_empty_idle_queue() {
        // (1) Queue empty, no scan in flight ⇒ the run may end, `sealed` is set.
        let shared = shared_with_queue(VecDeque::new());
        assert!(seal_now(&shared));
        assert!(shared.lock().unwrap().sealed, "sealed must be set");
    }

    #[test]
    fn seal_now_refuses_while_rows_are_still_queued() {
        // (2) An injection landed after the worker's empty pop: the queue is not
        // empty, so sealing would strand those rows in a queue nobody drains.
        let shared = shared_with_queue(VecDeque::from(vec![(0, 0, PathBuf::from("/a.jpg"))]));
        assert!(!seal_now(&shared));
        assert!(!shared.lock().unwrap().sealed, "sealed must stay false");
    }

    #[test]
    fn seal_now_refuses_while_a_scan_is_in_flight() {
        // (3) A scan in flight keeps the run open even with an empty queue.
        let shared = shared_with_queue(VecDeque::new());
        shared.lock().unwrap().scans_in_flight = 1;
        assert!(!seal_now(&shared));
        assert!(!shared.lock().unwrap().sealed, "sealed must stay false");
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
        assert_eq!(
            status_text(FolderStatus::NotFound, 0).0,
            tr("Ordner nicht gefunden")
        );
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
        assert_eq!(status_text(FolderStatus::NotFound, 0).1, AMBER);
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
    fn progress_is_empty_for_a_missing_folder() {
        // A missing source must not claim progress, even with a known total
        // from the last run — the state is "not found", not "x % done".
        assert_eq!(folder_progress_text(FolderStatus::NotFound, 0, 0), "");
        assert_eq!(folder_progress_text(FolderStatus::NotFound, 3, 4), "");
        assert_eq!(folder_progress_text(FolderStatus::NotFound, 4, 4), "");
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
