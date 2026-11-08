use super::*;
use rune::{Env, Runtime};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TEST_PATH: AtomicUsize = AtomicUsize::new(0);

#[test]
fn effective_dark_respects_appearance_override_and_system_fallback() {
    for system_dark in [false, true] {
        assert!(!effective_dark(Appearance::Light, system_dark));
        assert!(effective_dark(Appearance::Dark, system_dark));
        assert_eq!(effective_dark(Appearance::System, system_dark), system_dark);
    }
}

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
fn slow_mode_toggle_is_live_and_never_persists_config() {
    let (mut runtime, path) = runtime();
    assert!(!runtime.state.slow_mode);
    assert!(runtime.find("slow-mode-row").is_some());

    click(&mut runtime, "slow-mode-row");

    assert!(runtime.state.slow_mode);
    assert!(!path.exists(), "transient slow mode must not save Config");

    click(&mut runtime, "slow-mode-row");

    assert!(!runtime.state.slow_mode);
    assert!(!path.exists(), "toggling slow mode off must not save Config");
}

#[test]
fn autostart_toggle_is_fixture_only_and_never_persists_config() {
    let (mut runtime, path) = runtime();
    assert!(!runtime.state.autostart_enabled);
    assert!(runtime.find("autostart-row").is_some());

    click(&mut runtime, "autostart-row");

    assert!(runtime.state.autostart_enabled);
    assert_eq!(runtime.state.config.autostart_enabled, None);
    assert!(!path.exists(), "fixture autostart must not save Config");
}

#[test]
fn album_edit_is_initialized_from_and_persisted_to_config() {
    let path = test_path();
    let mut config = Config::default();
    config.album_name = "Existing album".to_owned();
    let mut state = SettingsState::new(config);
    state.save_path = Some(path.clone());
    assert_eq!(state.album_edit.text, "Existing album");

    state.album_edit.replace_text("Updated album");
    state.album_name_changed();

    assert_eq!(state.config.album_name, "Updated album");
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("album_name = \"Updated album\""));
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
