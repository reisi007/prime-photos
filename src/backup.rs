use crate::config::Config;
use crate::dedup::{DedupIndex, Origin};
use crate::folders::{self, ConflictResolution, FolderStatus, FolderStore};
use crate::i18n::{tr, tr1};
use crate::login::Session;
use crate::upload::{FileState, QueueEntry, UploadShared, run_upload};
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

#[cfg(feature = "legacy-eframe")]
pub(crate) mod eframe_ui;
#[cfg(feature = "legacy-eframe")]
mod eframe_app;

#[cfg(feature = "legacy-eframe")]
pub(crate) enum FolderAction {
    Upload(usize),
    Retry(usize),
    Remove(usize),
    OpenFinder(usize),
    ToggleEnabled(usize),
    Edit(usize),
}

#[cfg(feature = "legacy-eframe")]
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
#[cfg(feature = "legacy-eframe")]
fn aggregate_progress(rows: &[Row]) -> (usize, usize) {
    rows.iter()
        .fold((0usize, 0usize), |(done, total), r| {
            (done + r.done, total + r.total)
        })
}

/// Live progress is scoped to the current run; the library-wide row sum is for
/// the idle display only.
#[cfg(feature = "legacy-eframe")]
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
#[cfg(feature = "legacy-eframe")]
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
#[cfg(feature = "legacy-eframe")]
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
    if idx == crate::platform::app_shared::ONE_TIME_IDX {
        return None;
    }
    let path = paths.get(&idx)?;
    store.folders.iter().position(|folder| folder.path == *path)
}

/// Resolve a current store row path to its plan-time index. Folder paths are
/// unique within a run; if a malformed/legacy state violates that invariant,
/// choose the lowest matching plan index so the result remains deterministic.
/// The one-time target is synthetic and never maps to a persisted folder row.
#[cfg(feature = "legacy-eframe")]
fn plan_index_for(paths: &HashMap<usize, PathBuf>, path: &Path) -> Option<usize> {
    paths
        .iter()
        .filter_map(|(&idx, folder_path)| {
            (idx != crate::platform::app_shared::ONE_TIME_IDX && folder_path == path)
                .then_some(idx)
        })
        .min()
}

/// Apply a run's folder counters to the current store rows, resolving each
/// plan-time index by the path captured for that run.
pub(crate) fn apply_folder_progress(
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
#[cfg(feature = "legacy-eframe")]
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
#[cfg(feature = "legacy-eframe")]
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
#[cfg(feature = "legacy-eframe")]
fn queue_folder_display(
    paths: &HashMap<usize, PathBuf>,
    idx: usize,
    ambiguous: &HashSet<String>,
) -> Option<(String, String)> {
    if idx == crate::platform::app_shared::ONE_TIME_IDX {
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
#[cfg(feature = "legacy-eframe")]
fn ambiguous_folder_names_for_paths(paths: &HashMap<usize, PathBuf>) -> HashSet<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (&idx, path) in paths {
        if idx == crate::platform::app_shared::ONE_TIME_IDX {
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
#[cfg(feature = "legacy-eframe")]
const QUEUE_ROW_H: f32 = 20.0;
#[cfg(feature = "legacy-eframe")]
const QUEUE_HEADER_H: f32 = 20.0;
/// Headless-measured production consumption above the table's first row with
/// the theme: a 20 px header-cell response at a 3 px top inset, 8 px to the
/// 1 px divider, then 8 px to the first row. The split reserves all 40 px.
#[cfg(feature = "legacy-eframe")]
const QUEUE_HEADER_TOTAL_H: f32 = 40.0;
/// Compact column carrying the backup folder of each queue row (user decision
/// 2026-10-16): without it the files of several targets are indistinguishable.
#[cfg(feature = "legacy-eframe")]
const QUEUE_FOLDER_W: f32 = 120.0;
#[cfg(feature = "legacy-eframe")]
const QUEUE_SIZE_W: f32 = 80.0;
/// Wide enough for "In Bearbeitung · 100 %" at 12 pt (Issue #30); the text is
/// truncated instead of painting over the next column if a translation is
/// longer.
#[cfg(feature = "legacy-eframe")]
const QUEUE_STATUS_W: f32 = 140.0;
#[cfg(feature = "legacy-eframe")]
const QUEUE_MIN_NAME_W: f32 = 80.0;
/// Item spacing egui adds after the queue `ScrollArea`.
#[cfg(feature = "legacy-eframe")]
const QUEUE_TABLE_TRAILING_H: f32 = 8.0;
/// Fixed height of the aggregate progress row (spinner / progress bar).
#[cfg(feature = "legacy-eframe")]
const QUEUE_PROGRESS_ROW_H: f32 = 26.0;
/// Height of the aggregate progress zone under the queue table: the 6 px gap,
/// the fixed 26 px row and the 8 px trailing item spacing. The row is reserved
/// in every branch, so the splitter and folder heading keep a deterministic y.
#[cfg(feature = "legacy-eframe")]
const QUEUE_PROGRESS_ZONE_H: f32 = 6.0 + QUEUE_PROGRESS_ROW_H + 8.0;
/// Hit area of the draggable horizontal splitter and the air above/below it.
#[cfg(feature = "legacy-eframe")]
const SPLITTER_H: f32 = 10.0;
#[cfg(feature = "legacy-eframe")]
const SPLITTER_GAP: f32 = 3.0;
/// Splitter zone including the trailing item spacing: 3 + 10 + 8 + 3 = 24 px.
#[cfg(feature = "legacy-eframe")]
const SPLITTER_ZONE_H: f32 = 2.0 * SPLITTER_GAP + SPLITTER_H + 8.0;
/// Headless-measured folder disclosure layout row (26 px, even though its text
/// label response is 16 px) plus the trailing item spacing: 26 + 8 = 34 px.
#[cfg(feature = "legacy-eframe")]
const FOLDER_HEADING_ZONE_H: f32 = 26.0 + 8.0;
/// Air between the folder heading and the card list.
#[cfg(feature = "legacy-eframe")]
const FOLDER_HEADING_GAP: f32 = 6.0;
/// Air after the folder `ScrollArea` before the result footer.
#[cfg(feature = "legacy-eframe")]
const POST_LIST_GAP: f32 = 6.0;
/// Air between the post-list gap and the footer line.
#[cfg(feature = "legacy-eframe")]
const FOOTER_GAP: f32 = 6.0;
/// Line box of the 11 pt result footer.
#[cfg(feature = "legacy-eframe")]
const FOOTER_LINE_H: f32 = 13.0;
/// Gap between two page sections when no splitter separates them.
#[cfg(feature = "legacy-eframe")]
const SECTION_GAP: f32 = 6.0;
/// Height the folder list must leave for the result footer below it:
/// table trailing 8 + post-list 6 + footer gap 6 + line 13 = 33 px.
#[cfg(feature = "legacy-eframe")]
const QUEUE_FOOTER_H: f32 =
    QUEUE_TABLE_TRAILING_H + POST_LIST_GAP + FOOTER_GAP + FOOTER_LINE_H;
/// Floor of the queue-table viewport when the window has room for both floors.
#[cfg(feature = "legacy-eframe")]
const QUEUE_MIN_VIEWPORT_H: f32 = 72.0;
/// Floor of the folder-list viewport when the window has room for both floors.
#[cfg(feature = "legacy-eframe")]
const FOLDER_MIN_VIEWPORT_H: f32 = 96.0;
/// Default share of the splittable height taken by the queue while both
/// sections are open (Issue #33, user wish 2026-11-03).
pub(crate) const BACKUP_QUEUE_RATIO_DEFAULT: f32 = 0.35;
/// Smallest supported queue share; the splitter cannot make the queue smaller.
pub(crate) const BACKUP_QUEUE_RATIO_MIN: f32 = 0.20;
/// Largest supported queue share; the folder list keeps the rest.
pub(crate) const BACKUP_QUEUE_RATIO_MAX: f32 = 0.80;
/// Chrome below the table while **both** sections are open: table trailing,
/// progress zone, splitter zone, folder heading zone and the footer area.
/// 8 + 40 + 24 + 34 + 6 + 33 = 145 px.
#[cfg(feature = "legacy-eframe")]
const QUEUE_BELOW_H: f32 = QUEUE_TABLE_TRAILING_H
    + QUEUE_PROGRESS_ZONE_H
    + SPLITTER_ZONE_H
    + FOLDER_HEADING_ZONE_H
    + FOLDER_HEADING_GAP
    + QUEUE_FOOTER_H;
/// Chrome below the table when only the queue is open: table trailing, progress,
/// section gap, folder heading, footer gap and line. 8 + 40 + 6 + 34 + 6 + 13
/// = 107 px.
#[cfg(feature = "legacy-eframe")]
const QUEUE_ONLY_BELOW_H: f32 = QUEUE_TABLE_TRAILING_H
    + QUEUE_PROGRESS_ZONE_H
    + SECTION_GAP
    + FOLDER_HEADING_ZONE_H
    + FOOTER_GAP
    + FOOTER_LINE_H;
/// Chrome below the queue section when only the folder list is open: progress,
/// section gap, folder heading, heading/list gap and the footer area.
/// 40 + 6 + 34 + 6 + 33 = 119 px.
#[cfg(feature = "legacy-eframe")]
const FOLDER_ONLY_BELOW_H: f32 = QUEUE_PROGRESS_ZONE_H
    + SECTION_GAP
    + FOLDER_HEADING_ZONE_H
    + FOLDER_HEADING_GAP
    + QUEUE_FOOTER_H;
/// Vertical inner margin of a folder card (top and bottom).
#[cfg(feature = "legacy-eframe")]
const FOLDER_CARD_INNER_Y: i8 = 10;
/// Vertical outer margin of a folder card (top and bottom).
#[cfg(feature = "legacy-eframe")]
const FOLDER_CARD_OUTER_Y: i8 = 3;
/// Vertical gap between two folder cards, applied as the folder list's
/// `item_spacing.y`. The cards draw at their natural height in the scroll area.
#[cfg(feature = "legacy-eframe")]
const FOLDER_CARD_GAP: f32 = 6.0;

/// Column geometry of the queue table, derived once per frame and reused by the
/// header and **every** row (Issue #30: both used to recompute the name width,
/// and the header's cells were centred while the values were not).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg(feature = "legacy-eframe")]
struct QueueColumns {
    /// Width the table occupies: the page content minus the scroll-bar strip
    /// the theme reserves.
    table_w: f32,
    folder_w: f32,
    name_w: f32,
    size_w: f32,
    status_w: f32,
}

/// Pure table geometry. `gap` is the theme's horizontal `item_spacing` (three
/// gaps between four cells), `bar_w` the reserved scroll-bar strip. The table
/// ends at `content_w - bar_w`; the name takes what is left, at least
/// [`QUEUE_MIN_NAME_W`] (the minimum window is wide enough for that).
#[cfg(feature = "legacy-eframe")]
fn queue_columns(content_w: f32, gap: f32, bar_w: f32) -> QueueColumns {
    let table_w = (content_w - bar_w).max(0.0);
    let name_w = (table_w - QUEUE_FOLDER_W - QUEUE_SIZE_W - QUEUE_STATUS_W - 3.0 * gap)
        .max(QUEUE_MIN_NAME_W);
    QueueColumns {
        table_w,
        folder_w: QUEUE_FOLDER_W,
        name_w,
        size_w: QUEUE_SIZE_W,
        status_w: QUEUE_STATUS_W,
    }
}

/// Clamp a persisted or runtime split ratio into the supported range. A
/// non-finite value (hand-written `nan`/`inf`) falls back to the default rather
/// than poisoning the layout (Issue #33).
pub(crate) fn sanitize_backup_queue_ratio(ratio: f32) -> f32 {
    if ratio.is_finite() {
        ratio.clamp(BACKUP_QUEUE_RATIO_MIN, BACKUP_QUEUE_RATIO_MAX)
    } else {
        BACKUP_QUEUE_RATIO_DEFAULT
    }
}

/// Apply one frame's vertical drag delta (pixels) to the queue ratio. The delta
/// is normalised by the splittable height, so the same drag feels the same at
/// every window size. A non-finite or empty splittable height leaves the ratio
/// usable instead of producing `NaN`.
#[cfg(feature = "legacy-eframe")]
fn adjusted_backup_queue_ratio(ratio: f32, delta_y: f32, splittable_h: f32) -> f32 {
    if !delta_y.is_finite() || !splittable_h.is_finite() || splittable_h <= 0.0 {
        return sanitize_backup_queue_ratio(ratio);
    }
    sanitize_backup_queue_ratio(ratio + delta_y / splittable_h)
}

/// Hard layout invariant: the two backup sections can never be closed at the
/// same time. A section may only close while the other one is open.
#[cfg(feature = "legacy-eframe")]
fn keep_one_section_open(open: bool, other_open: bool) -> bool {
    open || !other_open
}

/// A section can be toggled only while its sibling is open and its parent UI
/// is enabled.
#[cfg(feature = "legacy-eframe")]
fn section_toggle_enabled(ui_enabled: bool, other_open: bool) -> bool {
    ui_enabled && other_open
}

/// A section-header click changes state only when the header may be toggled.
#[cfg(feature = "legacy-eframe")]
fn section_toggle_after_click(
    clicked: bool,
    can_toggle: bool,
    is_open: bool,
    other_open: bool,
) -> Option<bool> {
    if !clicked || !can_toggle {
        return None;
    }
    let next = keep_one_section_open(!is_open, other_open);
    (next != is_open).then_some(next)
}

/// Split the splittable height by the queue ratio, respecting both viewport
/// floors when the height has room for them. In a tiny window the split
/// degrades proportionally; sizes never become negative.
#[cfg(feature = "legacy-eframe")]
fn split_viewports(total: f32, ratio: f32) -> (f32, f32) {
    let total = if total.is_finite() {
        total.max(0.0)
    } else {
        0.0
    };
    if total == 0.0 {
        return (0.0, 0.0);
    }
    let ratio = sanitize_backup_queue_ratio(ratio);
    let desired = total * ratio;
    if total >= QUEUE_MIN_VIEWPORT_H + FOLDER_MIN_VIEWPORT_H {
        let queue = desired.clamp(QUEUE_MIN_VIEWPORT_H, total - FOLDER_MIN_VIEWPORT_H);
        (queue, total - queue)
    } else {
        let queue = desired.clamp(0.0, total);
        (queue, total - queue)
    }
}

/// Height the splitter can actually distribute below the table column header
/// and above the fixed chrome.
#[cfg(feature = "legacy-eframe")]
fn backup_split_splittable(free: f32) -> f32 {
    (free - QUEUE_HEADER_TOTAL_H - QUEUE_BELOW_H).max(0.0)
}

/// Allocate the queue-table and folder-list viewports for the current
/// disclosure and splitter state. The saved ratio is only read while **both**
/// sections are open; with exactly one open, that section receives every free
/// pixel and the ratio stays untouched.
#[cfg(feature = "legacy-eframe")]
fn page_heights_for_sections(
    free: f32,
    queue_open: bool,
    folders_open: bool,
    ratio: f32,
) -> (f32, f32) {
    let free = if free.is_finite() { free.max(0.0) } else { 0.0 };
    match (queue_open, folders_open) {
        (true, true) => split_viewports(
            (free - QUEUE_HEADER_TOTAL_H - QUEUE_BELOW_H).max(0.0),
            ratio,
        ),
        (true, false) => (
            (free - QUEUE_HEADER_TOTAL_H - QUEUE_ONLY_BELOW_H).max(0.0),
            0.0,
        ),
        (false, true) => (0.0, (free - FOLDER_ONLY_BELOW_H).max(0.0)),
        (false, false) => (0.0, 0.0),
    }
}

/// Whether the folder list may draw in this frame. The folder header toggles
/// *after* the split was computed: opening waits for the next frame's both-open
/// split, while closing keeps the list for one final frame in its existing
/// viewport. Drawing from the layout snapshot preserves that allocation; the
/// actual-height cap below keeps the footer safe even in a tiny window.
#[cfg(feature = "legacy-eframe")]
fn folder_list_draws(expanded: bool, expanded_for_layout: bool) -> bool {
    match (expanded, expanded_for_layout) {
        (true, true) => true,   // stable open
        (true, false) => false, // opening: wait for the new split
        (false, true) => true,  // closing: draw in the old split for this frame
        (false, false) => false, // stable closed
    }
}

/// Status bucket of the live queue's flat ordering: **our own** order, decided
/// by the user on 2026-10-26 — "In Bearbeitung" → "Fehlgeschlagen" → "Warten"
/// → "Abgeschlossen" → "Übersprungen". The original status table is at most a
/// loose reference (Processing first, Errored second; see
/// docs/original/SPEC_STATUS_DETAIL.md §3.3); it additionally sorts by
/// `SortDate` and does not preserve insertion order, so its key is
/// deliberately **not** copied.
#[cfg(feature = "legacy-eframe")]
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
#[cfg(feature = "legacy-eframe")]
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

/// The immutable production inputs handed to the one shared upload runner.
/// Keeping the plan/publish/launch path here means eframe and the Rune spike do
/// not grow separate scanners or upload implementations.
pub(crate) struct UploadLaunch {
    items: Vec<(usize, usize, PathBuf)>,
    config: Config,
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
    folder_opts: HashMap<usize, (bool, ConflictResolution)>,
    destination_roots: HashMap<usize, PathBuf>,
}

impl UploadLaunch {
    pub(crate) async fn run(self) {
        run_upload(
            self.items,
            self.config,
            self.shared,
            self.paused,
            self.concurrency,
            self.limit_kbps,
            self.album,
            self.folder_opts,
            self.destination_roots,
        )
        .await;
    }

    pub(crate) fn spawn(self, runtime: &tokio::runtime::Runtime) {
        runtime.spawn(self.run());
    }
}

/// Shared, explicit-user-action path for persisted target uploads. This does
/// not run from route construction or polling; callers choose when the source
/// check, scan, dedupe index, and production upload are allowed to start.
pub(crate) fn prepare_scheduled_upload_shared(
    store: &mut FolderStore,
    indices: Vec<usize>,
    config: &mut Config,
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    album: String,
    slow_mode: bool,
    persist_store: bool,
 ) -> Option<UploadLaunch> {
    prepare_scheduled_upload_with(
        store,
        indices,
        config,
        shared,
        paused,
        album,
        slow_mode,
        persist_store,
        |path| path.is_dir(),
        UploadPlan::build,
        |store| {
            let _ = store.save();
        },
    )
}

fn prepare_scheduled_upload_with(
    store: &mut FolderStore,
    indices: Vec<usize>,
    config: &mut Config,
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    album: String,
    slow_mode: bool,
    persist_store: bool,
    is_dir: impl Fn(&Path) -> bool,
    build_plan: impl FnOnce(&FolderStore, &[usize]) -> Option<UploadPlan>,
    mut save_store: impl FnMut(&FolderStore),
) -> Option<UploadLaunch> {
    let mut changed = false;
    let mut missing = false;
    let mut present = Vec::new();
    for &index in &indices {
        let Some(folder) = store.folders.get_mut(index) else {
            continue;
        };
        let exists = is_dir(&folder.path);
        let next = source_status(exists, folder.status);
        if next != folder.status {
            folder.status = next;
            changed = true;
        }
        if exists {
            present.push(index);
        } else {
            missing = true;
        }
    }
    if changed && persist_store {
        save_store(store);
    }

    let mut destination_roots: HashMap<usize, PathBuf> = present
        .iter()
        .filter_map(|&index| {
            store
                .folders
                .get(index)
                .map(|folder| (index, folder.path.clone()))
        })
        .collect();
    let Some(mut plan) = build_plan(store, &present).or_else(|| {
        if present.is_empty() {
            return None;
        }
        let mut empty = UploadPlan::empty();
        for &index in &present {
            if let Some(folder) = store.folders.get(index) {
                empty.per_folder.insert(index, 0);
                empty.folder_opts.insert(
                    index,
                    (folder.dedup, folder.conflict_resolution),
                );
            }
        }
        Some(empty)
    }) else {
        let message = if missing {
            tr("Der Quellordner ist gerade nicht erreichbar (z. B. externes Laufwerk nicht angeschlossen).")
        } else {
            tr("Keine Fotos in diesem Ordner gefunden.")
        };
        shared.lock().unwrap().log(message);
        return None;
    };

    // Include empty selected targets in destination preflight, even when a
    // different selected target contains upload candidates.
    for &index in &present {
        if let Some(folder) = store.folders.get(index) {
            plan.per_folder.entry(index).or_insert(0);
            plan.folder_opts
                .entry(index)
                .or_insert((folder.dedup, folder.conflict_resolution));
            destination_roots
                .entry(index)
                .or_insert_with(|| folder.path.clone());
        }
    }

    let concurrency = config.concurrency();
    let limit_kbps = if config.limit_enabled {
        config.limit_kbps
    } else {
        0
    };
    let message = plan.start_message();
    publish_plan(
        &mut plan,
        store,
        &shared,
        message,
        Some(slow_mode),
        persist_store,
        &mut save_store,
    );
    if slow_mode {
        shared
            .lock()
            .unwrap()
            .log(tr("Slow-Mode aktiv: 1 Datei gleichzeitig."));
    }
    paused.send_replace(false);
    config.paused = false;
    Some(UploadLaunch {
        items: plan.items,
        config: config.clone(),
        shared,
        paused,
        concurrency,
        limit_kbps,
        album,
        folder_opts: plan.folder_opts,
        destination_roots,
    })
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
    slow_mode: Option<bool>,
) {
    let mut s = shared.lock().unwrap();
    // UI start paths pass the app's transient value explicitly. A detached
    // supervisor passes `None` and keeps the latest shared value, which is also
    // updated immediately by `set_slow_mode` while the supervisor is alive.
    s.slow_mode = slow_mode.unwrap_or(s.slow_mode);
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
fn publish_folder_status(
    plan: &UploadPlan,
    store: &mut FolderStore,
    persist_store: bool,
    save_store: &mut impl FnMut(&FolderStore),
) {
    for (&i, &count) in &plan.per_folder {
        if let Some(f) = store.folders.get_mut(i) {
            f.total_files = count;
        }
    }
    if persist_store {
        save_store(store);
    }
}

/// Publish a plan for **persisted** folder targets: the shared state above
/// plus the folder status/side effects. The one-time path uses
/// [`publish_state`] alone.
fn publish_plan(
    plan: &mut UploadPlan,
    store: &mut FolderStore,
    shared: &Arc<Mutex<UploadShared>>,
    msg: String,
    slow_mode: Option<bool>,
    persist_store: bool,
    save_store: &mut impl FnMut(&FolderStore),
) {
    let folder_paths = plan_folder_paths(plan, store);
    publish_state(plan, shared, msg, folder_paths, slow_mode);
    publish_folder_status(plan, store, persist_store, save_store);
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

/// A failed store read parks every store-dependent part of the watcher pass
/// until the next poll tick. Events and scan requests may still wake the loop,
/// but cannot advance this deadline.
#[derive(Default)]
struct WatchStoreRetry {
    retry_at: Option<Instant>,
}

impl WatchStoreRetry {
    fn eligible(&self, now: Instant) -> bool {
        self.retry_at.is_none_or(|retry_at| now >= retry_at)
    }

    fn remaining(&self, now: Instant) -> Option<Duration> {
        self.retry_at
            .filter(|retry_at| now < *retry_at)
            .map(|retry_at| retry_at.duration_since(now))
    }

    fn failed(&mut self, now: Instant) {
        self.retry_at = Some(now + WATCH_POLL);
    }

    fn succeeded(&mut self) {
        self.retry_at = None;
    }
}

/// The only store-load gate used by `watch_loop`: the work callback is not
/// invoked for an ineligible retry or a failed read, leaving all parked work
/// untouched until a fresh snapshot is available.
fn with_watch_store<T>(
    retry: &mut WatchStoreRetry,
    now: impl Fn() -> Instant,
    load: impl FnOnce() -> Option<FolderStore>,
    process: impl FnOnce(&mut FolderStore) -> T,
) -> Option<T> {
    if !retry.eligible(now()) {
        return None;
    }
    let Some(mut store) = load() else {
        // Anchor the backoff to the failure's observation, not the start of a
        // potentially slow store read.
        retry.failed(now());
        return None;
    };
    retry.succeeded();
    Some(process(&mut store))
}

/// Set once the supervising background task has been spawned for this process —
/// `ensure_background_sync` is idempotent through it.
static BG_SYNC_STARTED: AtomicBool = AtomicBool::new(false);

/// Start the shared background-sync supervisor for a non-eframe Rune host.
///
/// The upload planner, watermark pass, watcher loop, and store finalizer remain
/// the same as the existing app path. The caller owns the returned sender so a
/// newly added folder can be handed to the supervisor while another run is
/// active.
#[cfg(feature = "rune-backup")]
pub(crate) fn spawn_background_sync(
    runtime: &tokio::runtime::Runtime,
    shared: Arc<Mutex<UploadShared>>,
    paused: watch::Sender<bool>,
    session: Arc<Mutex<Session>>,
    cfg: Config,
    album: String,
) -> Option<tokio::sync::mpsc::UnboundedSender<PathBuf>> {
    if !cfg.background_sync
        || cfg.session_id.is_empty()
        || cfg.cookie_x_acb.is_empty()
        || BG_SYNC_STARTED.swap(true, Ordering::SeqCst)
    {
        return None;
    }

    let concurrency = cfg.concurrency();
    let limit = if cfg.limit_enabled {
        cfg.limit_kbps
    } else {
        0
    };
    let (scan_tx, scan_rx) = tokio::sync::mpsc::unbounded_channel::<PathBuf>();
    runtime.spawn(async move {
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
        watch_loop(
            shared, paused, session, cfg, concurrency, limit, album, scan_rx,
        )
        .await;
    });
    Some(scan_tx)
}

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
            let wakeup = {
                let mut state = shared.lock().unwrap();
                state.mark_store_dirty()
            };
            if let Some(wakeup) = wakeup {
                wakeup();
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
    // The detached supervisor has no `PrimePhotoApp`; the shared field is the
    // live app value and remains authoritative even if the user toggles while
    // this run is being prepared.
    let mut save_store = |store: &FolderStore| {
        let _ = store.save();
    };
    publish_plan(&mut plan, store, &shared, msg, None, true, &mut save_store);

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

/// Keep event paths that have not yet been mapped to a folder. This is used
/// while the persisted store is unavailable; the same set also de-duplicates
/// overlapping watcher reports across those retries.
fn park_event_paths(
    pending: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    paths: impl IntoIterator<Item = PathBuf>,
) {
    for path in paths {
        if seen.insert(path.clone()) {
            pending.push(path);
        }
    }
}

/// Folder indices in parked work come from the store snapshot that first saw
/// them. Re-resolve their paths against each successful fresh snapshot before
/// planning, since folders can be added, removed, or reordered while work waits.
fn refresh_pending_folders(
    store: &FolderStore,
    pending: &mut Vec<(usize, PathBuf)>,
    seen: &mut HashSet<PathBuf>,
) {
    if pending.is_empty() {
        return;
    }
    let paths: Vec<PathBuf> = pending.iter().map(|(_, path)| path.clone()).collect();
    let mapped = map_to_folders(store, &paths);
    pending.clear();
    seen.clear();
    park_paths(pending, seen, mapped);
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
    store: Option<&mut FolderStore>,
) -> Vec<PathBuf> {
    // No switch or no sign-in → no watcher at all (the same guard that keeps a
    // run from starting). A missing store here means the caller deliberately
    // disabled watching; a failed load must skip this function so existing
    // watcher ownership is retained until a successful reconciliation.
    let Some(store) = store.filter(|_| {
        cfg.background_sync && !cfg.session_id.is_empty() && !cfg.cookie_x_acb.is_empty()
    }) else {
        watchers.clear();
        return Vec::new();
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
    let (returned, store_changed) = lift_returned_sources(store, |p| p.is_dir());
    let desired: Vec<PathBuf> = store
        .enabled_indices()
        .iter()
        .filter_map(|&i| store.folders.get(i).map(|f| f.path.clone()))
        .collect();
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
        let _ = store.save();
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
    // Event paths that have not yet been mapped to a folder are held until a
    // fresh store snapshot is available.
    let mut deferred_event_paths: Vec<PathBuf> = Vec::new();
    let mut deferred_event_seen: HashSet<PathBuf> = HashSet::new();
    let mut store_retry = WatchStoreRetry::default();
    loop {
        // Freshest settings + store on every pass (the supervisor is detached
        // from the UI struct, so the persisted files are its source — except the
        // session material, which is overlaid from the shared `Session` because
        // the cookie fields are `#[serde(skip)]`).
        let mut cfg = Config::load().unwrap_or(last_config.clone());
        overlay_session_cookies(&mut cfg, &session);
        let watching_enabled = cfg.background_sync
            && !cfg.session_id.is_empty()
            && !cfg.cookie_x_acb.is_empty();
        if !watching_enabled {
            // This does not depend on folders.toml: disabling background sync or
            // signing out still relinquishes watcher ownership immediately.
            sync_watchers(&mut watchers, &tx, &cfg, None);
        }

        // One load attempt per pass serves watcher reconciliation and every
        // store-dependent queue. If it fails, the callback is not invoked, so
        // parked work is untouched and currently-working watchers are retained.
        let needs_store = watching_enabled
            || !pending.is_empty()
            || !pending_scans.is_empty()
            || !deferred_event_paths.is_empty();
        if needs_store {
            let _ = with_watch_store(
                &mut store_retry,
                Instant::now,
                load_store_logged,
                |store| {
                    // A source folder that just came back (`NotFound` → present)
                    // is read in **once**, with its full stock. The pause remains
                    // enforced by `handle_scan_requests` below.
                    if watching_enabled {
                        for path in sync_watchers(&mut watchers, &tx, &cfg, Some(store)) {
                            if !pending_scans.contains(&path) {
                                pending_scans.push(path);
                            }
                        }
                    }

                    // Parked entries carry indices from the snapshot that first
                    // saw them; re-resolve paths before planning against this one.
                    refresh_pending_folders(store, &mut pending, &mut pending_seen);
                    if !pending.is_empty() {
                        handle_files(
                            std::iter::empty(),
                            &mut pending,
                            &mut pending_seen,
                            store,
                            &shared,
                            &paused,
                            &cfg,
                            concurrency,
                            limit_kbps,
                            &album,
                        );
                    }

                    // A folder added while a run was busy is read on the
                    // supervisor task, parallel to the live upload.
                    if !pending_scans.is_empty() {
                        handle_scan_requests(
                            &mut pending_scans,
                            &mut scan_park_logged,
                            store,
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

                    if !deferred_event_paths.is_empty() {
                        let paths = std::mem::take(&mut deferred_event_paths);
                        deferred_event_seen.clear();
                        let files = map_to_folders(store, &paths);
                        handle_files(
                            files,
                            &mut pending,
                            &mut pending_seen,
                            store,
                            &shared,
                            &paused,
                            &cfg,
                            concurrency,
                            limit_kbps,
                            &album,
                        );
                    }
                },
            );
        }

        // Wait for the next event, the poll tick (which keeps the watcher set and
        // the pending list moving while nothing changes) — or a folder the user
        // just added while a run was busy.
        let wait = store_retry
            .remaining(Instant::now())
            .unwrap_or(WATCH_POLL);
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
            _ = tokio::time::sleep(wait) => continue,
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
        // Mapping requires the persisted folder set, so hold paths until the next
        // eligible successful store load. Overlapping reports are deduplicated
        // without disturbing already-parked work.
        park_event_paths(
            &mut deferred_event_paths,
            &mut deferred_event_seen,
            paths,
        );
    }
}

/// The persisted folder a remembered answer would be written into — `None`
/// exactly when [`remember_conflict_in_folder`] refuses to write:
/// - the transient drag & drop target
///   (`crate::platform::app_shared::ONE_TIME_IDX`), which has no `folders.toml`
///   entry at all and stays run-only,
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
    if idx == crate::platform::app_shared::ONE_TIME_IDX {
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
pub(crate) fn remember_conflict_in_folder(
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
pub(crate) fn conflict_remember_label(store: &FolderStore, idx: usize, file_path: &Path) -> String {
    remember_folder(store, idx, file_path)
        .map(|f| remember_checkbox_label(Some(&f.name)))
        .unwrap_or_else(|| remember_checkbox_label(None))
}

/// Resolve the production worker's pending name conflict using the same shared
/// answer fields and guarded per-target remembered policy as the eframe dialog.
pub(crate) fn answer_pending_conflict(
    store: &mut FolderStore,
    shared: &Arc<Mutex<UploadShared>>,
    answer: ConflictResolution,
    remember: bool,
    persist_store: bool,
) {
    let (pending_idx, pending_path) = {
        let mut state = shared.lock().unwrap();
        let pending = (state.pending_conflict_idx, state.pending_conflict_path.clone());
        state.conflict_remember = remember;
        state.conflict_answer = Some(answer);
        pending
    };
    if remember {
        if let (Some(index), Some(path)) = (pending_idx, pending_path) {
            if remember_conflict_in_folder(store, index, &path, answer) && persist_store {
                let _ = store.save();
            }
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::VecDeque;

    /// `n` distinct event entries for one folder, as `park_paths` takes them.
    fn files(range: std::ops::Range<usize>) -> Vec<(usize, PathBuf)> {
        range
            .map(|i| (0, PathBuf::from(format!("/photos/{i}.jpg"))))
            .collect()
    }

    /// A `Row` reduced to what the aggregate reads (`idx`, `total`, `done`).
    #[cfg(feature = "legacy-eframe")]
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
            (
                crate::platform::app_shared::ONE_TIME_IDX,
                PathBuf::from("/photos/a"),
            ),
        ]);

        assert_eq!(row_index_for(&store, &paths, 7), Some(1));
        assert_eq!(row_index_for(&store, &paths, 3), Some(0));
        assert_eq!(row_index_for(&store, &paths, 9), None);
        assert_eq!(
            row_index_for(&store, &paths, crate::platform::app_shared::ONE_TIME_IDX),
            None
        );
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn plan_index_for_resolves_path_and_returns_none_for_unknown_or_empty() {
        let paths = HashMap::from([
            (7, PathBuf::from("/photos/b")),
            (3, PathBuf::from("/photos/a")),
            (
                crate::platform::app_shared::ONE_TIME_IDX,
                PathBuf::from("/photos/one-time"),
            ),
        ]);

        assert_eq!(plan_index_for(&paths, Path::new("/photos/b")), Some(7));
        assert_eq!(plan_index_for(&paths, Path::new("/photos/a")), Some(3));
        assert_eq!(plan_index_for(&paths, Path::new("/photos/missing")), None);
        assert_eq!(plan_index_for(&HashMap::new(), Path::new("/photos/a")), None);
        assert_eq!(plan_index_for(&paths, Path::new("/photos/one-time")), None);
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn plan_index_for_duplicate_paths_chooses_the_lowest_index_deterministically() {
        let path = Path::new("/photos/a");
        let paths = HashMap::from([
            (9, path.to_path_buf()),
            (2, path.to_path_buf()),
            (crate::platform::app_shared::ONE_TIME_IDX, path.to_path_buf()),
        ]);

        for _ in 0..5 {
            assert_eq!(plan_index_for(&paths, path), Some(2));
        }
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
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
    #[cfg(feature = "legacy-eframe")]
    fn aggregate_is_zero_without_rows() {
        assert_eq!(aggregate_progress(&[]), (0, 0));
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn aggregate_sums_folders_and_the_one_time_row() {
        // Two persisted folders plus the transient one-time row
        // (`crate::platform::app_shared::ONE_TIME_IDX`): the idle aggregate sums
        // over all three.
        let rows = vec![
            row(0, 100, 40),
            row(1, 20, 20),
            row(crate::platform::app_shared::ONE_TIME_IDX, 5, 1),
        ];
        assert_eq!(aggregate_progress(&rows), (61, 125));
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn active_aggregate_uses_run_counts_and_idle_aggregate_sums_rows() {
        let rows = vec![row(0, 11_510, 11_510), row(1, 80, 40)];
        assert_eq!(displayed_progress(true, 3, 50, &rows), (3, 50));
        assert_eq!(displayed_progress(false, 0, 0, &rows), (11_550, 11_590));
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
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
    #[cfg(feature = "legacy-eframe")]
    fn idle_row_done_ignores_a_stale_run_entry() {
        let done = row_done(false, Some(50), 3);

        assert_eq!(done, 3);
        assert_eq!(capped_done(done, 12), 3);
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn idle_finalized_row_still_shows_complete() {
        let total = 12;
        let done = capped_done(row_done(false, Some(2), total), total);

        assert_eq!(crate::upload::folder_progress_text(FolderStatus::Done, done, total), tr("Vollständig"));
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn idle_row_total_ignores_a_stale_run_entry() {
        // The shared run map can retain an entry for a folder index that now
        // belongs to another row. Idle totals must come from the store instead.
        assert_eq!(row_total(false, Some(50), 12), 12);
        assert_eq!(row_total(true, Some(50), 12), 50);
        assert_eq!(row_total(true, None, 12), 0);
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn aggregate_ignores_a_target_without_a_total() {
        // A target that knows no total contributes nothing — it must not skew
        // the sum (and the caller guards the division by zero).
        let rows = vec![row(0, 0, 0), row(1, 10, 3)];
        assert_eq!(aggregate_progress(&rows), (3, 10));
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
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
    #[cfg(feature = "legacy-eframe")]
    fn backup_layout_chrome_constants_match() {
        assert_eq!(QUEUE_HEADER_TOTAL_H, 40.0);
        assert_eq!(QUEUE_PROGRESS_ZONE_H, 40.0);
        assert_eq!(SPLITTER_ZONE_H, 24.0);
        assert_eq!(FOLDER_HEADING_ZONE_H, 34.0);
        assert_eq!(QUEUE_FOOTER_H, 33.0);
        assert_eq!(QUEUE_BELOW_H, 145.0);
        assert_eq!(QUEUE_ONLY_BELOW_H, 107.0);
        assert_eq!(FOLDER_ONLY_BELOW_H, 119.0);
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn backup_layout_chrome_sums_match_their_parts() {
        assert_eq!(
            QUEUE_BELOW_H,
            QUEUE_TABLE_TRAILING_H
                + QUEUE_PROGRESS_ZONE_H
                + SPLITTER_ZONE_H
                + FOLDER_HEADING_ZONE_H
                + FOLDER_HEADING_GAP
                + QUEUE_FOOTER_H
        );
        assert_eq!(
            QUEUE_ONLY_BELOW_H,
            QUEUE_TABLE_TRAILING_H
                + QUEUE_PROGRESS_ZONE_H
                + SECTION_GAP
                + FOLDER_HEADING_ZONE_H
                + FOOTER_GAP
                + FOOTER_LINE_H
        );
        assert_eq!(
            FOLDER_ONLY_BELOW_H,
            QUEUE_PROGRESS_ZONE_H
                + SECTION_GAP
                + FOLDER_HEADING_ZONE_H
                + FOLDER_HEADING_GAP
                + QUEUE_FOOTER_H
        );
    }

    #[test]
    fn backup_queue_ratio_sanitizes_clamps_and_handles_non_finite() {
        assert_eq!(sanitize_backup_queue_ratio(0.5), 0.5);
        assert_eq!(sanitize_backup_queue_ratio(-1.0), BACKUP_QUEUE_RATIO_MIN);
        assert_eq!(sanitize_backup_queue_ratio(1.0), BACKUP_QUEUE_RATIO_MAX);
        for ratio in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                sanitize_backup_queue_ratio(ratio),
                BACKUP_QUEUE_RATIO_DEFAULT
            );
        }
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn splitter_drag_maps_pixels_to_ratio_and_clamps() {
        let splittable = 400.0;
        assert!((adjusted_backup_queue_ratio(0.35, 40.0, splittable) - 0.45).abs() < 0.001);
        assert_eq!(
            adjusted_backup_queue_ratio(0.35, 1000.0, splittable),
            BACKUP_QUEUE_RATIO_MAX
        );
        assert_eq!(
            adjusted_backup_queue_ratio(0.35, -1000.0, splittable),
            BACKUP_QUEUE_RATIO_MIN
        );
        for invalid in [0.0, -10.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                adjusted_backup_queue_ratio(0.35, 10.0, invalid),
                0.35,
                "splittable {invalid}"
            );
        }
        assert_eq!(adjusted_backup_queue_ratio(0.35, f32::NAN, 400.0), 0.35);
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn both_open_split_fills_the_free_height_and_respects_minimums() {
        let free = 800.0;
        let (queue_h, list_h) = page_heights_for_sections(free, true, true, 0.35);
        assert!((queue_h + list_h + QUEUE_HEADER_TOTAL_H + QUEUE_BELOW_H - free).abs() < 0.001);
        assert!(queue_h >= QUEUE_MIN_VIEWPORT_H);
        assert!(list_h >= FOLDER_MIN_VIEWPORT_H);

        // At exactly the two floors, neither side can be pushed below its
        // minimum — regardless of how hard the ratio pulls.
        let min_total = QUEUE_MIN_VIEWPORT_H + FOLDER_MIN_VIEWPORT_H;
        assert_eq!(
            split_viewports(min_total, BACKUP_QUEUE_RATIO_MIN),
            (QUEUE_MIN_VIEWPORT_H, FOLDER_MIN_VIEWPORT_H)
        );
        assert_eq!(
            split_viewports(min_total, BACKUP_QUEUE_RATIO_MAX),
            (QUEUE_MIN_VIEWPORT_H, FOLDER_MIN_VIEWPORT_H)
        );
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn both_open_split_never_goes_negative_or_overflows_in_tiny_windows() {
        let chrome = QUEUE_HEADER_TOTAL_H + QUEUE_BELOW_H;
        for free in [0.0, 10.0, 50.0, 100.0, chrome + 40.0] {
            for ratio in [0.0, 0.2, 0.5, 0.9, f32::NAN] {
                let (queue_h, list_h) = page_heights_for_sections(free, true, true, ratio);
                assert!(
                    queue_h >= 0.0 && list_h >= 0.0,
                    "free {free}, ratio {ratio}"
                );
                assert!(
                    queue_h + list_h <= (free - chrome).max(0.0) + 0.001,
                    "free {free}, ratio {ratio}"
                );
            }
        }
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn one_open_section_ignores_the_ratio_and_gets_all_free_space() {
        let free = 600.0;
        let expected_queue = free - QUEUE_HEADER_TOTAL_H - QUEUE_ONLY_BELOW_H;
        let expected_list = free - FOLDER_ONLY_BELOW_H;
        for ratio in [0.2, 0.8] {
            assert_eq!(
                page_heights_for_sections(free, true, false, ratio),
                (expected_queue, 0.0)
            );
            assert_eq!(
                page_heights_for_sections(free, false, true, ratio),
                (0.0, expected_list)
            );
        }
        assert_eq!(
            page_heights_for_sections(free, false, false, 0.5),
            (0.0, 0.0)
        );
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn page_heights_at_900_px_match_the_literal_budget() {
        // 900 - QUEUE_HEADER_TOTAL_H(40) - QUEUE_BELOW_H(145) = 715; 35 % = 250.25.
        let (queue_h, list_h) = page_heights_for_sections(900.0, true, true, 0.35);
        assert!((queue_h - 250.25).abs() < 0.001, "queue {queue_h}");
        assert!((list_h - 464.75).abs() < 0.001, "list {list_h}");
        // 900 - 40 - QUEUE_ONLY_BELOW_H(107) = 753.
        assert_eq!(
            page_heights_for_sections(900.0, true, false, 0.35),
            (753.0, 0.0)
        );
        // 900 - FOLDER_ONLY_BELOW_H(119) = 781.
        assert_eq!(
            page_heights_for_sections(900.0, false, true, 0.35),
            (0.0, 781.0)
        );
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn folder_list_draws_the_previous_viewport_for_each_transition() {
        assert!(folder_list_draws(true, true));
        assert!(!folder_list_draws(true, false)); // opening waits for the split
        assert!(folder_list_draws(false, true)); // closing draws its final frame
        assert!(!folder_list_draws(false, false));
    }

    #[test]
    fn backup_queue_ratio_serializes_as_a_short_decimal() {
        let mut config = crate::config::Config::default();
        config.backup_queue_ratio = 0.35;

        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized
            .lines()
            .any(|line| line == "backup_queue_ratio = 0.35"));
        assert!(!serialized.contains("0.349999"));
        let back: crate::config::Config = toml::from_str(&serialized).unwrap();
        assert_eq!(back.backup_queue_ratio(), 0.35);
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn keep_one_section_open_enforces_the_invariant() {
        assert!(keep_one_section_open(false, false));
        assert!(!keep_one_section_open(false, true));
        assert!(keep_one_section_open(true, false));
        assert!(keep_one_section_open(true, true));
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn last_open_section_toggle_is_disabled() {
        assert!(!section_toggle_enabled(true, false));
        assert!(section_toggle_enabled(true, true));
        assert!(!section_toggle_enabled(false, true));
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn section_header_click_toggles_only_when_allowed() {
        assert_eq!(section_toggle_after_click(true, true, true, true), Some(false));
        assert_eq!(section_toggle_after_click(true, true, false, true), Some(true));
        assert_eq!(section_toggle_after_click(true, false, true, false), None);
        assert_eq!(section_toggle_after_click(false, true, true, true), None);
        assert_eq!(section_toggle_after_click(true, true, true, false), None);
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn collapsed_allocation_ignores_ratio_and_reopening_applies_it() {
        let free = 900.0;
        let collapsed_at_min_ratio = page_heights_for_sections(free, false, true, 0.2);
        let collapsed_at_max_ratio = page_heights_for_sections(free, false, true, 0.8);
        assert_eq!(
            collapsed_at_min_ratio,
            (0.0, free - FOLDER_ONLY_BELOW_H)
        );
        assert_eq!(collapsed_at_min_ratio, collapsed_at_max_ratio);

        // The collapsed allocator ignores the ratio; when both sections reopen,
        // it applies whichever saved value the caller supplies.
        let reopened_at_min_ratio = page_heights_for_sections(free, true, true, 0.2);
        let reopened_at_saved_ratio = page_heights_for_sections(free, true, true, 0.7);
        assert_ne!(reopened_at_min_ratio, reopened_at_saved_ratio);
        assert_eq!(
            reopened_at_saved_ratio,
            split_viewports(backup_split_splittable(free), 0.7)
        );
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
    fn queue_columns_fill_the_table_and_keep_the_name_at_least_80() {
        // Issue #30: the table ends exactly at `content - scroll bar`, the four
        // cells plus their three gaps add up to the table width, and the name
        // column never drops below 80 px — not even at the minimum window
        // (760 - 220 sidebar - 2 * 16 page gutter = 508 content px).
        for content_w in [1400.0, 1000.0, 760.0, 508.0] {
            let cols = queue_columns(content_w, 8.0, 18.0);
            assert_eq!(cols.table_w, content_w - 18.0, "content {content_w}");
            assert!(cols.name_w >= QUEUE_MIN_NAME_W, "content {content_w}");
            assert_eq!(
                cols.folder_w + cols.name_w + cols.size_w + cols.status_w + 3.0 * 8.0,
                cols.table_w,
                "columns must add up exactly at content {content_w}"
            );
            assert_eq!(cols.size_w, 80.0);
            assert_eq!(cols.status_w, 140.0);
        }
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
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
    #[cfg(feature = "legacy-eframe")]
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
    #[cfg(feature = "legacy-eframe")]
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
    #[cfg(feature = "legacy-eframe")]
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
    #[cfg(feature = "legacy-eframe")]
    fn queue_layout_of_an_empty_queue_is_empty() {
        assert!(queue_layout(&[]).is_empty());
    }

    #[test]
    #[cfg(feature = "legacy-eframe")]
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

        publish_state(&mut plan, &shared, "test".to_string(), folder_paths, None);

        #[cfg(feature = "legacy-eframe")]
        {
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
        #[cfg(not(feature = "legacy-eframe"))]
        assert_eq!(
            shared.lock().unwrap().folder_paths,
            HashMap::from([(0, PathBuf::from("/src/published-folder"))]),
            "the published state carries the plan-time folder path"
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
    fn scheduled_upload_uses_injected_services_for_present_and_empty_targets() {
        let root = scratch_scan_folder("scheduled-helper");
        let first_path = root.join("first");
        let empty_path = root.join("empty");
        let missing_path = root.join("missing");
        let mut store = FolderStore {
            folders: vec![
                folders::BackupFolder {
                    status: FolderStatus::NotFound,
                    conflict_resolution: ConflictResolution::SaveAsNew,
                    ..folders::BackupFolder::new(first_path.clone())
                },
                folders::BackupFolder {
                    status: FolderStatus::Paused,
                    dedup: false,
                    conflict_resolution: ConflictResolution::Ignore,
                    ..folders::BackupFolder::new(empty_path.clone())
                },
                folders::BackupFolder::new(missing_path),
            ],
        };
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let (paused, pause_state) = watch::channel(true);
        let mut config = Config {
            paused: true,
            ..Config::default()
        };
        let builder_called = Cell::new(false);
        let source_checks = Cell::new(0);
        let save_calls = Cell::new(0);
        let selected_indices = [0, 1];

        let launch = prepare_scheduled_upload_with(
            &mut store,
            vec![0, 1, 2],
            &mut config,
            shared.clone(),
            paused,
            "synthetic album".to_owned(),
            true,
            false,
            |path| {
                source_checks.set(source_checks.get() + 1);
                path == first_path.as_path() || path == empty_path.as_path()
            },
            |store, indices| {
                builder_called.set(true);
                assert_eq!(indices, selected_indices);
                assert_eq!(store.folders[0].status, FolderStatus::Idle);
                assert_eq!(store.folders[1].status, FolderStatus::Idle);
                assert_eq!(store.folders[2].status, FolderStatus::NotFound);
                // No files are returned: the helper's empty-plan fallback must
                // still retain both present destinations and their options.
                None
            },
            |_| save_calls.set(save_calls.get() + 1),
        )
        .expect("present targets produce an empty destination plan");

        assert!(builder_called.get());
        assert_eq!(source_checks.get(), 3);
        assert_eq!(save_calls.get(), 0, "persist_store=false must not save");
        assert_eq!(store.folders[0].status, FolderStatus::Idle);
        assert_eq!(store.folders[1].status, FolderStatus::Idle);
        assert_eq!(store.folders[2].status, FolderStatus::NotFound);
        assert_eq!(launch.items.len(), 0);
        assert_eq!(launch.album, "synthetic album");
        assert_eq!(
            launch.destination_roots,
            HashMap::from([(0, first_path.clone()), (1, empty_path.clone())])
        );
        assert_eq!(
            launch.folder_opts.get(&0),
            Some(&(true, ConflictResolution::SaveAsNew))
        );
        assert_eq!(
            launch.folder_opts.get(&1),
            Some(&(false, ConflictResolution::Ignore))
        );
        assert!(!config.paused);
        assert!(!*pause_state.borrow());
        {
            let state = shared.lock().unwrap();
            assert!(state.active);
            assert!(state.slow_mode);
            assert_eq!(state.total, 0);
            assert!(state.queue.is_empty());
            assert_eq!(state.folder_total, HashMap::from([(0, 0), (1, 0)]));
            assert_eq!(
                state.folder_paths,
                HashMap::from([(0, first_path), (1, empty_path)])
            );
        }
    }

    #[test]
    fn scheduled_upload_logs_missing_sources_without_persisting_or_unpausing() {
        let root = scratch_scan_folder("scheduled-missing");
        let path = root.join("missing");
        let mut store = store_with(&path.to_string_lossy(), FolderStatus::Idle);
        let shared = Arc::new(Mutex::new(UploadShared::default()));
        let (paused, pause_state) = watch::channel(true);
        let mut config = Config {
            paused: true,
            ..Config::default()
        };
        let builder_called = Cell::new(false);
        let save_calls = Cell::new(0);

        let launch = prepare_scheduled_upload_with(
            &mut store,
            vec![0],
            &mut config,
            shared.clone(),
            paused,
            String::new(),
            false,
            false,
            |_| false,
            |_, indices| {
                builder_called.set(true);
                assert!(indices.is_empty());
                None
            },
            |_| save_calls.set(save_calls.get() + 1),
        );

        assert!(launch.is_none());
        assert!(builder_called.get());
        assert_eq!(save_calls.get(), 0, "persist_store=false must not save");
        assert_eq!(store.folders[0].status, FolderStatus::NotFound);
        assert!(config.paused);
        assert!(*pause_state.borrow());
        let state = shared.lock().unwrap();
        assert!(!state.active);
        assert!(state
            .log
            .last()
            .is_some_and(|message| message.contains("Quellordner")));
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
        publish_state(&mut plan, &shared, "test".to_string(), folder_paths, None);
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
            None,
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

    #[test]
    fn watcher_store_retry_waits_for_the_poll_tick() {
        let failed_at = Instant::now();
        let mut retry = WatchStoreRetry::default();
        assert!(with_watch_store(&mut retry, || failed_at, || None, |_| ()).is_none());

        let before_tick = failed_at + WATCH_POLL - Duration::from_nanos(1);
        let mut loaded = false;
        assert!(with_watch_store(
            &mut retry,
            || before_tick,
            || {
                loaded = true;
                Some(FolderStore::default())
            },
            |_| (),
        )
        .is_none());
        assert!(!loaded, "a wake before the poll tick must not retry the read");

        let at_tick = failed_at + WATCH_POLL;
        assert!(retry.eligible(at_tick));
        assert!(with_watch_store(
            &mut retry,
            || at_tick,
            || {
                loaded = true;
                Some(FolderStore::default())
            },
            |_| (),
        )
        .is_some());
        assert!(loaded);
        assert!(retry.remaining(at_tick).is_none());
    }

    #[test]
    fn watcher_store_retry_starts_after_a_slow_failed_load_completes() {
        let started_at = Instant::now();
        let completed_at = started_at + WATCH_POLL;
        let now = Cell::new(started_at);
        let mut retry = WatchStoreRetry::default();

        assert!(with_watch_store(
            &mut retry,
            || now.get(),
            || {
                // Model a load that consumes a full poll interval before failing.
                now.set(completed_at);
                None
            },
            |_| (),
        )
        .is_none());
        assert_eq!(retry.remaining(completed_at), Some(WATCH_POLL));

        // At the old, pre-load deadline the retry is still parked.
        let mut loaded = false;
        assert!(with_watch_store(
            &mut retry,
            || now.get(),
            || {
                loaded = true;
                Some(FolderStore::default())
            },
            |_| (),
        )
        .is_none());
        assert!(!loaded, "a slow failed load must start a fresh poll delay");

        let before_deadline = completed_at + WATCH_POLL - Duration::from_nanos(1);
        now.set(before_deadline);
        assert!(!retry.eligible(now.get()));

        let deadline = completed_at + WATCH_POLL;
        now.set(deadline);
        assert!(with_watch_store(
            &mut retry,
            || now.get(),
            || {
                loaded = true;
                Some(FolderStore::default())
            },
            |_| (),
        )
        .is_some());
        assert!(loaded);
    }

    #[test]
    fn failed_watcher_store_load_leaves_parked_work_untouched() {
        let now = Instant::now();
        let mut retry = WatchStoreRetry::default();
        let mut pending = files(0..2);
        let mut pending_seen: HashSet<PathBuf> = pending.iter().map(|(_, p)| p.clone()).collect();
        let mut pending_scans = vec![PathBuf::from("/photos/scan")];
        let mut event_paths = vec![PathBuf::from("/photos/new.jpg")];
        let mut event_seen: HashSet<PathBuf> = event_paths.iter().cloned().collect();

        let result = with_watch_store(&mut retry, || now, || None, |_| {
            pending.clear();
            pending_seen.clear();
            pending_scans.clear();
            event_paths.clear();
            event_seen.clear();
        });

        assert!(result.is_none());
        assert_eq!(pending, files(0..2));
        assert_eq!(pending_seen.len(), 2);
        assert_eq!(pending_scans, vec![PathBuf::from("/photos/scan")]);
        assert_eq!(event_paths, vec![PathBuf::from("/photos/new.jpg")]);
        assert_eq!(event_seen, HashSet::from([PathBuf::from("/photos/new.jpg")]));
    }

    #[test]
    fn refreshing_parked_paths_uses_fresh_indices_and_keeps_dedup_exact() {
        let store = FolderStore {
            folders: vec![
                folders::BackupFolder::new(PathBuf::from("/photos/b")),
                folders::BackupFolder::new(PathBuf::from("/photos/a")),
            ],
        };
        let a = PathBuf::from("/photos/a/one.jpg");
        let b = PathBuf::from("/photos/b/two.jpg");
        let mut pending = vec![(0, a.clone()), (9, b.clone()), (3, a.clone())];
        let mut seen = HashSet::from([a.clone(), b.clone()]);

        refresh_pending_folders(&store, &mut pending, &mut seen);

        assert_eq!(pending, vec![(1, a.clone()), (0, b.clone())]);
        assert_eq!(seen, HashSet::from([a, b]));
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
        // The one-time queue has no `folders.toml` entry
        // (`crate::platform::app_shared::ONE_TIME_IDX`), so there is nothing to
        // write — it stays run-only by design.
        let root = scratch_folder("onetime");
        let mut store = asking_store(&root);
        let file = root.join("photo.jpg");

        assert!(!remember_conflict_in_folder(
            &mut store,
            crate::platform::app_shared::ONE_TIME_IDX,
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
        assert!(
            remember_folder(&store, crate::platform::app_shared::ONE_TIME_IDX, &own_file)
                .is_none()
        );
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
            conflict_remember_label(&store, crate::platform::app_shared::ONE_TIME_IDX, &own_file),
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

        #[cfg(feature = "legacy-eframe")]
        {
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
        #[cfg(not(feature = "legacy-eframe"))]
        assert_eq!(
            shared.lock().unwrap().folder_paths.get(&0),
            Some(&PathBuf::from("/src/injected-folder")),
            "the injected path is available to the shared run state"
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
