use super::super::test_support::{a11y_contains_text, find_a11y_node, signed_out_state};
use super::*;
use crate::upload::QueueEntry;
use rune::{Env, Runtime};

fn production_runtime(state: LiveState) -> Runtime<LiveState> {
    let mut runtime = Runtime::new(state, live_view, (1180.0, 1400.0), Env::new(false, 1.0));
    runtime.frame(0.0);
    runtime
}

fn state_with_in_flight(uploads_in_flight: usize) -> LiveState {
    let state = signed_out_state();
    let mut upload = state.upload.lock().unwrap();
    upload.active = true;
    upload.uploads_in_flight = uploads_in_flight;
    drop(upload);
    state
}

#[test]
fn queue_render_order_sorts_by_status_and_keeps_insertion_order_within_a_status() {
    use FileState::*;
    let states = [
        Waiting, Done, Uploading, Skipped, Failed, Waiting, Uploading, Done,
    ];
    assert_eq!(queue_render_order(&states), [2, 6, 4, 0, 5, 1, 7, 3]);
}

#[test]
fn retry_folder_index_resolves_the_store_row_not_the_queue_position() {
    use crate::folders::BackupFolder;
    use std::collections::HashMap;
    use std::path::PathBuf;

    let folders = [
        BackupFolder::new(PathBuf::from("/synthetic/b")),
        BackupFolder::new(PathBuf::from("/synthetic/a")),
    ];
    let folder_paths = HashMap::from([
        (0, PathBuf::from("/synthetic/a")),
        (1, PathBuf::from("/synthetic/b")),
    ]);
    assert_eq!(retry_folder_index(1, &folder_paths, &folders), Some(0));
    assert_eq!(retry_folder_index(0, &folder_paths, &folders), Some(1));
    assert_eq!(retry_folder_index(9, &folder_paths, &folders), None);
}

#[test]
fn upload_in_flight_counter_appears_only_while_uploads_are_running() {
    let runtime = production_runtime(state_with_in_flight(3));
    assert!(runtime.find("upload-in-flight").is_some());
    let accessibility = runtime.accessibility_json();
    assert!(a11y_contains_text(&accessibility, "3 Uploads laufen gerade"), "{accessibility}");
    let node = find_a11y_node(&accessibility, "upload-in-flight")
        .expect("in-flight counter is accessible");
    assert_eq!(node["role"], "status");

    let runtime = production_runtime(state_with_in_flight(0));
    assert!(runtime.find("upload-in-flight").is_none());
    let accessibility = runtime.accessibility_json();
    assert!(!a11y_contains_text(&accessibility, "0 Uploads laufen gerade"), "{accessibility}");

    let runtime = production_runtime(state_with_in_flight(1));
    assert!(runtime.find("upload-in-flight").is_some());
    let accessibility = runtime.accessibility_json();
    assert!(a11y_contains_text(&accessibility, "1 Upload läuft gerade"), "{accessibility}");
}

/// Collect the AccessKit names in tree order, reduced to the unique file names
/// that the queue rows prefix onto their "name · folder" label.
fn queue_names_in_tree(node: &serde_json::Value, names: &[&str]) -> Vec<String> {
    let mut rendered = Vec::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if let Some(label) = node["name"].as_str() {
            if let Some(name) = names.iter().find(|name| label.starts_with(**name)) {
                rendered.push((*name).to_owned());
            }
        }
        if let Some(children) = node["children"].as_array() {
            stack.extend(children.iter().rev());
        }
    }
    rendered
}

#[test]
fn queue_rows_render_in_status_order_but_the_queue_keeps_insertion_order() {
    use FileState::*;
    let entries = [
        ("wait-delta.jpg", Waiting),
        ("done-zeta.jpg", Done),
        ("upload-alpha.jpg", Uploading),
        ("skip-eta.jpg", Skipped),
        ("fail-gamma.jpg", Failed),
        ("wait-epsilon.jpg", Waiting),
        ("upload-beta.jpg", Uploading),
    ];
    let names = entries.map(|(name, _)| name);

    let mut state = signed_out_state();
    state.store.folders = vec![
        crate::folders::BackupFolder::new(std::path::PathBuf::from("/synthetic/b")),
        crate::folders::BackupFolder::new(std::path::PathBuf::from("/synthetic/a")),
    ];
    {
        let mut upload = state.upload.lock().unwrap();
        upload.active = true;
        upload.total = entries.len();
        upload.done = entries.iter().filter(|(_, state)| *state == Done).count();
        upload.folder_paths = std::collections::HashMap::from([
            (0, std::path::PathBuf::from("/synthetic/a")),
            (1, std::path::PathBuf::from("/synthetic/b")),
        ]);
        upload.queue = entries
            .iter()
            .map(|(name, state)| QueueEntry {
                folder: 1,
                name: (*name).to_owned(),
                state: *state,
                size: 0,
                bytes_sent: 0,
            })
            .collect();
    }

    let runtime = production_runtime(state);
    let accessibility = runtime.accessibility_json();
    assert_eq!(
        queue_names_in_tree(&accessibility, &names),
        [
            "upload-alpha.jpg",
            "upload-beta.jpg",
            "fail-gamma.jpg",
            "wait-delta.jpg",
            "wait-epsilon.jpg",
            "done-zeta.jpg",
            "skip-eta.jpg",
        ],
        "queue rows render in status order with a stable insertion order per status: {accessibility}"
    );
    for (index, (_, state)) in entries.iter().enumerate() {
        assert!(runtime.find(&format!("queue-item-{index}")).is_some());
        assert_eq!(
            runtime.find(&format!("queue-retry-{index}")).is_some(),
            *state == Failed,
            "retry action belongs only to the failed queue row {index}"
        );
    }

    let upload = runtime.state.upload.lock().unwrap();
    assert_eq!(
        upload.queue.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(),
        names,
        "rendering must not reorder the worker-indexed queue"
    );
}
