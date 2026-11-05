//! Fixtures **without** a live run: settled folders, failures, the empty
//! states, the settings page and the signed-out start.

use super::fixture::{
    Fixture, PICTURES, base_config, entry, folder, settled_run, signed_in_config,
};
use crate::app::{Page, SettingsTab};
use crate::folders::{FolderStatus, FolderStore};
use crate::upload::{FileState, UploadShared};

pub(super) fn all_done() -> Fixture {
    let folders = vec![
        folder(
            &format!("{PICTURES}/Photos 2024"),
            1240,
            1240,
            FolderStatus::Done,
            "30.10.2026 22:10",
        ),
        folder(
            &format!("{PICTURES}/Travel"),
            486,
            486,
            FolderStatus::Done,
            "30.10.2026 22:10",
        ),
        folder(
            &format!("{PICTURES}/Family"),
            2380,
            2380,
            FolderStatus::Done,
            "30.10.2026 22:10",
        ),
    ];

    Fixture {
        config: signed_in_config(),
        store: FolderStore { folders },
        // No run, no queue: the footer must read "Alles aktuell" and the queue
        // its empty state.
        upload: UploadShared::default(),
        one_time: None,
        paused: false,
        page: Page::Backup,
        settings_tab: SettingsTab::General,
        account_name: Some("Alex Example".to_string()),
        config_error: None,
        source_exists: vec![true, true, true],
    }
}

pub(super) fn failures() -> Fixture {
    let folders = vec![
        folder(
            &format!("{PICTURES}/Photos 2023"),
            320,
            317,
            FolderStatus::Error,
            "27.10.2026 07:55",
        ),
        folder(
            &format!("{PICTURES}/Photos 2022"),
            1450,
            1450,
            FolderStatus::Done,
            "27.10.2026 07:58",
        ),
    ];

    // The last run's rows stay visible after it finished (`active == false`):
    // 4 succeeded, 3 failed — the exact mix the outcome line reports. The
    // per-folder counters come from the same rows, so the "Fehler" card and the
    // failed table rows agree (the real app keeps them after the run, too).
    let mut queue = Vec::with_capacity(7);
    for i in 0..7 {
        let state = if i < 4 {
            FileState::Done
        } else {
            FileState::Failed
        };
        queue.push(entry(i, 0, &folders[0].path, state, 0));
    }
    let mut upload = settled_run(&folders, queue);
    upload.log = vec!["3 Dateien konnten nicht hochgeladen werden.".to_string()];

    Fixture {
        config: signed_in_config(),
        store: FolderStore { folders },
        upload,
        one_time: None,
        paused: false,
        page: Page::Backup,
        settings_tab: SettingsTab::General,
        account_name: Some("Alex Example".to_string()),
        config_error: Some("Die Einstellungen konnten nicht geladen werden.".to_string()),
        source_exists: vec![true, true],
    }
}

pub(super) fn empty() -> Fixture {
    Fixture {
        config: signed_in_config(),
        store: FolderStore::default(),
        upload: UploadShared::default(),
        one_time: None,
        paused: false,
        page: Page::Backup,
        settings_tab: SettingsTab::General,
        account_name: Some("Alex Example".to_string()),
        config_error: None,
        source_exists: Vec::new(),
    }
}

pub(super) fn settings() -> Fixture {
    let folders = vec![
        folder(
            &format!("{PICTURES}/Photos 2024"),
            1240,
            1240,
            FolderStatus::Done,
            "26.10.2026 18:12",
        ),
        folder(
            &format!("{PICTURES}/Travel"),
            486,
            486,
            FolderStatus::Done,
            "25.10.2026 09:41",
        ),
        folder(
            &format!("{PICTURES}/Family"),
            2380,
            2380,
            FolderStatus::Done,
            "24.10.2026 20:05",
        ),
    ];

    let mut config = signed_in_config();
    config.concurrency = 3;
    config.limit_enabled = true;
    config.limit_kbps = 250;

    Fixture {
        config,
        store: FolderStore { folders },
        upload: UploadShared::default(),
        one_time: None,
        paused: false,
        page: Page::Settings,
        settings_tab: SettingsTab::General,
        account_name: Some("Alex Example".to_string()),
        config_error: None,
        source_exists: vec![true, true, true],
    }
}

/// The same settings data on the second tab (Issue #30): the bandwidth page
/// has its own control heights and must also sit inside the page gutter.
pub(super) fn settings_bandwidth() -> Fixture {
    let mut fixture = settings();
    fixture.settings_tab = SettingsTab::Bandwidth;
    fixture
}

pub(super) fn signed_out() -> Fixture {
    Fixture {
        // No session material at all: `is_logged_in()` is false, so the app
        // renders its own sign-in screen (no webview is created until the user
        // presses the button).
        config: base_config(),
        store: FolderStore::default(),
        upload: UploadShared::default(),
        one_time: None,
        paused: false,
        page: Page::Backup,
        settings_tab: SettingsTab::General,
        account_name: None,
        config_error: None,
        source_exists: Vec::new(),
    }
}
