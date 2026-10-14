use crate::app::{Page, ONE_TIME_IDX, PrimePhotoApp};
use crate::backup_config::{BackupConfigForm, ConfigFormMode};
use crate::config::Config;
use crate::dedup::DedupIndex;
use crate::folders::{self, ConflictResolution, FolderStatus, FolderStore};
use crate::i18n::{tr, tr1, tr2, tr3};
use crate::icons;
use crate::theme;
use crate::upload::{
    outcome_text, run_upload, status_text, upload_progress_text, FileState, QueueEntry,
    UploadShared,
};
use eframe::egui::{self, RichText};
// NOTE: `notify` is the filesystem-event crate — not `crate::notify`, the
// desktop-notification module (they are unrelated and both stay).
use notify;
use notify::Watcher;
use rfd::FileDialog;
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

    /// File size of one entry, counted once into the run's byte total. Never
    /// panics: unreadable files just do not contribute (the ETA then falls back
    /// to the file-count rate).
    fn note_size(&mut self, path: &Path) {
        if let Ok(meta) = std::fs::metadata(path) {
            self.bytes_total += meta.len();
            self.sized_total += 1;
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
                plan.queue.push(QueueEntry {
                    folder: i,
                    name,
                    state: FileState::Waiting,
                });
                plan.note_size(&p);
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
            plan.queue.push(QueueEntry {
                folder: idx,
                name,
                state: FileState::Waiting,
            });
            plan.note_size(&p);
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
            plan.queue.push(QueueEntry {
                folder: *i,
                name,
                state: FileState::Waiting,
            });
            plan.note_size(p);
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
    // previous run must never silently overwrite files in this one.
    s.pending_conflict = None;
    s.conflict_answer = None;
    s.conflict_remember = false;
    s.conflict_override = None;
    s.log(msg);
}

/// Mark the plan's persisted folders as uploading (folder-targets only). The
/// one-time target is transient, so it never reaches this and never gets saved.
fn publish_folder_status(plan: &UploadPlan, store: &mut FolderStore) {
    for (&i, &count) in &plan.per_folder {
        if let Some(f) = store.folders.get_mut(i) {
            f.status = FolderStatus::Uploading;
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
            index.record_folder_scan(&scan.path, scan.scanned_max);
        }
    });
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
    last_config: Config,
    concurrency: usize,
    limit_kbps: u32,
    album: String,
) {
    // Freshest settings first: the supervisor is detached from the UI struct, so
    // the persisted config is the only source that reflects changes made after
    // it was spawned (login/logout, the `background_sync` toggle).
    let cfg = Config::load().unwrap_or(last_config);
    if !can_start(&cfg, &paused, &shared) {
        return;
    }

    // Enabled folders come from the persisted store (saved on every change).
    let mut store = FolderStore::load();
    let indices = store.enabled_indices();
    if indices.is_empty() {
        return;
    }

    // Same SQLite, same connection style as the MD5 index (`src/dedup.rs`).
    let index = DedupIndex::open();
    let mut wanted: Vec<(usize, Option<i64>)> = Vec::new();
    let mut scans: Vec<FolderScan> = Vec::new();
    for i in indices {
        let Some(f) = store.folders.get(i) else {
            continue;
        };
        let watermark = index.folder_max_mtime(&f.path);
        // A folder we cannot even stat (gone, ejected, no access) answers
        // `None`: it is left alone rather than treated as "unchanged", and
        // scanning it would find nothing anyway.
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
    let store = if cfg.background_sync
        && !cfg.session_id.is_empty()
        && !cfg.cookie_x_acb.is_empty()
    {
        Some(FolderStore::load())
    } else {
        None
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
        // from the UI struct, so the persisted files are its only source).
        let cfg = Config::load().unwrap_or(last_config.clone());
        sync_watchers(&mut watchers, &tx, &cfg);

        // Retry what was remembered while a run was busy or paused: no new burst,
        // just the parked stock — merged with itself (a no-op) so no path is taken
        // up twice, then started if a run is allowed now.
        if !pending.is_empty() {
            let mut store = FolderStore::load();
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
        let mut store = FolderStore::load();
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
        // key, `tr` is applied at render time. Dedup and conflict handling keep
        // the folder defaults (docs/SPEC_ONE_TIME_UPLOAD.md §10.5).
        self.one_time = Some(folders::BackupFolder {
            path: display_path,
            name: "Einmalige Sicherung".to_string(),
            enabled: true,
            dedup: true,
            conflict_resolution: ConflictResolution::Ignore,
            total_files: files.len(),
            uploaded_files: 0,
            last_updated: String::new(),
            status: FolderStatus::Uploading,
        });

        // One plan, one synthetic folder index: the counters, the live queue
        // and `folder_opts` look exactly like a single-folder run.
        let mut plan = UploadPlan::from_files(
            ONE_TIME_IDX,
            true,
            ConflictResolution::Ignore,
            files,
        );
        let concurrency = if self.slow_mode { 1 } else { self.concurrency };
        let limit = if self.limit_enabled {
            self.limit_kbps
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
        let pending = { self.upload.lock().unwrap().pending_conflict.clone() };
        let Some(file_name) = pending else {
            return;
        };
        // The worker only polls the shared state; it does not paint frames, so
        // keep the dialog (and the repaint that shows it) alive from here.
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        let mut remember = self.upload.lock().unwrap().conflict_remember;
        let mut answer: Option<ConflictResolution> = None;
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
                ui.checkbox(&mut remember, tr("Nicht noch einmal fragen"));
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
            let mut s = self.upload.lock().unwrap();
            s.conflict_remember = remember;
            s.conflict_answer = Some(a);
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
        let cfg = self.config.clone();
        let concurrency = if self.slow_mode { 1 } else { self.concurrency };
        let limit = if self.limit_enabled {
            self.limit_kbps
        } else {
            0
        };
        let album = self.album_name.clone();
        self.rt.spawn(async move {
            background_sync_pass(
                shared.clone(),
                paused.clone(),
                cfg.clone(),
                concurrency,
                limit,
                album.clone(),
            )
            .await;
            // From then on the OS events decide.
            watch_loop(shared, paused, cfg, concurrency, limit, album).await;
        });
    }

    pub(crate) fn finalize_upload(&mut self) {
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
            if let Some(&done) = folder_done.get(&i) {
                // The run's own file count goes with it: `uploaded_files` alone
                // was written back, which left `total_files` from an earlier scan
                // or run and made the row read "480 von 150". Only targets that
                // really took part in the run are touched.
                if let Some(&total) = folder_total.get(&i) {
                    f.total_files = total;
                }
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
        // The transient one-time target is settled the same way, but from the
        // synthetic `ONE_TIME_IDX` counters and onto `self.one_time` — nothing
        // is written back to the store here.
        if let Some(one) = self.one_time.as_mut() {
            if let Some(&done) = folder_done.get(&ONE_TIME_IDX) {
                if let Some(&total) = folder_total.get(&ONE_TIME_IDX) {
                    one.total_files = total;
                }
                one.uploaded_files = done;
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
                self.add_backup_folder();
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

        // Live per-file queue, grouped by backup folder
        // (docs/SPEC_UPLOAD_PROGRESS.md §4.1/§4.3). The grouped structure is
        // cached and only rebuilt when the run's *file states* changed
        // (`UploadShared::rev`): a 5 000-file run otherwise cloned every file
        // name on every frame, for nothing — only the counters move.
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
                            .map(|f| crate::text::nfc(&f.name))
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
        if !self.queue_cache.1.is_empty() {
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
            egui::CollapsingHeader::new(tr1("Warteschlange ({})", remaining)).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        // Cap the rendered rows so a very large run stays cheap.
                        let mut shown = 0usize;
                        for (folder, files) in &self.queue_cache.1 {
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
                                // The state label is one of five fixed strings
                                // (translated once per row, not allocated).
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
            let mut rows: Vec<Row> = self
                .store
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
                rows.push(Row {
                    idx: ONE_TIME_IDX,
                    name: tr("Einmalige Sicherung").to_string(),
                    path: one.path.clone(),
                    enabled: true,
                    status: one.status,
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
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                // While a file drag hovers the window, the backup page keeps
                // its hands off: no clicks out from under the overlay.
                if self.dropping_files {
                    ui.disable();
                }
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
                                    ui.label(RichText::new(crate::text::nfc(&r.name)).strong().size(15.0));
                                    ui.label(
                                        RichText::new(tr1(
                                            "{}% hochgeladen",
                                            format!("{:.0}", pct * 100.0),
                                        ))
                                        .size(12.0),
                                    );
                                    // The target's own outcome breakdown (user
                                    // decision 2026-10-14). Only shown when the
                                    // run produced at least one result for it —
                                    // an untouched row (all zero) stays quiet.
                                    let outcome =
                                        outcome_text(r.new, r.existing, r.unchanged, r.failed);
                                    if !outcome.is_empty() {
                                        ui.label(theme::muted(outcome).size(11.0));
                                    }
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
                                            theme::muted(crate::text::nfc(
                                                &r.path.display().to_string(),
                                            ))
                                            .size(11.0),
                                        );
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
                                                        .image_tint_follows_text_color(true),
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
                                                        .image_tint_follows_text_color(true),
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

        // Footer: exactly **one** aggregated progress display over *all* backup
        // targets — the persisted folders plus the transient one-time row
        // (`rows` above). It replaces the old header bar, which only showed the
        // currently running run, and it stays visible while idle (user decision
        // 2026-10-14). Without any target nothing is drawn — the empty-folder
        // hint above stays untouched.
        if !rows.is_empty() {
            ui.add_space(8.0);
            let (sum_done, sum_total) = aggregate_progress(&rows);
            let frac = if sum_total > 0 {
                (sum_done as f32 / sum_total as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            // The byte/ETA detail only exists for a live run; `rows` is a plain
            // snapshot, so the shared state is locked once more here (no guard
            // is held at this point). The outcome counters are read here too —
            // they are already the aggregate over every target (maintained
            // directly by the workers, no second summation of the rows).
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
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(240.0)
                        .text(text),
                );
            });
            // Extra result line (user decision 2026-10-14): the run's outcome
            // breakdown over **all** targets. It is a result line, not a second
            // progress bar, and it stays hidden while every counter is 0 (before
            // any run, or freshly reset) — no empty or misleading line.
            if !outcome.is_empty() {
                ui.add_space(2.0);
                ui.label(theme::muted(outcome).size(11.0));
            }
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
}
