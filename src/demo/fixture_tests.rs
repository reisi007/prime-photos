//! Fixture invariants: what every scenario must guarantee so the screenshots
//! show a state the real app could actually produce.

use super::*;
use crate::folders::{BackupFolder, FolderStatus};
use crate::upload::FileState;
use std::collections::HashMap;

#[test]
fn every_fixture_has_one_source_flag_per_folder() {
    for scenario in Scenario::ALL {
        let fixture = fixture(scenario);
        assert_eq!(
            fixture.source_exists.len(),
            fixture.store.folders.len(),
            "scenario {} must fake exactly one path per folder",
            scenario.id()
        );
    }
}

#[test]
fn mixed_matches_the_reference_screenshot() {
    let fixture = fixture(Scenario::Mixed);
    assert_eq!(fixture.store.folders.len(), 6);
    assert_eq!(
        fixture
            .source_exists
            .iter()
            .filter(|exists| **exists)
            .count(),
        3,
        "three present, three missing sources"
    );
    assert_eq!(fixture.upload.queue.len(), 187);
    assert_eq!(fixture.upload.total, 187);
    assert_eq!(fixture.upload.done, 37);
    assert!(
        fixture.upload.active,
        "the aggregate must show the live run"
    );

    let count = |state: FileState| {
        fixture
            .upload
            .queue
            .iter()
            .filter(|entry| entry.state == state)
            .count()
    };
    assert_eq!(count(FileState::Done), 37);
    assert_eq!(count(FileState::Uploading), 9);
    assert_eq!(count(FileState::Waiting), 141);
    // "Warteschlange (150)": waiting + uploading.
    let remaining = fixture
        .upload
        .queue
        .iter()
        .filter(|entry| matches!(entry.state, FileState::Waiting | FileState::Uploading))
        .count();
    assert_eq!(remaining, 150);

    // The Ordner column of every queue row resolves through the run's
    // captured plan paths.
    assert_eq!(fixture.upload.folder_paths.len(), 3);
    for entry in &fixture.upload.queue {
        assert!(
            fixture.upload.folder_paths.contains_key(&entry.folder),
            "queue row without a folder path"
        );
    }

    // The uploading rows cover the percentages from the reference screenshot,
    // including a full-bar 100 % that is still "Uploading".
    let pcts: Vec<u64> = fixture
        .upload
        .queue
        .iter()
        .filter(|entry| entry.state == FileState::Uploading)
        .map(|entry| crate::upload::calculate_pct(entry.size, entry.bytes_sent))
        .collect();
    for expected in [12, 100, 21, 8] {
        assert!(pcts.contains(&expected), "missing {expected}% in {pcts:?}");
    }

    // The three present folders carry queue rows, so `backup_ui` treats them as
    // part of the run: the cards must read "Upload-Vorgang läuft" with the
    // counts of their own rows, never their persisted "abgeschlossen".
    assert_eq!(fixture.upload.folder_total.get(&0), Some(&63));
    assert_eq!(fixture.upload.folder_done.get(&0), Some(&13));
    assert_eq!(fixture.upload.folder_total.get(&1), Some(&62));
    assert_eq!(fixture.upload.folder_done.get(&1), Some(&12));
    assert_eq!(fixture.upload.folder_total.get(&2), Some(&62));
    assert_eq!(fixture.upload.folder_done.get(&2), Some(&12));
    // The missing sources have no scan and therefore no run counters.
    for idx in 3..6 {
        assert!(
            !fixture.upload.folder_total.contains_key(&idx),
            "missing source {idx} must stay out of the run"
        );
    }
}

#[test]
fn mixed_flags_the_missing_sources() {
    let fixture = fixture(Scenario::Mixed);
    let missing: Vec<&BackupFolder> = fixture
        .store
        .folders
        .iter()
        .zip(&fixture.source_exists)
        .filter(|(_, exists)| !**exists)
        .map(|(folder, _)| folder)
        .collect();
    assert_eq!(missing.len(), 3);
    for folder in missing {
        assert!(
            folder.path.starts_with("/Volumes"),
            "a missing source must point at an unmounted volume: {}",
            folder.path.display()
        );
    }
}

#[test]
fn all_done_has_nothing_left_to_do() {
    let fixture = fixture(Scenario::AllDone);
    assert!(!fixture.upload.active);
    assert!(fixture.upload.queue.is_empty(), "the queue must be empty");
    assert!(
        fixture
            .store
            .folders
            .iter()
            .all(|f| f.status == FolderStatus::Done && f.uploaded_files == f.total_files),
        "every folder must be settled"
    );
}

#[test]
fn failures_carry_the_error_folder_and_the_config_error() {
    let fixture = fixture(Scenario::Failures);
    assert_eq!(
        fixture.config_error.as_deref(),
        Some("Die Einstellungen konnten nicht geladen werden.")
    );
    assert!(
        fixture
            .store
            .folders
            .iter()
            .any(|f| f.status == FolderStatus::Error)
    );
    assert!(
        fixture
            .upload
            .queue
            .iter()
            .any(|entry| entry.state == FileState::Failed)
    );
    // The settled queue (no run active) still shows its rows.
    assert!(!fixture.upload.active);
    assert!(fixture.upload.failed > 0);
}

#[test]
fn signed_in_scenarios_have_dummy_session_material() {
    for scenario in Scenario::ALL {
        let fixture = fixture(scenario);
        if scenario == Scenario::SignedOut {
            continue;
        }
        assert!(
            !fixture.config.session_id.is_empty() && !fixture.config.cookie_x_acb.is_empty(),
            "scenario {} must render as signed in",
            scenario.id()
        );
        assert_eq!(fixture.config.country, "de");
        assert_eq!(fixture.config.language.as_deref(), Some("de"));
        assert_eq!(fixture.account_name.as_deref(), Some("Alex Example"));
    }
}

#[test]
fn signed_out_has_no_session_and_no_folders() {
    let fixture = fixture(Scenario::SignedOut);
    assert!(fixture.config.session_id.is_empty());
    assert!(fixture.config.cookie_x_acb.is_empty());
    assert!(fixture.config.cookie_at_acb.is_empty());
    assert!(fixture.config.cookie_ubid_acb.is_empty());
    assert!(fixture.config.cookie_x_amz_access_token.is_empty());
    assert!(fixture.store.folders.is_empty());
    assert!(fixture.upload.queue.is_empty());
    assert!(fixture.account_name.is_none());
    assert_eq!(fixture.config.country, "de");
}

#[test]
fn paused_fixture_is_paused_with_visible_rows() {
    let fixture = fixture(Scenario::Paused);
    assert!(fixture.paused);
    assert!(fixture.config.paused);
    assert!(
        !fixture.upload.queue.is_empty(),
        "a paused run keeps its rows on screen"
    );
    assert!(fixture.upload.active);
}

/// The real model publishes `folder_paths`, `folder_total`, `folder_done` and
/// the outcome counters **together, from the plan**. Every fixture must agree
/// with its own queue — otherwise `backup_ui` renders a card state the real app
/// cannot produce (a finished folder next to its live rows, or card counts that
/// contradict the table).
#[test]
fn every_queue_row_agrees_with_its_folder_counters() {
    for scenario in Scenario::ALL {
        let fixture = fixture(scenario);
        let upload = &fixture.upload;
        let folders = &fixture.store.folders;

        for entry in &upload.queue {
            assert!(
                entry.folder < folders.len(),
                "scenario {}: queue folder {} out of range",
                scenario.id(),
                entry.folder
            );
            assert!(
                fixture.source_exists[entry.folder],
                "scenario {}: a queue row points at a missing source ({})",
                scenario.id(),
                folders[entry.folder].path.display()
            );
        }

        let mut expected_total: HashMap<usize, usize> = HashMap::new();
        let mut expected_done: HashMap<usize, usize> = HashMap::new();
        let mut expected_failed: HashMap<usize, usize> = HashMap::new();
        for entry in &upload.queue {
            *expected_total.entry(entry.folder).or_default() += 1;
            match entry.state {
                FileState::Done => *expected_done.entry(entry.folder).or_default() += 1,
                FileState::Failed => *expected_failed.entry(entry.folder).or_default() += 1,
                _ => {}
            }
        }

        assert_eq!(
            upload.total,
            upload.queue.len(),
            "scenario {}: the run total must be the queue length",
            scenario.id()
        );
        assert_eq!(
            upload.done,
            expected_done.values().sum::<usize>(),
            "scenario {}: done must match the finished rows",
            scenario.id()
        );
        assert_eq!(
            upload.failed,
            expected_failed.values().sum::<usize>(),
            "scenario {}: failed must match the failed rows",
            scenario.id()
        );

        // Exactly the folders with rows are in the run, with exactly the counts
        // of those rows.
        assert_eq!(
            upload.folder_total,
            expected_total,
            "scenario {}: folder_total must be the rows per folder",
            scenario.id()
        );
        for &idx in expected_total.keys() {
            assert_eq!(
                upload.folder_done.get(&idx).copied().unwrap_or(0),
                expected_done.get(&idx).copied().unwrap_or(0),
                "scenario {}: folder {idx} done count",
                scenario.id()
            );
            assert_eq!(
                upload.folder_new.get(&idx).copied().unwrap_or(0),
                expected_done.get(&idx).copied().unwrap_or(0),
                "scenario {}: folder {idx} new-upload count",
                scenario.id()
            );
            assert_eq!(
                upload.folder_failed.get(&idx).copied().unwrap_or(0),
                expected_failed.get(&idx).copied().unwrap_or(0),
                "scenario {}: folder {idx} failure count",
                scenario.id()
            );
            assert_eq!(
                upload.folder_paths.get(&idx),
                Some(&folders[idx].path),
                "scenario {}: folder {idx} path",
                scenario.id()
            );
        }

        // Every path in `folder_paths` belongs to a counter set, and a folder
        // without rows has neither a path nor counters.
        assert_eq!(
            upload.folder_paths.len(),
            upload.folder_total.len(),
            "scenario {}: folder_paths and folder_total must be published together",
            scenario.id()
        );
        for idx in upload.folder_paths.keys() {
            assert!(
                upload.folder_total.contains_key(idx),
                "scenario {}: folder_paths entry {idx} without folder_total",
                scenario.id()
            );
        }
        for idx in 0..folders.len() {
            if !expected_total.contains_key(&idx) {
                assert!(
                    !upload.folder_total.contains_key(&idx),
                    "scenario {}: folder {idx} has no rows but run counters",
                    scenario.id()
                );
                assert!(
                    !upload.folder_paths.contains_key(&idx),
                    "scenario {}: folder {idx} has no rows but a run path",
                    scenario.id()
                );
            }
        }

        // The per-folder sums are the run counters.
        assert_eq!(
            upload.folder_done.values().sum::<usize>(),
            upload.done,
            "scenario {}: per-folder done must sum to the run",
            scenario.id()
        );
        assert_eq!(
            upload.folder_failed.values().sum::<usize>(),
            upload.failed,
            "scenario {}: per-folder failures must sum to the run",
            scenario.id()
        );
    }
}
