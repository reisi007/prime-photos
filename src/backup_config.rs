use crate::folders::{ConflictResolution, ContentFilter};
use std::path::PathBuf;

#[cfg(feature = "legacy-eframe")]
pub(crate) mod eframe_ui;

/// Whether the "Datensicherungseinstellungen" window is configuring a new
/// backup or one that already exists. Edit mode stores the folder **path** (not
/// a row index) so a folder removed while the window is open cannot make Save
/// write to the wrong backup.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ConfigFormMode {
    Create,
    Edit(PathBuf),
}

/// Result of one rendered frame of the shared backup-configuration form body.
pub(crate) enum ConfigFormAction {
    Save,
    Cancel,
    /// The "Ändern …" button: ask the app to open the asynchronous native folder
    /// picker (Welle 5.1). The picked folder is applied back to this form via
    /// `poll_folder_picker`.
    PickFolder,
}

/// State of the "Datensicherungseinstellungen" window, shared by Create and Edit.
pub(crate) struct BackupConfigForm {
    pub(crate) path: PathBuf,
    pub(crate) dedup: bool,
    pub(crate) conflict_resolution: ConflictResolution,
    /// Content filter — the original's two "Fotos sichern"/"Videos sichern"
    /// checkboxes (`docs/original/SPEC_CONTENT_FILTER_DESTINATION.md` §2.1). `Photos` by
    /// default; the effective filter stays `is_supported_image`.
    pub(crate) content_filter: ContentFilter,
    pub(crate) mode: ConfigFormMode,
    /// Inline validation message; keeps the window open after an invalid Save.
    pub(crate) error: Option<String>,
}
