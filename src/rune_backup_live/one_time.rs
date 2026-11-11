use super::LiveState;
use crate::config::Config;
use crate::dedup::DedupIndex;
use crate::folders::{self, BackupFolder, ConflictResolution, ContentFilter, FolderStatus};
use crate::i18n::{tr, tr1};
use crate::platform::app_shared::ONE_TIME_IDX;
use crate::upload::{FileState, QueueEntry, UploadShared, run_upload};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropPathKind {
    File,
    Directory,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropError {
    Busy,
    SignedOut,
    Empty,
}

struct OneTimePlan {
    items: Vec<(usize, usize, PathBuf)>,
    queue: Vec<QueueEntry>,
    folder_opts: HashMap<usize, (bool, ConflictResolution)>,
    bytes_total: u64,
    sized_total: usize,
}

impl OneTimePlan {
    fn from_files(
        files: Vec<PathBuf>,
        dedup: bool,
        conflict: ConflictResolution,
        mut file_size: impl FnMut(&Path) -> Option<u64>,
    ) -> Self {
        let mut plan = OneTimePlan {
            items: Vec::with_capacity(files.len()),
            queue: Vec::with_capacity(files.len()),
            folder_opts: HashMap::from([(ONE_TIME_IDX, (dedup, conflict))]),
            bytes_total: 0,
            sized_total: 0,
        };
        for path in files {
            let row_index = plan.items.len();
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size = file_size(&path);
            let size = if let Some(size) = size {
                plan.bytes_total += size;
                plan.sized_total += 1;
                size
            } else {
                0
            };
            plan.queue.push(QueueEntry {
                folder: ONE_TIME_IDX,
                name,
                state: FileState::Waiting,
                size,
                bytes_sent: 0,
            });
            plan.items.push((row_index, ONE_TIME_IDX, path));
        }
        plan
    }

    fn count(&self) -> usize {
        self.items.len()
    }
}

fn accepts_drop_path(path: &Path) -> bool {
    folders::is_supported_image(path) || path.is_dir()
}

fn filter_drop_paths(
    paths: &[PathBuf],
    mut classify: impl FnMut(&Path) -> DropPathKind,
    mut scan_directory: impl FnMut(&Path) -> Vec<PathBuf>,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        match classify(path) {
            DropPathKind::File if folders::is_supported_image(path) => files.push(path.clone()),
            DropPathKind::Directory => {
                files.extend(
                    scan_directory(path)
                        .into_iter()
                        .filter(|candidate| folders::is_supported_image(candidate)),
                );
            }
            DropPathKind::File | DropPathKind::Other => {}
        }
    }
    files
}

fn supported_images(files: Vec<PathBuf>) -> Vec<PathBuf> {
    files
        .into_iter()
        .filter(|path| folders::is_supported_image(path))
        .collect()
}

fn collect_after_guards(
    paths: &[PathBuf],
    busy: bool,
    logged_in: bool,
    session_invalid: bool,
    classify: impl FnMut(&Path) -> DropPathKind,
    scan_directory: impl FnMut(&Path) -> Vec<PathBuf>,
) -> Result<Vec<PathBuf>, DropError> {
    if busy {
        return Err(DropError::Busy);
    }
    if !logged_in || session_invalid {
        return Err(DropError::SignedOut);
    }
    let files = filter_drop_paths(paths, classify, scan_directory);
    if files.is_empty() {
        Err(DropError::Empty)
    } else {
        Ok(files)
    }
}

fn can_resume(
    busy: bool,
    logged_in: bool,
    session_invalid: bool,
    background_sync: bool,
    paused: bool,
) -> bool {
    !busy && logged_in && !session_invalid && background_sync && !paused
}

fn common_parent(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut common = paths.first()?.parent()?.to_path_buf();
    for path in paths.iter().skip(1) {
        let parent = path.parent()?;
        while !parent.starts_with(&common) {
            common = common.parent()?.to_path_buf();
        }
    }
    (!common.as_os_str().is_empty()).then_some(common)
}

fn transient_target(config: &Config, display_path: PathBuf, total: usize) -> BackupFolder {
    let (dedup, conflict_resolution) = folders::default_options(config);
    BackupFolder {
        path: display_path,
        name: "Einmalige Sicherung".to_owned(),
        enabled: true,
        dedup,
        conflict_resolution,
        content_filter: ContentFilter::Photos,
        total_files: total,
        uploaded_files: 0,
        last_updated: String::new(),
        status: FolderStatus::Uploading,
    }
}

fn publish_state(
    plan: &mut OneTimePlan,
    shared: &Arc<Mutex<UploadShared>>,
    slow_mode: bool,
) {
    let mut state = shared.lock().unwrap();
    state.slow_mode = slow_mode;
    state.active = true;
    state.started = Some(Instant::now());
    state.done = 0;
    state.failed = 0;
    state.current.clear();
    state.log.clear();
    state.folder_total = HashMap::from([(ONE_TIME_IDX, plan.count())]);
    state.folder_paths.clear();
    state.folder_done.clear();
    state.folder_failed.clear();
    state.uploaded_new = 0;
    state.conflict_skipped = 0;
    state.unchanged_skipped = 0;
    state.folder_new.clear();
    state.folder_conflict.clear();
    state.folder_unchanged.clear();
    state.uploaded_ids.clear();
    state.total = plan.count();
    state.queue = std::mem::take(&mut plan.queue);
    state.bytes_total = if plan.sized_total == plan.count() {
        plan.bytes_total
    } else {
        0
    };
    state.bytes_done = 0;
    state.net_bytes = 0;
    state.net_started = None;
    state.upload_secs = 0.0;
    state.pending_conflict = None;
    state.pending_conflict_idx = None;
    state.pending_conflict_path = None;
    state.conflict_answer = None;
    state.conflict_remember = false;
    state.conflict_override = None;
    state.log(tr1("{} Datei(en) werden hochgeladen …", plan.count()));
}

pub(super) fn restore_persisted(config: &Config) -> (Option<BackupFolder>, bool) {
    let Some((display_path, files)) = DedupIndex::open().one_time_queue() else {
        return (None, false);
    };
    let files = supported_images(files);
    if files.is_empty() {
        return (None, false);
    }
    let (dedup, conflict_resolution) = folders::default_options(config);
    (
        Some(BackupFolder {
            path: display_path,
            name: "Einmalige Sicherung".to_owned(),
            enabled: true,
            dedup,
            conflict_resolution,
            content_filter: ContentFilter::Photos,
            total_files: files.len(),
            uploaded_files: 0,
            last_updated: String::new(),
            status: FolderStatus::Idle,
        }),
        true,
    )
}

impl LiveState {
    pub(super) fn start_one_time_upload(&mut self, paths: Vec<PathBuf>) {
        let busy = self.upload.lock().unwrap().active;
        let logged_in = self.logged_in();
        let session_invalid = self.session.lock().unwrap().invalid;
        let files = match collect_after_guards(
            &paths,
            busy,
            logged_in,
            session_invalid,
            |path| {
                if path.is_dir() {
                    DropPathKind::Directory
                } else if path.is_file() {
                    DropPathKind::File
                } else {
                    DropPathKind::Other
                }
            },
            folders::scan_images,
        ) {
            Ok(files) => files,
            Err(DropError::Busy) => {
                self.status = Some(tr("Es läuft bereits ein Upload.").to_owned());
                return;
            }
            Err(DropError::SignedOut) => {
                self.status = Some(
                    tr("Bitte zuerst anmelden, bevor ein Upload gestartet wird.").to_owned(),
                );
                return;
            }
            Err(DropError::Empty) => {
                self.status = Some(
                    tr("Keine Fotos in diesem Ordner gefunden.").to_owned(),
                );
                return;
            }
        };

        let display_path = if paths.len() == 1 {
            paths[0].clone()
        } else {
            common_parent(&paths).unwrap_or_default()
        };
        DedupIndex::open().set_one_time_queue(&display_path, &files);
        self.begin_one_time_run(display_path, files);
    }

    pub(super) fn resume_one_time(&mut self) -> bool {
        if !self.one_time_resume {
            return false;
        }
        let busy = self.upload.lock().unwrap().active;
        let logged_in = self.logged_in();
        let session_invalid = self.session.lock().unwrap().invalid;
        let paused = *self.paused.borrow();
        if !can_resume(
            busy,
            logged_in,
            session_invalid,
            self.config.background_sync,
            paused,
        ) {
            return false;
        }
        let Some((display_path, files)) = DedupIndex::open().one_time_queue() else {
            self.one_time_resume = false;
            return false;
        };
        let files = supported_images(files);
        if files.is_empty() {
            self.one_time_resume = false;
            return false;
        }
        self.one_time_resume = false;
        self.begin_one_time_run(display_path, files);
        true
    }

    fn begin_one_time_run(&mut self, display_path: PathBuf, files: Vec<PathBuf>) {
        let target = transient_target(&self.config, display_path, files.len());
        let (dedup, conflict_resolution) = folders::default_options(&self.config);
        let mut plan = OneTimePlan::from_files(files, dedup, conflict_resolution, |path| {
            std::fs::metadata(path).ok().map(|metadata| metadata.len())
        });
        let slow_mode = self.upload.lock().unwrap().slow_mode;
        let concurrency = self.config.concurrency();
        let limit_kbps = if self.config.limit_enabled {
            self.config.limit_kbps
        } else {
            0
        };
        publish_state(&mut plan, &self.upload, slow_mode);
        if slow_mode {
            self.upload
                .lock()
                .unwrap()
                .log(tr("Slow-Mode aktiv: 1 Datei gleichzeitig."));
        }
        self.one_time = Some(target);
        self.paused.send_replace(false);
        self.config.paused = false;
        self.persist_config_if_allowed();
        self.was_active = true;
        self.status = Some(tr("Upload-Vorgang gestartet.").to_owned());

        let config = self.config.clone();
        let shared = self.upload.clone();
        let paused = self.paused.clone();
        let album = self.config.album_name.clone();
        self.runtime.spawn(async move {
            run_upload(
                plan.items,
                config,
                shared,
                paused,
                concurrency,
                limit_kbps,
                album,
                plan.folder_opts,
                HashMap::new(),
            )
            .await;
        });
    }
}

pub(super) fn finish_if_one_time(
    state: &mut LiveState,
    totals: &HashMap<usize, usize>,
    done: &HashMap<usize, usize>,
    failed: &HashMap<usize, usize>,
    run_done: usize,
    run_failed: usize,
    run_total: usize,
    cancelled: bool,
) -> bool {
    let Some(&total) = totals.get(&ONE_TIME_IDX) else {
        return false;
    };
    if cancelled {
        return true;
    }
    if let Some(one_time) = state.one_time.as_mut() {
        one_time.total_files = total;
        one_time.uploaded_files = done.get(&ONE_TIME_IDX).copied().unwrap_or(0);
        one_time.last_updated = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
        one_time.status = if failed.get(&ONE_TIME_IDX).copied().unwrap_or(0) > 0 {
            FolderStatus::Error
        } else {
            FolderStatus::Done
        };
    }
    if run_failed == 0 && run_done + run_failed >= run_total {
        DedupIndex::open().clear_one_time_queue();
    }
    true
}

mod view;
#[cfg(test)]
mod tests;
pub(super) use view::{backup_row, drop_zone};
