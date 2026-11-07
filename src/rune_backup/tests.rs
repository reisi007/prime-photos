use super::*;
use rune::{Env, Runtime};

fn runtime(state: BackupState) -> Runtime<BackupState> {
    let mut runtime = Runtime::new(state, view, WINDOW_SIZE, Env::default());
    runtime.frame(0.0);
    runtime
}

fn click(runtime: &mut Runtime<BackupState>, key: &str) {
    let (x, y) = runtime
        .find(key)
        .unwrap_or_else(|| panic!("missing Rune backup element {key:?}"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

#[test]
fn empty_scene_add_target_cta_only_shows_fixture_notice() {
    let mut state = BackupState::default();
    empty_scene(&mut state);
    let mut runtime = runtime(state);

    for key in ["screen", "targets", "empty-state", "add-target-cta"] {
        assert!(runtime.find(key).is_some(), "missing keyed node {key:?}");
    }
    assert!(runtime.find("fixture-notice").is_none());

    click(&mut runtime, "add-target-cta");

    assert_eq!(runtime.state.notice, Some(ADD_TARGET_NOTICE));
    assert!(runtime.find("fixture-notice").is_some());
}

#[test]
fn uploading_scene_has_synthetic_file_counts_and_pause_is_local() {
    let mut state = BackupState::default();
    uploading_scene(&mut state);
    let mut runtime = runtime(state);

    for key in [
        "screen",
        "global-progress",
        "global-progress-meter",
        "targets",
        "target-family",
        "target-trips",
        "queue",
        "queue-row-1",
        "pause-resume",
    ] {
        assert!(runtime.find(key).is_some(), "missing keyed node {key:?}");
    }
    assert_eq!(UPLOADING_TARGETS[0].completed_files, 3);
    assert_eq!(UPLOADING_TARGETS[0].total_files, 12);
    assert_eq!(UPLOADING_TARGETS[1].completed_files, 0);
    assert_eq!(UPLOADING_QUEUE[0].name, "Beispiel_001.jpg");
    assert!(UPLOADING_TARGETS
        .iter()
        .all(|target| !target.name.contains('/')));

    click(&mut runtime, "pause-resume");
    assert!(runtime.state.paused);
    assert!(runtime.find("pause-resume").is_some());

    click(&mut runtime, "pause-resume");
    assert!(!runtime.state.paused);
}

#[test]
fn queue_item_status_tracks_pause_only_for_active_rows() {
    let active = UPLOADING_QUEUE[0];
    let waiting = UPLOADING_QUEUE[1];

    assert_eq!(queue_item_status(active, false), "Wird übertragen");
    assert_eq!(queue_item_status(active, true), "Angehalten");
    assert_eq!(queue_item_status(active, false), "Wird übertragen");
    assert_eq!(queue_item_status(waiting, false), "Warten");
    assert_eq!(queue_item_status(waiting, true), "Warten");
}

#[test]
fn failure_scene_retry_only_shows_fixture_notice() {
    let mut state = BackupState::default();
    failure_scene(&mut state);
    let mut runtime = runtime(state);

    for key in [
        "screen",
        "targets",
        "target-failed",
        "queue",
        "failed-queue-row",
        "retry-failed",
    ] {
        assert!(runtime.find(key).is_some(), "missing keyed node {key:?}");
    }
    assert_eq!(FAILED_TARGET.completed_files, 2);
    assert_eq!(FAILED_TARGET.total_files, 5);
    assert!(runtime.find("fixture-notice").is_none());

    click(&mut runtime, "retry-failed");

    assert_eq!(runtime.state.notice, Some(RETRY_NOTICE));
    assert!(runtime.find("fixture-notice").is_some());
}

#[test]
fn fixture_scenes_reset_to_deterministic_synthetic_state() {
    let mut state = BackupState {
        scene: Scene::Failure,
        paused: true,
        notice: Some(RETRY_NOTICE),
    };

    empty_scene(&mut state);
    assert_eq!(state.scene, Scene::Empty);
    assert!(!state.paused);
    assert_eq!(state.notice, None);

    uploading_scene(&mut state);
    assert_eq!(state.scene, Scene::Uploading);
    assert!(!state.paused);
    assert_eq!(state.notice, None);

    failure_scene(&mut state);
    assert_eq!(state.scene, Scene::Failure);
    assert!(!state.paused);
    assert_eq!(state.notice, None);
}

#[test]
fn fixture_run_requires_a_parser_recognized_headless_action() {
    assert!(run(Vec::new()).is_err());
    assert!(!has_headless_action(&["--snap=fixture.png".to_owned()]));
    assert!(!has_headless_action(&[
        "--native".to_owned(),
        "--script".to_owned(),
        "fixture.json".to_owned(),
    ]));

    for action in ["--snap", "--tree", "--script"] {
        assert!(has_headless_action(&[
            action.to_owned(),
            "fixture-output".to_owned(),
        ]));
    }
}
