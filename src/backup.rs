use crate::app::{FolderPickerTarget, Page, ONE_TIME_IDX, PrimePhotoApp};
use crate::backup_config::{BackupConfigForm, ConfigFormMode};
use crate::config::Config;
use crate::dedup::DedupIndex;
use crate::folders::{self, ConflictResolution, FolderStatus, FolderStore};
use crate::i18n::{tr, tr1, tr2, tr3};
use crate::icons;
use crate::login::Session;
use crate::theme;
use crate::upload::{
    FileState, QueueEntry, UploadShared, calculate_pct, folder_progress_text, outcome_text,
    run_upload, status_text, upload_progress_text,
};
use eframe::egui::{self, RichText};
// NOTE: `notify` is the filesystem-event crate — not `crate::notify`, the
// desktop-notification module (they are unrelated and both stay).
use notify;
use notify::Watcher;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

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
    /// Run outcome breakdown of **this** target (only in-memory, for the row's
    /// own result line): real uploads, 409 "already in the account", local
    /// dedup/stat skips and failures. All 0 for a target the run never touched.
    new: usize,
    existing: usize,
    unchanged: usize,
    failed: usize,
    last_updated: String,
}

/// Sum over **every** target — `(done, total)` — the single source of the
/// footer's aggregate progress (user decision 2026-10-14: one display over all
/// backup targets, the one-time row included). Pure: no lock, no UI, no
/// filesystem, and therefore cheap to test. A target that knows no total
/// (`total == 0`) contributes nothing and can never skew the sum; the caller
/// guards the division by zero.
fn aggregate_progress(rows: &[Row]) -> (usize, usize) {
    rows.iter()
        .fold((0usize, 0usize), |(done, total), r| {
            (done + r.done, total + r.total)
        })
}

/// Clamp a target's completed count to its total for display (Welle 5.11): an
/// inconsistent legacy state (`total_files < uploaded_files`, once rendered as
/// "320 %") must never read above 100 %. `total == 0` yields `0` — the folder's
/// progress text is empty then ([`crate::upload::folder_progress_text`]) and the
/// aggregate bar guards its division. Pure, so the row's percent text and the
/// aggregate bar below stay consistent.
fn capped_done(done: usize, total: usize) -> usize {
    done.min(total)
}

/// Forced pause (user decision 2026-10-16, `docs/SPEC_MISSING_SOURCE.md`): a
/// backup target whose source folder is gone must be **paused**, not errored —
/// and it must recover by itself once the source is back.
///
/// Pure and total, so every path that decides a folder's state reaches the same
/// verdict:
/// - the source is not stat-able (`path_exists == false`) ⇒ `Paused`,
/// - the source is back and the folder was `Paused` ⇒ `Idle` (the forced pause
///   is lifted; the never-advanced watermark makes the next cold start re-scan
///   the files that were missed),
/// - otherwise the status is left untouched.
///
/// `FolderStatus::Paused` is *only* ever written by this rule, so it cannot
/// collide with the **global** user pause (`Config::paused`), which is a separate
/// switch and stays untouched here.
fn source_status(path_exists: bool, current: FolderStatus) -> FolderStatus {
    if !path_exists {
        FolderStatus::Paused
    } else if current == FolderStatus::Paused {
        FolderStatus::Idle
    } else {
        current
    }
}

/// Label of a backup folder in the queue (rows and group headers): the folder
/// name, plus the parent path in parentheses when **more than one** backup
/// folder carries that name (`ambiguous`) — two folders both named "Sport"
/// would otherwise be indistinguishable in the queue (user decision
/// 2026-10-16). The ambiguity is computed once per frame over the whole
/// store, so the rule is stable while the queue is open. The name is shown
/// NFC-normalised, like every other displayed path in the app.
fn queue_folder_label(name: &str, path: &Path, ambiguous: &HashSet<String>) -> String {
    let name = crate::text::nfc(name);
    if !ambiguous.contains(&name) {
        return name;
    }
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => {
            format!("{} ({})", name, crate::text::nfc(&p.display().to_string()))
        }
        // No parent to disambiguate with: the plain name stays.
        _ => name,
    }
}

/// The set of folder names the store carries more than once — the input of
/// [`queue_folder_label`]. Cheap (the store holds a handful of folders) and
/// built once per frame, not per queue row. Names are NFC-normalised so the
/// set matches the displayed form.
fn ambiguous_folder_names(folders: &[folders::BackupFolder]) -> HashSet<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for f in folders {
        *counts.entry(crate::text::nfc(&f.name)).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .filter(|(_, c)| *c > 1)
        .map(|(name, _)| name)
        .collect()
}

/// Fixed geometry of the live queue table (docs/SPEC_UPLOAD_PROGRESS.md §7.8):
/// the name column takes the remaining width, folder/size/status are fixed, the
/// row height is fixed. The header uses the same widths, so both line up.
const QUEUE_ROW_H: f32 = 20.0;
const QUEUE_HEADER_H: f32 = 20.0;
/// Compact column carrying the backup folder of each queue row (user decision
/// 2026-10-16): without it the files of several targets are indistinguishable.
const QUEUE_FOLDER_W: f32 = 120.0;
const QUEUE_SIZE_W: f32 = 72.0;
const QUEUE_STATUS_W: f32 = 104.0;
const QUEUE_MIN_NAME_W: f32 = 80.0;
/// Height of the centred empty-state block while the queue is empty.
const QUEUE_EMPTY_H: f32 = 72.0;
/// Floor of the queue table's height as a fraction of the page's free height
/// (user decision 2026-10-16: no absolute heights — the queue is the
/// secondary column and shrinks with the window).
const QUEUE_MIN_FRAC: f32 = 0.20;
/// Ceiling of the queue table's height as a fraction of the free height: the
/// queue never takes more than half of the page — the folders matter more.
const QUEUE_MAX_FRAC: f32 = 0.50;
/// **Measured** chrome the queue table must leave **below itself** so the rest
/// of the page stays reachable (headless with egui 0.31.1): the aggregate
/// progress bar with its 6 px gap (56 px to the folder heading, i.e. the bar,
/// a 12 px gap and the "Automatisch gesicherte Ordner:" heading) plus the folder
/// list's own footer room ([`QUEUE_FOOTER_H`]). 80 px measured, 88 px with a
/// small margin. This is measured content, not a panel share — it stays
/// absolute on purpose; only the *range caps/floors* of the two scroll areas
/// are relative. The Backup page has no scroll area of its own, so the table
/// may never eat this space; the list below then gets what is left.
const QUEUE_BELOW_H: f32 = 88.0;
/// Height the folder list must leave for the footer below it (8 px gap + line).
const QUEUE_FOOTER_H: f32 = 30.0;
/// Height of one folder card (Variant A), **measured headless** with egui
/// 0.31.1 at the default font: name line + small path/progress/updated line +
/// outcome line + frame margins. The list's floor is derived from it — three
/// cards plus their gaps stay visible before the list scrolls.
const FOLDER_CARD_H: f32 = 60.0;
/// Vertical gap between two folder cards (the frame's outer margin, 4 + 4).
const FOLDER_CARD_GAP: f32 = 8.0;
/// Floor the folder list claims before the queue is laid out: three measured
/// cards with their gaps — derived from the card height, not a round number.
const FOLDER_LIST_MIN_H: f32 = 3.0 * FOLDER_CARD_H + 2.0 * FOLDER_CARD_GAP;
/// Ceiling of the folder list as a fraction of the free height (user decision
/// 2026-10-16: relative, not the old fixed 320 px).
const FOLDER_LIST_MAX_FRAC: f32 = 0.45;

/// Vertical budget of the Backup page's two scroll areas for a given free
/// height — the single source both draw sites use, so the queue (drawn first)
/// leaves exactly what the folder list was promised.
///
/// Priority (user decision 2026-10-16): the **folder list claims its height
/// first**; the queue takes the rest, but never more than half of the free
/// height. All caps and floors are fractions of `free` except the measured
/// chrome ([`QUEUE_BELOW_H`], [`QUEUE_FOOTER_H`], [`QUEUE_HEADER_H`]) and the
/// list's card-derived floor, which stay absolute.
///
/// Guarantees for every `free >= 0`:
/// - `queue_h + list_h + QUEUE_HEADER_H + QUEUE_BELOW_H <= free` — nothing
///   runs off the bottom edge (the 2 px margin inside `QUEUE_BELOW_H` covers
///   the header being 2 px more conservative than the measured 106 px);
/// - `queue_h <= QUEUE_MAX_FRAC * free` — the queue stays at or below 50 %;
/// - `list_h >= min(FOLDER_LIST_MIN_H, …)` — three cards fit whenever the
///   window is tall enough to give the queue its floor too.
fn page_heights(free: f32) -> (f32, f32) {
    // The queue's fixed chrome: its own column header plus everything below
    // the table (aggregate bar zone + footer room).
    let chrome = QUEUE_HEADER_H + QUEUE_BELOW_H;
    // 1) The folder list first: its card-derived floor, capped by its fraction
    //    of the free height — and never so much that the queue loses its floor
    //    or the chrome is eaten.
    let list_h = FOLDER_LIST_MIN_H
        .min(FOLDER_LIST_MAX_FRAC * free)
        .min((free - chrome - QUEUE_MIN_FRAC * free).max(0.0));
    // 2) The queue takes what is left, capped at half the free height; its
    //    floor only applies when it actually fits.
    let rest = (free - chrome - list_h).max(0.0);
    let queue_h = rest
        .min(QUEUE_MAX_FRAC * free)
        .max((QUEUE_MIN_FRAC * free).min(rest));
    (queue_h, list_h)
}

/// One line of the live queue table: a backup-folder group header or a file row.
/// Both are drawn at the same fixed height, so `ScrollArea::show_rows` can still
/// virtualise the list (docs/SPEC_UPLOAD_PROGRESS.md §7.8) — it needs a uniform
/// row *height*, not flat data.
enum QueueRow {
    /// A backup-folder group header: the folder label plus the number of its
    /// files that are still open (`Waiting`/`Uploading`).
    Header { label: String, open: usize },
    /// A file row. `queue_idx` points into `UploadShared::queue`, so the live
    /// name/size/state/path are read from there instead of being cloned per
    /// frame.
    File { queue_idx: usize },
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
    /// Known total size of the run and the number of files it was measured
    /// from. Drives the byte-based ETA (`UploadShared::bytes_total`); a file
    /// whose size cannot be read simply contributes nothing.
    bytes_total: u64,
    sized_total: usize,
    queue: Vec<QueueEntry>,
    per_folder: HashMap<usize, usize>,
    folder_opts: HashMap<usize, (bool, ConflictResolution)>,
}

impl UploadPlan {
    fn empty() -> Self {
        UploadPlan {
            items: Vec::new(),
            bytes_total: 0,
            sized_total: 0,
            queue: Vec::new(),
            per_folder: HashMap::new(),
            folder_opts: HashMap::new(),
        }
    }

    /// File size of one entry, counted once into the run's byte total and
    /// returned so the queue row can reuse it without a second `stat`. Never
    /// panics: an unreadable file contributes nothing (`0`) and the ETA then
    /// falls back to the file-count rate.
    fn note_size(&mut self, path: &Path) -> u64 {
        if let Ok(meta) = std::fs::metadata(path) {
            let len = meta.len();
            self.bytes_total += len;
            self.sized_total += 1;
            len
        } else {
            0
        }
    }

    fn build(store: &FolderStore, indices: &[usize]) -> Option<Self> {
        // A manual run takes every supported file: no folder has a watermark.
        let wanted: Vec<(usize, Option<i64>)> = indices.iter().map(|&i| (i, None)).collect();
        Self::build_since(store, &wanted)
    }

    /// Build a plan from a **watermark** selection: `wanted` pairs a store index
    /// with the watermark its files must beat (`None` = take everything).
    ///
    /// Same walk, same order and same counters as [`UploadPlan::build`], so a
    /// manual run is exactly `build` with every watermark `None`. Used by the
    /// cold-start safety net (user decision 2026-10-14): an unchanged folder
    /// never even reaches a plan.
    fn build_since(store: &FolderStore, wanted: &[(usize, Option<i64>)]) -> Option<Self> {
        let mut plan = Self::empty();
        for &(i, watermark) in wanted {
            let Some(f) = store.folders.get(i) else {
                continue;
            };
            let imgs = folders::scan_new_since(&f.path, watermark);
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
                let size = plan.note_size(&p);
                plan.queue.push(QueueEntry {
                    folder: i,
                    name,
                    state: FileState::Waiting,
                    size,
                    path: p.clone(),
                    bytes_sent: 0,
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

    /// Build a plan from an **explicit** file list instead of a folder scan
    /// (the one-time drag & drop path): every entry belongs to the single
    /// synthetic folder index `idx`, so the whole queue is one folder as far as
    /// `run_upload`'s counters and `folder_opts` are concerned.
    fn from_files(
        idx: usize,
        dedup: bool,
        conflict: ConflictResolution,
        files: Vec<PathBuf>,
    ) -> Self {
        let mut plan = Self::empty();
        plan.per_folder.insert(idx, files.len());
        plan.folder_opts.insert(idx, (dedup, conflict));
        for p in files {
            let row = plan.items.len();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size = plan.note_size(&p);
            plan.queue.push(QueueEntry {
                folder: idx,
                name,
                state: FileState::Waiting,
                size,
                path: p.clone(),
                bytes_sent: 0,
            });
            plan.items.push((row, idx, p));
        }
        plan
    }

    /// Build a plan from an explicit **per-folder** file list (the watcher
    /// path): every file keeps its own backup-folder index, so the run looks
    /// exactly like a manual folder run to `run_upload` — same counters, same
    /// `folder_opts`, same queue grouping — while only the files the OS actually
    /// named are uploaded. No folder is walked.
    fn build_from_events(store: &FolderStore, files: &[(usize, PathBuf)]) -> Option<Self> {
        let mut plan = Self::empty();
        for (i, p) in files {
            let Some(f) = store.folders.get(*i) else {
                continue;
            };
            *plan.per_folder.entry(*i).or_insert(0) += 1;
            plan.folder_opts.insert(*i, (f.dedup, f.conflict_resolution));
            let row = plan.items.len();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size = plan.note_size(p);
            plan.queue.push(QueueEntry {
                folder: *i,
                name,
                state: FileState::Waiting,
                size,
                path: p.clone(),
                bytes_sent: 0,
            });
            plan.items.push((row, *i, p.clone()));
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
/// queue and `msg` as the run-start log line). Both start paths go through
/// here, so a manual run, a background run and a one-time run look and behave
/// identically.
///
/// Takes the live queue out of `plan`; the caller keeps `items`/`folder_opts`
/// for the worker spawn.
fn publish_state(plan: &mut UploadPlan, shared: &Arc<Mutex<UploadShared>>, msg: String) {
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
    // The outcome breakdown is per run too (user decision 2026-10-14): the
    // footer line and every folder row start from zero, so no previous run's
    // numbers can survive into this one.
    s.uploaded_new = 0;
    s.conflict_skipped = 0;
    s.unchanged_skipped = 0;
    s.folder_new.clear();
    s.folder_conflict.clear();
    s.folder_unchanged.clear();
    s.uploaded_ids.clear();
    s.total = plan.items.len();
    s.queue = std::mem::take(&mut plan.queue);
    // A brand-new queue identity: bump the revision so the UI cache cannot keep
    // showing the previous run's entries (`queue_cache_rev` would otherwise
    // still match).
    s.touch_queue();
    // Byte-based ETA counters: the known run size up front, everything settled /
    // transferred back to zero so the first rate sample is clean. The byte ETA
    // is only meaningful when *every* file's size is known (`sized_total`),
    // otherwise the file-count fallback in `eta_secs` takes over.
    s.bytes_total = if plan.sized_total == plan.items.len() {
        plan.bytes_total
    } else {
        0
    };
    s.bytes_done = 0;
    s.net_bytes = 0;
    s.net_started = None;
    s.upload_secs = 0.0;
    // Conflict prompt state is per run: a "don't ask again" choice from a
    // previous run must never silently overwrite files in this one — and no
    // answer may outlive the prompt it belonged to.
    s.pending_conflict = None;
    s.pending_conflict_idx = None;
    s.pending_conflict_path = None;
    s.conflict_answer = None;
    s.conflict_remember = false;
    s.conflict_override = None;
    s.log(msg);
}

/// Record the plan's file count on its persisted folders (folder-targets only).
/// The one-time target is transient, so it never reaches this and never gets
/// saved.
///
/// The persisted **status** is deliberately not written here (Welle 5.1 rest):
/// the row renders "uploading" **live** from `UploadShared::active` +
/// `folder_total` (see `backup_ui`), so a persisted `Uploading` would be a
/// second, redundant claim — and a false one after a cancelled run or a quit in
/// the middle of a run, because both finalizers return early on `cancelled` and
/// never reset it. A file count is a number, not a claim.
fn publish_folder_status(plan: &UploadPlan, store: &mut FolderStore) {
    for (&i, &count) in &plan.per_folder {
        if let Some(f) = store.folders.get_mut(i) {
            f.total_files = count;
        }
    }
    let _ = store.save();
}

/// Publish a plan for **persisted** folder targets: the shared state above
/// plus the folder status/side effects. The one-time path uses
/// [`publish_state`] alone.
fn publish_plan(
    plan: &mut UploadPlan,
    store: &mut FolderStore,
    shared: &Arc<Mutex<UploadShared>>,
    msg: String,
) {
    publish_state(plan, shared, msg);
    publish_folder_status(plan, store);
}

/// Deepest directory that contains every dropped path, or `None` when they sit
/// on different roots (e.g. two volumes). Only used to label the transient
/// one-time target of a multi-path drop — a display value, never a scan root.
fn common_parent(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut common = paths.first()?.parent()?.to_path_buf();
    for p in paths.iter().skip(1) {
        let parent = p.parent()?;
        while !parent.starts_with(&common) {
            common = common.parent()?.to_path_buf();
        }
    }
    if common.as_os_str().is_empty() {
        None
    } else {
        Some(common)
    }
}

// ---------------------------------------------------------------------------
// Background sync (PORT-5) + change detection (user decision 2026-10-14):
// one supervising task per process
// ---------------------------------------------------------------------------

/// Debounce for filesystem events: a burst (a camera import, a folder copy) is
/// collected and handled once instead of once per file. Mirrors the original's
/// 5 s folder debounce (`docs/SPEC_CHANGE_DETECTION.md` §4.3).
const WATCH_DEBOUNCE: Duration = Duration::from_secs(5);

/// How often the watcher supervisor re-syncs its watcher set with the persisted
/// store (a folder added/enabled/disabled/removed, the `background_sync`
/// switch) and retries the files it remembered while a run was busy or paused.
/// Same value the original uses for its watcher run-loop poll
/// (`BackupWatcherPollIntervalInSeconds` = 30 s, §4.3).
const WATCH_POLL: Duration = Duration::from_secs(30);

/// Set once the supervising background task has been spawned for this process —
/// `ensure_background_sync` is idempotent through it.
static BG_SYNC_STARTED: AtomicBool = AtomicBool::new(false);

/// Persist the counters of a finished **background** run. `finalize_upload` is
/// only reached through the UI hook for manual runs (`was_active` in `update`),
/// so the background chain writes the same folder state itself.
fn finalize_background_store(shared: &Arc<Mutex<UploadShared>>) {
    // A cancelled run (sign-out) is not a finished run: no folder status, no
    // counters, no `last_updated` (user decision 2026-10-16). The uploaded part
    // stays in the dedup index; the rest is picked up by the next start.
    if shared.lock().unwrap().cancelled {
        return;
    }
    let (folder_total, folder_done, folder_failed) = {
        let s = shared.lock().unwrap();
        (
            s.folder_total.clone(),
            s.folder_done.clone(),
            s.folder_failed.clone(),
        )
    };
    let mut store = match load_store_logged() {
        Some(store) => store,
        None => return,
    };
    let now = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
    for (i, f) in store.folders.iter_mut().enumerate() {
        // `folder_total` — the run's plan — is the trigger, not `folder_done`:
        // it holds every folder the run took part in, **including** one whose
        // scan found nothing to upload (user decision 2026-10-16). Such a
        // folder never gets a `folder_done` entry, so a `folder_done` trigger
        // would leave it without a total and without a state forever — the row
        // would keep claiming nothing was ever done. A folder absent from
        // `folder_total` was never part of the run and stays untouched; that is
        // also what keeps a **never-scanned** folder (equally `total == 0`)
        // from showing a completed state.
        if let Some(&total) = folder_total.get(&i) {
            f.total_files = total;
            f.uploaded_files = folder_done.get(&i).copied().unwrap_or(0);
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

/// One enabled backup folder the start-time watermark pass decided to scan.
struct FolderScan {
    /// Store index at decision time. Only read back to look up this run's
    /// per-folder failure counters in the shared state.
    idx: usize,
    /// Folder path at decision time — never re-resolved from the store: while a
    /// run uploads, the user may add or remove folders and shift the indices.
    path: PathBuf,
    /// Maximum mtime the decision was based on. Written back once the run has
    /// finished (and only for a folder that came through clean).
    scanned_max: i64,
}

/// Whether a detached run may start right now — the same four guards the old
/// periodic tick checked: the `background_sync` switch, a live sign-in, no pause
/// and no active run. Neither the start-time watermark pass nor a filesystem
/// event may bypass them.
fn can_start(cfg: &Config, paused: &watch::Sender<bool>, shared: &Arc<Mutex<UploadShared>>) -> bool {
    if *paused.borrow() {
        return false;
    }
    cfg.background_sync
        // Same predicate as `PrimePhotoApp::is_logged_in` (session + acb cookie).
        && !cfg.session_id.is_empty()
        && !cfg.cookie_x_acb.is_empty()
        && !shared.lock().unwrap().active
}

/// Publish a plan and spawn its worker — the shared tail of every detached run
/// (the start-time watermark pass and the watcher path), so both look and behave
/// identically. Takes the live queue out of `plan`; the worker gets the items
/// and the per-folder options.
fn start_background_run(
    mut plan: UploadPlan,
    store: &mut FolderStore,
    shared: Arc<Mutex<UploadShared>>,
    cfg: Config,
    paused: watch::Sender<bool>,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
    msg: String,
    scans: Vec<FolderScan>,
) {
    publish_plan(&mut plan, store, &shared, msg);

    // Spawned, not awaited: the supervisor must keep watching while the run
    // uploads (the `active` guard in `can_start` already prevents a second one).
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
        // A cancelled run must not advance the watermark: leaving it at its old
        // value makes the next start re-scan (and re-upload) the files this run
        // never got to (user decision 2026-10-16).
        if after.lock().unwrap().cancelled {
            return;
        }
        // Advance the watermark of every folder this pass scanned — but not of
        // one that failed: leaving it at its old value makes the next start
        // re-scan those files instead of walking past them.
        if scans.is_empty() {
            return;
        }
        let failed: Vec<usize> = {
            let s = after.lock().unwrap();
            s.folder_failed.keys().copied().collect()
        };
        let index = DedupIndex::open();
        for scan in scans {
            if failed.contains(&scan.idx) {
                continue;
            }
            // Only a folder that is *there* may advance its watermark (user
            // decision 2026-10-16): a drive yanked mid-run must not make the
            // next start believe the folder was fully scanned — the files that
            // were missed would then never be picked up again.
            if !scan.path.is_dir() {
                continue;
            }
            index.record_folder_scan(&scan.path, scan.scanned_max);
        }
    });
}

/// Overlay the live in-memory session cookies onto a config loaded from disk.
///
/// The `Config` cookie fields are `#[serde(skip)]` (S-C, user decision
/// 2026-10-15), so `Config::load()` always returns empty ones. The detached
/// supervisor has no access to `PrimePhotoApp::config` and must take the session
/// material from the shared [`Session`] instead. The lock is scoped to this
/// function, so it is never held across an `await`.
fn overlay_session_cookies(cfg: &mut Config, session: &Arc<Mutex<Session>>) {
    let cookies = session.lock().unwrap().cookies.clone();
    cfg.session_id = cookies.session_id;
    cfg.cookie_x_acb = cookies.cookie_x_acb;
    cfg.cookie_at_acb = cookies.cookie_at_acb;
    cfg.cookie_ubid_acb = cookies.cookie_ubid_acb;
    cfg.cookie_x_amz_access_token = cookies.cookie_x_amz_access_token;
}

/// Load the persisted store for the detached supervisor, reporting a damaged
/// one on the log facade and yielding `None`.
///
/// `FolderStore::load` already sets a damaged file aside; the supervisor must
/// not keep running against the resulting empty default store, so every caller
/// treats `None` as "no store this pass" (user decision 2026-10-16). The
/// technical detail stays in the log, never in the UI.
fn load_store_logged() -> Option<FolderStore> {
    match FolderStore::load() {
        Ok(store) => Some(store),
        Err(e) => {
            log::error!("Sicherungsziele konnten nicht geladen werden: {e}");
            None
        }
    }
}

/// The start-time safety net (user decision 2026-10-14): one **metadata-only**
/// walk per enabled folder decides whether anything changed since the last run.
/// A folder whose maximum mtime did not grow costs no hash, no per-file DB query
/// and no upload plan; only the changed folders are scanned — and only for the
/// files newer than their watermark.
///
/// Runs detached from the UI struct (the supervisor owns its own clones), so
/// every guard is re-checked here — it never panics into the app and never
/// touches the UI thread.
async fn background_sync_pass(
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    session: Arc<Mutex<Session>>,
    last_config: Config,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
) {
    // Freshest settings first: the supervisor is detached from the UI struct, so
    // the persisted config is the source for everything except the **session
    // material**. The cookie fields are `#[serde(skip)]` — `Config::load()` always
    // yields empty ones — so the live cookies are overlaid from the shared
    // in-memory session; without that, `can_start` would never see a sign-in.
    let mut cfg = Config::load().unwrap_or(last_config);
    overlay_session_cookies(&mut cfg, &session);
    if !can_start(&cfg, &paused, &shared) {
        return;
    }

    // Enabled folders come from the persisted store (saved on every change). A
    // damaged store was set aside by `FolderStore::load`; without it there is
    // nothing to sync, so the pass ends here instead of running against an
    // empty default store (user decision 2026-10-16).
    let Some(mut store) = load_store_logged() else {
        return;
    };
    let indices = store.enabled_indices();
    if indices.is_empty() {
        return;
    }

    // Same SQLite, same connection style as the MD5 index (`src/dedup.rs`).
    let index = DedupIndex::open();
    let mut wanted: Vec<(usize, Option<i64>)> = Vec::new();
    let mut scans: Vec<FolderScan> = Vec::new();
    let mut store_changed = false;
    for i in indices {
        let Some(f) = store.folders.get_mut(i) else {
            continue;
        };
        // Forced pause (user decision 2026-10-16): a source folder that is not
        // stat-able is *paused*, not silently skipped, so the row says why it
        // stopped — and a folder whose drive came back is lifted to `Idle`
        // again. The watermark of a missing folder is never touched (no `scans`
        // entry), so the next cold start re-scans everything it missed.
        let exists = f.path.is_dir();
        let next = source_status(exists, f.status);
        if next != f.status {
            f.status = next;
            store_changed = true;
        }
        if !exists {
            continue;
        }
        let watermark = index.folder_max_mtime(&f.path);
        // A folder we cannot even stat (no access) answers `None`: it is left
        // alone rather than treated as "unchanged", and scanning it would find
        // nothing anyway.
        let Some(current) = folders::max_mtime(&f.path) else {
            continue;
        };
        // "Not greater" is unchanged — nothing to hash, nothing to record.
        if current <= watermark.unwrap_or(i64::MIN) {
            continue;
        }
        wanted.push((i, watermark));
        scans.push(FolderScan {
            idx: i,
            path: f.path.clone(),
            scanned_max: current,
        });
    }
    // Persist a status change even when nothing is left to scan: a pause or its
    // lifting must survive the pass.
    if store_changed {
        let _ = store.save();
    }
    if wanted.is_empty() {
        return;
    }
    let Some(plan) = UploadPlan::build_since(&store, &wanted) else {
        return;
    };
    let count = plan.count();
    start_background_run(
        plan,
        &mut store,
        shared.clone(),
        cfg,
        paused,
        concurrency,
        limit_kbps,
        album,
        format!("Hintergrund-Sync: {count} Datei(en) werden hochgeladen …"),
        scans,
    );
}

/// Fold one watcher event into `paths`: only supported image **files** survive.
/// A directory is not a photo, and a path that no longer exists is a deletion —
/// both are ignored, exactly like the original's event type gate
/// (`docs/SPEC_CHANGE_DETECTION.md` §4.3). Returns `false` on a watcher error so
/// the caller can re-arm the set. `seen` is the batch's path set, so a path the OS
/// reports more than once is folded in only once — O(1) per path, no scan of the
/// list, and no cap: the batch grows as large as the burst.
fn collect_event_paths(
    res: notify::Result<notify::Event>,
    paths: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) -> bool {
    match res {
        Ok(ev) => {
            for p in ev.paths {
                if p.is_file() && folders::is_supported_image(&p) && seen.insert(p.clone()) {
                    paths.push(p);
                }
            }
            true
        }
        Err(e) => {
            log::warn!("Ordner-Wächter: {e}");
            false
        }
    }
}

/// Attach each event path to its backup folder. The watchers are recursive, so a
/// file deep in the tree reports its full path and is matched against the folder
/// roots; a path under no *enabled* folder is dropped (the folder was disabled
/// or removed while the event was in flight).
fn map_to_folders(store: &FolderStore, paths: &[PathBuf]) -> Vec<(usize, PathBuf)> {
    let roots: Vec<(usize, &Path)> = store
        .enabled_indices()
        .into_iter()
        .filter_map(|i| store.folders.get(i).map(|f| (i, f.path.as_path())))
        .collect();
    paths
        .iter()
        .filter_map(|p| {
            roots
                .iter()
                .find(|(_, root)| p.starts_with(root))
                .map(|(i, _)| (*i, p.clone()))
        })
        .collect()
}

/// Merge `files` into the parked stock, de-duplicated by **path** (the identity
/// of an event). Returns how many paths were newly taken up, so a caller can tell
/// a real addition from a redundant re-report. Pure — no store, no watcher, no
/// filesystem, no runtime state — and therefore cheap to test.
///
/// There is deliberately **no upper bound**: a burst of 10 000+ files (a camera
/// import, a folder copy, a long pause) is kept in full. Memory grows with the
/// number of *distinct* changed paths (~1.2 MB at 10 000, ~12 MB at 100 000 —
/// negligible), and nothing is ever dropped.
fn park_paths(
    pending: &mut Vec<(usize, PathBuf)>,
    seen: &mut HashSet<PathBuf>,
    files: impl IntoIterator<Item = (usize, PathBuf)>,
) -> usize {
    let mut added = 0;
    for (idx, path) in files {
        if seen.insert(path.clone()) {
            pending.push((idx, path));
            added += 1;
        }
    }
    added
}

/// Fold `files` into the parked stock and, if a run may start right now, upload
/// **all** of them in a single run; otherwise they stay parked until a later pass
/// is allowed to start one.
///
/// `pending`/`pending_seen` are the "seen while a run was busy or paused" list and
/// its exact path set; the caller keeps them across passes. Merging instead of
/// replacing is what makes a burst that arrives while paths are already parked
/// lose nothing, and de-duplication by path keeps the list at one entry per file
/// even when the OS reports an event more than once. The list is unbounded; an
/// empty merge with an empty stock is a no-op.
fn handle_files(
    files: impl IntoIterator<Item = (usize, PathBuf)>,
    pending: &mut Vec<(usize, PathBuf)>,
    pending_seen: &mut HashSet<PathBuf>,
    store: &mut FolderStore,
    shared: &Arc<Mutex<UploadShared>>,
    paused: &watch::Sender<bool>,
    cfg: &Config,
    concurrency: usize,
    limit_kbps: u32,
    album: &str,
) {
    // Merge first, never replace: the new burst and the parked stock become one
    // de-duplicated list, so a burst that lands mid-pause cannot drop the old one.
    park_paths(pending, pending_seen, files);
    if pending.is_empty() || !can_start(cfg, paused, shared) {
        return;
    }
    let batch = std::mem::take(pending);
    // `pending_seen` mirrors `pending` exactly: both are cleared together.
    pending_seen.clear();
    let Some(plan) = UploadPlan::build_from_events(store, &batch) else {
        return;
    };
    let count = plan.count();
    start_background_run(
        plan,
        store,
        shared.clone(),
        cfg.clone(),
        paused.clone(),
        concurrency,
        limit_kbps,
        album.to_string(),
        tr1("{} Datei(en) werden hochgeladen …", count),
        // An event run never advances a folder watermark: the watermark is the
        // *start-time* safety net. The files it did upload sit in the MD5 index
        // and are skipped cheaply by the stat check, while a run that failed is
        // still caught by the next start.
        Vec::new(),
    );
}

/// One watcher per enabled backup folder, re-synced with the persisted store on
/// every pass: a folder that was added, enabled, disabled or removed (and the
/// `background_sync` switch) is reflected within `WATCH_POLL`. A folder we
/// cannot watch (gone, ejected) is logged and simply absent until the next pass.
fn sync_watchers(
    watchers: &mut HashMap<PathBuf, notify::RecommendedWatcher>,
    tx: &tokio::sync::mpsc::UnboundedSender<notify::Result<notify::Event>>,
    cfg: &Config,
) {
    // No switch or no sign-in → no watcher at all (the same guard that keeps a
    // run from starting).
    let mut store = if cfg.background_sync
        && !cfg.session_id.is_empty()
        && !cfg.cookie_x_acb.is_empty()
    {
        // A damaged store was set aside by `FolderStore::load`; without it there
        // is nothing to watch (the error is already logged).
        load_store_logged()
    } else {
        None
    };
    // Forced pause on the watcher path (user decision 2026-10-16): every pass
    // re-decides each enabled folder's source state with the same pure rule the
    // cold-start pass and the row use — a folder whose drive is gone becomes
    // `Paused`, one whose drive came back is lifted to `Idle` again. This is
    // also why a folder we cannot watch is already `Paused` before the watcher
    // attempt below. Nothing else is touched; in particular the global user
    // pause is a separate switch.
    let mut store_changed = false;
    if let Some(s) = store.as_mut() {
        for i in s.enabled_indices() {
            if let Some(f) = s.folders.get_mut(i) {
                let next = source_status(f.path.is_dir(), f.status);
                if next != f.status {
                    f.status = next;
                    store_changed = true;
                }
            }
        }
    }
    let desired: Vec<PathBuf> = store
        .as_ref()
        .map(|s| {
            s.enabled_indices()
                .iter()
                .filter_map(|&i| s.folders.get(i).map(|f| f.path.clone()))
                .collect()
        })
        .unwrap_or_default();
    watchers.retain(|p, _| desired.contains(p));
    for path in desired {
        if watchers.contains_key(&path) {
            continue;
        }
        let tx = tx.clone();
        let watched = path.clone();
        let made = notify::RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            notify::Config::default(),
        )
        .and_then(|mut w| {
            w.watch(&watched, notify::RecursiveMode::Recursive)?;
            Ok(w)
        });
        match made {
            Ok(w) => {
                watchers.insert(path, w);
            }
            Err(e) => log::warn!(
                "Ordner-Wächter für {} konnte nicht eingerichtet werden: {e}",
                path.display()
            ),
        }
    }
    if store_changed {
        if let Some(s) = store.as_ref() {
            let _ = s.save();
        }
    }
}

/// The supervising watcher task, for the rest of the process: one watcher per
/// enabled backup folder, a debounce per burst, and a "seen while a run was busy
/// or paused" list that is merged, never replaced. Detached from the UI struct —
/// every guard is re-checked here, it never panics into the app and never touches
/// the UI thread.
///
/// The pending list has **no cap**: a burst of 10 000+ files (a camera import, a
/// folder copy, a long pause) is carried in full, one entry per *distinct* changed
/// path (~1.2 MB at 10 000, ~12 MB at 100 000 — negligible), and nothing is
/// dropped. When a run is allowed again, everything parked goes into a single run.
async fn watch_loop(
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    session: Arc<Mutex<Session>>,
    last_config: Config,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Result<notify::Event>>();
    let mut watchers: HashMap<PathBuf, notify::RecommendedWatcher> = HashMap::new();
    let mut pending: Vec<(usize, PathBuf)> = Vec::new();
    // `pending_seen` holds exactly the paths in `pending`; together they make the
    // merge O(1) per path instead of a linear scan of the list.
    let mut pending_seen: HashSet<PathBuf> = HashSet::new();
    loop {
        // Freshest settings + store on every pass (the supervisor is detached
        // from the UI struct, so the persisted files are its source — except the
        // session material, which is overlaid from the shared `Session` because
        // the cookie fields are `#[serde(skip)]`).
        let mut cfg = Config::load().unwrap_or(last_config.clone());
        overlay_session_cookies(&mut cfg, &session);
        sync_watchers(&mut watchers, &tx, &cfg);

        // Retry what was remembered while a run was busy or paused: no new burst,
        // just the parked stock — merged with itself (a no-op) so no path is taken
        // up twice, then started if a run is allowed now.
        if !pending.is_empty() {
            // A damaged store was set aside by `FolderStore::load`; without it
            // there is nothing to attribute the parked paths to, so this pass is
            // skipped (the error is already logged).
            let Some(mut store) = load_store_logged() else {
                continue;
            };
            handle_files(
                std::iter::empty(),
                &mut pending,
                &mut pending_seen,
                &mut store,
                &shared,
                &paused,
                &cfg,
                concurrency,
                limit_kbps,
                &album,
            );
        }

        // Wait for the next event — or just for the poll tick, which is what
        // keeps the watcher set and the pending list moving while nothing
        // changes.
        let first = match tokio::time::timeout(WATCH_POLL, rx.recv()).await {
            Ok(Some(ev)) => ev,
            // `Ok(None)`: every sender is gone, so every watcher is, too.
            Ok(None) => break,
            Err(_) => continue,
        };
        // Debounce: give the burst time to land, then take all of it at once.
        tokio::time::sleep(WATCH_DEBOUNCE).await;
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut batch_seen: HashSet<PathBuf> = HashSet::new();
        let rest = std::iter::from_fn(|| rx.try_recv().ok());
        for res in std::iter::once(first).chain(rest) {
            if !collect_event_paths(res, &mut paths, &mut batch_seen) {
                // A watcher error (ejected volume, watch limit): drop the whole
                // set and let the next pass re-arm it — that poll is the
                // backoff, the app keeps running either way.
                watchers.clear();
            }
        }
        let Some(mut store) = load_store_logged() else {
            continue;
        };
        let files = map_to_folders(&store, &paths);
        // Merge the new burst with the parked stock and start one run for all of
        // it if one is allowed. Merging (not replacing) is what keeps an earlier
        // burst — e.g. one parked during a pause — from being dropped by this one;
        // an empty batch (only directories, only deletions) therefore cannot lose
        // what is still parked either.
        handle_files(
            files,
            &mut pending,
            &mut pending_seen,
            &mut store,
            &shared,
            &paused,
            &cfg,
            concurrency,
            limit_kbps,
            &album,
        );
    }
}

/// Persist a remembered conflict answer into the folder the asked file belongs
/// to — the single place that writes it (user decision 2026-10-15).
///
/// "Nicht noch einmal fragen" keeps working without a prompt in **that folder**
/// on later runs, so the answer lands in the folder's own `conflict_resolution`
/// (`folders.toml`). No schema change: the field already exists, and a folder
/// that keeps its value simply keeps whatever policy it had.
///
/// Refuses — and writes nothing — whenever the target is not provably a
/// persisted folder this file belongs to:
/// - the transient drag & drop target (`ONE_TIME_IDX`), which has no
///   `folders.toml` entry at all and stays run-only,
/// - an index outside the store (`FolderStore::remove` shifts indices), and
/// - a file that does not live below that folder's own path — the decisive
///   guard, because the shifted index is exactly what could point at a
///   neighbouring folder otherwise.
///
/// Returns whether it wrote.
fn remember_conflict_in_folder(
    store: &mut FolderStore,
    idx: usize,
    file_path: &Path,
    answer: ConflictResolution,
) -> bool {
    if idx == ONE_TIME_IDX {
        return false;
    }
    let Some(folder) = store.folders.get_mut(idx) else {
        return false;
    };
    if !file_path.starts_with(&folder.path) {
        return false;
    }
    folder.conflict_resolution = answer;
    true
}

/// Checkbox text of the conflict dialog. `None` stands for the transient
/// drag & drop target, which has no folder to persist the answer into.
///
/// A persisted folder is named, mirroring the original pattern: the answer is
/// written into **that** folder (see [`remember_conflict_in_folder`]), so the
/// checkbox says which folder will stop asking. `None` keeps the neutral
/// "Nicht noch einmal fragen" — the run-wide override still applies, it is just
/// not persisted anywhere.
fn remember_checkbox_label(folder_name: Option<&str>) -> String {
    match folder_name {
        Some(name) => tr1(
            "Nicht noch einmal nach ‚{}'-Ordner fragen",
            crate::text::nfc(name),
        ),
        None => tr("Nicht noch einmal fragen").to_string(),
    }
}

impl PrimePhotoApp {
    // -- folders ------------------------------------------------------------

    /// Ask for a folder for a **new** backup target. The native dialog is opened
    /// asynchronously (Welle 5.1) — the UI keeps running; `poll_folder_picker`
    /// opens the Create form once a folder was picked.
    pub(crate) fn add_backup_folder(&mut self, ctx: &egui::Context) {
        self.spawn_folder_picker(FolderPickerTarget::NewBackup, ctx);
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
        // Forced pause (user decision 2026-10-16): a target whose source folder
        // is gone is paused, not scanned — and it is kept **out** of the plan so
        // a run can never settle it as `Done`. A folder whose drive came back is
        // lifted to `Idle` here as well. The missing source gets a plain-language
        // message instead of "no photos found".
        let mut changed = false;
        let mut missing = false;
        let mut present: Vec<usize> = Vec::new();
        for &i in &indices {
            let Some(f) = self.store.folders.get_mut(i) else {
                continue;
            };
            let exists = f.path.is_dir();
            let next = source_status(exists, f.status);
            if next != f.status {
                f.status = next;
                changed = true;
            }
            if exists {
                present.push(i);
            } else {
                missing = true;
            }
        }
        if changed {
            let _ = self.store.save();
        }
        let Some(mut plan) = UploadPlan::build(&self.store, &present) else {
            if missing {
                self.log(tr(
                    "Der Quellordner ist gerade nicht erreichbar (z. B. externes Laufwerk nicht angeschlossen).",
                ));
            } else {
                self.log(tr("Keine Fotos oder Videos in diesem Ordner gefunden."));
            }
            return;
        };
        let concurrency = if self.slow_mode { 1 } else { self.config.concurrency() };
        let limit = if self.config.limit_enabled {
            self.config.limit_kbps
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
            )
            .await;
        });
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
            self.log(tr("Keine Fotos oder Videos in diesem Ordner gefunden."));
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
        // The transient target itself: the stored name stays the plain German
        // key, `tr` is applied at render time. Dedup and conflict handling both
        // follow their application default (user decision 2026-10-15; before
        // that `dedup: true` / `ConflictResolution::default()` — same values
        // unless the user changed the app defaults in the settings).
        self.one_time = Some(folders::BackupFolder {
            path: display_path,
            name: "Einmalige Sicherung".to_string(),
            enabled: true,
            dedup: self.config.dedup_default,
            conflict_resolution: self.config.conflict_resolution_default,
            content_filter: folders::ContentFilter::Photos,
            total_files: files.len(),
            uploaded_files: 0,
            last_updated: String::new(),
            status: FolderStatus::Uploading,
        });

        // One plan, one synthetic folder index: the counters, the live queue
        // and `folder_opts` look exactly like a single-folder run.
        let mut plan = UploadPlan::from_files(
            ONE_TIME_IDX,
            self.config.dedup_default,
            self.config.conflict_resolution_default,
            files,
        );
        let concurrency = if self.slow_mode { 1 } else { self.config.concurrency() };
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
            )
            .await;
        });
    }

    /// Modal prompt for a name conflict (`ConflictResolution::Ask`,
    /// docs/SPEC_DEDUP.md §4.1). Rendered from `update` independently of the
    /// current page; the answer is written into the shared state, where the
    /// waiting worker picks it up. Drawn only while a conflict is pending.
    pub(crate) fn conflict_dialog(&mut self, ctx: &egui::Context) {
        // Snapshot the prompt *and* its provenance under one lock. The worker
        // clears all three together once it takes the answer, so reading them in
        // one go is what keeps the persisted folder consistent with the name
        // the user actually saw.
        let (file_name, pending_idx, pending_path) = {
            let s = self.upload.lock().unwrap();
            (
                s.pending_conflict.clone(),
                s.pending_conflict_idx,
                s.pending_conflict_path.clone(),
            )
        };
        let Some(file_name) = file_name else {
            return;
        };
        // The worker only polls the shared state; it does not paint frames, so
        // keep the dialog (and the repaint that shows it) alive from here.
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        let mut remember = self.upload.lock().unwrap().conflict_remember;
        let mut answer: Option<ConflictResolution> = None;
        // Name the folder the answer is persisted into — but only when the
        // pending target really is one. The transient drag & drop queue
        // (`ONE_TIME_IDX`) has no `folders.toml` entry, and an index the store
        // no longer holds must not borrow a neighbouring folder's name; both
        // fall back to the neutral text and stay run-only, exactly like
        // `remember_conflict_in_folder` refuses to write for them.
        let remember_label = match pending_idx {
            Some(idx) if idx != ONE_TIME_IDX => self
                .store
                .folders
                .get(idx)
                .map(|f| remember_checkbox_label(Some(&f.name)))
                .unwrap_or_else(|| remember_checkbox_label(None)),
            _ => remember_checkbox_label(None),
        };
        // NOTE: egui 0.31 has no `Window::modal`; the dialog is centred and
        // covers the page it matters for. A second upload run is still
        // impossible while it waits (`upload_busy()`).
        egui::Window::new(tr("Dateiname-Konflikt"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(tr1(
                    "„{}“ existiert bereits in Amazon Photos.",
                    &file_name,
                ));
                ui.checkbox(&mut remember, remember_label.as_str());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button(tr("Ignorieren"))).clicked() {
                        answer = Some(ConflictResolution::Ignore);
                    }
                    if ui.button(tr("Als neue Datei speichern")).clicked() {
                        answer = Some(ConflictResolution::SaveAsNew);
                    }
                    if ui.button(tr("Ersetzen")).clicked() {
                        answer = Some(ConflictResolution::Replace);
                    }
                });
            });
        if let Some(a) = answer {
            {
                let mut s = self.upload.lock().unwrap();
                s.conflict_remember = remember;
                s.conflict_answer = Some(a);
            }
            // "Nicht noch einmal fragen" also persists into the folder the asked
            // file belongs to (user decision 2026-10-15), so the next run there
            // stops asking. Purely additive: the run-wide override the worker
            // applies from `conflict_remember` is untouched, and the transient
            // drag & drop target has no folder to write to — it stays run-only.
            if remember {
                if let (Some(idx), Some(path)) = (pending_idx, &pending_path) {
                    if remember_conflict_in_folder(&mut self.store, idx, path, a) {
                        // Same as every other folder write: a failed save must
                        // not cost the user the answer they just gave.
                        let _ = self.store.save();
                    }
                }
            }
        } else {
            // Remember the checkbox even before the user commits to an option.
            self.upload.lock().unwrap().conflict_remember = remember;
        }
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
    fn ensure_background_sync(&mut self) {
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
        let concurrency = if self.slow_mode { 1 } else { self.config.concurrency() };
        let limit = if self.config.limit_enabled {
            self.config.limit_kbps
        } else {
            0
        };
        let album = self.album_name.clone();
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
            watch_loop(shared, paused, session, cfg, concurrency, limit, album).await;
        });
    }

    pub(crate) fn finalize_upload(&mut self) {
        // A cancelled run (sign-out) is not a finished run: no folder status, no
        // counters, no `last_updated`, no store save, and the one-time queue is
        // kept so the next start resumes it (user decision 2026-10-16). The
        // already-uploaded part stays in the dedup index — nothing is uploaded
        // twice.
        if self.upload.lock().unwrap().cancelled {
            return;
        }
        let (folder_done, folder_failed, folder_total) = {
            let s = self.upload.lock().unwrap();
            (
                s.folder_done.clone(),
                s.folder_failed.clone(),
                s.folder_total.clone(),
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
        for (i, f) in self.store.folders.iter_mut().enumerate() {
            // `folder_total` — the run's plan — is the trigger, not
            // `folder_done` (user decision 2026-10-16): it holds every folder
            // the run took part in, **including** one whose scan found nothing
            // to upload — a folder with no pending file never gets a
            // `folder_done` entry, so the old trigger left it unsettled and the
            // row kept showing an empty state. Such a folder is now recorded as
            // `Done` ("gescannt + nichts zu tun ⇒ abgeschlossen"). The trap
            // this must not fall into: a **never-scanned** folder also has
            // `total == 0`, but it is absent from `folder_total` (only plan
            // folders are), so it is untouched and keeps claiming nothing.
            if let Some(&total) = folder_total.get(&i) {
                // The run's own file count goes with it: `uploaded_files` alone
                // was written back, which left `total_files` from an earlier scan
                // or run and made the row read "480 von 150". Only targets that
                // really took part in the run are touched.
                f.total_files = total;
                f.uploaded_files = folder_done.get(&i).copied().unwrap_or(0);
                f.last_updated = now.clone();
                f.status = if folder_failed.get(&i).copied().unwrap_or(0) > 0 {
                    FolderStatus::Error
                } else {
                    FolderStatus::Done
                };
            }
        }
        let _ = self.store.save();
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
                self.add_backup_folder(ui.ctx());
            }
            let all_paused = *self.paused.borrow();
            let label = if all_paused {
                tr("Alle Sicherungsvorgänge fortsetzen")
            } else {
                tr("Alle Sicherungsvorgänge anhalten")
            };
            // Only the live "is a run active" flag stays here; the aggregated
            // progress display moved to the footer below (user decision
            // 2026-10-14): one display over *all* targets, not just this run.
            let running = self.upload.lock().unwrap().active;
            if ui
                .add_enabled(
                    // Pausable in the idle state too (user decision 2026-10-14):
                    // with no run active it blocks the background sync, and a
                    // pure drag & drop run has no persisted folders either.
                    !self.store.folders.is_empty() || running,
                    theme::secondary_button(label),
                )
                .clicked()
            {
                let next = !all_paused;
                self.paused.send_replace(next);
                // Persist the new state (user decision 2026-10-14): the pause
                // survives a restart. `save_config` is the existing path — it
                // reports a failure through `config_error` as usual.
                self.config.paused = next;
                self.save_config();
            }
        });

        // Snapshot rows for read-only rendering. Built **before** the queue table
        // (Welle 3.1/3.3): the aggregate progress next to the queue and the live
        // per-folder status both read them, and neither may hold the shared lock
        // twice.
        let rows: Vec<Row> = {
            let s = self.upload.lock().unwrap();
            let mut rows: Vec<Row> = self
                .store
                .folders
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let total = s.folder_total.get(&i).copied().unwrap_or(f.total_files);
                    let done = s.folder_done.get(&i).copied().unwrap_or(f.uploaded_files);
                    // Cap the completed count (Welle 5.11): a legacy folder whose
                    // `total_files < uploaded_files` must not read above 100 % in
                    // the row's progress text; the same value feeds the aggregate.
                    let done = capped_done(done, total);
                    // Live status (Welle 3.3): while the run counts this target in
                    // `folder_total` it is *uploading*, regardless of the (stale)
                    // persisted status — the persisted outcome only applies once
                    // the run settled and `active` is false again. A background
                    // run writes its result into a fresh `FolderStore::load()`
                    // (`finalize_background_store`), never into `self.store`, so
                    // without this the row kept showing the previous outcome.
                    let status = if s.active && s.folder_total.contains_key(&i) {
                        FolderStatus::Uploading
                    } else if f.enabled {
                        // The forced pause is a *live* condition (user decision
                        // 2026-10-16): the detached supervisor writes a missing
                        // source to `folders.toml`, which the UI's in-memory store
                        // never reloads — so the row applies the very same rule
                        // (`source_status`) itself. Same helper, no second logic;
                        // a returned drive flips the row back on its own.
                        source_status(f.path.is_dir(), f.status)
                    } else {
                        f.status
                    };
                    Row {
                        idx: i,
                        name: f.name.clone(),
                        path: f.path.clone(),
                        enabled: f.enabled,
                        status,
                        total,
                        done,
                        new: s.folder_new.get(&i).copied().unwrap_or(0),
                        existing: s.folder_conflict.get(&i).copied().unwrap_or(0),
                        unchanged: s.folder_unchanged.get(&i).copied().unwrap_or(0),
                        failed: s.folder_failed.get(&i).copied().unwrap_or(0),
                        last_updated: f.last_updated.clone(),
                    }
                })
                .collect();
            // The transient one-time target is a single extra row after the
            // persisted folders — same chrome, but it has no row-action icons
            // (kebab/refresh) and no pause toggle (nothing to configure,
            // nothing to stop).
            if let Some(one) = self.one_time.as_ref() {
                let total = s
                    .folder_total
                    .get(&ONE_TIME_IDX)
                    .copied()
                    .unwrap_or(one.total_files);
                let done = s
                    .folder_done
                    .get(&ONE_TIME_IDX)
                    .copied()
                    .unwrap_or(one.uploaded_files);
                // Same cap as the persisted rows (Welle 5.11).
                let done = capped_done(done, total);
                // Same live-status rule as the persisted folders (Welle 3.3).
                let status = if s.active && s.folder_total.contains_key(&ONE_TIME_IDX) {
                    FolderStatus::Uploading
                } else {
                    one.status
                };
                rows.push(Row {
                    idx: ONE_TIME_IDX,
                    name: tr("Einmalige Sicherung").to_string(),
                    path: one.path.clone(),
                    enabled: true,
                    status,
                    total,
                    done,
                    new: s.folder_new.get(&ONE_TIME_IDX).copied().unwrap_or(0),
                    existing: s.folder_conflict.get(&ONE_TIME_IDX).copied().unwrap_or(0),
                    unchanged: s.folder_unchanged.get(&ONE_TIME_IDX).copied().unwrap_or(0),
                    failed: s.folder_failed.get(&ONE_TIME_IDX).copied().unwrap_or(0),
                    last_updated: one.last_updated.clone(),
                });
            }
            rows
        };

        let mut actions: Vec<FolderAction> = Vec::new();

        // Live per-file queue, grouped by backup folder
        // (docs/SPEC_UPLOAD_PROGRESS.md §4.1/§4.3). The grouped structure is
        // cached and only rebuilt when the run's *file states* changed
        // (`UploadShared::rev`): a 5 000-file run otherwise cloned every file
        // name on every frame, for nothing — only the counters move.
        // Names the store carries more than once: those queue rows and group
        // headers show "Name (Pfad)" so same-named folders stay
        // distinguishable (user decision 2026-10-16).
        let ambiguous = ambiguous_folder_names(&self.store.folders);
        let rev = self.upload.lock().unwrap().rev;
        if self.queue_cache_rev != rev || self.queue_cache.1.is_empty() {
            let groups: Vec<(String, Vec<(String, FileState)>)> = {
                let s = self.upload.lock().unwrap();
                let mut groups: Vec<(String, Vec<(String, FileState)>)> = Vec::new();
                // Folder index -> position in `groups` (keeps first-seen order).
                let mut slot: HashMap<usize, usize> = HashMap::new();
                for e in &s.queue {
                    let folder = if e.folder == ONE_TIME_IDX {
                        // The transient one-time target is not in `store.folders`,
                        // so its queue group is labelled directly.
                        tr("Einmalige Sicherung").to_string()
                    } else {
                        self.store
                            .folders
                            .get(e.folder)
                            .map(|f| queue_folder_label(&f.name, &f.path, &ambiguous))
                            .unwrap_or_default()
                    };
                    let idx = match slot.get(&e.folder).copied() {
                        Some(i) => i,
                        None => {
                            groups.push((folder, Vec::new()));
                            slot.insert(e.folder, groups.len() - 1);
                            groups.len() - 1
                        }
                    };
                    groups[idx].1.push((crate::text::nfc(&e.name), e.state));
                }
                groups
            };
            self.queue_cache = (rev, groups);
            self.queue_cache_rev = rev;
        }
        // Live per-file queue as a virtualised table (Welle 3.1,
        // docs/SPEC_UPLOAD_PROGRESS.md §7.3/§7.8): the queue is permanent
        // main-area content with fixed columns, no longer collapsed away. The
        // rows stay grouped by backup folder — a background run puts **every**
        // enabled folder into one queue (src/backup.rs, run planning), so without
        // the group headers the files of several targets would be
        // indistinguishable. `ScrollArea::show_rows` only needs a uniform row
        // height, so the list is a flat `Vec<QueueRow>` of group headers and file
        // rows that share that height.
        let remaining: usize = self
            .queue_cache
            .1
            .iter()
            .map(|(_, files)| {
                files
                    .iter()
                    .filter(|(_, st)| matches!(st, FileState::Waiting | FileState::Uploading))
                    .count()
            })
            .sum();
        // Flatten the grouped cache and the live queue into one row list. The
        // cache and this grouping both walk `UploadShared::queue` in first-seen
        // folder order, so group `g` takes its label and per-file states from the
        // cache while the file rows carry only a queue index — no file name is
        // cloned per frame. `QueueEntry.folder` is the grouping key again.
        let layout: Vec<QueueRow> = {
            let s = self.upload.lock().unwrap();
            // Folder index -> its queue positions, in first-seen order.
            let mut order: Vec<usize> = Vec::new();
            let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
            for (i, e) in s.queue.iter().enumerate() {
                if !groups.contains_key(&e.folder) {
                    order.push(e.folder);
                }
                groups.entry(e.folder).or_default().push(i);
            }
            let mut layout: Vec<QueueRow> = Vec::with_capacity(s.queue.len() + order.len());
            for (g, folder) in order.iter().enumerate() {
                let (label, open) = self
                    .queue_cache
                    .1
                    .get(g)
                    .map(|(label, files)| {
                        let open = files
                            .iter()
                            .filter(|(_, st)| {
                                matches!(st, FileState::Waiting | FileState::Uploading)
                            })
                            .count();
                        (label.clone(), open)
                    })
                    .unwrap_or_default();
                // A group whose label could not be resolved (a stale folder
                // index) is drawn without its header, exactly as before.
                if !label.is_empty() {
                    layout.push(QueueRow::Header { label, open });
                }
                layout.extend(groups[folder].iter().map(|&i| QueueRow::File { queue_idx: i }));
            }
            layout
        };

        ui.add_space(12.0);
        ui.label(RichText::new(tr1("Warteschlange ({})", remaining)).strong().size(14.0));

        // Shared geometry: header and rows derive the name width the same way so
        // the four columns line up. The scroll bar's width is reserved only when
        // it is not floating (egui's default `ScrollStyle` floats ⇒ 0).
        let avail = ui.available_width();
        let spacing_x = ui.spacing().item_spacing.x;
        let bar_w = ui.spacing().scroll.allocated_width();
        let name_w = (avail
            - bar_w
            - QUEUE_FOLDER_W
            - QUEUE_SIZE_W
            - QUEUE_STATUS_W
            - 3.0 * spacing_x)
        .max(QUEUE_MIN_NAME_W);

        // The page's vertical budget, decided **before** the queue is drawn: the
        // folder list claims its height first, the queue takes the rest — but
        // never more than half of the free height (user decision 2026-10-16).
        // `list_h` is used again at the folder list's own draw time; the
        // function is pure, so both sites agree.
        let free = ui.available_height();
        let (table_h, list_h) = page_heights(free);

        if layout.is_empty() {
            // Centred empty state (Welle 3.1) instead of an empty table. The
            // block may not be taller than the table it replaces (plus the
            // header it saves), or it would eat the list's promised height in a
            // very small window.
            let empty_h = QUEUE_EMPTY_H.min(QUEUE_HEADER_H + table_h + 2.0);
            ui.allocate_ui(egui::vec2(avail, empty_h), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(theme::muted(tr("Keine Uploads in der Warteschlange")));
                });
            });
        } else {
            // Fixed column header (Ordner · Name · Größe · Status), above the
            // scroll area so it never scrolls away.
            ui.horizontal(|ui| {
                ui.add_sized(
                    [QUEUE_FOLDER_W, QUEUE_HEADER_H],
                    egui::Label::new(RichText::new(tr("Ordner")).strong().size(11.0)),
                );
                ui.add_sized(
                    [name_w, QUEUE_HEADER_H],
                    egui::Label::new(RichText::new(tr("Name")).strong().size(11.0)),
                );
                ui.add_sized(
                    [QUEUE_SIZE_W, QUEUE_HEADER_H],
                    egui::Label::new(RichText::new(tr("Größe")).strong().size(11.0)),
                );
                ui.add_sized(
                    [QUEUE_STATUS_W, QUEUE_HEADER_H],
                    egui::Label::new(RichText::new(tr("Status")).strong().size(11.0)),
                );
            });
            // A page that has no scroll area of its own: the table gets exactly
            // the height `page_heights` reserved for it — never more than what
            // is free, so at the minimum window size nothing runs off the
            // bottom. The list below keeps whatever was promised to it.
            egui::ScrollArea::vertical()
                // Own scroll state (user decision 2026-10-16): egui keys a
                // ScrollArea's offset by `make_persistent_id("scroll_area")`
                // when no `id_salt` is given, so the queue and the folder list
                // — both drawn on this page — would share one offset and any
                // wheel scroll would move both at once.
                .id_salt("queue")
                .max_height(table_h)
                // Honor the cap even when it is tiny: the default
                // `min_scrolled_height` (64 px) would otherwise overrule a
                // smaller `max_height` and push the list below off the edge.
                .min_scrolled_height(0.0)
                // `QUEUE_ROW_H` is the *measured* row height without spacing:
                // every row below is allocated at exactly that height, so it
                // advances by `QUEUE_ROW_H + item_spacing.y` (20 + 8 = 28 px),
                // which is precisely what `show_rows` derives internally.
                .show_rows(ui, QUEUE_ROW_H, layout.len(), |ui, range| {
                    // Lock the shared state once per frame for the visible slice.
                    let s = self.upload.lock().unwrap();
                    for i in range {
                        match &layout[i] {
                            QueueRow::Header { label, open } => {
                                // Group line, muted and without a status column
                                // (§7.8). Both row kinds are allocated at exactly
                                // `QUEUE_ROW_H`, so each advances by
                                // `QUEUE_ROW_H + item_spacing.y` — the same height
                                // `show_rows` derives. A plain `ui.horizontal`
                                // here would instead expand to the theme's
                                // `interact_size.y` (26 px), making this row
                                // taller than the file rows and desynchronising
                                // the virtualised scroll positions.
                                let row_w = ui.available_width();
                                ui.allocate_ui_with_layout(
                                    egui::vec2(row_w, QUEUE_ROW_H),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        // The group's folder spans the Ordner and
                                        // Name columns — the group *is* the
                                        // folder, so the file column stays empty.
                                        ui.add_sized(
                                            [QUEUE_FOLDER_W + name_w, QUEUE_ROW_H],
                                            egui::Label::new(
                                                RichText::new(label.as_str()).strong().size(12.0),
                                            )
                                            .truncate(),
                                        );
                                        ui.label(theme::muted(tr1("{} offen", open)).size(11.0));
                                    },
                                );
                            }
                            QueueRow::File { queue_idx } => {
                                let Some(e) = s.queue.get(*queue_idx) else {
                                    continue;
                                };
                                // Befund 11c: an `Uploading` row additionally shows
                                // the real per-file percentage once bytes have gone
                                // out, so progress is visible per file and not only
                                // as the coarse state. The whole visible string is
                                // one i18n key — never assembled from two parts.
                                let (txt, col): (String, egui::Color32) = match e.state {
                                    FileState::Waiting => (tr("Warten").to_string(), theme::MUTED),
                                    FileState::Uploading => {
                                        let pct = calculate_pct(e.size, e.bytes_sent);
                                        if pct > 0 {
                                            (tr1("In Bearbeitung · {}%", pct), theme::TEAL)
                                        } else {
                                            (tr("In Bearbeitung").to_string(), theme::TEAL)
                                        }
                                    }
                                    FileState::Done => {
                                        (tr("Abgeschlossen").to_string(), theme::GREEN)
                                    }
                                    FileState::Failed => {
                                        (tr("Fehlgeschlagen").to_string(), theme::RED)
                                    }
                                    FileState::Skipped => {
                                        (tr("Übersprungen").to_string(), theme::MUTED)
                                    }
                                };
                                // A long name is truncated in the cell; the full
                                // path shows on hover (§7.3).
                                let full = e.path.display().to_string();
                                // The row's backup folder — the same label the
                                // group header carries (disambiguated when the
                                // store holds the name twice).
                                let folder_label = if e.folder == ONE_TIME_IDX {
                                    tr("Einmalige Sicherung").to_string()
                                } else {
                                    self.store
                                        .folders
                                        .get(e.folder)
                                        .map(|f| queue_folder_label(&f.name, &f.path, &ambiguous))
                                        .unwrap_or_default()
                                };
                                let folder_tip = self
                                    .store
                                    .folders
                                    .get(e.folder)
                                    .map(|f| f.path.display().to_string())
                                    .unwrap_or_default();
                                // Allocated at exactly `QUEUE_ROW_H` so the row
                                // advances by `QUEUE_ROW_H + item_spacing.y`,
                                // matching the group rows and `show_rows` (see
                                // the header arm above).
                                let row_w = ui.available_width();
                                ui.allocate_ui_with_layout(
                                    egui::vec2(row_w, QUEUE_ROW_H),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add_sized(
                                            [QUEUE_FOLDER_W, QUEUE_ROW_H],
                                            egui::Label::new(
                                                RichText::new(folder_label).size(12.0),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(folder_tip);
                                        ui.add_sized(
                                            [name_w, QUEUE_ROW_H],
                                            egui::Label::new(
                                                RichText::new(crate::text::nfc(&e.name)).size(12.0),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(full);
                                        ui.add_sized(
                                            [QUEUE_SIZE_W, QUEUE_ROW_H],
                                            egui::Label::new(
                                                RichText::new(crate::text::format_size(e.size))
                                                    .size(12.0),
                                            ),
                                        );
                                        let status = ui.add_sized(
                                            [QUEUE_STATUS_W, QUEUE_ROW_H],
                                            egui::Label::new(
                                                RichText::new(txt).color(col).size(12.0),
                                            ),
                                        );
                                        // Optional byte detail while a transfer is
                                        // live (Befund 11c).
                                        if e.state == FileState::Uploading && e.size > 0 {
                                            status.on_hover_text(tr2(
                                                "{} von {}",
                                                crate::text::format_size(e.bytes_sent),
                                                crate::text::format_size(e.size),
                                            ));
                                        }
                                    },
                                );
                            }
                        }
                    }
                });
        }

        // Aggregate progress: **one** display over all targets, now directly
        // under the queue (Welle 3.1) instead of in the footer. It stays visible
        // while idle and is still drawn while a run is live even if its last
        // target was just removed (the run continues). The byte/ETA detail only
        // exists for a live run; the outcome counters are already the aggregate
        // over every target (kept for the footer line below).
        let (running, progress_text, outcome) = {
            let s = self.upload.lock().unwrap();
            let outcome = outcome_text(
                s.uploaded_new,
                s.conflict_skipped,
                s.unchanged_skipped,
                s.failed,
            );
            if s.active {
                (true, upload_progress_text(&s), outcome)
            } else {
                (false, String::new(), outcome)
            }
        };
        let (sum_done, sum_total) = aggregate_progress(&rows);
        if running || sum_total > 0 {
            ui.add_space(6.0);
            let frac = if sum_total > 0 {
                (sum_done as f32 / sum_total as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ui.horizontal(|ui| {
                // A spinner belongs to this one display and only while a run is
                // active — never a second, separate indicator.
                if running {
                    ui.spinner();
                }
                let text = if running {
                    // File counts over every target, plus the run's byte/ETA
                    // detail from `upload_progress_text` (kept verbatim).
                    tr3(
                        "Gesamt: {} von {} Dateien · {}",
                        sum_done,
                        sum_total,
                        progress_text,
                    )
                } else if sum_total > 0 && sum_done >= sum_total {
                    tr("Alles aktuell").to_string()
                } else {
                    tr2("Gesamt: {} von {} Dateien", sum_done, sum_total)
                };
                // No `desired_width`: egui then takes everything the row has
                // left after the spinner, so the bar spans the full width and
                // the counter text is not clipped at 240 px.
                ui.add(egui::ProgressBar::new(frac).text(text));
            });
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

        // The folder list gets exactly the height `page_heights` promised it
        // before the queue was drawn (user decision 2026-10-16: the folders
        // matter more than the queue). The draw-time clamp keeps the footer
        // reachable even if the queue section consumed more than its
        // reservation (the empty-queue block in a very small window).
        let list_h = list_h.min((ui.available_height() - QUEUE_FOOTER_H).max(0.0));
        egui::ScrollArea::vertical()
            // Own scroll state, for the same reason as the queue's salt above:
            // without it both areas share one offset and scroll together.
            .id_salt("folders")
            .max_height(list_h)
            // Honor a shrinking cap: `ScrollArea`'s default `min_scrolled_height`
            // (64 px) would otherwise override a smaller `max_height` and push the
            // footer below off the bottom edge in a very small window.
            .min_scrolled_height(0.0)
            .show(ui, |ui| {
                // While a file drag hovers the window, the backup page keeps
                // its hands off: no clicks out from under the overlay.
                if self.dropping_files {
                    ui.disable();
                }
                for r in &rows {
                    let (status_txt, status_col) = status_text(r.status, r.total);
                    egui::Frame::new()
                        .fill(theme::CARD_BG)
                        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .outer_margin(egui::Margin::symmetric(0, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(crate::text::nfc(&r.name)).strong().size(15.0));
                                    // Line 2 (Variant A, user decision 2026-10-16):
                                    // the original's three building blocks, each
                                    // exactly once — the path (always visible), the
                                    // progress line and "Zuletzt aktualisiert".
                                    // Empty parts are omitted, so a never-scanned
                                    // folder shows nothing but its path.
                                    let mut line2 =
                                        crate::text::nfc(&r.path.display().to_string());
                                    let progress =
                                        folder_progress_text(r.status, r.done, r.total);
                                    if !progress.is_empty() {
                                        line2.push_str(" · ");
                                        line2.push_str(&progress);
                                    }
                                    if !r.last_updated.is_empty() {
                                        line2.push_str(" · ");
                                        line2.push_str(&tr1(
                                            "Zuletzt aktualisiert: {}",
                                            &r.last_updated,
                                        ));
                                    }
                                    ui.label(theme::muted(line2).size(11.0));
                                    // The target's own outcome breakdown (user
                                    // decision 2026-10-14). Only shown when the
                                    // run produced at least one result for it —
                                    // an untouched row (all zero) stays quiet.
                                    let outcome =
                                        outcome_text(r.new, r.existing, r.unchanged, r.failed);
                                    if !outcome.is_empty() {
                                        ui.label(theme::muted(outcome).size(11.0));
                                    }
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        // The one-time target is transient:
                                        // no kebab/refresh action icons (nothing
                                        // to edit, reveal or stop) and no pause
                                        // toggle.
                                        if r.idx != ONE_TIME_IDX {
                                            // Row actions. `egui::menu_button`
                                            // cannot carry an image (0.31 has no
                                            // `From<Image> for WidgetText`), so
                                            // the kebab is a `Button::image` and
                                            // the popup is driven by hand via
                                            // `toggle_popup` + `popup_below_widget`.
                                            // The tint follows the theme text
                                            // colour (the raster is white).
                                            let kebab = ui
                                                .add(
                                                    egui::Button::image(icons::kebab_icon(ui))
                                                        .image_tint_follows_text_color(true)
                                                        .frame(false),
                                                )
                                                .on_hover_text(tr("Aktionen"));
                                            let popup_id =
                                                ui.make_persistent_id(("folder_row_actions", r.idx));
                                            if kebab.clicked() {
                                                ui.memory_mut(|m| m.toggle_popup(popup_id));
                                            }
                                            egui::popup::popup_below_widget(
                                                ui,
                                                popup_id,
                                                &kebab,
                                                egui::popup::PopupCloseBehavior::CloseOnClick,
                                                |ui| {
                                                    // popup_below_widget sizes the popup to
                                                    // its parent widget — here the ~26 px
                                                    // kebab icon — which would wrap every
                                                    // entry character by character.
                                                    ui.set_min_width(220.0);
                                                    ui.style_mut().wrap_mode =
                                                        Some(egui::TextWrapMode::Extend);
                                                    if ui
                                                        .button(tr("Im Dateimanager anzeigen"))
                                                        .clicked()
                                                    {
                                                        actions.push(FolderAction::OpenFinder(r.idx));
                                                    }
                                                    if ui.button(tr("Wiederholen")).clicked() {
                                                        actions.push(FolderAction::Retry(r.idx));
                                                    }
                                                    if ui.button(tr("Bearbeiten")).clicked() {
                                                        actions.push(FolderAction::Edit(r.idx));
                                                    }
                                                    ui.separator();
                                                    if ui
                                                        .button(tr("Datensicherung stoppen"))
                                                        .clicked()
                                                    {
                                                        actions.push(FolderAction::Remove(r.idx));
                                                    }
                                                },
                                            );
                                            // The rescan is its own, always
                                            // visible button — no longer hidden
                                            // inside the popup.
                                            if ui
                                                .add(
                                                    egui::Button::image(icons::refresh_icon(ui))
                                                        .image_tint_follows_text_color(true)
                                                        .frame(false),
                                                )
                                                .on_hover_text(tr("Jetzt scannen und hochladen"))
                                                .clicked()
                                            {
                                                actions.push(FolderAction::Upload(r.idx));
                                            }
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

        // Footer: only the run's outcome breakdown over **all** targets is left
        // here — the aggregate progress display moved up next to the queue
        // (Welle 3.1). This is a result line, not a second progress bar, and it
        // stays hidden while every counter is 0 (before any run, or freshly
        // reset) and while there is no live run and no real total — no empty or
        // misleading line.
        if (running || sum_total > 0) && !outcome.is_empty() {
            ui.add_space(8.0);
            ui.label(theme::muted(outcome).size(11.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` distinct event entries for one folder, as `park_paths` takes them.
    fn files(range: std::ops::Range<usize>) -> Vec<(usize, PathBuf)> {
        range
            .map(|i| (0, PathBuf::from(format!("/photos/{i}.jpg"))))
            .collect()
    }

    /// A `Row` reduced to what the aggregate reads (`idx`, `total`, `done`).
    fn row(idx: usize, total: usize, done: usize) -> Row {
        Row {
            idx,
            name: String::new(),
            path: PathBuf::new(),
            enabled: true,
            status: FolderStatus::Idle,
            total,
            done,
            new: 0,
            existing: 0,
            unchanged: 0,
            failed: 0,
            last_updated: String::new(),
        }
    }

    #[test]
    fn aggregate_is_zero_without_rows() {
        assert_eq!(aggregate_progress(&[]), (0, 0));
    }

    #[test]
    fn aggregate_sums_folders_and_the_one_time_row() {
        // Two persisted folders plus the transient one-time row
        // (`ONE_TIME_IDX`): the footer sums over all three.
        let rows = vec![row(0, 100, 40), row(1, 20, 20), row(ONE_TIME_IDX, 5, 1)];
        assert_eq!(aggregate_progress(&rows), (61, 125));
    }

    #[test]
    fn aggregate_ignores_a_target_without_a_total() {
        // A target that knows no total contributes nothing — it must not skew
        // the sum (and the caller guards the division by zero).
        let rows = vec![row(0, 0, 0), row(1, 10, 3)];
        assert_eq!(aggregate_progress(&rows), (3, 10));
    }

    #[test]
    fn capped_done_never_exceeds_total() {
        // Normal progress passes through unchanged.
        assert_eq!(capped_done(5, 10), 5);
        assert_eq!(capped_done(10, 10), 10);
        // The inconsistent legacy state (once shown as "320 %") is capped at 100 %.
        assert_eq!(capped_done(320, 100), 100);
        // No known total: no progress, and never a division by zero downstream.
        assert_eq!(capped_done(0, 0), 0);
        assert_eq!(capped_done(3, 0), 0);
    }

    #[test]
    fn source_status_pauses_missing_and_lifts_when_back() {
        use FolderStatus::{Done, Error, Idle, Paused, Scanning, Uploading};
        // A missing source pauses the target, from any state.
        for current in [Idle, Scanning, Uploading, Paused, Done, Error] {
            assert_eq!(source_status(false, current), Paused);
        }
        // A source that is back lifts the forced pause ...
        assert_eq!(source_status(true, Paused), Idle);
        // ... and leaves every other state alone (no run state is rewritten).
        for current in [Idle, Scanning, Uploading, Done, Error] {
            assert_eq!(source_status(true, current), current);
        }
    }

    #[test]
    fn park_keeps_every_path_without_a_cap() {
        let mut pending: Vec<(usize, PathBuf)> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();

        // A burst far beyond the old 500-path cap: all of it survives.
        let added = park_paths(&mut pending, &mut seen, files(0..2_000));

        assert_eq!(added, 2_000);
        assert_eq!(pending.len(), 2_000);
        assert_eq!(seen.len(), 2_000);
    }

    #[test]
    fn park_deduplicates_by_path() {
        let mut pending: Vec<(usize, PathBuf)> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let p = PathBuf::from("/photos/a.jpg");

        assert_eq!(park_paths(&mut pending, &mut seen, vec![(0, p.clone())]), 1);
        // The same path again is already known: nothing is added.
        assert_eq!(park_paths(&mut pending, &mut seen, vec![(0, p.clone())]), 0);

        assert_eq!(pending.len(), 1);
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn park_merges_a_new_burst_without_loss_or_duplicates() {
        let mut pending: Vec<(usize, PathBuf)> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();

        // A stock parked while a run was busy or paused ...
        park_paths(&mut pending, &mut seen, files(0..100));
        // ... plus a burst that overlaps it (50..150): only the 50 new paths are
        // added, the 50 overlapping ones are neither duplicated nor dropped.
        let added = park_paths(&mut pending, &mut seen, files(50..150));

        assert_eq!(added, 50);
        assert_eq!(pending.len(), 150);
        assert_eq!(seen.len(), 150);
        // The invariant the merge relies on: `seen` mirrors `pending` exactly.
        let pending_paths: HashSet<PathBuf> =
            pending.iter().map(|(_, p)| p.clone()).collect();
        assert_eq!(pending_paths, seen);
    }

    // -- "Nicht noch einmal fragen" persisted per folder -------------------

    #[test]
    fn remember_label_is_neutral_for_the_one_time_queue() {
        // `None` stands for the transient drag & drop target: it has no
        // `folders.toml` entry, so its answer stays run-wide only and there is
        // no folder the checkbox could name.
        let label = remember_checkbox_label(None);

        assert_eq!(label, "Nicht noch einmal fragen");
        assert!(
            !label.contains("Ordner"),
            "the neutral label must not name any folder, got: {label}"
        );
    }

    #[test]
    fn remember_label_names_the_folder() {
        // A persisted folder owns the answer (see `remember_conflict_in_folder`),
        // so the checkbox names which folder will stop asking.
        let label = remember_checkbox_label(Some("Sport"));

        assert!(
            label.contains("Sport"),
            "expected the folder name in: {label}"
        );
        assert!(
            label.contains("-Ordner fragen"),
            "expected the folder wording in: {label}"
        );
    }

    /// A folder path under the system temp dir. No test here may touch the real
    /// `folders.toml`; these are also paths that merely *look* like a folder,
    /// because the guard is a pure path check and needs no real directory.
    fn scratch_folder(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "prime-photos-remember-{}-{name}",
            std::process::id()
        ))
    }

    /// A store holding one folder at `path` whose policy is `Ask` — the only
    /// state a folder can be in for the prompt to appear at all.
    fn asking_store(path: &Path) -> FolderStore {
        let mut store = FolderStore::default();
        let mut folder = folders::BackupFolder::new(path.to_path_buf());
        folder.conflict_resolution = ConflictResolution::Ask;
        store.folders.push(folder);
        store
    }

    #[test]
    fn remember_ignores_the_transient_drag_and_drop_target() {
        // The one-time queue has no `folders.toml` entry (`ONE_TIME_IDX`), so
        // there is nothing to write — it stays run-only by design.
        let root = scratch_folder("onetime");
        let mut store = asking_store(&root);
        let file = root.join("photo.jpg");

        assert!(!remember_conflict_in_folder(
            &mut store,
            ONE_TIME_IDX,
            &file,
            ConflictResolution::Replace,
        ));
        assert_eq!(
            store.folders[0].conflict_resolution,
            ConflictResolution::Ask,
            "the one-time target must not touch a persisted folder"
        );
    }

    #[test]
    fn remember_refuses_an_index_outside_the_store() {
        // `FolderStore::remove` shifts indices mid-run: the remembered index can
        // point past the end. Writing nothing is the only safe reaction.
        let mut store = asking_store(&scratch_folder("oob"));
        let file = store.folders[0].path.join("photo.jpg");

        assert!(!remember_conflict_in_folder(
            &mut store,
            1,
            &file,
            ConflictResolution::Replace,
        ));
        assert_eq!(
            store.folders[0].conflict_resolution,
            ConflictResolution::Ask
        );
    }

    #[test]
    fn remember_refuses_a_file_outside_the_folder() {
        // The decisive guard against shifted indices: an index that now belongs
        // to a *different* folder must not inherit this answer.
        let root = scratch_folder("outside");
        let other = scratch_folder("other");
        let mut store = asking_store(&other);
        // A second folder, so index 0 is not the only candidate.
        let mut neighbour = folders::BackupFolder::new(root.clone());
        neighbour.conflict_resolution = ConflictResolution::Ask;
        store.folders.push(neighbour);
        let outside_file = root.join("photo.jpg");

        assert!(!remember_conflict_in_folder(
            &mut store,
            0,
            &outside_file,
            ConflictResolution::Replace,
        ));
        assert_eq!(
            store.folders[0].conflict_resolution,
            ConflictResolution::Ask,
            "a folder that does not contain the asked file must stay untouched"
        );
        assert_eq!(
            store.folders[1].conflict_resolution,
            ConflictResolution::Ask
        );
    }

    #[test]
    fn remember_writes_the_answer_into_the_owning_folder() {
        let root = scratch_folder("happy");
        let mut store = asking_store(&root);
        let file = root.join("sub").join("photo.jpg");

        assert!(remember_conflict_in_folder(
            &mut store,
            0,
            &file,
            ConflictResolution::Replace,
        ));
        assert_eq!(
            store.folders[0].conflict_resolution,
            ConflictResolution::Replace
        );

        // The persisted form is the lowercase serde spelling of the enum
        // (`#[serde(rename_all = "lowercase")]`) — an existing `folders.toml`
        // must round-trip, so this pins "replace", not "Replace" or
        // "save_as_new".
        let toml = toml::to_string_pretty(&store).expect("a store serializes");
        assert!(
            toml.contains("conflict_resolution = \"replace\""),
            "expected the lowercase spelling, got: {toml}"
        );
    }
}
