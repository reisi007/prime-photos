use super::{QueueStatus, SmokeQueueItem, SmokeScene, SmokeState, SmokeTarget, TargetStatus};
use super::super::LivePage;
use crate::about::LicenseTab;

const UPLOADING_PCT: [u8; 9] = [12, 100, 21, 8, 64, 37, 3, 45, 72];

pub(super) fn state_for(scene: SmokeScene) -> SmokeState {
    let mut state = SmokeState {
        scene,
        page: LivePage::Backup,
        license_tab: LicenseTab::Components,
        paused: false,
        targets: Vec::new(),
        queue: Vec::new(),
        progress_done: 0,
        progress_total: 0,
        notice: "Ausschließlich synthetische Beispieldaten – keine Konten, Ordner oder Uploads."
            .to_owned(),
        retry_count: 0,
    };

    match scene {
        SmokeScene::Mixed => {
            state.targets = vec![
                target("Photos 2024", 13, 63, TargetStatus::Uploading),
                target("Travel", 12, 62, TargetStatus::Uploading),
                target("Family", 12, 62, TargetStatus::Uploading),
                target("Photos 2019", 918, 918, TargetStatus::Missing),
                target("External Photos 2021", 2043, 2043, TargetStatus::Missing),
                target("Network Photos 2022", 0, 0, TargetStatus::Missing),
            ];
            state.progress_done = 37;
            state.progress_total = 187;
            state.queue = queue(187, 37, 9, 0, &["Photos 2024", "Travel", "Family"]);
        }
        SmokeScene::Uploading | SmokeScene::Paused => {
            state.paused = scene == SmokeScene::Paused;
            state.targets = vec![
                target("Photos 2025", 2, 18, TargetStatus::Uploading),
                target("Camera Imports", 2, 18, TargetStatus::Uploading),
                target("Exports", 1, 17, TargetStatus::Uploading),
            ];
            state.progress_done = 5;
            state.progress_total = 53;
            state.queue = queue(
                53,
                5,
                8,
                0,
                &["Photos 2025", "Camera Imports", "Exports"],
            );
        }
        SmokeScene::AllDone => {
            state.targets = vec![
                target("Photos 2024", 1240, 1240, TargetStatus::Done),
                target("Travel", 486, 486, TargetStatus::Done),
                target("Family", 2380, 2380, TargetStatus::Done),
            ];
        }
        SmokeScene::Failures => {
            state.targets = vec![
                target("Photos 2023", 317, 320, TargetStatus::Error),
                target("Photos 2022", 1450, 1450, TargetStatus::Done),
            ];
            state.progress_done = 4;
            state.progress_total = 7;
            state.notice = "Fixture-Ladefehler: Einstellungen konnten nicht geladen werden."
                .to_owned();
            state.queue = queue(7, 4, 0, 3, &["Photos 2023"]);
        }
        SmokeScene::Empty => {}
        SmokeScene::ManyFolders => {
            let live_folders = [
                ("Photos 2024", 1, 5),
                ("Travel", 1, 4),
                ("Family", 1, 4),
                ("Camera Imports", 0, 4),
                ("Screenshots", 0, 4),
                ("Dokumente-Scans", 0, 4),
            ];
            state.targets = live_folders
                .into_iter()
                .map(|(name, done, total)| target(name, done, total, TargetStatus::Uploading))
                .chain([
                    target("Old Camera", 1990, 2018, TargetStatus::Waiting),
                    target("Defekt", 290, 310, TargetStatus::Error),
                    target("Neu", 0, 0, TargetStatus::NotScanned),
                    target("Photos 2019", 918, 918, TargetStatus::Missing),
                    target("External Photos 2021", 2043, 2043, TargetStatus::Missing),
                    target("Network Photos 2022", 0, 0, TargetStatus::Missing),
                ])
                .collect();
            state.progress_done = 3;
            state.progress_total = 25;
            state.queue = queue(
                25,
                3,
                4,
                0,
                &[
                    "Photos 2024",
                    "Travel",
                    "Family",
                    "Camera Imports",
                    "Screenshots",
                    "Dokumente-Scans",
                ],
            );
        }
        SmokeScene::About => {
            state = state_for(SmokeScene::Mixed);
            state.scene = SmokeScene::About;
            state.page = LivePage::About;
        }
        SmokeScene::Licenses => {
            state = state_for(SmokeScene::Mixed);
            state.scene = SmokeScene::Licenses;
            state.page = LivePage::Licenses;
        }
    }

    state
}

fn target(name: &str, completed: usize, total: usize, status: TargetStatus) -> SmokeTarget {
    SmokeTarget {
        name: name.to_owned(),
        completed,
        total,
        status,
    }
}

fn queue(
    total: usize,
    done: usize,
    uploading: usize,
    failed: usize,
    folders: &[&str],
) -> Vec<SmokeQueueItem> {
    let mut uploading_index: usize = 0;
    (0..total)
        .map(|index| {
            let status = if index < done {
                QueueStatus::Done
            } else if index < done + uploading {
                uploading_index += 1;
                QueueStatus::Uploading
            } else if index >= total - failed {
                QueueStatus::Failed
            } else {
                QueueStatus::Waiting
            };
            SmokeQueueItem {
                name: format!("IMG_{:04}.HEIC", 2043 + index),
                folder: folders[index % folders.len()].to_owned(),
                status,
                percent: UPLOADING_PCT[uploading_index.saturating_sub(1) % UPLOADING_PCT.len()],
            }
        })
        .collect()
}
