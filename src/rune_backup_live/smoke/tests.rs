use super::*;
use rune::{Env, Runtime};

fn runtime_at(state: SmokeState, dark: bool, size: (f32, f32)) -> Runtime<SmokeState> {
    let mut runtime = Runtime::new(
        state,
        smoke_view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    runtime
}

fn runtime(state: SmokeState, dark: bool) -> Runtime<SmokeState> {
    runtime_at(state, dark, WINDOW_SIZE)
}

fn click(runtime: &mut Runtime<SmokeState>, key: &str) {
    let (x, y) = runtime.find(key).expect("smoke control exists");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

fn select_scene(name: &str) -> SmokeState {
    let mut state = SmokeState::default();
    let (_, select) = SMOKE_SCENES
        .iter()
        .find(|(scene, _)| *scene == name)
        .unwrap_or_else(|| panic!("missing smoke scene {name:?}"));
    select(&mut state);
    state
}

#[test]
fn smoke_route_starts_with_the_mixed_legacy_backup_state() {
    let runtime = runtime(SmokeState::default(), false);
    assert_eq!(runtime.state.scene, SmokeScene::Mixed);
    assert_eq!(runtime.state.page, LivePage::Backup);
    for key in [
        "rune-backup-smoke",
        "smoke-header",
        "smoke-title-backup",
        "smoke-scene-name",
        "smoke-progress",
        "smoke-progress-count",
        "smoke-target-0",
        "smoke-target-5",
        "smoke-queue-0",
        "smoke-queue-186",
        "smoke-pause",
        "smoke-advance",
        "smoke-add-target",
        "smoke-retry",
        "smoke-scene",
    ] {
        assert!(runtime.find(key).is_some(), "missing smoke node {key:?}");
    }
    assert_eq!(runtime.state.progress_done, 37);
    assert_eq!(runtime.state.progress_total, 187);
    assert_eq!(runtime.state.targets.len(), 6);
    assert_eq!(runtime.state.queue.len(), 187);
}

#[test]
fn smoke_fixture_exposes_the_legacy_scene_ids_and_complete_scene_trees() {
    let expected_ids = [
        "mixed",
        "uploading",
        "paused",
        "all-done",
        "failures",
        "empty",
        "many-folders",
        "about",
        "licenses",
    ];
    assert_eq!(
        SMOKE_SCENES
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        expected_ids.to_vec()
    );

    for dark in [false, true] {
        for (name, select) in SMOKE_SCENES {
            let mut state = SmokeState::default();
            select(&mut state);
            let scene = state.scene;
            assert_eq!(scene.id(), *name);
            let runtime = runtime(state, dark);
            assert!(runtime.find("rune-backup-smoke").is_some());
            assert!(runtime.find("smoke-header").is_some());
            assert!(runtime.find("smoke-scene-name").is_some());

            if matches!(scene, SmokeScene::About | SmokeScene::Licenses) {
                let route_key = if scene == SmokeScene::About {
                    "rune-about-page"
                } else {
                    "rune-licenses-page"
                };
                assert!(runtime.find(route_key).is_some(), "missing {route_key:?}");
                assert!(runtime.find("smoke-progress").is_none());
                assert!(runtime.find("smoke-targets").is_none());
                assert!(runtime.find("smoke-queue").is_none());
                continue;
            }

            for key in [
                "smoke-progress",
                "smoke-progress-status",
                "smoke-progress-count",
                "smoke-controls",
                "smoke-targets",
                "smoke-queue",
            ] {
                assert!(runtime.find(key).is_some(), "missing {key:?} in {scene:?}");
            }
            match scene {
                SmokeScene::Mixed => {
                    assert_eq!(runtime.state.targets.len(), 6);
                    assert_eq!(runtime.state.queue.len(), 187);
                    assert!(runtime.find("smoke-target-status-3").is_some());
                    assert!(runtime.find("smoke-queue-186").is_some());
                }
                SmokeScene::Uploading => {
                    assert_eq!(runtime.state.targets.len(), 3);
                    assert_eq!(runtime.state.queue.len(), 53);
                    assert!(!runtime.state.paused);
                }
                SmokeScene::Paused => {
                    assert_eq!(runtime.state.queue.len(), 53);
                    assert!(runtime.state.paused);
                }
                SmokeScene::AllDone => {
                    assert_eq!(runtime.state.targets.len(), 3);
                    assert!(runtime.state.queue.is_empty());
                    assert!(runtime.find("smoke-empty-queue").is_some());
                }
                SmokeScene::Failures => {
                    assert_eq!(runtime.state.targets.len(), 2);
                    assert_eq!(runtime.state.queue.len(), 7);
                    assert!(runtime.find("smoke-target-status-0").is_some());
                }
                SmokeScene::Empty => {
                    assert!(runtime.state.targets.is_empty());
                    assert!(runtime.state.queue.is_empty());
                    assert!(runtime.find("smoke-empty-targets").is_some());
                    assert!(runtime.find("smoke-empty-queue").is_some());
                }
                SmokeScene::ManyFolders => {
                    assert_eq!(runtime.state.targets.len(), 12);
                    assert_eq!(runtime.state.queue.len(), 25);
                    assert!(runtime.find("smoke-target-11").is_some());
                    assert!(runtime.find("smoke-queue-24").is_some());
                }
                SmokeScene::About | SmokeScene::Licenses => unreachable!(),
            }
        }
    }
}

#[test]
fn smoke_controls_only_change_local_synthetic_state() {
    let mut runtime = runtime(select_scene("failures"), false);
    click(&mut runtime, "smoke-add-target");
    assert_eq!(runtime.state.targets.len(), 3);
    click(&mut runtime, "smoke-pause");
    assert!(runtime.state.paused);
    let completed = runtime.state.progress_done;
    click(&mut runtime, "smoke-advance");
    assert_eq!(runtime.state.progress_done, completed);
    click(&mut runtime, "smoke-pause");
    let failed_before = runtime
        .state
        .queue
        .iter()
        .filter(|item| item.status == QueueStatus::Failed)
        .count();
    let waiting_before = runtime
        .state
        .queue
        .iter()
        .filter(|item| item.status == QueueStatus::Waiting)
        .count();
    click(&mut runtime, "smoke-retry");
    assert_eq!(runtime.state.retry_count, 1);
    assert_eq!(runtime.state.targets[0].status, TargetStatus::Waiting);
    assert_eq!(
        runtime
            .state
            .queue
            .iter()
            .filter(|item| item.status == QueueStatus::Failed)
            .count(),
        failed_before - 1
    );
    assert_eq!(
        runtime
            .state
            .queue
            .iter()
            .filter(|item| item.status == QueueStatus::Waiting)
            .count(),
        waiting_before + 1
    );
    assert!(runtime.state.notice.contains("kein Upload gestartet"));
}

#[test]
fn about_and_licenses_use_the_production_routes_and_interact_in_both_themes() {
    for dark in [false, true] {
        let mut about_runtime = runtime(select_scene("about"), dark);
        for key in [
            "rune-backup-smoke",
            "smoke-header",
            "smoke-title-about",
            "rune-about-page",
            "license-open",
            "about-close",
        ] {
            assert!(
                about_runtime.find(key).is_some(),
                "missing About node {key:?}"
            );
        }
        for key in ["smoke-progress", "smoke-controls", "smoke-targets", "smoke-queue"] {
            assert!(
                about_runtime.find(key).is_none(),
                "backup node {key:?} leaked into About"
            );
        }

        click(&mut about_runtime, "license-open");
        assert_eq!(about_runtime.state.page, LivePage::Licenses);
        assert!(about_runtime.find("smoke-title-licenses").is_some());
        assert!(about_runtime.find("rune-licenses-page").is_some());
        assert!(about_runtime.find("license-line-0").is_some());
        click(&mut about_runtime, "license-tab-icons");
        assert_eq!(about_runtime.state.license_tab, LicenseTab::Icons);
        assert!(about_runtime.find("attribution-line-0").is_some());
        click(&mut about_runtime, "license-back-about");
        assert_eq!(about_runtime.state.page, LivePage::About);
        click(&mut about_runtime, "about-close");
        assert_eq!(about_runtime.state.page, LivePage::Backup);
        assert!(about_runtime.find("smoke-progress").is_some());

        let mut licenses_runtime = runtime(select_scene("licenses"), dark);
        for key in [
            "rune-backup-smoke",
            "smoke-header",
            "smoke-title-licenses",
            "rune-licenses-page",
            "license-tab-components",
            "license-tab-icons",
            "license-close",
        ] {
            assert!(
                licenses_runtime.find(key).is_some(),
                "missing licenses node {key:?}"
            );
        }
        for key in ["smoke-progress", "smoke-controls", "smoke-targets", "smoke-queue"] {
            assert!(
                licenses_runtime.find(key).is_none(),
                "backup node {key:?} leaked into licenses"
            );
        }
        click(&mut licenses_runtime, "license-tab-icons");
        assert_eq!(licenses_runtime.state.license_tab, LicenseTab::Icons);
        click(&mut licenses_runtime, "license-back-about");
        assert_eq!(licenses_runtime.state.page, LivePage::About);
        click(&mut licenses_runtime, "license-open");
        assert_eq!(licenses_runtime.state.page, LivePage::Licenses);
        click(&mut licenses_runtime, "license-close");
        assert_eq!(licenses_runtime.state.page, LivePage::Backup);
        assert!(licenses_runtime.find("smoke-progress").is_some());
    }
}

#[test]
fn named_scene_changes_replace_the_whole_local_fixture() {
    let mut state = SmokeState::default();
    state.paused = true;
    state.retry_count = 4;
    state.targets.clear();
    state.show_scene(SmokeScene::Failures);
    assert_eq!(state.scene, SmokeScene::Failures);
    assert!(!state.paused);
    assert_eq!(state.retry_count, 0);
    assert_eq!(state.targets.len(), 2);
    assert_eq!(state.queue.len(), 7);

    state.show_scene(SmokeScene::Empty);
    assert!(state.targets.is_empty());
    assert!(state.queue.is_empty());
    state.show_scene(SmokeScene::ManyFolders);
    assert_eq!(state.targets.len(), 12);
    assert_eq!(state.queue.len(), 25);
}

#[test]
fn backup_scene_control_cycles_only_through_local_backup_fixtures() {
    let mut runtime = runtime(SmokeState::default(), false);
    for expected in [
        SmokeScene::Uploading,
        SmokeScene::Paused,
        SmokeScene::AllDone,
        SmokeScene::Failures,
        SmokeScene::Empty,
        SmokeScene::ManyFolders,
        SmokeScene::Mixed,
    ] {
        click(&mut runtime, "smoke-scene");
        assert_eq!(runtime.state.scene, expected);
    }
}

#[test]
fn every_scene_keeps_its_view_tree_inside_minimum_and_resized_viewports() {
    for size in [WINDOW_SIZE, (900.0, 680.0), MIN_WINDOW_SIZE] {
        for (name, select) in SMOKE_SCENES {
            let mut state = SmokeState::default();
            select(&mut state);
            let scene = state.scene;
            assert_eq!(scene.id(), *name);
            let runtime = runtime_at(state, false, size);
            for key in ["rune-backup-smoke", "smoke-header", "smoke-scene-name"] {
                let (x, y) = runtime.find(key).expect("shared scene node exists");
                assert!(x >= 0.0 && x <= size.0, "{key} x={x} outside {size:?}");
                assert!(y >= 0.0 && y <= size.1, "{key} y={y} outside {size:?}");
            }
            if !matches!(scene, SmokeScene::About | SmokeScene::Licenses) {
                for key in [
                    "smoke-progress",
                    "smoke-content",
                    "smoke-pause",
                    "smoke-advance",
                    "smoke-add-target",
                    "smoke-scene",
                    "smoke-retry",
                ] {
                    let (x, y) = runtime.find(key).expect("backup node remains in tree");
                    assert!(x >= 0.0 && x <= size.0, "{key} x={x} outside {size:?}");
                    assert!(y >= 0.0 && y <= size.1, "{key} y={y} outside {size:?}");
                }
            }
        }
    }
}
