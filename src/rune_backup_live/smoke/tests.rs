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
fn smoke_route_starts_with_interactive_synthetic_backup_content() {
    let runtime = runtime(SmokeState::default(), false);
    assert_eq!(runtime.state.scene, SmokeScene::Alert);
    assert_eq!(runtime.state.page, LivePage::Backup);
    for key in [
        "rune-backup-smoke",
        "smoke-title-backup",
        "smoke-progress",
        "smoke-target-0",
        "smoke-queue-0",
        "smoke-pause",
        "smoke-advance",
        "smoke-add-target",
        "smoke-retry",
        "smoke-scene",
    ] {
        assert!(runtime.find(key).is_some(), "missing smoke node {key:?}");
    }
    assert_eq!(runtime.state.targets.len(), 2);
    assert_eq!(runtime.state.queue.len(), 3);
}

#[test]
fn smoke_controls_only_change_local_synthetic_state() {
    let mut runtime = runtime(SmokeState::default(), false);
    click(&mut runtime, "smoke-add-target");
    assert_eq!(runtime.state.targets.len(), 3);
    click(&mut runtime, "smoke-pause");
    assert!(runtime.state.paused);
    let completed = runtime.state.targets[0].completed;
    click(&mut runtime, "smoke-advance");
    assert_eq!(runtime.state.targets[0].completed, completed);
    click(&mut runtime, "smoke-retry");
    assert_eq!(runtime.state.retry_count, 1);
    assert!(!runtime.state.targets[1].failed);
}

#[test]
fn smoke_view_builds_empty_populated_and_alert_scenes_in_both_system_themes() {
    for dark in [false, true] {
        for scene in [SmokeScene::Empty, SmokeScene::Populated, SmokeScene::Alert] {
            let mut state = SmokeState::default();
            state.show_scene(scene);
            let runtime = runtime(state, dark);
            assert!(runtime.find("rune-backup-smoke").is_some());
            assert!(runtime.find("smoke-controls").is_some());
            match scene {
                SmokeScene::Empty => {
                    assert!(runtime.find("smoke-empty-targets").is_some());
                    assert!(runtime.find("smoke-empty-queue").is_some());
                }
                SmokeScene::Populated => {
                    assert!(runtime.find("smoke-target-0").is_some());
                    assert!(runtime.find("smoke-queue-0").is_some());
                }
                SmokeScene::Alert => {
                    assert!(runtime.find("smoke-target-1").is_some());
                    assert!(runtime.find("smoke-queue-2").is_some());
                }
                SmokeScene::About | SmokeScene::Licenses => {
                    panic!("about and licenses have dedicated route coverage");
                }
            }
            assert_eq!(
                crate::rune_theme::palette(dark).primary_foreground,
                tok::theme(dark).text,
            );
        }
    }
}

#[test]
fn smoke_about_and_licenses_scenes_select_production_routes_and_interact_in_both_themes() {
    for dark in [false, true] {
        let about = select_scene("about");
        assert_eq!(about.scene, SmokeScene::About);
        assert_eq!(about.page, LivePage::About);
        let mut about_runtime = runtime(about, dark);
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
        for key in [
            "smoke-progress",
            "smoke-controls",
            "smoke-targets",
            "smoke-queue",
            "smoke-target-0",
            "smoke-queue-0",
        ] {
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
        assert!(about_runtime.find("smoke-title-about").is_some());
        click(&mut about_runtime, "about-close");
        assert_eq!(about_runtime.state.page, LivePage::Backup);
        assert!(about_runtime.find("smoke-progress").is_some());

        let licenses = select_scene("licenses");
        assert_eq!(licenses.scene, SmokeScene::Licenses);
        assert_eq!(licenses.page, LivePage::Licenses);
        let mut licenses_runtime = runtime(licenses, dark);
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
        for key in [
            "smoke-progress",
            "smoke-controls",
            "smoke-targets",
            "smoke-queue",
        ] {
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
fn smoke_scene_control_cycles_only_through_local_synthetic_states() {
    let mut runtime = runtime(SmokeState::default(), false);
    click(&mut runtime, "smoke-scene");
    assert_eq!(runtime.state.scene, SmokeScene::Empty);
    assert!(runtime.state.targets.is_empty());
    assert!(runtime.state.queue.is_empty());
    click(&mut runtime, "smoke-scene");
    assert_eq!(runtime.state.scene, SmokeScene::Populated);
    assert!(runtime.state.targets.iter().all(|target| !target.failed));
    click(&mut runtime, "smoke-scene");
    assert_eq!(runtime.state.scene, SmokeScene::Alert);
    assert!(runtime.state.targets.iter().any(|target| target.failed));
}

#[test]
fn smoke_layout_remains_inside_initial_and_resized_viewports() {
    let mut runtime = runtime(SmokeState::default(), false);
    for size in [WINDOW_SIZE, (900.0, 680.0), MIN_WINDOW_SIZE] {
        runtime.set_viewport(size, 1.0);
        runtime.frame(1.0 / 60.0);
        for key in [
            "smoke-pause",
            "smoke-advance",
            "smoke-add-target",
            "smoke-scene",
            "smoke-retry",
        ] {
            let (x, y) = runtime.find(key).expect("smoke control stays in layout");
            assert!(x >= 0.0 && x <= size.0, "{key} x={x} outside {size:?}");
            assert!(y >= 0.0 && y <= size.1, "{key} y={y} outside {size:?}");
        }
    }
}
