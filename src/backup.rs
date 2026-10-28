use crate::app::{FolderPickerTarget, Page, ONE_TIME_IDX, PrimePhotoApp};
use crate::backup_config::{BackupConfigForm, ConfigFormMode};
use crate::config::Config;
use crate::dedup::{DedupIndex, Origin};
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

/// Sum over **every** target — `(done, total)` — for the idle aggregate display
/// (user decision 2026-10-14: one display over all backup targets, the one-time
/// row included). During a run the aggregate instead uses run-level counters.
/// Pure: no lock, no UI, no filesystem, and therefore cheap to test. A target
/// that knows no total (`total == 0`) contributes nothing and can never skew
/// the sum; the caller guards the division by zero.
fn aggregate_progress(rows: &[Row]) -> (usize, usize) {
    rows.iter()
        .fold((0usize, 0usize), |(done, total), r| {
            (done + r.done, total + r.total)
        })
}

/// Live progress is scoped to the current run; the library-wide row sum is for
/// the idle display only.
fn displayed_progress(
    running: bool,
    run_done: usize,
    run_total: usize,
    rows: &[Row],
) -> (usize, usize) {
    if running {
        (run_done, run_total)
    } else {
        aggregate_progress(rows)
    }
}

/// Live completion is scoped to the current run. The shared map outlives a run,
/// and its positional folder keys can become stale when store rows shift; idle
/// rows must always use their persisted completion count.
fn row_done(in_run: bool, folder_done_entry: Option<usize>, persisted_done: usize) -> usize {
    if in_run {
        folder_done_entry.unwrap_or(0)
    } else {
        persisted_done
    }
}

/// Use live plan totals only while that row belongs to the active run. The
/// shared map outlives a run, and its positional folder keys can become stale
/// when the store's rows shift; idle rows must use their persisted totals.
fn row_total(
    in_run: bool,
    folder_total_entry: Option<usize>,
    persisted_total: usize,
) -> usize {
    if in_run {
        folder_total_entry.unwrap_or(0)
    } else {
        persisted_total
    }
}

/// Resolve a plan-time folder index to its current store row through the folder
/// path captured when the plan was published. Store indices can shift if a
/// folder is removed while the run is active; the one-time target is synthetic
/// and never resolves to a persisted row.
fn row_index_for(
    store: &FolderStore,
    paths: &HashMap<usize, PathBuf>,
    idx: usize,
) -> Option<usize> {
    if idx == ONE_TIME_IDX {
        return None;
    }
    let path = paths.get(&idx)?;
    store.folders.iter().position(|folder| folder.path == *path)
}

/// Resolve a current store row path to its plan-time index. Folder paths are
/// unique within a run; if a malformed/legacy state violates that invariant,
/// choose the lowest matching plan index so the result remains deterministic.
/// The one-time target is synthetic and never maps to a persisted folder row.
fn plan_index_for(paths: &HashMap<usize, PathBuf>, path: &Path) -> Option<usize> {
    paths
        .iter()
        .filter_map(|(&idx, folder_path)| {
            (idx != ONE_TIME_IDX && folder_path == path).then_some(idx)
        })
        .min()
}

/// Apply a run's folder counters to the current store rows, resolving each
/// plan-time index by the path captured for that run.
fn apply_folder_progress(
    store: &mut FolderStore,
    paths: &HashMap<usize, PathBuf>,
    folder_total: &HashMap<usize, usize>,
    folder_done: &HashMap<usize, usize>,
    folder_failed: &HashMap<usize, usize>,
    now: &str,
) {
    for (&plan_idx, &total) in folder_total {
        let Some(row_idx) = row_index_for(store, paths, plan_idx) else {
            continue;
        };
        let folder = &mut store.folders[row_idx];
        folder.total_files = total;
        folder.uploaded_files = folder_done.get(&plan_idx).copied().unwrap_or(0);
        folder.last_updated = now.to_owned();
        folder.status = if folder_failed.get(&plan_idx).copied().unwrap_or(0) > 0 {
            FolderStatus::Error
        } else {
            FolderStatus::Done
        };
    }
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

/// Forced state for a missing source (user decisions 2026-10-16, 2026-10-17,
/// `docs/original/SPEC_MISSING_SOURCE.md`): a backup target whose source folder
/// is gone must be flagged **`NotFound`** — the original's
/// `ScheduleError.LocalFolderNotFound` ("Ordner nicht gefunden", AMBER), not
/// errored — and it must recover by itself once the source is back.
///
/// Pure and total, so every path that decides a folder's state reaches the same
/// verdict:
/// - the source is not stat-able (`path_exists == false`) ⇒ `NotFound`,
/// - the source is back and the folder was `NotFound` — or a `Paused` written
///   by an older `folders.toml` — ⇒ `Idle` (the state is lifted; the
///   never-advanced watermark makes the next cold start re-scan the files that
///   were missed),
/// - otherwise the status is left untouched.
///
/// This rule never touches the **global** user pause (`Config::paused`): that is
/// a separate switch and keeps the `Paused` wording of the real pause.
fn source_status(path_exists: bool, current: FolderStatus) -> FolderStatus {
    if !path_exists {
        FolderStatus::NotFound
    } else if current == FolderStatus::NotFound || current == FolderStatus::Paused {
        FolderStatus::Idle
    } else {
        current
    }
}

/// Whether this pass is the **return of a missing source**: the folder was
/// `NotFound` and its path is back. Exactly this edge (`NotFound` → present)
/// must read the folder's stock in once, so `watch_loop` parks it for a full
/// scan (no watermark, no "only changes"). Deliberately narrow:
/// - a folder that was `Paused` is **not** a return (user decision 2026-10-17),
/// - an `Idle`/`Done`/… folder whose path is simply still there is not one
///   either — otherwise every `WATCH_POLL` pass would scan again.
///
/// Pure (no store, no filesystem), so the edge is testable on its own.
fn returned_from_missing(current: FolderStatus, path_exists: bool) -> bool {
    path_exists && current == FolderStatus::NotFound
}

/// Re-decide every enabled folder's source state with the pure rule
/// [`source_status`] and return the paths that just **came back**
/// ([`returned_from_missing`]) — the folders `watch_loop` parks for a
/// full-stock scan. Also reports whether any state actually moved, so the
/// caller saves the store only then.
///
/// The existence check is a parameter (`Path::is_dir` at the real call site, a
/// plain predicate in the tests), so no test needs a real drive.
fn lift_returned_sources(
    store: &mut FolderStore,
    path_exists: impl Fn(&Path) -> bool,
) -> (Vec<PathBuf>, bool) {
    let mut returned = Vec::new();
    let mut changed = false;
    for i in store.enabled_indices() {
        if let Some(f) = store.folders.get_mut(i) {
            let exists = path_exists(&f.path);
            if returned_from_missing(f.status, exists) {
                returned.push(f.path.clone());
            }
            let next = source_status(exists, f.status);
            if next != f.status {
                f.status = next;
                changed = true;
            }
        }
    }
    (returned, changed)
}

/// Label of a backup folder in the queue's per-row folder column: the folder
/// name, plus the **parent folder's name** in parentheses when **more than
/// one** backup folder carries that name (`ambiguous`) — two folders both
/// named "Sport" would otherwise be indistinguishable in the queue (user
/// decision 2026-10-16). Only the parent's *name* ("Sport (Pictures)"), not
/// its full path: the label must fit the 120 px column, the full path stays in
/// the row's tooltip (`folder_tip` at the call site). The ambiguity is
/// computed once per frame over the active run's captured paths, so removed
/// store rows do not change their queue labels. The name is shown
/// NFC-normalised, like every other displayed path in the app.
fn queue_folder_label(name: &str, path: &Path, ambiguous: &HashSet<String>) -> String {
    let name = crate::text::nfc(name);
    if !ambiguous.contains(&name) {
        return name;
    }
    match path.parent().and_then(|p| p.file_name()) {
        Some(parent) => format!("{} ({})", name, crate::text::nfc(&parent.to_string_lossy())),
        // No parent name to disambiguate with (root/empty): the plain name stays.
        None => name,
    }
}

/// Resolve a queue plan index using the path captured when that plan started.
/// The store may have shifted or removed the row since then. The returned pair
/// is the display label and full-path tooltip; synthetic/missing targets have
/// no resolved display and keep their caller's neutral fallback.
fn queue_folder_display(
    paths: &HashMap<usize, PathBuf>,
    idx: usize,
    ambiguous: &HashSet<String>,
) -> Option<(String, String)> {
    if idx == ONE_TIME_IDX {
        return None;
    }
    let path = paths.get(&idx)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Some((
        queue_folder_label(&name, path, ambiguous),
        path.display().to_string(),
    ))
}

/// The set of folder names represented by the active plan paths more than
/// once — the input of [`queue_folder_label`]. Cheap (the run holds a handful
/// of folders) and built once per frame, not per queue row. Names are
/// NFC-normalised so the set matches the displayed form.
fn ambiguous_folder_names_for_paths(paths: &HashMap<usize, PathBuf>) -> HashSet<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (&idx, path) in paths {
        if idx == ONE_TIME_IDX {
            continue;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        *counts.entry(crate::text::nfc(&name)).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .filter(|(_, c)| *c > 1)
        .map(|(name, _)| name)
        .collect()
}

/// Fixed geometry of the live queue table (docs/original/SPEC_UPLOAD_PROGRESS.md §7.8):
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
/// a 12 px gap and the "Automatisch gesicherte Ordner:" heading), the 8 px air
/// between that heading and the folder list (UI review 0.1.1) plus the folder
/// list's own footer room ([`QUEUE_FOOTER_H`]). That is 80 px measured plus the
/// 8 px air = 88 px, 96 px with a small margin. This is measured content, not a
/// panel share — it stays absolute on purpose; only the *range caps/floors* of
/// the two scroll areas are relative. The Backup page has no scroll area of its
/// own, so the table may never eat this space; the list below then gets what is
/// left.
const QUEUE_BELOW_H: f32 = 96.0;
/// Height the folder list must leave for the footer below it (8 px gap + line).
const QUEUE_FOOTER_H: f32 = 30.0;
/// Height of one folder card (Variant A), **measured headless** with egui
/// 0.31.1 at the default font: name line + small path/progress/updated line +
/// outcome line + frame margins. The list's content height is derived from it
/// (Welle 5.17): `page_heights` needs exactly this plus the gaps to size
/// the area to what the cards actually occupy.
const FOLDER_CARD_H: f32 = 60.0;
/// Vertical gap between two folder cards (the frame's outer margin, 4 + 4).
const FOLDER_CARD_GAP: f32 = 8.0;

/// Vertical budget of the Backup page's two scroll areas for a given free
/// height — the single source both draw sites use, so the queue (drawn first)
/// leaves exactly what the folder list was promised.
///
/// Priority (user decision 2026-10-16, Welle 5.17): the **folder list takes
/// what its content needs** — `cards` measured cards plus the gaps between
/// them (0 cards ⇒ 0) — but never so much that the queue falls below its
/// floor or the chrome is eaten; the queue then takes the rest, capped at
/// half of the free height. All caps and floors are fractions of `free`
/// except the measured chrome ([`QUEUE_BELOW_H`], [`QUEUE_FOOTER_H`],
/// [`QUEUE_HEADER_H`]) and the card metrics ([`FOLDER_CARD_H`],
/// [`FOLDER_CARD_GAP`]), which stay absolute.
///
/// Guarantees for every `free >= 0`:
/// - `queue_h + list_h + QUEUE_HEADER_H + QUEUE_BELOW_H <= free` — nothing
///   runs off the bottom edge (the 2 px margin inside `QUEUE_BELOW_H` covers
///   the header being 2 px more conservative than the measured 114 px);
/// - `queue_h <= QUEUE_MAX_FRAC * free` — the queue stays at or below 50 %;
/// - `list_h` never spends the queue's floor reservation — the queue keeps
///   at least `QUEUE_MIN_FRAC * free` whenever the chrome fits at all.
fn page_heights(free: f32, cards: usize) -> (f32, f32) {
    // The queue's fixed chrome: its own column header plus everything below
    // the table (aggregate bar zone + footer room).
    let chrome = QUEUE_HEADER_H + QUEUE_BELOW_H;
    // 1) The folder list first: exactly its content's need — `cards` measured
    //    cards plus the gaps between them (0 cards ⇒ 0) — capped so the queue
    //    keeps its floor and the chrome is never eaten.
    let content_h =
        cards as f32 * FOLDER_CARD_H + cards.saturating_sub(1) as f32 * FOLDER_CARD_GAP;
    let list_h = content_h.min((free - chrome - QUEUE_MIN_FRAC * free).max(0.0));
    // 2) The queue takes what is left, capped at half the free height; its
    //    floor only applies when it actually fits.
    let rest = (free - chrome - list_h).max(0.0);
    let queue_h = rest
        .min(QUEUE_MAX_FRAC * free)
        .max((QUEUE_MIN_FRAC * free).min(rest));
    (queue_h, list_h)
}

/// Status bucket of the live queue's flat ordering: **our own** order, decided
/// by the user on 2026-10-26 — "In Bearbeitung" → "Fehlgeschlagen" → "Warten"
/// → "Abgeschlossen" → "Übersprungen". The original status table is at most a
/// loose reference (Processing first, Errored second; see
/// docs/original/SPEC_STATUS_DETAIL.md §3.3); it additionally sorts by
/// `SortDate` and does not preserve insertion order, so its key is
/// deliberately **not** copied.
fn status_rank(state: FileState) -> u8 {
    match state {
        FileState::Uploading => 0,
        FileState::Failed => 1,
        FileState::Waiting => 2,
        FileState::Done => 3,
        FileState::Skipped => 4,
    }
}

/// Flat, global queue layout: queue indices sorted by status bucket, insertion
/// order (the queue index) inside a bucket. Folder membership deliberately does
/// not matter — no group headers, no folder grouping (user decision
/// 2026-10-26). Pure, so the ordering is testable without a `Ui`.
fn queue_layout(queue: &[QueueEntry]) -> Vec<usize> {
    let mut layout: Vec<usize> = (0..queue.len()).collect();
    layout.sort_by_key(|&i| (status_rank(queue[i].state), i));
    layout
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
    /// Number of supported files considered before the local dedup filter.
    files_seen: usize,
    /// Files omitted by the local stat match, split by their recorded origin.
    unchanged: usize,
    content_skip: usize,
    /// Per-folder counterparts of the two skip counters above.
    folder_unchanged: HashMap<usize, usize>,
    folder_content_skip: HashMap<usize, usize>,
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
            files_seen: 0,
            unchanged: 0,
            content_skip: 0,
            folder_unchanged: HashMap::new(),
            folder_content_skip: HashMap::new(),
            bytes_total: 0,
            sized_total: 0,
            queue: Vec::new(),
            per_folder: HashMap::new(),
            folder_opts: HashMap::new(),
        }
    }

    /// A zero-file plan still carries every selected backup as a target so the
    /// run can create its mirrored destination before reporting an empty scan.
    fn empty_for_targets(store: &FolderStore, indices: &[usize]) -> Self {
        let mut plan = Self::empty();
        plan.add_targets(store, indices);
        plan
    }

    /// Fold `indices` into the plan as run targets even when they contribute no
    /// file: a folder with zero files still has a mirrored cloud destination
    /// that the run's preflight must resolve/create. Touches neither the queue
    /// nor the counters nor a watermark — a plan that already carries files
    /// keeps them and only gains the empty folders' per-folder entries.
    fn add_targets(&mut self, store: &FolderStore, indices: &[usize]) {
        for &i in indices {
            if let Some(folder) = store.folders.get(i) {
                self.per_folder.entry(i).or_insert(0);
                self.folder_opts
                    .insert(i, (folder.dedup, folder.conflict_resolution));
            }
        }
    }

    /// File size of one entry, counted once into the run's byte total and
    /// returned so the queue row can reuse it without a second `stat`. Never
    /// panics: an unreadable file contributes nothing (`0`) and the ETA then
    /// falls back to the file-count rate.
    fn note_size(&mut self, path: &Path) -> u64 {
        self.note_size_value(std::fs::metadata(path).ok().map(|meta| meta.len()))
    }

    /// Count a size already obtained by the folder scan, avoiding a second stat.
    fn note_size_value(&mut self, size: Option<u64>) -> u64 {
        if let Some(len) = size {
            self.bytes_total += len;
            self.sized_total += 1;
            len
        } else {
            0
        }
    }

    fn build(store: &FolderStore, indices: &[usize]) -> Option<Self> {
        // A manual run scans every supported file without a watermark, then
        // queues only files that do not match the local stat/dedup index.
        let wanted: Vec<(usize, Option<i64>)> = indices.iter().map(|&i| (i, None)).collect();
        Self::build_since(store, &wanted)
    }

    /// Build a plan from a **watermark** selection: `wanted` pairs a store index
    /// with the watermark its files must beat (`None` = take everything).
    ///
    /// Same file order as [`UploadPlan::build`], so a manual run is exactly
    /// `build` with every watermark `None`. That manual scan stats every
    /// supported file once and uses the local index to keep unchanged files out
    /// of the queue. Used by the cold-start safety net (user decision 2026-10-14)
    /// as well.
    fn build_since(store: &FolderStore, wanted: &[(usize, Option<i64>)]) -> Option<Self> {
        let index = DedupIndex::open();
        Self::build_since_with_stat_matches(store, wanted, |path, size, mtime| {
            index.stat_matches(path, size, mtime)
        })
    }

    /// Injectable stat lookup keeps the plan filter testable without touching
    /// the user's persistent dedup database.
    fn build_since_with_stat_matches(
        store: &FolderStore,
        wanted: &[(usize, Option<i64>)],
        mut stat_matches: impl FnMut(&Path, u64, i64) -> Option<Origin>,
    ) -> Option<Self> {
        let mut plan = Self::empty();
        for &(i, watermark) in wanted {
            let Some(f) = store.folders.get(i) else {
                continue;
            };
            plan.folder_opts.insert(i, (f.dedup, f.conflict_resolution));
            plan.per_folder.entry(i).or_insert(0);
            // A manual scan has no watermark, so use the metadata-free image
            // walk and read each file's metadata only once below. The cold-start
            // path keeps its watermark selector, which excludes older files
            // before they reach the plan.
            let imgs = match watermark {
                Some(watermark) => folders::scan_new_since(&f.path, Some(watermark)),
                None => folders::scan_images(&f.path),
            };
            for p in imgs {
                let meta = std::fs::metadata(&p).ok();
                let mtime = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                if watermark.is_some_and(|watermark| {
                    !mtime.is_some_and(|mtime| mtime > watermark)
                }) {
                    continue;
                }
                plan.files_seen += 1;
                let size = meta.as_ref().map(|m| m.len());
                let size_for_index = size.unwrap_or(0);
                let mtime_for_index = mtime.unwrap_or(0);
                if f.dedup
                    && let Some(origin) = stat_matches(&p, size_for_index, mtime_for_index)
                {
                    match origin {
                        Origin::Uploaded => {
                            plan.unchanged += 1;
                            *plan.folder_unchanged.entry(i).or_insert(0) += 1;
                        }
                        Origin::ContentSkip => {
                            plan.content_skip += 1;
                            *plan.folder_content_skip.entry(i).or_insert(0) += 1;
                        }
                    }
                    continue;
                }
                // Same order as `items`: the row is this entry's position in
                // the queue, the folder is the backup-folder index `i`.
                let row = plan.items.len();
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let size = plan.note_size_value(size);
                plan.queue.push(QueueEntry {
                    folder: i,
                    name,
                    state: FileState::Waiting,
                    size,
                    path: p.clone(),
                    bytes_sent: 0,
                });
                plan.items.push((row, i, p));
                *plan.per_folder.entry(i).or_insert(0) += 1;
            }
        }
        if plan.files_seen == 0 {
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
        plan.files_seen = files.len();
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
    /// `folder_opts`, same queue layout — while only the files the OS actually
    /// named are uploaded. No folder is walked.
    fn build_from_events(store: &FolderStore, files: &[(usize, PathBuf)]) -> Option<Self> {
        let mut plan = Self::empty();
        for (i, p) in files {
            let Some(f) = store.folders.get(*i) else {
                continue;
            };
            plan.files_seen += 1;
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

    /// Number of files this run still needs to process after local stat skips.
    fn count(&self) -> usize {
        self.items.len()
    }

    /// Start message for a scan that found files but no upload candidates.
    fn start_message(&self) -> String {
        if self.files_seen > 0 && self.items.is_empty() {
            tr("Alles aktuell").to_string()
        } else {
            tr1("{} Datei(en) werden hochgeladen …", self.count())
        }
    }
}

/// Publish a scanned plan on the shared upload state (active, counters, live
/// queue and `msg` as the run-start log line). Both start paths go through
/// here, so a manual run, a background run and a one-time run look and behave
/// identically.
///
/// Takes the live queue out of `plan`; the caller keeps `items`/`folder_opts`
/// for the worker spawn.
fn publish_state(
    plan: &mut UploadPlan,
    shared: &Arc<Mutex<UploadShared>>,
    msg: String,
    folder_paths: HashMap<usize, PathBuf>,
) {
    let mut s = shared.lock().unwrap();
    s.active = true;
    s.started = Some(Instant::now());
    s.done = 0;
    s.failed = 0;
    s.current.clear();
    s.log.clear();
    s.folder_total = plan.per_folder.clone();
    s.folder_paths = folder_paths;
    s.folder_done.clear();
    s.folder_failed.clear();
    // The outcome breakdown is per run too (user decision 2026-10-14): local
    // stat skips are seeded from the plan, while worker outcomes start at zero;
    // no previous run's numbers can survive into this one.
    s.uploaded_new = 0;
    s.conflict_skipped = plan.content_skip;
    s.unchanged_skipped = plan.unchanged;
    s.folder_new.clear();
    s.folder_conflict = plan.folder_content_skip.clone();
    s.folder_unchanged = plan.folder_unchanged.clone();
    s.uploaded_ids.clear();
    s.total = plan.items.len();
    s.queue = std::mem::take(&mut plan.queue);
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
    let folder_paths = plan_folder_paths(plan, store);
    publish_state(plan, shared, msg, folder_paths);
    publish_folder_status(plan, store);
}

/// Capture the plan-time index → path association for a published folder plan.
fn plan_folder_paths(plan: &UploadPlan, store: &FolderStore) -> HashMap<usize, PathBuf> {
    plan
        .per_folder
        .keys()
        .filter_map(|&i| store.folders.get(i).map(|folder| (i, folder.path.clone())))
        .collect()
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
/// 5 s folder debounce (`docs/original/SPEC_CHANGE_DETECTION.md` §4.3).
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
    let (folder_total, folder_done, folder_failed, folder_paths) = {
        let s = shared.lock().unwrap();
        (
            s.folder_total.clone(),
            s.folder_done.clone(),
            s.folder_failed.clone(),
            s.folder_paths.clone(),
        )
    };
    let mut store = match load_store_logged() {
        Some(store) => store,
        None => return,
    };
    let now = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
    apply_folder_progress(
        &mut store,
        &folder_paths,
        &folder_total,
        &folder_done,
        &folder_failed,
        &now,
    );
    match store.save() {
        Ok(()) => {
            let ctx = {
                let mut state = shared.lock().unwrap();
                state.store_dirty = true;
                state.ctx.clone()
            };
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        }
        Err(err) => log::error!("Sicherungsziele konnten nicht gespeichert werden: {err}"),
    }
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

/// Take an already scanned plan into the **live** run: append its rows to the
/// shared queue, its items to the very queue the workers drain, and its folder
/// options/counts to the shared state. Returns the plan unchanged when there is
/// no run to take it into (the caller starts an ordinary run then).
///
/// The decision happens **under the shared lock** and requires the run to still
/// accept work (`active && !sealed && !cancelled && item_queue.is_some()`); the
/// worker that ends the run sets `sealed` under the same lock, so the two can
/// never race an item into a queue nobody drains. A folder that is **already**
/// part of this run (its index is in `folder_total` or its path is in
/// `folder_paths`) is refused too, so a re-triggered folder cannot queue its
/// rows twice or make the index-to-path map ambiguous.
fn inject_into_live_run(
    mut plan: UploadPlan,
    store: &mut FolderStore,
    shared: &Arc<Mutex<UploadShared>>,
) -> Option<UploadPlan> {
    // The takes below happen **after** the check, so the refusal path can hand
    // the plan back untouched.
    let per_folder = plan.per_folder.clone();
    {
        let mut s = shared.lock().unwrap();
        // Deduplicate across time (Verify B): a folder that is already part of
        // this run must not be injected again — its rows would land twice and
        // `folder_total`/`total` would read "x von 2x". The caller parks the
        // folder on this refusal, like on any other.
        let already_here = per_folder.keys().any(|i| s.folder_total.contains_key(i));
        let path_already_here = per_folder.keys().any(|&i| {
            store
                .folders
                .get(i)
                .is_some_and(|folder| s.folder_paths.values().any(|path| path == &folder.path))
        });
        if path_already_here && s.active && !s.sealed && !s.cancelled && s.item_queue.is_some() {
            s.log(tr("Der Ordner wird in diesem Lauf bereits verarbeitet."));
            return Some(plan);
        }
        if already_here || !(s.active && !s.sealed && !s.cancelled && s.item_queue.is_some()) {
            return Some(plan);
        }
        let base = s.queue.len();
        let rows = std::mem::take(&mut plan.queue); // Reihenfolge == items
        let items = std::mem::take(&mut plan.items);
        let n = items.len();
        s.queue.extend(rows);
        if let Some(q) = s.item_queue.as_ref() {
            let mut q = q.lock().unwrap();
            for (row, idx, path) in items {
                q.push_back((base + row, idx, path));
            }
        }
        s.total += n;
        for (&i, &count) in &per_folder {
            *s.folder_total.entry(i).or_insert(0) += count;
            if let Some(folder) = store.folders.get(i) {
                s.folder_paths.entry(i).or_insert_with(|| folder.path.clone());
                s.destination_roots
                    .entry(i)
                    .or_insert_with(|| folder.path.clone());
            }
        }
        s.unchanged_skipped += plan.unchanged;
        s.conflict_skipped += plan.content_skip;
        for (&i, &count) in &plan.folder_unchanged {
            *s.folder_unchanged.entry(i).or_insert(0) += count;
        }
        for (&i, &count) in &plan.folder_content_skip {
            *s.folder_conflict.entry(i).or_insert(0) += count;
        }
        // Byte-ETA: nur weiterführen, wenn beide Seiten vollständig bekannt sind —
        // sonst auf den Datei-Zähler-Fallback zurückfallen (wie `publish_state`).
        s.bytes_total = if s.bytes_total > 0 && plan.sized_total == n {
            s.bytes_total + plan.bytes_total
        } else {
            0
        };
        s.folder_opts.extend(std::mem::take(&mut plan.folder_opts));
        s.log(if plan.files_seen > 0 && n == 0 {
            tr("Alles aktuell").to_string()
        } else {
            tr1("{} Datei(en) zusätzlich aufgenommen.", n)
        });
    }
    // Der Ordner-Zähler gehört auch auf die persistierten Ziele (wie
    // `publish_folder_status` es für einen normalen Plan tut).
    for (&i, &count) in &per_folder {
        if let Some(f) = store.folders.get_mut(i) {
            f.total_files = count;
        }
    }
    let _ = store.save();
    None
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
    let mut destination_roots = plan_folder_paths(&plan, store);
    for scan in &scans {
        plan.per_folder.entry(scan.idx).or_insert(0);
        if let Some(folder) = store.folders.get(scan.idx) {
            plan.folder_opts
                .entry(scan.idx)
                .or_insert((folder.dedup, folder.conflict_resolution));
        }
        destination_roots
            .entry(scan.idx)
            .or_insert_with(|| scan.path.clone());
    }
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
            destination_roots,
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

/// Load the persisted store, reporting a damaged one on the log facade and
/// yielding `None`.
///
/// `FolderStore::load` already sets a damaged file aside; callers must not use
/// the resulting empty default store as if it were valid. The technical detail
/// stays in the log, never in the UI.
pub(crate) fn load_store_logged() -> Option<FolderStore> {
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
    let mut target_only: Vec<usize> = Vec::new();
    let mut scans: Vec<FolderScan> = Vec::new();
    let mut store_changed = false;
    for i in indices {
        let Some(f) = store.folders.get_mut(i) else {
            continue;
        };
        // Missing source (user decisions 2026-10-16, 2026-10-17): a source
        // folder that is not stat-able is flagged `NotFound` ("Ordner nicht
        // gefunden"), not silently skipped, so the row says why it stopped —
        // and a folder whose drive came back is lifted to `Idle` again. The
        // watermark of a missing folder is never touched (no `scans` entry), so
        // the next cold start re-scans everything it missed.
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
        // No stat-able file at any depth (an empty — or unreadable — folder):
        // there is nothing to scan and no watermark to advance. The folder is
        // kept as a **target only**, so the run's preflight still resolves and,
        // if needed, creates its mirrored destination. Leaving it out here was
        // why an empty backup never got its cloud folder on the start pass.
        let Some(current) = folders::max_mtime(&f.path) else {
            target_only.push(i);
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
    // Persist a status change even when nothing is left to scan: the missing
    // source (`NotFound`) or its lifting must survive the pass.
    if store_changed {
        let _ = store.save();
    }
    if wanted.is_empty() && target_only.is_empty() {
        return;
    }
    // Every selected folder is a destination target: the scanned ones (also
    // when their scan yields no supported image) and the empty ones, which only
    // need their mirrored folder. No watermark is fabricated for those.
    let mut target_indices: Vec<usize> = scans.iter().map(|scan| scan.idx).collect();
    target_indices.extend(target_only);
    let mut plan = UploadPlan::build_since(&store, &wanted).unwrap_or_else(UploadPlan::empty);
    plan.add_targets(&store, &target_indices);
    let msg = plan.start_message();
    start_background_run(
        plan,
        &mut store,
        shared.clone(),
        cfg,
        paused,
        concurrency,
        limit_kbps,
        album,
        format!("Hintergrund-Sync: {msg}"),
        scans,
    );
}

/// Fold one watcher event into `paths`: only supported image **files** survive.
/// A directory is not a photo, and a path that no longer exists is a deletion —
/// both are ignored, exactly like the original's event type gate
/// (`docs/original/SPEC_CHANGE_DETECTION.md` §4.3). Returns `false` on a watcher error so
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
    let msg = plan.start_message();
    start_background_run(
        plan,
        store,
        shared.clone(),
        cfg.clone(),
        paused.clone(),
        concurrency,
        limit_kbps,
        album.to_string(),
        msg,
        // An event run never advances a folder watermark: the watermark is the
        // *start-time* safety net. The files it did upload sit in the MD5 index
        // and are skipped cheaply by the stat check, while a run that failed is
        // still caught by the next start.
        Vec::new(),
    );
}

/// Number of scans in flight — RAII so that no early return can leave the
/// counter (and with it a waiting run) stuck.
struct ScansInFlight(Arc<Mutex<UploadShared>>);

impl ScansInFlight {
    fn new(shared: &Arc<Mutex<UploadShared>>) -> Self {
        shared.lock().unwrap().scans_in_flight += 1;
        Self(shared.clone())
    }
}

impl Drop for ScansInFlight {
    fn drop(&mut self) {
        // Eine Sperre, nicht zwei im selben Ausdruck: sonst könnte der Guard des
        // einen Aufrufs den Zähler des anderen überschreiben.
        let mut s = self.0.lock().unwrap();
        s.scans_in_flight = s.scans_in_flight.saturating_sub(1);
    }
}

/// Read folders the user added (or re-triggered) while a run was busy: resolve
/// them against the **freshest** store, scan a single folder at a time and take
/// its files into the live run; when nothing is running any more, start an
/// ordinary run — or keep the folder parked until a run is allowed again.
///
/// The walk happens on the supervisor task, so it runs **parallel** to a
/// running upload (the live workers keep uploading while this walks).
fn handle_scan_requests(
    pending_scans: &mut Vec<PathBuf>,
    park_logged: &mut bool,
    store: &mut FolderStore,
    shared: &Arc<Mutex<UploadShared>>,
    cfg: &Config,
    paused: &watch::Sender<bool>,
    concurrency: usize,
    limit_kbps: u32,
    album: &str,
) {
    if pending_scans.is_empty() {
        return;
    }
    let wanted = std::mem::take(pending_scans);

    // Resolve against the **fresh** store (a folder may have been removed while
    // the request was parked) and flag a missing source the same way the manual
    // path does — a missing source is reported, never parked.
    let mut present: Vec<(usize, PathBuf)> = Vec::new();
    let mut store_changed = false;
    for p in wanted {
        let Some(i) = store.folders.iter().position(|f| f.path == p) else {
            // Removed meanwhile: nothing to scan.
            continue;
        };
        let Some(f) = store.folders.get_mut(i) else {
            continue;
        };
        let exists = p.is_dir();
        let next = source_status(exists, f.status);
        if next != f.status {
            f.status = next;
            store_changed = true;
        }
        if !exists {
            // A missing source is not parked: the next attempt would fail again
            // — it is flagged and reported like the manual path.
            let _ = store.save();
            shared.lock().unwrap().log(tr(
                "Der Quellordner ist gerade nicht erreichbar (z. B. externes Laufwerk nicht angeschlossen).",
            ));
            continue;
        }
        present.push((i, p));
    }
    if store_changed {
        let _ = store.save();
    }
    if present.is_empty() {
        return;
    }

    // A run is in flight: hold it open (the RAII guard raises `scans_in_flight`
    // for the whole scan+inject) and take every folder's plan into it. The walk
    // runs here, on the supervisor task — the live workers keep uploading.
    if shared.lock().unwrap().active {
        let _guard = ScansInFlight::new(shared);
        for (i, p) in present {
            match UploadPlan::build_since(store, &[(i, None)]) {
                Some(plan) => {
                    // Refused only when the run stopped accepting work (cancel):
                    // park the folder and retry on a later pass.
                    if inject_into_live_run(plan, store, shared).is_some() {
                        if !pending_scans.contains(&p) {
                            pending_scans.push(p);
                        }
                    }
                }
                None => {
                    let plan = UploadPlan::empty_for_targets(store, &[i]);
                    if inject_into_live_run(plan, store, shared).is_some() {
                        if !pending_scans.contains(&p) {
                            pending_scans.push(p);
                        }
                    }
                }
            }
        }
        return;
    }

    // Nothing is running: park everything until a run is allowed again, else
    // start one ordinary run for all the folders at once.
    if !can_start(cfg, paused, shared) {
        for (_, p) in present {
            if !pending_scans.contains(&p) {
                pending_scans.push(p);
            }
        }
        // Log once per parking episode (Verify E): the 30 s tick and every FS
        // event run this branch again while the folder stays parked, so a repeat
        // would spam the run log.
        if !*park_logged {
            shared
                .lock()
                .unwrap()
                .log(tr("Der Ordner wird beim nächsten Lauf gesichert."));
            *park_logged = true;
        }
        return;
    }
    let indices: Vec<usize> = present.iter().map(|(i, _)| *i).collect();
    let Some(plan) = UploadPlan::build(store, &indices)
        .or_else(|| Some(UploadPlan::empty_for_targets(store, &indices)))
    else {
        return;
    };
    let msg = plan.start_message();
    start_background_run(
        plan,
        store,
        shared.clone(),
        cfg.clone(),
        paused.clone(),
        concurrency,
        limit_kbps,
        album.to_string(),
        msg,
        // Like the event path: a run triggered by a user request advances no
        // folder watermark (the watermark is the *start-time* safety net).
        Vec::new(),
    );
}

/// One watcher per enabled backup folder, re-synced with the persisted store on
/// every pass: a folder that was added, enabled, disabled or removed (and the
/// `background_sync` switch) is reflected within `WATCH_POLL`. A folder we
/// cannot watch (gone, ejected) is logged and simply absent until the next pass.
///
/// Returns the paths that just **came back** from `NotFound` (Welle 5.33), so
/// the caller can park them for a one-time full-stock scan.
fn sync_watchers(
    watchers: &mut HashMap<PathBuf, notify::RecommendedWatcher>,
    tx: &tokio::sync::mpsc::UnboundedSender<notify::Result<notify::Event>>,
    cfg: &Config,
) -> Vec<PathBuf> {
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
    // Missing source on the watcher path (user decisions 2026-10-16,
    // 2026-10-17): every pass re-decides each enabled folder's source state with
    // the same pure rule the cold-start pass and the row use — a folder whose
    // drive is gone becomes `NotFound`, one whose drive came back is lifted to
    // `Idle` again. This is also why a folder we cannot watch is already
    // `NotFound` before the watcher attempt below. Nothing else is touched; in
    // particular the global user pause is a separate switch.
    //
    // The paths that just came back are handed to the caller: `watch_loop` parks
    // them so the returned folder's stock is read in **once** (Welle 5.33).
    let (returned, store_changed) = match store.as_mut() {
        Some(s) => lift_returned_sources(s, |p| p.is_dir()),
        None => (Vec::new(), false),
    };
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
    returned
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
    mut scan_rx: tokio::sync::mpsc::UnboundedReceiver<PathBuf>,
) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<notify::Result<notify::Event>>();
    let mut watchers: HashMap<PathBuf, notify::RecommendedWatcher> = HashMap::new();
    let mut pending: Vec<(usize, PathBuf)> = Vec::new();
    // `pending_seen` holds exactly the paths in `pending`; together they make the
    // merge O(1) per path instead of a linear scan of the list.
    let mut pending_seen: HashSet<PathBuf> = HashSet::new();
    // Folders the user added (or re-triggered) while a run was busy: parked here
    // and read in at the top of the loop (parallel to a running upload).
    let mut pending_scans: Vec<PathBuf> = Vec::new();
    // The "next run" notice is logged once per parking episode (Verify E): reset
    // whenever the parked list drains (a run took the folders over).
    let mut scan_park_logged = false;
    loop {
        // Freshest settings + store on every pass (the supervisor is detached
        // from the UI struct, so the persisted files are its source — except the
        // session material, which is overlaid from the shared `Session` because
        // the cookie fields are `#[serde(skip)]`).
        let mut cfg = Config::load().unwrap_or(last_config.clone());
        overlay_session_cookies(&mut cfg, &session);
        // A source folder that just came back (`NotFound` → present) is read in
        // **once**, with its full stock: park it here — before the scan block
        // below — so this same pass handles it (parallel to a live run, or as a
        // normal run). The global pause is respected downstream: when it is set,
        // `handle_scan_requests` keeps the folder parked and it starts as soon as
        // the pause is lifted. The edge is one-shot (`NotFound` is lifted to
        // `Idle`), so no pass parks it twice.
        for path in sync_watchers(&mut watchers, &tx, &cfg) {
            if !pending_scans.contains(&path) {
                pending_scans.push(path);
            }
        }

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

        // A folder the user added while a run was busy: read it in here — on the
        // supervisor task, so the walk runs **parallel** to a live upload and its
        // files go into the running run (or a normal run starts, or it stays
        // parked).
        if !pending_scans.is_empty() {
            let Some(mut store) = load_store_logged() else {
                continue;
            };
            handle_scan_requests(
                &mut pending_scans,
                &mut scan_park_logged,
                &mut store,
                &shared,
                &cfg,
                &paused,
                concurrency,
                limit_kbps,
                &album,
            );
            if pending_scans.is_empty() {
                scan_park_logged = false;
            }
        }

        // Wait for the next event, the poll tick (which keeps the watcher set and
        // the pending list moving while nothing changes) — or a folder the user
        // just added while a run was busy.
        let first = tokio::select! {
            ev = rx.recv() => match ev {
                Some(ev) => ev,
                // `None`: every sender is gone, so every watcher is, too.
                None => break,
            },
            // Park it; the top of the loop reads it in (parallel to the running
            // upload).
            Some(path) = scan_rx.recv() => {
                if !pending_scans.contains(&path) {
                    pending_scans.push(path);
                }
                continue;
            }
            _ = tokio::time::sleep(WATCH_POLL) => continue,
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

/// The persisted folder a remembered answer would be written into — `None`
/// exactly when [`remember_conflict_in_folder`] refuses to write:
/// - the transient drag & drop target (`ONE_TIME_IDX`), which has no
///   `folders.toml` entry at all and stays run-only,
/// - an index outside the store (`FolderStore::remove` shifts indices), and
/// - a file that does not live below that folder's own path — the decisive
///   guard, because the shifted index is exactly what could point at a
///   neighbouring folder otherwise.
///
/// Both the checkbox label ([`conflict_remember_label`]) and the write
/// ([`remember_conflict_in_folder`]) go through this one query, so the label
/// can never name a folder that would not actually receive the answer.
fn remember_folder<'a>(
    store: &'a FolderStore,
    idx: usize,
    file_path: &Path,
) -> Option<&'a folders::BackupFolder> {
    if idx == ONE_TIME_IDX {
        return None;
    }
    store
        .folders
        .get(idx)
        .filter(|f| file_path.starts_with(&f.path))
}

/// Persist a remembered conflict answer into the folder the asked file belongs
/// to — the single place that writes it (user decision 2026-10-15).
///
/// "Nicht noch einmal fragen" keeps working without a prompt in **that folder**
/// on later runs, so the answer lands in the folder's own `conflict_resolution`
/// (`folders.toml`). No schema change: the field already exists, and a folder
/// that keeps its value simply keeps whatever policy it had.
///
/// Refuses — and writes nothing — whenever [`remember_folder`] answers `None`
/// (transient target, index outside the store, file outside the folder path).
///
/// Returns whether it wrote.
fn remember_conflict_in_folder(
    store: &mut FolderStore,
    idx: usize,
    file_path: &Path,
    answer: ConflictResolution,
) -> bool {
    if remember_folder(store, idx, file_path).is_none() {
        return false;
    }
    // The guard above proved the index is in range; `map_or` keeps the write
    // total without a panic path.
    store.folders.get_mut(idx).map_or(false, |folder| {
        folder.conflict_resolution = answer;
        true
    })
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

/// The label for a **pending** conflict: names the folder the answer would be
/// persisted into — exactly the folder [`remember_conflict_in_folder`] writes
/// to, decided by the shared guard [`remember_folder`] — and stays neutral
/// whenever there is none (transient drag & drop target, index outside the
/// store, or a file that does not live below the folder's path). Both
/// decisions are the same query, so the checkbox never names a folder that
/// would not receive the answer — the shifted-index case included.
fn conflict_remember_label(store: &FolderStore, idx: usize, file_path: &Path) -> String {
    remember_folder(store, idx, file_path)
        .map(|f| remember_checkbox_label(Some(&f.name)))
        .unwrap_or_else(|| remember_checkbox_label(None))
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
        // Missing source (user decisions 2026-10-16, 2026-10-17): a target whose
        // source folder is gone is flagged `NotFound`, not scanned — and it is
        // kept **out** of the plan so a run can never settle it as `Done`. A
        // folder whose drive came back is lifted to `Idle` here as well. The
        // missing source gets a plain-language message instead of "no photos
        // found".
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
        let mut destination_roots: HashMap<usize, PathBuf> = present
            .iter()
            .filter_map(|&i| {
                self.store
                    .folders
                    .get(i)
                    .map(|folder| (i, folder.path.clone()))
            })
            .collect();
        let Some(mut plan) = UploadPlan::build(&self.store, &present).or_else(|| {
            if present.is_empty() {
                return None;
            }
            let mut empty = UploadPlan::empty();
            for &i in &present {
                if let Some(folder) = self.store.folders.get(i) {
                    empty.per_folder.insert(i, 0);
                    empty.folder_opts.insert(
                        i,
                        (folder.dedup, folder.conflict_resolution),
                    );
                }
            }
            Some(empty)
        }) else {
            if missing {
                self.log(tr(
                    "Der Quellordner ist gerade nicht erreichbar (z. B. externes Laufwerk nicht angeschlossen).",
                ));
            } else {
                self.log(tr("Keine Fotos oder Videos in diesem Ordner gefunden."));
            }
            return;
        };
        // Include empty selected backups in the run's destination preflight,
        // even when another selected folder has files.
        for &i in &present {
            if let Some(folder) = self.store.folders.get(i) {
                plan.per_folder.entry(i).or_insert(0);
                plan.folder_opts
                    .entry(i)
                    .or_insert((folder.dedup, folder.conflict_resolution));
                destination_roots
                    .entry(i)
                    .or_insert_with(|| folder.path.clone());
            }
        }
        let concurrency = if self.slow_mode { 1 } else { self.config.concurrency() };
        let limit = if self.config.limit_enabled {
            self.config.limit_kbps
        } else {
            0
        };
        let msg = plan.start_message();
        publish_plan(
            &mut plan,
            &mut self.store,
            &self.upload,
            msg,
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
                destination_roots,
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
            HashMap::new(),
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

    /// Modal prompt for a name conflict (`ConflictResolution::Ask`,
    /// docs/original/SPEC_DEDUP.md §4.1). Rendered from `update` independently of the
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
        // Name the folder the answer is persisted into — through
        // `conflict_remember_label`, i.e. the exact guard
        // `remember_conflict_in_folder` writes with: the transient drag & drop
        // queue (`ONE_TIME_IDX`) has no `folders.toml` entry, an index the
        // store no longer holds must not borrow a neighbouring folder's name,
        // and a file outside the folder path means nothing would be written
        // there either. All three fall back to the neutral text and stay
        // run-only, exactly like the write refuses them.
        let remember_label = match (pending_idx, pending_path.as_deref()) {
            (Some(idx), Some(path)) => conflict_remember_label(&self.store, idx, path),
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
                    let plan_idx = plan_index_for(&s.folder_paths, &f.path);
                    let in_run = s.active
                        && plan_idx.is_some_and(|plan_idx| s.folder_total.contains_key(&plan_idx));
                    let total = row_total(
                        in_run,
                        plan_idx.and_then(|plan_idx| s.folder_total.get(&plan_idx).copied()),
                        f.total_files,
                    );
                    let done = row_done(
                        in_run,
                        plan_idx.and_then(|plan_idx| s.folder_done.get(&plan_idx).copied()),
                        f.uploaded_files,
                    );
                    // Cap the completed count (Welle 5.11): a legacy folder whose
                    // `total_files < uploaded_files` must not read above 100 % in
                    // the row's progress text or the idle aggregate.
                    let done = capped_done(done, total);
                    // Live status (Welle 3.3): while the run counts this target in
                    // `folder_total` it is *uploading*, regardless of the (stale)
                    // persisted status — the persisted outcome only applies once
                    // the run settled and `active` is false again. A background
                    // run writes its result into a fresh `FolderStore::load()`
                    // (`finalize_background_store`), never into `self.store`, so
                    // without this the row kept showing the previous outcome.
                    let status = if in_run {
                        FolderStatus::Uploading
                    } else if f.enabled {
                        // The missing-source state is a *live* condition (user
                        // decisions 2026-10-16, 2026-10-17): the detached
                        // supervisor writes a missing source to `folders.toml`,
                        // which the UI's in-memory store never reloads — so the
                        // row applies the very same rule (`source_status`) itself.
                        // Same helper, no second logic; a returned drive flips
                        // the row back on its own.
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
                        new: plan_idx
                            .and_then(|plan_idx| s.folder_new.get(&plan_idx).copied())
                            .unwrap_or(0),
                        existing: plan_idx
                            .and_then(|plan_idx| s.folder_conflict.get(&plan_idx).copied())
                            .unwrap_or(0),
                        unchanged: plan_idx
                            .and_then(|plan_idx| s.folder_unchanged.get(&plan_idx).copied())
                            .unwrap_or(0),
                        failed: plan_idx
                            .and_then(|plan_idx| s.folder_failed.get(&plan_idx).copied())
                            .unwrap_or(0),
                        last_updated: f.last_updated.clone(),
                    }
                })
                .collect();
            // The transient one-time target is a single extra row after the
            // persisted folders — same chrome, but it has no row-action icons
            // (kebab/refresh) and no pause toggle (nothing to configure,
            // nothing to stop).
            if let Some(one) = self.one_time.as_ref() {
                let in_run = s.active && s.folder_total.contains_key(&ONE_TIME_IDX);
                let total = row_total(
                    in_run,
                    s.folder_total.get(&ONE_TIME_IDX).copied(),
                    one.total_files,
                );
                let done = row_done(
                    in_run,
                    s.folder_done.get(&ONE_TIME_IDX).copied(),
                    one.uploaded_files,
                );
                // Same cap as the persisted rows (Welle 5.11).
                let done = capped_done(done, total);
                // Same live-status rule as the persisted folders (Welle 3.3).
                let status = if in_run {
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

        // Live per-file queue as one flat, virtualised table (Welle 3.1,
        // docs/original/SPEC_UPLOAD_PROGRESS.md §7.3/§7.8): permanent main-area
        // content with fixed columns. The rows are sorted globally by file
        // status — no folder grouping and no group headers (user decision
        // 2026-10-26, see [`status_rank`]). The layout only carries queue
        // indices, so no file name is cloned per frame. The Ordner column keeps
        // the run's captured plan paths; same-named folders are disambiguated
        // with "Name (parent)" (user decision 2026-10-16).
        let (folder_paths, remaining, layout) = {
            let s = self.upload.lock().unwrap();
            let remaining = s
                .queue
                .iter()
                .filter(|e| matches!(e.state, FileState::Waiting | FileState::Uploading))
                .count();
            (s.folder_paths.clone(), remaining, queue_layout(&s.queue))
        };
        let ambiguous = ambiguous_folder_names_for_paths(&folder_paths);

        ui.add_space(12.0);
        ui.label(RichText::new(tr1("Warteschlange ({})", remaining)).strong().size(14.0));

        // Shared geometry: header and rows derive the name width the same way so
        // the four columns line up. The scroll bar's width is reserved with the
        // floating style too: `theme.rs` sets `floating_allocated_width =
        // bar_width + 8`, so `allocated_width()` yields 18, not 0.
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
        // folder list takes what its `rows.len()` cards need, capped so the queue
        // keeps its floor; the queue takes the rest, never more than half of the
        // free height (user decision 2026-10-16, Welle 5.17). `list_h` is used
        // again at the folder list's own draw time; the function is pure, so
        // both sites agree.
        let free = ui.available_height();
        let (table_h, list_h) = page_heights(free, rows.len());

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
                    for &queue_idx in &layout[range] {
                        let Some(e) = s.queue.get(queue_idx) else {
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
                            FileState::Done => (tr("Abgeschlossen").to_string(), theme::GREEN),
                            FileState::Failed => (tr("Fehlgeschlagen").to_string(), theme::RED),
                            FileState::Skipped => (tr("Übersprungen").to_string(), theme::MUTED),
                        };
                        // A long name is truncated in the cell; the full
                        // path shows on hover (§7.3).
                        let full = e.path.display().to_string();
                        // The row's backup folder (disambiguated when the
                        // active run holds the name twice).
                        let (folder_label, folder_tip) = if e.folder == ONE_TIME_IDX {
                            (tr("Einmalige Sicherung").to_string(), String::new())
                        } else {
                            queue_folder_display(&folder_paths, e.folder, &ambiguous)
                                .unwrap_or_default()
                        };
                        // Allocated at exactly `QUEUE_ROW_H` so the row
                        // advances by `QUEUE_ROW_H + item_spacing.y`,
                        // matching what `show_rows` derives.
                        let row_w = ui.available_width();
                        ui.allocate_ui_with_layout(
                            egui::vec2(row_w, QUEUE_ROW_H),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add_sized(
                                    [QUEUE_FOLDER_W, QUEUE_ROW_H],
                                    egui::Label::new(RichText::new(folder_label).size(12.0))
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
                                        RichText::new(crate::text::format_size(e.size)).size(12.0),
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
                });
        }

        // Aggregate progress: **one** display under the queue. While a run is
        // live it shows only the run counters (including when its last target
        // row was removed); while idle it sums all target rows. The byte/ETA
        // detail only exists for a live run; outcome counters remain aggregated
        // over every target (kept for the footer line below).
        let (running, progress_text, outcome, run_done, run_total) = {
            let s = self.upload.lock().unwrap();
            let outcome = outcome_text(
                s.uploaded_new,
                s.conflict_skipped,
                s.unchanged_skipped,
                s.failed,
            );
            if s.active {
                (
                    true,
                    upload_progress_text(&s),
                    outcome,
                    s.done,
                    s.total,
                )
            } else {
                (false, String::new(), outcome, 0, 0)
            }
        };
        let (sum_done, sum_total) = displayed_progress(running, run_done, run_total, &rows);
        if running && sum_total == 0 {
            // An all-current scan is active only while its no-op run is being
            // finalized. Show the same completion wording as the idle state,
            // without a meaningless 0/0 bar or spinner.
            ui.add_space(6.0);
            ui.label(progress_text);
        } else if running || sum_total > 0 {
            ui.add_space(6.0);
            let frac = if sum_total > 0 {
                (sum_done as f32 / sum_total as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            // The row width must be remembered **before** the `horizontal`:
            // while a run is live the spinner in front of the bar already
            // consumes part of it, and a `ProgressBar` without `desired_width`
            // takes the whole row regardless — so it visibly overflowed the
            // right edge (~105 %). The bar now gets an explicit width: the full
            // row without a spinner, the row minus the spinner and the item
            // spacing with one (clamped at 0).
            let row_w = ui.available_width();
            ui.horizontal(|ui| {
                // A spinner belongs to this one display and only while a run is
                // active — never a second, separate indicator.
                if running {
                    ui.spinner();
                }
                let text = if running {
                    // The run's file counts plus its byte/ETA detail from
                    // `upload_progress_text` (kept verbatim).
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
                // The spinner is a square of `interact_size.y` (26 px, theme);
                // the layout adds `item_spacing.x` between the two widgets.
                let bar_w = if running {
                    (row_w - ui.spacing().item_spacing.x - ui.style().spacing.interact_size.y)
                        .max(0.0)
                } else {
                    row_w
                };
                ui.add(egui::ProgressBar::new(frac).desired_width(bar_w).text(text));
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

        // 8 px air between the heading above and the card list below (UI review
        // 0.1.1): without it the list touched the "Automatisch gesicherte
        // Ordner:" heading, and a scrolled half-visible card stuck to it. The
        // gap is part of the chrome `QUEUE_BELOW_H` reserves.
        ui.add_space(8.0);

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
    use std::collections::VecDeque;

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
    fn row_index_for_resolves_paths_and_skips_removed_and_one_time_targets() {
        let store = FolderStore {
            folders: vec![
                folders::BackupFolder::new(PathBuf::from("/photos/a")),
                folders::BackupFolder::new(PathBuf::from("/photos/b")),
            ],
        };
        let paths = HashMap::from([
            (7, PathBuf::from("/photos/b")),
            (3, PathBuf::from("/photos/a")),
            (9, PathBuf::from("/photos/removed")),
            (ONE_TIME_IDX, PathBuf::from("/photos/a")),
        ]);

        assert_eq!(row_index_for(&store, &paths, 7), Some(1));
        assert_eq!(row_index_for(&store, &paths, 3), Some(0));
        assert_eq!(row_index_for(&store, &paths, 9), None);
        assert_eq!(row_index_for(&store, &paths, ONE_TIME_IDX), None);
    }

    #[test]
    fn plan_index_for_resolves_path_and_returns_none_for_unknown_or_empty() {
        let paths = HashMap::from([
            (7, PathBuf::from("/photos/b")),
            (3, PathBuf::from("/photos/a")),
            (ONE_TIME_IDX, PathBuf::from("/photos/one-time")),
        ]);

        assert_eq!(plan_index_for(&paths, Path::new("/photos/b")), Some(7));
        assert_eq!(plan_index_for(&paths, Path::new("/photos/a")), Some(3));
        assert_eq!(plan_index_for(&paths, Path::new("/photos/missing")), None);
        assert_eq!(plan_index_for(&HashMap::new(), Path::new("/photos/a")), None);
        assert_eq!(plan_index_for(&paths, Path::new("/photos/one-time")), None);
    }

    #[test]
    fn plan_index_for_duplicate_paths_chooses_the_lowest_index_deterministically() {
        let path = Path::new("/photos/a");
        let paths = HashMap::from([
            (9, path.to_path_buf()),
            (2, path.to_path_buf()),
            (ONE_TIME_IDX, path.to_path_buf()),
        ]);

        for _ in 0..5 {
            assert_eq!(plan_index_for(&paths, path), Some(2));
        }
    }

    #[test]
    fn live_row_after_store_removal_uses_its_plan_counters() {
        // A was plan index 0 and has since been removed; B is now the first
        // store row but remains plan index 1 for this active run.
        let paths = HashMap::from([
            (0, PathBuf::from("/photos/removed")),
            (1, PathBuf::from("/photos/b")),
        ]);
        let folder_total = HashMap::from([(0, 100), (1, 20)]);
        let folder_done = HashMap::from([(0, 90), (1, 7)]);
        let store_path = Path::new("/photos/b");

        let plan_idx = plan_index_for(&paths, store_path);
        let in_run = plan_idx.is_some_and(|idx| folder_total.contains_key(&idx));
        let total = row_total(
            in_run,
            plan_idx.and_then(|idx| folder_total.get(&idx).copied()),
            999,
        );
        let done = row_done(
            in_run,
            plan_idx.and_then(|idx| folder_done.get(&idx).copied()),
            888,
        );

        assert_eq!(plan_idx, Some(1));
        assert_eq!((done, total), (7, 20));
    }

    #[test]
    fn finalization_uses_plan_path_after_store_rows_shift() {
        let planned_store = FolderStore {
            folders: vec![
                folders::BackupFolder::new(PathBuf::from("/photos/a")),
                folders::BackupFolder::new(PathBuf::from("/photos/b")),
            ],
        };
        let mut plan = UploadPlan::empty();
        plan.per_folder.insert(0, 3);
        let paths = plan_folder_paths(&plan, &planned_store);

        let mut store = FolderStore {
            folders: vec![
                folders::BackupFolder {
                    total_files: 10,
                    uploaded_files: 8,
                    ..folders::BackupFolder::new(PathBuf::from("/photos/b"))
                },
                folders::BackupFolder {
                    total_files: 20,
                    uploaded_files: 17,
                    ..folders::BackupFolder::new(PathBuf::from("/photos/a"))
                },
            ],
        };
        // The plan saw A at index 0; after a store edit, B now occupies 0 and
        // A has moved to row 1. Finalization must follow A's captured path.
        let folder_total = plan.per_folder;
        let folder_done = HashMap::from([(0, 2)]);
        let folder_failed = HashMap::new();

        apply_folder_progress(
            &mut store,
            &paths,
            &folder_total,
            &folder_done,
            &folder_failed,
            "now",
        );

        assert_eq!(store.folders[0].total_files, 10);
        assert_eq!(store.folders[0].uploaded_files, 8);
        assert_eq!(store.folders[0].status, FolderStatus::Idle);
        assert_eq!(store.folders[1].total_files, 3);
        assert_eq!(store.folders[1].uploaded_files, 2);
        assert_eq!(store.folders[1].last_updated, "now");
        assert_eq!(store.folders[1].status, FolderStatus::Done);
    }

    #[test]
    fn aggregate_is_zero_without_rows() {
        assert_eq!(aggregate_progress(&[]), (0, 0));
    }

    #[test]
    fn aggregate_sums_folders_and_the_one_time_row() {
        // Two persisted folders plus the transient one-time row
        // (`ONE_TIME_IDX`): the idle aggregate sums over all three.
        let rows = vec![row(0, 100, 40), row(1, 20, 20), row(ONE_TIME_IDX, 5, 1)];
        assert_eq!(aggregate_progress(&rows), (61, 125));
    }

    #[test]
    fn active_aggregate_uses_run_counts_and_idle_aggregate_sums_rows() {
        let rows = vec![row(0, 11_510, 11_510), row(1, 80, 40)];
        assert_eq!(displayed_progress(true, 3, 50, &rows), (3, 50));
        assert_eq!(displayed_progress(false, 0, 0, &rows), (11_550, 11_590));
    }

    #[test]
    fn live_row_with_no_done_entry_starts_at_zero_not_persisted_count() {
        let total = 50;
        let done = capped_done(row_done(true, None, 11_510), total);
        let frac = (done as f32 / total as f32).clamp(0.0, 1.0);

        assert_eq!(done, 0);
        assert_eq!(total, 50);
        assert_eq!(frac, 0.0);
        assert_eq!(row_done(true, Some(7), 11_510), 7);
        assert_eq!(row_done(false, None, 11_510), 11_510);
    }

    #[test]
    fn idle_row_done_ignores_a_stale_run_entry() {
        let done = row_done(false, Some(50), 3);

        assert_eq!(done, 3);
        assert_eq!(capped_done(done, 12), 3);
    }

    #[test]
    fn idle_finalized_row_still_shows_complete() {
        let total = 12;
        let done = capped_done(row_done(false, Some(2), total), total);

        assert_eq!(folder_progress_text(FolderStatus::Done, done, total), tr("Vollständig"));
    }

    #[test]
    fn idle_row_total_ignores_a_stale_run_entry() {
        // The shared run map can retain an entry for a folder index that now
        // belongs to another row. Idle totals must come from the store instead.
        assert_eq!(row_total(false, Some(50), 12), 12);
        assert_eq!(row_total(true, Some(50), 12), 50);
        assert_eq!(row_total(true, None, 12), 0);
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

    /// The list's content height: `cards` measured cards plus the gaps between
    /// them — the same formula `page_heights` uses, stated once here so the
    /// tests can assert against the content, not against a bare number.
    fn list_content_h(cards: usize) -> f32 {
        cards as f32 * FOLDER_CARD_H + cards.saturating_sub(1) as f32 * FOLDER_CARD_GAP
    }

    #[test]
    fn page_heights_three_cards_fit_when_tight() {
        // A tight window (400 px free): the list claims exactly its content —
        // three cards (3×60) plus two gaps (2×8) = 196 px — and the cap
        // (400 − 116 chrome − 80 queue floor = 204) leaves room for all of it.
        // Before Welle 5.17 the list was capped at 45 % of `free` (180 px) and
        // the third card was visibly cut off.
        let (queue_h, list_h) = page_heights(400.0, 3);
        assert_eq!(list_h, list_content_h(3));
        // The queue still gets more than its floor.
        assert!(queue_h >= QUEUE_MIN_FRAC * 400.0);
    }

    #[test]
    fn page_heights_ten_cards_keep_the_queue_floor() {
        // Ten cards (672 px of content) in a 500 px window: the list is capped
        // at exactly the point where the queue keeps its floor
        // (500 − 116 − 100 = 284) — never below `QUEUE_MIN_FRAC * free`.
        // `QUEUE_BELOW_H` grew by the 8 px heading→list air (UI review 0.1.1),
        // so the chrome term is 116 px, not the earlier 108 px.
        let (queue_h, list_h) = page_heights(500.0, 10);
        assert_eq!(
            list_h,
            500.0 - (QUEUE_HEADER_H + QUEUE_BELOW_H) - QUEUE_MIN_FRAC * 500.0
        );
        assert!(queue_h >= QUEUE_MIN_FRAC * 500.0);
        // The queue also never exceeds its ceiling.
        assert!(queue_h <= QUEUE_MAX_FRAC * 500.0);
    }

    #[test]
    fn page_heights_zero_cards_give_the_room_to_the_queue() {
        // No cards: the list collapses to zero and the queue takes the rest —
        // here capped at half the free height, well above its floor.
        let (queue_h, list_h) = page_heights(500.0, 0);
        assert_eq!(list_h, 0.0);
        assert_eq!(queue_h, QUEUE_MAX_FRAC * 500.0);
    }

    #[test]
    fn page_heights_tiny_window_never_overflows() {
        // Below the chrome (116 px) both areas collapse to zero — nothing may
        // run off the bottom edge.
        let (queue_h, list_h) = page_heights(50.0, 5);
        assert_eq!(queue_h, 0.0);
        assert_eq!(list_h, 0.0);
    }

    #[test]
    fn queue_label_disambiguates_with_the_parent_name_not_the_path() {
        // Two folders named "Sport" under different parents: the queue label
        // carries the parent's **name** so it fits the 120 px column — the
        // full path stays in the row's tooltip (user decision 2026-10-16).
        let ambiguous: HashSet<String> = ["Sport".to_string()].into_iter().collect();
        let label = queue_folder_label(
            "Sport",
            Path::new("/Users/florianreisinger/Pictures/Sport"),
            &ambiguous,
        );
        assert_eq!(label, "Sport (Pictures)");

        // Unambiguous names stay bare.
        let label = queue_folder_label(
            "Sport",
            Path::new("/Users/florianreisinger/Pictures/Sport"),
            &HashSet::new(),
        );
        assert_eq!(label, "Sport");

        // A root-level folder has no parent name: the plain name stays.
        let label = queue_folder_label("Sport", Path::new("/Sport"), &ambiguous);
        assert_eq!(label, "Sport");
    }

    #[test]
    fn status_rank_matches_the_decided_queue_order() {
        // User decision 2026-10-26: In Bearbeitung → Fehlgeschlagen → Warten →
        // Abgeschlossen → Übersprungen.
        let order = [
            FileState::Uploading,
            FileState::Failed,
            FileState::Waiting,
            FileState::Done,
            FileState::Skipped,
        ];
        for (rank, state) in order.into_iter().enumerate() {
            assert_eq!(status_rank(state), rank as u8);
        }
    }

    /// One queue entry with the fields the layout reads filled in.
    fn queue_entry(folder: usize, name: &str, state: FileState) -> QueueEntry {
        QueueEntry {
            folder,
            name: name.to_string(),
            state,
            size: 0,
            path: PathBuf::new(),
            bytes_sent: 0,
        }
    }

    #[test]
    fn queue_layout_is_global_and_stable_within_a_status_bucket() {
        // Two folders, deliberately interleaved: the layout must ignore folder
        // membership and bucket globally by status, keeping insertion order
        // inside a bucket (user decision 2026-10-26).
        let queue = vec![
            queue_entry(0, "a-waiting", FileState::Waiting),
            queue_entry(1, "b-done", FileState::Done),
            queue_entry(0, "a-uploading", FileState::Uploading),
            queue_entry(1, "b-waiting", FileState::Waiting),
            queue_entry(0, "a-failed", FileState::Failed),
            queue_entry(1, "b-skipped", FileState::Skipped),
            queue_entry(0, "a-done", FileState::Done),
            queue_entry(1, "b-failed", FileState::Failed),
        ];

        let layout = queue_layout(&queue);
        let names: Vec<&str> = layout.iter().map(|&i| queue[i].name.as_str()).collect();

        assert_eq!(
            names,
            [
                "a-uploading",
                "a-failed",
                "b-failed",
                "a-waiting",
                "b-waiting",
                "b-done",
                "a-done",
                "b-skipped",
            ]
        );
    }

    #[test]
    fn queue_layout_of_an_empty_queue_is_empty() {
        assert!(queue_layout(&[]).is_empty());
    }

    #[test]
    fn queue_label_resolves_plan_path_after_store_rows_shift() {
        let store = FolderStore {
            folders: vec![folders::BackupFolder::new(PathBuf::from("/photos/b"))],
        };
        // Plan index 0 was A. The current store's row 0 is now B, but the queue
        // must keep naming the captured plan path rather than that shifted row.
        let paths = HashMap::from([(0, PathBuf::from("/photos/a"))]);
        let ambiguous = ambiguous_folder_names_for_paths(&paths);

        let (label, tooltip) = queue_folder_display(&paths, 0, &ambiguous)
            .expect("the plan index has a captured folder path");

        assert_eq!(store.folders[0].path, PathBuf::from("/photos/b"));
        assert_eq!(label, "a");
        assert_eq!(tooltip, "/photos/a");
    }

    #[test]
    fn publish_state_publishes_folder_paths_for_the_queue() {
        let store = store_with("/src/published-folder", FolderStatus::Idle);
        let mut plan = literal_plan(&[(0, "/src/published-folder/photo.jpg")]);
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let folder_paths = plan_folder_paths(&plan, &store);

        publish_state(&mut plan, &shared, "test".to_string(), folder_paths);

        let display = {
            let state = shared.lock().unwrap();
            let ambiguous = ambiguous_folder_names_for_paths(&state.folder_paths);
            queue_folder_display(&state.folder_paths, 0, &ambiguous)
        };
        assert_eq!(
            display,
            Some((
                "published-folder".to_string(),
                "/src/published-folder".to_string(),
            )),
            "the published state already carries the plan's queue label"
        );
    }

    #[test]
    fn source_status_flags_missing_and_lifts_when_back() {
        use FolderStatus::{Done, Error, Idle, NotFound, Paused, Scanning, Uploading};
        // A missing source flags the target as not found, from any state.
        for current in [Idle, Scanning, Uploading, Paused, Done, Error, NotFound] {
            assert_eq!(source_status(false, current), NotFound);
        }
        // A source that is back lifts the state, both for `NotFound` and for a
        // `Paused` written by an older `folders.toml` ...
        assert_eq!(source_status(true, NotFound), Idle);
        assert_eq!(source_status(true, Paused), Idle);
        // ... and leaves every other state alone (no run state is rewritten).
        for current in [Idle, Scanning, Uploading, Done, Error] {
            assert_eq!(source_status(true, current), current);
        }
    }

    #[test]
    fn returned_from_missing_only_fires_for_a_back_folder_that_was_not_found() {
        use FolderStatus::{Done, Error, Idle, NotFound, Paused, Scanning, Uploading};
        // (a) The one edge that triggers: was `NotFound`, the path is back.
        assert!(returned_from_missing(NotFound, true));
        // (b) `NotFound` with the path still gone: nothing to read in.
        assert!(!returned_from_missing(NotFound, false));
        // (c)/(d) Every other state with the path present is **not** a return:
        // `Idle` must not re-scan every pass, and `Paused` is not a return by
        // design (user decision 2026-10-17).
        for current in [Idle, Scanning, Uploading, Paused, Done, Error] {
            assert!(!returned_from_missing(current, true), "{current:?}");
            assert!(!returned_from_missing(current, false), "{current:?}");
        }
    }

    /// A store with one enabled folder at `path` in `status`.
    fn store_with(path: &str, status: FolderStatus) -> FolderStore {
        let mut store = FolderStore::default();
        let mut folder = folders::BackupFolder::new(PathBuf::from(path));
        folder.status = status;
        store.folders.push(folder);
        store
    }

    fn scratch_scan_folder(name: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "prime-photos-scan-{}-{name}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn manual_plan_queues_only_files_without_a_matching_local_stat() {
        let root = scratch_scan_folder("dedup-filter");
        std::fs::create_dir_all(&root).unwrap();
        let unchanged_paths = [root.join("unchanged-a.jpg"), root.join("unchanged-b.jpg")];
        let new_path = root.join("new.jpg");
        for path in unchanged_paths.iter().chain([&new_path]) {
            std::fs::write(path, b"photo bytes").unwrap();
        }

        // This lookup represents the two rows in the local stat index while
        // keeping the test isolated from the user's persistent database.
        let recorded: HashMap<PathBuf, (u64, i64)> = unchanged_paths
            .iter()
            .map(|path| {
                let meta = std::fs::metadata(path).unwrap();
                let mtime = meta
                    .modified()
                    .unwrap()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                (path.clone(), (meta.len(), mtime))
            })
            .collect();
        let root_string = root.to_string_lossy().into_owned();
        let mut store = store_with(&root_string, FolderStatus::Idle);
        store.folders[0].dedup = true;

        let mut plan = UploadPlan::build_since_with_stat_matches(
            &store,
            &[(0, None)],
            |path, size, mtime| {
                recorded
                    .get(path)
                    .filter(|&&(recorded_size, recorded_mtime)| {
                        (recorded_size, recorded_mtime) == (size, mtime)
                    })
                    .map(|_| Origin::Uploaded)
            },
        )
        .expect("the scan found supported files");

        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.files_seen, 3);
        assert_eq!(plan.unchanged, 2);
        assert_eq!(plan.content_skip, 0);
        assert_eq!(plan.folder_unchanged.get(&0), Some(&2));
        assert_eq!(plan.per_folder.get(&0), Some(&1));
        assert_eq!(plan.queue.len(), 1);
        assert_eq!(plan.queue[0].name, "new.jpg");
        assert_eq!(plan.bytes_total, std::fs::metadata(&new_path).unwrap().len());

        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let folder_paths = plan_folder_paths(&plan, &store);
        publish_state(&mut plan, &shared, "test".to_string(), folder_paths);
        let state = shared.lock().unwrap();
        assert_eq!(state.total, 1);
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.unchanged_skipped, 2);
        assert_eq!(state.folder_unchanged.get(&0), Some(&2));
        assert_eq!(state.bytes_total, std::fs::metadata(&new_path).unwrap().len());
        assert_eq!(state.bytes_done, 0, "pre-scan skips do not add skip bytes");

        let all_current = UploadPlan::build_since_with_stat_matches(
            &store,
            &[(0, None)],
            |path, _, _| {
                Some(if path == unchanged_paths.first().unwrap() {
                    Origin::ContentSkip
                } else {
                    Origin::Uploaded
                })
            },
        )
        .expect("an all-current scan is still a participating plan");
        assert_eq!(all_current.files_seen, 3);
        assert!(all_current.items.is_empty());
        assert!(all_current.queue.is_empty());
        assert_eq!(all_current.unchanged, 2);
        assert_eq!(all_current.content_skip, 1);
        assert_eq!(all_current.start_message(), tr("Alles aktuell"));

        let all_current_shared = Arc::new(Mutex::new(UploadShared::default()));
        let mut all_current = all_current;
        let folder_paths = plan_folder_paths(&all_current, &store);
        publish_state(
            &mut all_current,
            &all_current_shared,
            tr("Alles aktuell").to_string(),
            folder_paths,
        );
        let state = all_current_shared.lock().unwrap();
        assert_eq!(state.total, 0);
        assert!(state.queue.is_empty());
        assert_eq!(state.unchanged_skipped, 2);
        assert_eq!(state.conflict_skipped, 1);
        assert_eq!(state.folder_unchanged.get(&0), Some(&2));
        assert_eq!(state.folder_conflict.get(&0), Some(&1));
        drop(state);

        let empty_root = scratch_scan_folder("empty");
        std::fs::create_dir_all(&empty_root).unwrap();
        let empty_root_string = empty_root.to_string_lossy().into_owned();
        let empty_store = store_with(&empty_root_string, FolderStatus::Idle);
        assert!(
            UploadPlan::build_since_with_stat_matches(&empty_store, &[(0, None)], |_, _, _| None)
                .is_none(),
            "an empty scan is distinct from a scan whose files are all current"
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(empty_root).unwrap();
    }

    #[test]
    fn empty_backup_reaches_the_plan_as_a_destination_target() {
        // An enabled folder with no file at all answers `max_mtime() == None`.
        // Such a folder has nothing to scan and no watermark, but the start pass
        // must still carry it as a **target** so the run's preflight resolves —
        // and if needed creates — its mirrored destination. This holds alone
        // and alongside a changed folder in the same pass.
        let root = scratch_scan_folder("empty-target");
        let empty = root.join("empty");
        let filled = root.join("filled");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&filled).unwrap();
        std::fs::write(filled.join("photo.jpg"), b"photo bytes").unwrap();

        let filled_string = filled.to_string_lossy().into_owned();
        let mut store = store_with(&filled_string, FolderStatus::Idle);
        store.folders.push(folders::BackupFolder::new(empty.clone()));
        assert_eq!(
            folders::max_mtime(&empty),
            None,
            "no file means no watermark to advance"
        );

        // Same pass, one changed folder: the file plan keeps its queue and only
        // gains the empty folder's target entry.
        let mut mixed =
            UploadPlan::build_since_with_stat_matches(&store, &[(0, None)], |_, _, _| None)
                .expect("the filled folder has a supported image");
        mixed.add_targets(&store, &[1]);
        assert_eq!(mixed.per_folder.get(&1), Some(&0));
        assert_eq!(
            mixed.folder_opts.get(&1),
            Some(&(store.folders[1].dedup, store.folders[1].conflict_resolution))
        );
        assert_eq!(mixed.items.len(), 1, "a target queues no file of its own");
        assert!(mixed.queue.iter().all(|row| row.folder == 0));
        let destinations = plan_folder_paths(&mixed, &store);
        assert_eq!(destinations.get(&0), Some(&filled));
        assert_eq!(
            destinations.get(&1),
            Some(&empty),
            "the empty folder's destination is part of the preflight"
        );

        // Empty-only pass: the plan has no files at all, but still the target.
        let mut only_empty = UploadPlan::empty();
        only_empty.add_targets(&store, &[1]);
        assert!(only_empty.items.is_empty() && only_empty.queue.is_empty());
        assert_eq!(plan_folder_paths(&only_empty, &store).get(&1), Some(&empty));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manual_plan_filters_with_real_in_memory_dedup_index() {
        let root = scratch_scan_folder("real-dedup-index");
        std::fs::create_dir_all(&root).unwrap();
        let unchanged_paths = [root.join("unchanged-a.jpg"), root.join("unchanged-b.jpg")];
        let new_path = root.join("new.jpg");
        for path in unchanged_paths.iter().chain([&new_path]) {
            std::fs::write(path, b"photo bytes").unwrap();
        }

        let idx = DedupIndex::in_memory();
        for path in &unchanged_paths {
            let meta = std::fs::metadata(path).unwrap();
            let mtime = meta
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            idx.record_upload(path, "test-md5", meta.len(), mtime, None);
        }

        let root_string = root.to_string_lossy().into_owned();
        let mut store = store_with(&root_string, FolderStatus::Idle);
        store.folders[0].dedup = true;
        let plan = UploadPlan::build_since_with_stat_matches(
            &store,
            &[(0, None)],
            |path, size, mtime| idx.stat_matches(path, size, mtime),
        )
        .expect("the scan found supported files");

        assert_eq!(plan.files_seen, 3);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.queue.len(), 1);
        assert_eq!(plan.queue[0].name, "new.jpg");
        assert_eq!(plan.unchanged, 2);
        assert_eq!(plan.folder_unchanged.get(&0), Some(&2));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_returned_source_is_parked_once_and_lifted_to_idle() {
        // (a) `NotFound` + path back ⇒ the path is returned (parked for a
        // full-stock scan) and the state lifts to `Idle`.
        let mut store = store_with("/photos/returned", FolderStatus::NotFound);
        let (returned, changed) = lift_returned_sources(&mut store, |_| true);
        assert_eq!(returned, vec![PathBuf::from("/photos/returned")]);
        assert!(changed);
        assert_eq!(store.folders[0].status, FolderStatus::Idle);

        // (c) The next pass sees `Idle` + present: **no** second park — exactly
        // one scan trigger per return.
        let (again, changed) = lift_returned_sources(&mut store, |_| true);
        assert!(again.is_empty());
        assert!(!changed);
    }

    #[test]
    fn a_still_missing_source_is_not_parked() {
        // (b) `NotFound` + path still gone ⇒ nothing is returned, the state
        // stays `NotFound`.
        let mut store = store_with("/photos/still-gone", FolderStatus::NotFound);
        let (returned, changed) = lift_returned_sources(&mut store, |_| false);
        assert!(returned.is_empty());
        assert!(!changed);
        assert_eq!(store.folders[0].status, FolderStatus::NotFound);
    }

    #[test]
    fn a_present_idle_source_is_not_parked_again() {
        // (c) `Idle` + path present is the steady state, not a return: nothing
        // is parked (no repeat every `WATCH_POLL`), the state is untouched.
        let mut store = store_with("/photos/idle", FolderStatus::Idle);
        let (returned, changed) = lift_returned_sources(&mut store, |_| true);
        assert!(returned.is_empty());
        assert!(!changed);
        assert_eq!(store.folders[0].status, FolderStatus::Idle);
    }

    #[test]
    fn a_present_paused_source_is_not_a_return() {
        // (d) `Paused` + path present: **not** a return (only `NotFound` →
        // present triggers, user decision 2026-10-17). The state lifts to
        // `Idle` (older `folders.toml`), but no scan is parked.
        let mut store = store_with("/photos/paused", FolderStatus::Paused);
        let (returned, changed) = lift_returned_sources(&mut store, |_| true);
        assert!(returned.is_empty());
        assert!(changed);
        assert_eq!(store.folders[0].status, FolderStatus::Idle);
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

        assert_eq!(label, tr("Nicht noch einmal fragen").to_string());
        assert!(
            !label.contains("Ordner"),
            "the neutral label must not name any folder, got: {label}"
        );
    }

    #[test]
    fn remember_label_names_the_folder() {
        // A persisted folder owns the answer (see `remember_conflict_in_folder`),
        // so the checkbox names which folder will stop asking.
        // Language-neutral: the expectation is the translated key itself, not
        // a hard-coded German literal, so the test holds under `Lang::En` too.
        let label = remember_checkbox_label(Some("Sport"));

        assert_eq!(
            label,
            tr1("Nicht noch einmal nach ‚{}'-Ordner fragen", "Sport"),
            "expected the folder-naming conflict-remember text"
        );
        assert!(
            label.contains("Sport"),
            "expected the folder name in: {label}"
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

    #[test]
    fn remember_folder_names_only_the_owning_folder() {
        // The query behind both the checkbox label and the write: the same
        // three refusals, so the two can never disagree about the target.
        let root = scratch_folder("folder-guard");
        let other = scratch_folder("folder-other");
        let mut store = asking_store(&other);
        // A second folder at `root`, so index 0 is a neighbour that does not
        // contain the asked file — the shifted-index scenario.
        let mut neighbour = folders::BackupFolder::new(root.clone());
        neighbour.conflict_resolution = ConflictResolution::Ask;
        store.folders.push(neighbour);
        let own_file = root.join("sub").join("photo.jpg");

        // Index in range, file below the folder path ⇒ that folder.
        let found = remember_folder(&store, 1, &own_file).expect("the owning folder is found");
        assert_eq!(found.path, root);
        // The transient drag & drop target ⇒ nothing to write into.
        assert!(remember_folder(&store, ONE_TIME_IDX, &own_file).is_none());
        // An index the store no longer holds ⇒ nothing.
        assert!(remember_folder(&store, 2, &own_file).is_none());
        // A shifted index pointing at a folder that does not contain the file
        // ⇒ nothing: the answer must not land in a foreign folder.
        assert!(remember_folder(&store, 0, &own_file).is_none());
    }

    #[test]
    fn remember_label_follows_the_write_guard() {
        // End to end for the checkbox text: it names exactly the folder that
        // would receive the write (guarded by the same `remember_folder`), so
        // the label can never promise a write that would not happen.
        let root = scratch_folder("label-guard");
        let other = scratch_folder("label-other");
        let mut store = asking_store(&other);
        let mut neighbour = folders::BackupFolder::new(root.clone());
        neighbour.conflict_resolution = ConflictResolution::Ask;
        store.folders.push(neighbour);
        let own_file = root.join("photo.jpg");

        // Normal case: the owning folder is named.
        assert_eq!(
            conflict_remember_label(&store, 1, &own_file),
            remember_checkbox_label(Some(&store.folders[1].name))
        );
        // Shifted index with a foreign file ⇒ neutral, exactly like the write
        // refuses it.
        assert_eq!(
            conflict_remember_label(&store, 0, &own_file),
            remember_checkbox_label(None)
        );
        // A file below a *different* folder than the indexed one ⇒ neutral.
        assert_eq!(
            conflict_remember_label(&store, 1, &other.join("photo.jpg")),
            remember_checkbox_label(None)
        );
        // Transient drag & drop target / dead index ⇒ neutral.
        assert_eq!(
            conflict_remember_label(&store, ONE_TIME_IDX, &own_file),
            remember_checkbox_label(None)
        );
        assert_eq!(
            conflict_remember_label(&store, 7, &own_file),
            remember_checkbox_label(None)
        );
    }

    // -- Welle 5.31: injecting a folder into a live run -----------------------

    /// A plan of `(folder index, path)` entries with a known size each — the
    /// shape `inject_into_live_run` consumes, built without walking a folder so
    /// the tests need no filesystem. `sized_total`/`bytes_total` are set by the
    /// caller when the byte-ETA path is under test.
    fn literal_plan(files: &[(usize, &str)]) -> UploadPlan {
        let mut plan = UploadPlan::empty();
        for &(idx, name) in files {
            plan.files_seen += 1;
            let row = plan.items.len();
            let path = PathBuf::from(name);
            plan.queue.push(QueueEntry {
                folder: idx,
                name: name.to_string(),
                state: FileState::Waiting,
                size: 1,
                path: path.clone(),
                bytes_sent: 0,
            });
            plan.items.push((row, idx, path));
            *plan.per_folder.entry(idx).or_insert(0) += 1;
        }
        plan
    }

    /// Serialize the tests that make `inject_into_live_run` write the store.
    /// With the `#[cfg(test)]` redirect in `folders::store_path` this writes a
    /// scratch file, never the developer's real `folders.toml` (Verify C); the
    /// lock only keeps two writers from interleaving.
    fn with_store_lock<T>(f: impl FnOnce() -> T) -> T {
        static LOCK: Mutex<()> = Mutex::new(());
        let _g = LOCK.lock().unwrap();
        f()
    }

    #[test]
    fn injection_takes_the_plan_into_the_live_run() {
        // An active, unsealed run with one file already queued: the injected plan
        // must land in the very queue the workers drain, shifted by the existing
        // rows, and its counters must grow.
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut s = shared.lock().unwrap();
            s.active = true;
            s.sealed = false;
            s.cancelled = false;
            s.item_queue = Some(queue.clone());
            s.queue = vec![QueueEntry {
                folder: 0,
                name: "existing.jpg".into(),
                state: FileState::Waiting,
                size: 1,
                path: PathBuf::from("/src/folder-a/existing.jpg"),
                bytes_sent: 0,
            }];
            s.total = 1;
            s.folder_total.insert(0, 1);
            s.folder_paths.insert(0, PathBuf::from("/src/folder-a"));
            s.bytes_total = 100;
        }
        let mut store = FolderStore::default();
        store
            .folders
            .push(folders::BackupFolder::new(PathBuf::from("/src/folder-a")));
        store
            .folders
            .push(folders::BackupFolder::new(PathBuf::from("/src/folder-b")));

        // Two files in folder 1, both sizes known: the byte ETA continues.
        let mut plan = literal_plan(&[(1, "/src/folder-b/a.jpg"), (1, "/src/folder-b/b.jpg")]);
        plan.bytes_total = 50;
        plan.sized_total = 2;

        let refused = with_store_lock(|| inject_into_live_run(plan, &mut store, &shared));
        assert!(refused.is_none(), "an active run must take the plan");

        let s = shared.lock().unwrap();
        assert_eq!(s.queue.len(), 3, "rows appended to the live queue");
        assert_eq!(s.total, 3);
        assert_eq!(s.folder_total.get(&0).copied(), Some(1));
        assert_eq!(s.folder_total.get(&1).copied(), Some(2));
        assert_eq!(s.folder_paths.get(&0), Some(&PathBuf::from("/src/folder-a")));
        assert_eq!(s.folder_paths.get(&1), Some(&PathBuf::from("/src/folder-b")));
        assert_eq!(s.bytes_total, 150, "100 + 50: both sides fully known");
        {
            let q = s.item_queue.as_ref().unwrap().lock().unwrap();
            let rows: Vec<(usize, usize)> = q.iter().map(|(r, i, _)| (*r, *i)).collect();
            assert_eq!(
                rows,
                vec![(1, 1), (2, 1)],
                "item rows shifted by the existing queue length"
            );
        }
        drop(s);
        assert_eq!(
            store.folders[1].total_files, 2,
            "the folder counter is persisted"
        );
    }

    #[test]
    fn injected_folder_path_is_available_to_the_queue() {
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut state = shared.lock().unwrap();
            state.active = true;
            state.item_queue = Some(queue);
        }
        let mut store = FolderStore {
            folders: vec![folders::BackupFolder::new(PathBuf::from(
                "/src/injected-folder",
            ))],
        };
        let plan = literal_plan(&[(0, "/src/injected-folder/photo.jpg")]);

        let refused = with_store_lock(|| inject_into_live_run(plan, &mut store, &shared));
        assert!(refused.is_none(), "the live run should accept this folder");

        let display = {
            let state = shared.lock().unwrap();
            let ambiguous = ambiguous_folder_names_for_paths(&state.folder_paths);
            queue_folder_display(&state.folder_paths, 0, &ambiguous)
        };
        assert_eq!(
            display,
            Some((
                "injected-folder".to_string(),
                "/src/injected-folder".to_string(),
            )),
            "the injected path is available to the queue"
        );
    }

    #[test]
    fn injection_is_refused_for_a_folder_already_in_the_run() {
        // Verify B: a folder that already hangs in this run must not be injected
        // again — its rows would land twice and `total`/`folder_total` would read
        // "x von 2x". Folder 0 is already part of the run, so a second plan for it
        // is refused untouched.
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut s = shared.lock().unwrap();
            s.active = true;
            s.sealed = false;
            s.cancelled = false;
            s.item_queue = Some(queue.clone());
            s.total = 1;
            s.folder_total.insert(0, 1);
            s.bytes_total = 100;
        }
        let mut store = FolderStore::default();
        store
            .folders
            .push(folders::BackupFolder::new(PathBuf::from("/src/folder-a")));

        let plan = literal_plan(&[(0, "/src/folder-a/a.jpg")]);
        let refused = with_store_lock(|| inject_into_live_run(plan, &mut store, &shared));
        assert!(refused.is_some(), "an already-running folder is refused");

        let s = shared.lock().unwrap();
        assert_eq!(s.total, 1, "total must not double");
        assert_eq!(s.folder_total.get(&0).copied(), Some(1));
        assert!(s.item_queue.as_ref().unwrap().lock().unwrap().is_empty());
    }

    #[test]
    fn injection_is_refused_when_a_running_path_has_a_new_plan_index() {
        // A was plan index 0. It was removed and re-added at store index 1 while
        // B now occupies index 0; the same path must not enter the run twice.
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut s = shared.lock().unwrap();
            s.active = true;
            s.sealed = false;
            s.cancelled = false;
            s.item_queue = Some(queue.clone());
            s.total = 3;
            s.folder_total.insert(0, 3);
            s.folder_paths.insert(0, PathBuf::from("/src/folder-a"));
        }
        let mut store = FolderStore {
            folders: vec![
                folders::BackupFolder::new(PathBuf::from("/src/folder-b")),
                folders::BackupFolder::new(PathBuf::from("/src/folder-a")),
            ],
        };

        let plan = literal_plan(&[(1, "/src/folder-a/again.jpg")]);
        let refused = with_store_lock(|| inject_into_live_run(plan, &mut store, &shared));
        assert!(refused.is_some(), "a path already in the run must be refused");

        let s = shared.lock().unwrap();
        assert_eq!(s.total, 3, "the run total must not grow");
        assert_eq!(s.folder_total, HashMap::from([(0, 3)]));
        assert_eq!(s.folder_paths, HashMap::from([(0, PathBuf::from("/src/folder-a"))]));
        assert!(s.item_queue.as_ref().unwrap().lock().unwrap().is_empty());
        assert!(s.log.iter().any(|line| line.contains("bereits verarbeitet")));
        drop(s);
        assert_eq!(store.folders[1].total_files, 0);
    }

    #[test]
    fn injection_is_refused_when_the_run_is_sealed() {
        // The last worker sealed the run: a late injection must be refused and
        // change nothing at all.
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut s = shared.lock().unwrap();
            s.active = true;
            s.sealed = true;
            s.item_queue = Some(queue.clone());
            s.total = 1;
            s.folder_total.insert(0, 1);
            s.bytes_total = 100;
        }
        let mut store = FolderStore::default();
        store
            .folders
            .push(folders::BackupFolder::new(PathBuf::from("/src/folder-a")));

        let plan = literal_plan(&[(0, "/src/folder-a/a.jpg")]);
        let refused = with_store_lock(|| inject_into_live_run(plan, &mut store, &shared));
        assert!(refused.is_some(), "a sealed run must refuse the plan");

        let s = shared.lock().unwrap();
        assert!(s.queue.is_empty(), "no row was appended");
        assert_eq!(s.total, 1);
        assert_eq!(s.bytes_total, 100);
        assert_eq!(s.folder_total.get(&0).copied(), Some(1));
        assert!(s.item_queue.as_ref().unwrap().lock().unwrap().is_empty());
        drop(s);
        assert_eq!(store.folders[0].total_files, 0);
    }

    #[test]
    fn injection_is_refused_without_a_live_run() {
        // No run at all: the plan comes back unchanged.
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let mut store = FolderStore::default();
        let plan = literal_plan(&[(0, "/src/folder-a/a.jpg")]);
        let out = with_store_lock(|| inject_into_live_run(plan, &mut store, &shared));
        assert!(out.is_some(), "no run: the plan comes back unchanged");
        assert_eq!(shared.lock().unwrap().total, 0);

        // A cancelled run (sign-out): still refused — it must take no new work.
        {
            let mut s = shared.lock().unwrap();
            s.active = true;
            s.cancelled = true;
            s.item_queue = Some(Arc::new(Mutex::new(VecDeque::new())));
        }
        let plan = literal_plan(&[(0, "/src/folder-a/a.jpg")]);
        let out = with_store_lock(|| inject_into_live_run(plan, &mut store, &shared));
        assert!(out.is_some(), "a cancelled run must refuse the plan");
        assert_eq!(shared.lock().unwrap().total, 0);
    }
}
