//! Persistent selection of backup folders ("Automatisch gesicherte Ordner"),
//! plus the single source of truth for supported photo extensions.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const STANDARD_EXT: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "tif", "tiff", "gif", "webp", "bmp",
];
const RAW_EXT: &[&str] = &[
    "cr2", "cr3", "arw", "nef", "dng", "raf", "rw2", "orf", "srw", "pef", "3fr", "iiq", "erf",
    "mos", "kdc", "dcr", "mrw", "raw",
];

/// Single source of truth for "is this a photo we may upload?".
/// `.xmp` sidecars are explicitly never treated as images.
pub fn is_supported_image(path: &Path) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };
    if ext == "xmp" {
        return false;
    }
    STANDARD_EXT.contains(&ext.as_str()) || RAW_EXT.contains(&ext.as_str())
}

/// Extensions for use in native file dialogs.
///
/// **Deliberately not wired up yet** and kept on purpose: this is the single
/// prepared hook for a future native file picker (project rule, see
/// `AGENTS.md`). No code path calls it today, so it is annotated with
/// `#[allow(dead_code)]` rather than removed — the picker will read from here.
#[allow(dead_code)]
pub fn supported_extensions() -> Vec<&'static str> {
    STANDARD_EXT.iter().chain(RAW_EXT.iter()).copied().collect()
}

/// Recursively collect supported image files below `dir`.
pub fn scan_images(dir: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .filter(|p| is_supported_image(p))
        .collect()
}

/// Modification time of `path` in Unix seconds, or `None` when it cannot be
/// read. There is deliberately no `0` fallback: an unreadable timestamp must
/// stay distinguishable from "no value" (same rule as `DedupIndex::stat_matches`).
fn file_mtime(path: &Path) -> Option<i64> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let secs = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(secs.as_secs() as i64)
}

/// Largest modification time (Unix seconds) of **any file** below `dir`, or
/// `None` when the directory cannot be walked at all or holds no readable
/// timestamp (gone, ejected, no access).
///
/// Pure metadata walk — no file is opened, read or hashed. This is the cheap
/// half of the cold-start safety net (user decision 2026-10-14): "did anything
/// in this folder change at all?" must cost one `stat` per file and nothing
/// else, so an unchanged folder produces no hash, no per-file DB query and no
/// upload plan. Every file counts, not just supported images: a folder whose
/// only change is a sidecar still counts as changed, which is the safe
/// direction (the scan then simply finds nothing new).
pub fn max_mtime(dir: &Path) -> Option<i64> {
    let mut max: Option<i64> = None;
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let Some(m) = file_mtime(entry.path()) else {
            continue;
        };
        max = Some(match max {
            Some(prev) => prev.max(m),
            None => m,
        });
    }
    max
}

/// Whether `path`'s modification time beats `watermark`. Without a watermark
/// everything is "new"; an unreadable mtime never is — the same safety rule
/// `DedupIndex::stat_matches` applies to its fast path.
fn is_newer_than(path: &Path, watermark: Option<i64>) -> bool {
    match (watermark, file_mtime(path)) {
        (None, _) => true,
        (Some(w), Some(m)) => m > w,
        (Some(_), None) => false,
    }
}

/// Supported images below `dir` whose modification time is **newer** than
/// `watermark` — all of them when there is no watermark.
///
/// Same walk, same filter and the same order as [`scan_images`], with one extra
/// gate: the watermark pass sees exactly the files a full scan would have seen,
/// minus the ones it already knows about. [`scan_images`] itself is untouched —
/// a manual "Jetzt scannen und hochladen" still takes every supported file.
pub fn scan_new_since(dir: &Path, watermark: Option<i64>) -> Vec<PathBuf> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .filter(|p| is_supported_image(p))
        .filter(|p| is_newer_than(p, watermark))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FolderStatus {
    #[default]
    Idle,
    Scanning,
    Uploading,
    Paused,
    Done,
    Error,
}

/// How a name conflict in the destination folder is resolved on upload.
///
/// A file's identity is its **content** (user decision 2026-10-14): the same
/// content is skipped — the server rejects an MD5 duplicate with HTTP 409 and
/// `errorCode = "DuplicatesConflictError"` — while different content is
/// uploaded, renamed when the name is already taken. The default is therefore
/// `SaveAsNew`.
///
/// Mirrors the original client's four options ("Umgang mit Dateinamenkonflikten",
/// `docs/SPEC_DEDUP.md` §4.1) and maps onto the wire parameter `conflictResolution`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConflictResolution {
    /// "Jedes Mal nachfragen" — the server keeps the existing file and reports
    /// the conflict (HTTP 409), which the upload worker turns into a per-file
    /// prompt (Ignorieren / Als neue Datei speichern / Ersetzen).
    Ask,
    /// "Ignorieren" — keeps the existing file (`conflictResolution=NONE`) and
    /// drops the new one. Not the default: a file whose content differs is meant
    /// to be uploaded, not silently skipped.
    Ignore,
    /// "Neue Datei automatisch umbenennen" (`conflictResolution=RENAME`) — the
    /// **file** default (a `folders.toml` entry without the key): different
    /// content is uploaded, renamed when the name is already taken. Newly
    /// created targets are pre-set with the application default (`Replace`,
    /// `Config::conflict_resolution_default`). In the radio groups of the
    /// backup-configuration and the settings dialog the option carries this
    /// label; the conflict dialog's button keeps its own wording, "Als neue
    /// Datei speichern" (the original names the same option differently in the
    /// two places).
    #[default]
    SaveAsNew,
    /// "Ersetzen" — a taken name is resolved in **two steps** (user decision
    /// 2026-10-15). The POST sends `conflictResolution=NONE` (the host rejects
    /// the `OVERWRITE` value with 404 and an empty body), and the 409's
    /// `errorDetails.conflictNodeIds[0]` names the node to replace with
    /// `PUT /v2/upload/<nodeId>` — live-proven: 200, the same node id, content
    /// actually swapped, **but only with the POST's query set** (`conflictResolution`,
    /// `fileSize`, `name`, `parentNodeId`, `contentDate`); without it the host
    /// answers 400 `MissingFileSize`. The overwrite is therefore **not** a wire
    /// value, which is why `wire_value` reports `NONE`.
    Replace,
}

impl ConflictResolution {
    /// Wire value for the `conflictResolution` upload parameter. `Ask` and
    /// `Ignore` both send `NONE` — the server keeps the existing file and reports
    /// the conflict; the worker decides whether to skip it or ask the user.
    /// `Replace` also sends `NONE`: no wire value expresses "overwrite" on this
    /// endpoint (`OVERWRITE` ⇒ 404), the overwrite is the follow-up
    /// `PUT /v2/upload/<nodeId>` (`urls::replace_upload_url`).
    pub fn wire_value(self) -> &'static str {
        match self {
            ConflictResolution::Ask | ConflictResolution::Ignore => "NONE",
            ConflictResolution::SaveAsNew => "RENAME",
            ConflictResolution::Replace => "NONE",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupFolder {
    pub path: PathBuf,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// "Duplikate vermeiden": when true, files already in the local MD5 index are
    /// skipped (path + md5). Off ⇒ every scanned file is uploaded.
    #[serde(default = "default_true")]
    pub dedup: bool,
    /// Name-conflict policy for this upload target.
    #[serde(default)]
    pub conflict_resolution: ConflictResolution,
    #[serde(default)]
    pub total_files: usize,
    #[serde(default)]
    pub uploaded_files: usize,
    #[serde(default)]
    pub last_updated: String,
    #[serde(default)]
    pub status: FolderStatus,
}

fn default_true() -> bool {
    true
}

impl BackupFolder {
    pub fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        Self {
            path,
            name,
            enabled: true,
            dedup: true,
            conflict_resolution: ConflictResolution::default(),
            total_files: 0,
            uploaded_files: 0,
            last_updated: String::new(),
            status: FolderStatus::Idle,
        }
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct FolderStore {
    #[serde(default)]
    pub folders: Vec<BackupFolder>,
}

impl FolderStore {
    pub fn load() -> Self {
        match std::fs::read_to_string(store_path()) {
            Ok(s) => toml::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        std::fs::write(path, content)
    }

    /// Add a folder unless it is already tracked.
    pub fn add(&mut self, path: PathBuf) -> bool {
        if self.folders.iter().any(|f| f.path == path) {
            return false;
        }
        self.folders.push(BackupFolder::new(path));
        true
    }

    /// Indices of the folders currently flagged `enabled`, in store order.
    ///
    /// The background sync (PORT-5) only picks those up; a disabled folder
    /// keeps its manual "Jetzt scannen und hochladen" (that path takes the
    /// index from the UI, not from here).
    pub(crate) fn enabled_indices(&self) -> Vec<usize> {
        self.folders
            .iter()
            .enumerate()
            .filter(|(_, f)| f.enabled)
            .map(|(i, _)| i)
            .collect()
    }
}

pub fn store_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("prime_photos")
        .join("folders.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty scratch directory under the system temp dir.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "prime-photos-folders-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn max_mtime_and_scan_new_since_gate_on_the_watermark() {
        let dir = scratch("watermark");
        let old = dir.join("old.jpg");
        let nested = dir.join("event");
        std::fs::create_dir_all(&nested).unwrap();
        let deep = nested.join("deep.png");
        let sidecar = dir.join("notes.txt");
        let _ = std::fs::write(&old, b"x");
        let _ = std::fs::write(&deep, b"x");
        let _ = std::fs::write(&sidecar, b"x");

        let watermark = max_mtime(&dir).expect("a directory with files");

        // Nothing is newer than the maximum we just read.
        assert!(scan_new_since(&dir, Some(watermark)).is_empty());
        // A watermark above every file behaves the same.
        assert!(scan_new_since(&dir, Some(i64::MAX)).is_empty());

        // No watermark → every supported image, recursively, in both files.
        let all = scan_new_since(&dir, None);
        assert_eq!(all.len(), 2);
        assert!(all.contains(&old));
        assert!(all.contains(&deep));
        assert_eq!(scan_new_since(&dir, Some(0)).len(), 2);

        // `scan_images` is untouched: same set, no watermark involved.
        assert_eq!(scan_images(&dir).len(), 2);

        // The watermark only grows when something is added.
        let later = dir.join("later.jpg");
        let _ = std::fs::write(&later, b"x");
        assert!(max_mtime(&dir).expect("still files") >= watermark);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn max_mtime_is_none_without_readable_files() {
        let missing = scratch("missing");
        let _ = std::fs::remove_dir_all(&missing);
        // A gone folder is "unknown", not "unchanged at time 0" — the caller
        // must be able to tell the two apart.
        assert_eq!(max_mtime(&missing), None);
        assert_eq!(max_mtime(Path::new("/definitely/not/here")), None);
    }

    #[test]
    fn conflict_resolution_defaults_to_save_as_new() {
        // A file's identity is its content (user decision 2026-10-14): the
        // default must upload content that differs, renamed when the name is
        // taken — never silently skip it.
        assert_eq!(ConflictResolution::default(), ConflictResolution::SaveAsNew);
    }

    #[test]
    fn every_conflict_resolution_maps_to_its_wire_value() {
        assert_eq!(ConflictResolution::Ask.wire_value(), "NONE");
        assert_eq!(ConflictResolution::Ignore.wire_value(), "NONE");
        assert_eq!(ConflictResolution::SaveAsNew.wire_value(), "RENAME");
        // `Replace` no longer claims the `OVERWRITE` value the host answers with
        // 404: it posts like `Ask`/`Ignore` and overwrites the node the 409 names
        // with `PUT /v2/upload/<nodeId>` (user decision 2026-10-15).
        assert_eq!(ConflictResolution::Replace.wire_value(), "NONE");
    }

    #[test]
    fn old_store_without_conflict_resolution_gets_the_new_default() {
        // An existing `folders.toml` predates the field; deserializing it must
        // land on the new default, not on `Ignore`.
        let folder: BackupFolder = toml::from_str(
            r#"
path = "/tmp/photos"
name = "Fotos"
"#,
        )
        .unwrap();
        assert_eq!(folder.conflict_resolution, ConflictResolution::SaveAsNew);
    }
}
