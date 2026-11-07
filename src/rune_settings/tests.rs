use super::*;
use rune::{Env, Runtime};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEST_PATH: AtomicUsize = AtomicUsize::new(0);

fn test_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "prime-photos-rune-settings-{}-{}.toml",
        std::process::id(),
        NEXT_TEST_PATH.fetch_add(1, Ordering::Relaxed)
    ))
}

fn runtime() -> (Runtime<SettingsState>, std::path::PathBuf) {
    let path = test_path();
    let mut state = SettingsState::new(Config::default());
    state.save_path = Some(path.clone());
    let mut runtime = Runtime::new(state, view, WINDOW_SIZE, Env::default());
    runtime.frame(0.0);
    (runtime, path)
}

fn click(runtime: &mut Runtime<SettingsState>, key: &str) {
    let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("missing {key}"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

#[test]
fn tabs_switch_the_live_settings_page() {
    let (mut runtime, _) = runtime();
    assert_eq!(runtime.state.tab, SettingsTab::General);

    click(&mut runtime, "tab-bandwidth");

    assert_eq!(runtime.state.tab, SettingsTab::Bandwidth);
    assert!(runtime.find("concurrency-slider").is_some());
}

#[test]
fn page_title_shifts_with_the_titlebar_inset() {
    let (mut runtime, _) = runtime();
    let initial_x = runtime.find("page-title").unwrap().0;
    let inset = 48.0;

    runtime.set_titlebar_inset(inset);
    for _ in 0..120 {
        runtime.frame(1.0 / 60.0);
    }

    let inset_x = runtime.find("page-title").unwrap().0;
    assert!(
        (inset_x - initial_x - inset).abs() < 0.01,
        "page-title x moved from {initial_x} to {inset_x}, expected {inset}"
    );
}

#[test]
fn appearance_change_is_live_and_persisted_to_an_injected_path() {
    let (mut runtime, path) = runtime();

    click(&mut runtime, "appearance-dark");

    assert_eq!(runtime.state.config.appearance, Appearance::Dark);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("appearance = \"dark\""));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn notification_toggle_updates_live_state_and_saves_config() {
    let (mut runtime, path) = runtime();
    assert!(runtime.state.config.notifications);

    click(&mut runtime, "notifications-row");

    assert!(!runtime.state.config.notifications);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("notifications = false"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn concurrency_slider_and_limit_input_update_the_config() {
    let (mut runtime, path) = runtime();
    click(&mut runtime, "tab-bandwidth");
    runtime.frame(1.0);
    let (x, y) = runtime.find("concurrency-slider").unwrap();
    let left_rail = x - SLIDER_WIDTH / 2.0 + 2.0;
    let right_rail = x + SLIDER_WIDTH / 2.0 - 2.0;
    runtime.pointer_down(left_rail, y);
    runtime.frame(1.0 / 60.0);
    runtime.pointer_move(right_rail, y);
    runtime.frame(1.0 / 60.0);
    runtime.pointer_up(right_rail, y);
    runtime.frame(1.0 / 60.0);
    assert_eq!(runtime.state.config.concurrency, 8);

    runtime.state.limit_edit.replace_text("640");
    runtime.state.set_limit_from_edit();
    assert_eq!(runtime.state.config.limit_kbps, 640);

    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("concurrency = 8"));
    assert!(saved.contains("limit_kbps = 640"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn save_failure_is_rendered_as_an_alert() {
    let path = test_path();
    std::fs::create_dir_all(&path).unwrap();
    let mut state = SettingsState::new(Config::default());
    state.save_path = Some(path.clone());
    let mut runtime = Runtime::new(state, view, WINDOW_SIZE, Env::default());
    runtime.frame(0.0);

    click(&mut runtime, "notifications-row");

    assert!(runtime.state.save_error.is_some());
    assert!(runtime.find("save-error").is_some());
    std::fs::remove_dir(path).unwrap();
}
