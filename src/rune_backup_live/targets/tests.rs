use super::super::test_support::{find_a11y_node, signed_out_state};
use super::super::LiveState;
use super::form::{
    apply_target_options, create_target_form, edit_target_form, select_target_photos,
};
use super::target_card;
use crate::folders::{BackupFolder, ConflictResolution, ContentFilter, FolderStatus};
use crate::upload::{FileState, QueueEntry};
use rune::{Cx, El, Env, Runtime, col, tok};
use std::path::PathBuf;

#[test]
fn create_form_defaults_to_photos_and_saves_that_filter() {
    let form = create_target_form(
        PathBuf::from("/photos"),
        true,
        ConflictResolution::Ask,
    );
    assert_eq!(form.content_filter, ContentFilter::Photos);

    let mut folder = BackupFolder::new(form.path.clone());
    apply_target_options(
        &mut folder,
        form.dedup,
        form.conflict,
        form.content_filter,
    );
    assert_eq!(folder.content_filter, ContentFilter::Photos);
}

#[test]
fn edit_form_round_trips_each_persisted_filter() {
    for persisted in [
        ContentFilter::Photos,
        ContentFilter::Videos,
        ContentFilter::PhotosAndVideo,
    ] {
        let mut folder = BackupFolder::new(PathBuf::from("/photos"));
        folder.content_filter = persisted;

        let form = edit_target_form(&folder);
        assert_eq!(form.content_filter, persisted);
        apply_target_options(
            &mut folder,
            form.dedup,
            form.conflict,
            form.content_filter,
        );
        assert_eq!(folder.content_filter, persisted);
    }
}

#[test]
fn photo_only_toggle_preserves_the_persisted_video_checkbox_selection() {
    for (persisted, photos_off, photos_on) in [
        (ContentFilter::Photos, ContentFilter::Photos, ContentFilter::Photos),
        (ContentFilter::Videos, ContentFilter::Videos, ContentFilter::Photos),
        (
            ContentFilter::PhotosAndVideo,
            ContentFilter::Videos,
            ContentFilter::PhotosAndVideo,
        ),
    ] {
        assert_eq!(select_target_photos(persisted, false), photos_off);
        assert_eq!(select_target_photos(persisted, true), photos_on);
    }
}

fn card_view(state: &LiveState, cx: &mut Cx<LiveState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    let colors = crate::rune_theme::palette(dark);
    cx.foreground(theme.text);
    cx.focus_ring(2.0, 2.0, colors.focus_ring);
    let mut cards = col()
        .key("card-fixture")
        .w_full()
        .h_full()
        .fill(theme.bg);
    for (index, folder) in state.store.folders.iter().enumerate() {
        cards = cards.child(target_card(
            state,
            index,
            folder,
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            colors.alert_foreground,
        ));
    }
    cards
}

fn card_runtime(state: LiveState) -> Runtime<LiveState> {
    let mut runtime = Runtime::new(state, card_view, (1180.0, 900.0), Env::new(false, 1.0));
    runtime.frame(0.0);
    runtime
}

fn state_with_folder(status: FolderStatus, enabled: bool) -> LiveState {
    let mut state = signed_out_state();
    state
        .store
        .folders
        .push(BackupFolder::new(PathBuf::from("/synthetic/photos")));
    state.store.folders[0].status = status;
    state.store.folders[0].enabled = enabled;
    state
}

fn start_run_for(state: &LiveState, index: usize, path: &str) {
    let mut upload = state.upload.lock().unwrap();
    upload.active = true;
    upload
        .folder_paths
        .insert(index, PathBuf::from(path));
}

fn a11y_list<'a>(node: &'a serde_json::Value, field: &str) -> Vec<&'a str> {
    node[field]
        .as_array()
        .map(|values| values.iter().filter_map(|value| value.as_str()).collect())
        .unwrap_or_default()
}

fn is_disabled(node: &serde_json::Value) -> bool {
    a11y_list(node, "states").contains(&"disabled")
}

fn click(runtime: &mut Runtime<LiveState>, key: &str) {
    let (x, y) = runtime
        .find(key)
        .unwrap_or_else(|| panic!("missing Rune target control {key:?}"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

#[test]
fn not_found_disables_upload_and_switch_but_keeps_options_and_remove() {
    let runtime = card_runtime(state_with_folder(FolderStatus::NotFound, true));
    let json = runtime.accessibility_json();
    for key in ["target-upload-0", "target-enabled-0"] {
        let node = find_a11y_node(&json, key).unwrap_or_else(|| panic!("{key} is accessible"));
        assert!(is_disabled(node), "{key} must be disabled: {node}");
        assert!(
            a11y_list(node, "actions").is_empty(),
            "{key} must have no press action: {node}"
        );
        assert_eq!(node["description"], "Ordner nicht gefunden");
    }
    assert_eq!(find_a11y_node(&json, "target-enabled-0").unwrap()["role"], "switch");
    for key in ["target-edit-0", "target-remove-0"] {
        let node = find_a11y_node(&json, key).unwrap_or_else(|| panic!("{key} is accessible"));
        assert!(!is_disabled(node), "{key} must stay active: {node}");
        assert!(
            a11y_list(node, "actions").contains(&"click"),
            "{key} must stay clickable: {node}"
        );
    }
}

#[test]
fn spinner_shows_for_the_active_folder_and_vanishes_when_the_run_ends() {
    let state = state_with_folder(FolderStatus::Uploading, true);
    start_run_for(&state, 0, "/synthetic/photos");
    {
        let mut upload = state.upload.lock().unwrap();
        upload.folder_total.insert(0, 10);
        upload.folder_done.insert(0, 3);
        upload.queue.push(QueueEntry {
            folder: 0,
            name: "a.jpg".into(),
            state: FileState::Uploading,
            size: 10,
            bytes_sent: 0,
        });
    }
    let mut runtime = card_runtime(state);
    assert!(runtime.find("target-spinner-0").is_some());
    let json = runtime.accessibility_json();
    let spinner = find_a11y_node(&json, "target-spinner-0").expect("spinner is accessible");
    assert_eq!(spinner["role"], "progress-indicator");
    assert_eq!(spinner["name"], "Upload-Vorgang läuft");
    assert!(
        find_a11y_node(&json, "target-spinner-0-arc").is_none(),
        "spinner icons are a11y-hidden"
    );

    runtime.update(|state, _| state.upload.lock().unwrap().active = false);
    runtime.frame(1.0 / 60.0);
    assert!(runtime.find("target-spinner-0").is_none());
}

#[test]
fn spinner_follows_folder_counts_and_ignores_other_folders() {
    let mut state = state_with_folder(FolderStatus::Uploading, true);
    state
        .store
        .folders
        .push(BackupFolder::new(PathBuf::from("/synthetic/other")));
    // Only the second folder participates; a count-behind first folder alone
    // must not show a spinner.
    start_run_for(&state, 1, "/synthetic/other");
    state.upload.lock().unwrap().folder_total.insert(0, 10);
    let runtime = card_runtime(state);
    assert!(runtime.find("target-spinner-0").is_none());

    // Now let the active folder's own counters lag behind: the spinner is
    // driven by the live run, not by a persisted FolderStatus.
    let state = state_with_folder(FolderStatus::Idle, true);
    start_run_for(&state, 0, "/synthetic/photos");
    {
        let mut upload = state.upload.lock().unwrap();
        upload.folder_total.insert(0, 10);
        upload.folder_done.insert(0, 2);
    }
    let runtime = card_runtime(state);
    assert!(runtime.find("target-spinner-0").is_some());
}

#[test]
fn remove_is_disabled_only_for_folders_in_the_active_run() {
    let mut state = state_with_folder(FolderStatus::Idle, true);
    state
        .store
        .folders
        .push(BackupFolder::new(PathBuf::from("/synthetic/other")));
    start_run_for(&state, 1, "/synthetic/other");
    let runtime = card_runtime(state);
    let json = runtime.accessibility_json();

    let active = find_a11y_node(&json, "target-remove-1").expect("active remove is accessible");
    assert!(is_disabled(active), "remove of a running folder must be disabled");
    assert_eq!(active["description"], "Ordner wird gerade gesichert");
    assert!(a11y_list(active, "actions").is_empty());
    let upload = find_a11y_node(&json, "target-upload-1").expect("upload stays mounted");
    assert!(!is_disabled(upload), "the append upload stays clickable");
    assert!(a11y_list(upload, "actions").contains(&"click"));

    let other = find_a11y_node(&json, "target-remove-0").expect("other remove is accessible");
    assert!(!is_disabled(other), "other folders stay removable");
    assert!(a11y_list(other, "actions").contains(&"click"));
}

#[test]
fn enabled_control_is_a_switch_with_checked_state_and_toggles() {
    let mut runtime = card_runtime(state_with_folder(FolderStatus::Idle, true));
    let json = runtime.accessibility_json();
    let switch = find_a11y_node(&json, "target-enabled-0").expect("switch is accessible");
    assert_eq!(switch["role"], "switch");
    assert_eq!(switch["name"], "Aktiviert");
    assert!(a11y_list(switch, "states").contains(&"checked"));
    assert!(runtime.find("target-enabled-0-switch").is_some());

    click(&mut runtime, "target-enabled-0");
    assert!(!runtime.state.store.folders[0].enabled, "click toggles the flag");
    let json = runtime.accessibility_json();
    let switch = find_a11y_node(&json, "target-enabled-0").expect("switch is accessible");
    assert_eq!(switch["name"], "Deaktiviert");
    assert!(a11y_list(switch, "states").contains(&"unchecked"));

    click(&mut runtime, "target-enabled-0");
    assert!(runtime.state.store.folders[0].enabled);
}

#[test]
fn clicking_disabled_controls_changes_nothing() {
    let mut runtime = card_runtime(state_with_folder(FolderStatus::NotFound, true));
    click(&mut runtime, "target-upload-0");
    click(&mut runtime, "target-enabled-0");
    assert!(runtime.state.store.folders[0].enabled, "disabled switch must not toggle");
    assert!(
        !runtime.state.upload.lock().unwrap().active,
        "disabled upload must not start a run"
    );
    assert_eq!(runtime.state.store.folders.len(), 1);

    let state = state_with_folder(FolderStatus::Idle, true);
    start_run_for(&state, 0, "/synthetic/photos");
    let mut runtime = card_runtime(state);
    click(&mut runtime, "target-remove-0");
    assert_eq!(runtime.state.store.folders.len(), 1, "disabled remove must not delete");
}
