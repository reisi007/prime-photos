//! Fixtures with a **live** upload run (an active queue and progress counters).

use super::fixture::{
    Fixture, PICTURES, UPLOADING_PCT, entry, folder, published_run, signed_in_config,
};
use crate::app::{Page, SettingsTab};
use crate::folders::{FolderStatus, FolderStore};
use crate::upload::FileState;

pub(super) fn mixed() -> Fixture {
    let folders = vec![
        folder(
            &format!("{PICTURES}/Fotos 2024"),
            1240,
            1240,
            FolderStatus::Done,
            "26.10.2026 18:12",
        ),
        folder(
            &format!("{PICTURES}/Reisen"),
            486,
            486,
            FolderStatus::Done,
            "25.10.2026 09:41",
        ),
        folder(
            &format!("{PICTURES}/Familie"),
            2380,
            2380,
            FolderStatus::Done,
            "24.10.2026 20:05",
        ),
        // Missing sources: the folder cards must show "Ordner nicht gefunden"
        // without any drive being mounted.
        folder(
            "/Volumes/Archiv/Fotos 2019",
            918,
            918,
            FolderStatus::NotFound,
            "20.10.2026 11:30",
        ),
        folder(
            "/Volumes/Fotos-2021",
            2043,
            2043,
            FolderStatus::NotFound,
            "18.10.2026 16:04",
        ),
        folder("/Volumes/Bilder-NAS/2022", 0, 0, FolderStatus::NotFound, ""),
    ];

    // 187 queue rows: 37 finished, 9 transferring, 141 waiting. The visible
    // "Warteschlange (150)" and the aggregate "37 von 187" both follow from
    // these numbers, exactly as in the real app. The per-folder counters are
    // derived from the same rows (`fixture::apply_queue_counters`), so the three
    // cards read the live state with their own real counts.
    let mut queue = Vec::with_capacity(187);
    let mut uploading_seen = 0usize;
    for i in 0..187 {
        let folder_idx = i % 3;
        let state = if i < 37 {
            FileState::Done
        } else if i < 46 {
            FileState::Uploading
        } else {
            FileState::Waiting
        };
        let pct = if state == FileState::Uploading {
            let pct = UPLOADING_PCT[uploading_seen % UPLOADING_PCT.len()];
            uploading_seen += 1;
            pct
        } else {
            0
        };
        queue.push(entry(i, folder_idx, &folders[folder_idx].path, state, pct));
    }
    let upload = published_run(&folders, queue);

    Fixture {
        config: signed_in_config(),
        store: FolderStore { folders },
        upload,
        one_time: None,
        paused: false,
        page: Page::Backup,
        settings_tab: SettingsTab::General,
        account_name: Some("Alex Beispiel".to_string()),
        config_error: None,
        source_exists: vec![true, true, true, false, false, false],
    }
}

/// `Uploading` and `Paused` render the same live run; only the global pause
/// flag differs. The folder rows are part of the run, so they show the live
/// state ("Upload-Vorgang läuft") with the run's own counters.
pub(super) fn uploading(paused: bool) -> Fixture {
    let folders = vec![
        folder(
            &format!("{PICTURES}/Fotos 2025"),
            640,
            615,
            FolderStatus::Idle,
            "31.10.2026 09:12",
        ),
        folder(
            &format!("{PICTURES}/Kamera-Import"),
            128,
            120,
            FolderStatus::Idle,
            "31.10.2026 09:12",
        ),
        folder(
            &format!("{PICTURES}/Export"),
            42,
            42,
            FolderStatus::Idle,
            "30.10.2026 21:40",
        ),
    ];

    let mut queue = Vec::with_capacity(53);
    for i in 0..53 {
        let folder_idx = i % 3;
        let state = if i < 5 {
            FileState::Done
        } else if i < 13 {
            FileState::Uploading
        } else {
            FileState::Waiting
        };
        let pct = UPLOADING_PCT[i % UPLOADING_PCT.len()];
        queue.push(entry(i, folder_idx, &folders[folder_idx].path, state, pct));
    }
    // Per-folder counters come from the rows themselves: the three cards show
    // exactly as many files as the table lists for them.
    let upload = published_run(&folders, queue);

    let mut config = signed_in_config();
    config.paused = paused;

    Fixture {
        config,
        store: FolderStore { folders },
        upload,
        one_time: None,
        paused,
        page: Page::Backup,
        settings_tab: SettingsTab::General,
        account_name: Some("Alex Beispiel".to_string()),
        config_error: None,
        source_exists: vec![true, true, true],
    }
}

pub(super) fn many_folders() -> Fixture {
    let folders = vec![
        folder(
            &format!("{PICTURES}/Fotos 2024"),
            1240,
            1240,
            FolderStatus::Done,
            "26.10.2026 18:12",
        ),
        folder(
            &format!("{PICTURES}/Reisen"),
            486,
            486,
            FolderStatus::Done,
            "25.10.2026 09:41",
        ),
        folder(
            &format!("{PICTURES}/Familie"),
            2380,
            2380,
            FolderStatus::Done,
            "24.10.2026 20:05",
        ),
        folder(
            &format!("{PICTURES}/Kamera-Import"),
            128,
            128,
            FolderStatus::Done,
            "23.10.2026 17:20",
        ),
        folder(
            &format!("{PICTURES}/Screenshots"),
            96,
            96,
            FolderStatus::Done,
            "22.10.2026 08:03",
        ),
        // Scanned, nothing to do: "Warten" (a total is known).
        folder(
            &format!("{PICTURES}/Dokumente-Scans"),
            64,
            12,
            FolderStatus::Idle,
            "21.10.2026 12:44",
        ),
        folder(
            &format!("{PICTURES}/Alte Kamera"),
            2018,
            1990,
            FolderStatus::Idle,
            "21.10.2026 12:44",
        ),
        folder(
            &format!("{PICTURES}/Defekt"),
            310,
            290,
            FolderStatus::Error,
            "20.10.2026 19:02",
        ),
        // Never scanned: no total, so neither a state word nor a percentage.
        folder(&format!("{PICTURES}/Neu"), 0, 0, FolderStatus::Idle, ""),
        // Missing sources.
        folder(
            "/Volumes/Archiv/Fotos 2019",
            918,
            918,
            FolderStatus::NotFound,
            "20.10.2026 11:30",
        ),
        folder(
            "/Volumes/Fotos-2021",
            2043,
            2043,
            FolderStatus::NotFound,
            "18.10.2026 16:04",
        ),
        folder("/Volumes/Bilder-NAS/2022", 0, 0, FolderStatus::NotFound, ""),
    ];

    // Only the present folders 0–5 carry rows, so only they join the run and
    // show the live state. 6–8 keep their settled states ("Warten", "Fehler",
    // never scanned) and 9–11 stay "Ordner nicht gefunden".
    let mut queue = Vec::with_capacity(25);
    for i in 0..25 {
        let folder_idx = i % 6;
        let state = if i < 3 {
            FileState::Done
        } else if i < 7 {
            FileState::Uploading
        } else {
            FileState::Waiting
        };
        let pct = UPLOADING_PCT[i % UPLOADING_PCT.len()];
        queue.push(entry(i, folder_idx, &folders[folder_idx].path, state, pct));
    }
    let upload = published_run(&folders, queue);

    Fixture {
        config: signed_in_config(),
        store: FolderStore { folders },
        upload,
        one_time: None,
        paused: false,
        page: Page::Backup,
        settings_tab: SettingsTab::General,
        account_name: Some("Alex Beispiel".to_string()),
        config_error: None,
        source_exists: vec![
            true, true, true, true, true, true, true, true, true, false, false, false,
        ],
    }
}
