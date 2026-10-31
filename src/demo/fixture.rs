//! The fixture data a demo run renders, plus the shared builders the scenario
//! modules use.

use super::Scenario;
use super::live_scenarios::{many_folders, mixed, uploading};
use super::settled_scenarios::{all_done, empty, failures, settings, signed_out};
use crate::app::{Page, SettingsTab};
use crate::config::Config;
use crate::folders::{BackupFolder, ConflictResolution, ContentFilter, FolderStatus, FolderStore};
use crate::upload::{FileState, QueueEntry, UploadShared};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The data a demo run renders: everything the real app would have loaded.
///
/// `source_exists` is the faked `path.is_dir()` for `store.folders[i]` — it lets
/// a fixture show "Ordner nicht gefunden" without any directory on disk (and
/// without touching `/Volumes`, which must never be created or probed).
pub(crate) struct Fixture {
    pub(crate) config: Config,
    pub(crate) store: FolderStore,
    pub(crate) upload: UploadShared,
    pub(crate) one_time: Option<BackupFolder>,
    pub(crate) paused: bool,
    pub(crate) page: Page,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) account_name: Option<String>,
    pub(crate) config_error: Option<String>,
    pub(crate) source_exists: Vec<bool>,
}

/// Build the fixture set for one scenario. Pure: no IO, no globals, no clock
/// beyond the relative `Instant`s the progress/ETA display needs.
pub(crate) fn fixture(scenario: Scenario) -> Fixture {
    match scenario {
        Scenario::Mixed => mixed(),
        Scenario::Uploading => uploading(false),
        Scenario::Paused => uploading(true),
        Scenario::AllDone => all_done(),
        Scenario::Failures => failures(),
        Scenario::Empty => empty(),
        Scenario::ManyFolders => many_folders(),
        Scenario::Settings => settings(),
        Scenario::SignedOut => signed_out(),
    }
}

// ---------------------------------------------------------------------------
// Shared builders
// ---------------------------------------------------------------------------

/// Plausible macOS photo locations. Nothing here is ever created or probed —
/// existence comes from [`Fixture::source_exists`].
pub(super) const PICTURES: &str = "/Users/beispiel/Bilder";

/// Sub-directories a scan would have descended into; they make the queue's
/// name column and hover tooltip look like a real folder tree.
const SUBDIRS: &[&str] = &[
    "2024-07",
    "2024-08",
    "Reisen/Island",
    "Familie/2023",
    "Kamera-Import",
];

/// Queue percentages of the actively transferring rows, including the exact
/// values from the reference screenshot (12, 100, 21, 8).
pub(super) const UPLOADING_PCT: [u64; 9] = [12, 100, 21, 8, 64, 37, 3, 45, 72];

/// A camera-like, unique file name for queue row `i`. Deliberately built from
/// the index (no random source), so the fixture **data** of a scenario is
/// reproducible; the animated spinner still differs between two captures.
fn photo_name(i: usize) -> String {
    match i % 5 {
        0 => format!("IMG_{}.HEIC", 2043 + i),
        1 => format!("DSC_{:04}.ARW", 842 + i),
        2 => format!("31_untitled_{:02}.cr3", i),
        3 => format!("P1{:06}.RW2", 2043 + i),
        _ => format!("_{:04}.CR2", 4821 + i),
    }
}

/// 13–16 MB — the size range of the mixed RAW/HEIC set the reference
/// screenshot shows.
fn file_size(i: usize) -> u64 {
    13_500_000 + (i % 7) as u64 * 400_000 + (i % 3) as u64 * 50_000
}

pub(super) fn folder(
    path: &str,
    total: usize,
    uploaded: usize,
    status: FolderStatus,
    last_updated: &str,
) -> BackupFolder {
    let path = PathBuf::from(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    BackupFolder {
        path,
        name,
        enabled: true,
        dedup: true,
        // The application default, exactly like a freshly created target.
        conflict_resolution: ConflictResolution::Replace,
        content_filter: ContentFilter::Photos,
        total_files: total,
        uploaded_files: uploaded,
        last_updated: last_updated.to_string(),
        status,
    }
}

pub(super) fn entry(
    i: usize,
    folder: usize,
    folder_path: &Path,
    state: FileState,
    pct: u64,
) -> QueueEntry {
    let name = photo_name(i);
    let size = file_size(i);
    let bytes_sent = match state {
        FileState::Done => size,
        FileState::Uploading => size * pct.min(100) / 100,
        _ => 0,
    };
    QueueEntry {
        folder,
        path: folder_path.join(SUBDIRS[i % SUBDIRS.len()]).join(&name),
        name,
        state,
        size,
        bytes_sent,
    }
}

/// `(size of the whole run, bytes settled)` of a queue. "Settled" counts both
/// finished rows and the bytes already handed to the transport, which is what
/// the real ETA estimate uses.
fn byte_totals(queue: &[QueueEntry]) -> (u64, u64) {
    let total = queue.iter().map(|e| e.size).sum();
    let settled = queue
        .iter()
        .filter(|e| e.state != FileState::Waiting)
        .map(|e| match e.state {
            FileState::Done => e.size,
            _ => e.bytes_sent,
        })
        .sum();
    (total, settled)
}

/// Derive the run's per-folder bookkeeping **from the queue rows**.
///
/// The real plan publishes `folder_paths`, `folder_total`, `folder_done` and the
/// outcome counters **together** (see `backup::publish_plan`). A fixture that
/// filled only `folder_paths` would make `backup_ui` treat the folder as "not in
/// this run" and render its persisted state next to live queue rows — a state
/// the real app cannot produce. Deriving every map from the same rows also
/// guarantees the card counts match the table.
///
/// Only folders that really have queue rows get an entry: a missing source (no
/// scan, no rows) must stay `NotFound` and must not claim a live run.
fn apply_queue_counters(upload: &mut UploadShared, folders: &[BackupFolder]) {
    for (idx, folder) in folders.iter().enumerate() {
        let rows: Vec<&QueueEntry> = upload
            .queue
            .iter()
            .filter(|entry| entry.folder == idx)
            .collect();
        if rows.is_empty() {
            continue;
        }
        upload.folder_paths.insert(idx, folder.path.clone());
        upload.folder_total.insert(idx, rows.len());
        let done = rows
            .iter()
            .filter(|entry| entry.state == FileState::Done)
            .count();
        upload.folder_done.insert(idx, done);
        // Every finished row of a fixture is a real upload: no 409/unchanged
        // rows are modelled, so `new` mirrors `done` exactly.
        if done > 0 {
            upload.folder_new.insert(idx, done);
        }
        let failed = rows
            .iter()
            .filter(|entry| entry.state == FileState::Failed)
            .count();
        if failed > 0 {
            upload.folder_failed.insert(idx, failed);
        }
    }
}

/// Shared tail of the live-run fixtures: the counters, the ETA basis and a
/// short, plausible log tail. `started` is relative so the ETA display is real
/// (and stable, because the byte-based estimate does not depend on wall time).
///
/// The run counters and the per-folder maps are derived from `queue`, never
/// passed in — that is what keeps "37 von 187", the queue length and the folder
/// cards consistent by construction.
pub(super) fn published_run(folders: &[BackupFolder], queue: Vec<QueueEntry>) -> UploadShared {
    let done = queue
        .iter()
        .filter(|entry| entry.state == FileState::Done)
        .count();
    let failed = queue
        .iter()
        .filter(|entry| entry.state == FileState::Failed)
        .count();
    let (bytes_total, bytes_done) = byte_totals(&queue);
    let current = queue
        .iter()
        .find(|entry| entry.state == FileState::Uploading)
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    let now = Instant::now();
    // Assign field by field: `UploadShared` has private bookkeeping (the
    // destination-root failure markers) that a functional update may not touch
    // from this module.
    let mut upload = UploadShared::default();
    upload.active = true;
    upload.total = queue.len();
    upload.done = done;
    upload.failed = failed;
    upload.started = Some(now - Duration::from_secs(60));
    upload.current = current;
    upload.queue = queue;
    upload.bytes_total = bytes_total;
    upload.bytes_done = bytes_done;
    upload.net_bytes = bytes_done;
    upload.net_started = Some(now - Duration::from_secs(42));
    upload.upload_secs = 42.0;
    apply_queue_counters(&mut upload, folders);
    upload.log = vec![
        format!(
            "{} Ordner geprüft, {} Dateien gefunden.",
            folders.len(),
            upload.total
        ),
        format!("{} von {} abgeschlossen.", upload.done, upload.total),
    ];
    upload
}

/// A **finished** run's leftovers: the real app keeps the last run's per-folder
/// counters in `UploadShared` after `active` dropped (they are only replaced at
/// the start of the next run), and that is what the settled rows render. The
/// counters come from the same queue rows as a live run, so a "Fehler" card and
/// its failed table rows still agree.
pub(super) fn settled_run(folders: &[BackupFolder], queue: Vec<QueueEntry>) -> UploadShared {
    let mut upload = UploadShared::default();
    upload.total = queue.len();
    upload.done = queue
        .iter()
        .filter(|entry| entry.state == FileState::Done)
        .count();
    upload.failed = queue
        .iter()
        .filter(|entry| entry.state == FileState::Failed)
        .count();
    upload.queue = queue;
    apply_queue_counters(&mut upload, folders);
    upload
}

// ---------------------------------------------------------------------------
// Config factories
// ---------------------------------------------------------------------------

/// The persisted part of the demo config: German marketplace, German UI, a
/// generated device serial — the same base a real install has.
pub(super) fn base_config() -> Config {
    let mut config = Config::default();
    config.country = "de".to_string();
    config.zone = "eu".to_string();
    config.language = Some("de".to_string());
    config.device_serial = "0123456789abcdef0123456789abcdef".to_string();
    config
}

/// Signed in: the session fields are dummy values, but non-empty, so every
/// signed-in scenario takes the real main-window path.
pub(super) fn signed_in_config() -> Config {
    let mut config = base_config();
    config.session_id = "demo-session-id".to_string();
    config.cookie_x_acb = "demo-x-acb".to_string();
    config.cookie_at_acb = "demo-at-acb".to_string();
    config.cookie_ubid_acb = "demo-ubid-acb".to_string();
    config.cookie_x_amz_access_token = "demo-access-token".to_string();
    config
}
