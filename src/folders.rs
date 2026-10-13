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
pub fn supported_extensions() -> Vec<&'static str> {
    STANDARD_EXT
        .iter()
        .chain(RAW_EXT.iter())
        .copied()
        .collect()
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupFolder {
    pub path: PathBuf,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
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
            total_files: 0,
            uploaded_files: 0,
            last_updated: String::new(),
            status: FolderStatus::Idle,
        }
    }

    pub fn percent(&self) -> f32 {
        if self.total_files == 0 {
            0.0
        } else {
            (self.uploaded_files as f32 / self.total_files as f32).clamp(0.0, 1.0)
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

    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.folders.len();
        self.folders.retain(|f| f.path != path);
        self.folders.len() != before
    }
}

pub fn store_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("amzn_photos_uploader")
        .join("folders.toml")
}
