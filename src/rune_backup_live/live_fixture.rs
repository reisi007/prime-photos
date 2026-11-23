//! Debug-only headless fixture: the **production** `live_view` with synthetic
//! data. Nothing here reads config, keychain, SQLite, the network or real
//! folders, and no upload or native window is ever started.

use super::test_support::signed_out_state;
use super::view;
use super::{LivePage, LiveState, WINDOW_SIZE};
use crate::config::{Appearance, Config};
use crate::folders::{BackupFolder, FolderStatus};
use crate::i18n::{self, Lang};
use crate::upload::{FileState, QueueEntry};
use rune::{App, AppSpec};
use std::path::PathBuf;

const FIXTURE_NOTICE: &str =
    "Synthetische Fixture-Daten – keine echten Ordner, Konten oder Uploads.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiveFixtureScene {
    Mixed,
    Empty,
    Settings,
    About,
    Licenses,
}

const SCENES: &[(&str, fn(&mut LiveState))] = &[
    ("mixed", |state| show_scene(state, LiveFixtureScene::Mixed)),
    ("empty", |state| show_scene(state, LiveFixtureScene::Empty)),
    ("settings", |state| show_scene(state, LiveFixtureScene::Settings)),
    ("about", |state| show_scene(state, LiveFixtureScene::About)),
    ("licenses", |state| show_scene(state, LiveFixtureScene::Licenses)),
];

/// Headless entry point for `--rune-backup-live-fixture`. The route parser has
/// already stripped its marker and checked that a headless action is present.
pub(crate) fn run(args: Vec<String>) -> Result<(), String> {
    // Screenshots must not depend on the user's locale.
    i18n::set_lang(Lang::De);
    let parsed = rune::tooling::Args::parse_from(args.clone());
    let appearance = if parsed.theme.as_deref() == Some("light") {
        Appearance::Light
    } else {
        Appearance::Dark
    };
    let spec = AppSpec {
        scenes: SCENES,
        ..AppSpec::new(
            "Prime Photos",
            WINDOW_SIZE,
            default_fixture_state,
            view::live_view,
        )
    };

    App::new(spec)
        .init(move || fixture_state(LiveFixtureScene::Mixed, appearance))
        .args(args)
        .try_run()
}

fn default_fixture_state() -> LiveState {
    fixture_state(LiveFixtureScene::Mixed, Appearance::System)
}

fn show_scene(state: &mut LiveState, scene: LiveFixtureScene) {
    let appearance = state.config.appearance;
    *state = fixture_state(scene, appearance);
}

fn fixture_state(scene: LiveFixtureScene, appearance: Appearance) -> LiveState {
    let mut state = signed_out_state();
    state.config = Config::default();
    state.config.appearance = appearance;
    state.config.language = Some("de".to_owned());
    state.config.album_name = "Synthetisches Album".to_owned();
    // Fake session material: the production view shows its signed-in layout,
    // while `persist_config`/`persist_store` stay false, so nothing is written.
    state.config.session_id = "synthetic-session-id".to_owned();
    state.config.cookie_x_acb = "synthetic-x-acb-cookie".to_owned();
    state.settings_limit_edit = rune::TextEdit::new(state.config.limit_kbps.to_string());
    state.settings_album_edit = rune::TextEdit::new(&state.config.album_name);
    state.settings_slider_origin = state.config.concurrency();
    state.page = match scene {
        LiveFixtureScene::Settings => LivePage::Settings,
        LiveFixtureScene::About => LivePage::About,
        LiveFixtureScene::Licenses => LivePage::Licenses,
        LiveFixtureScene::Mixed | LiveFixtureScene::Empty => LivePage::Backup,
    };
    state.status = Some(FIXTURE_NOTICE.to_owned());

    if scene != LiveFixtureScene::Empty {
        add_synthetic_folders(&mut state);
        add_synthetic_upload(&mut state);
    }
    state
}

/// Five folders covering every card status the production `target_card` can
/// show: an active upload, a finished one, a missing source, an error and a
/// disabled target.
fn add_synthetic_folders(state: &mut LiveState) {
    let folders = [
        (
            "/synthetic/photos-2024",
            "SYNTH Fotos 2024",
            FolderStatus::Uploading,
            true,
            63,
            13,
        ),
        (
            "/synthetic/familie-2019",
            "SYNTH Familie 2019",
            FolderStatus::Done,
            true,
            62,
            62,
        ),
        (
            "/synthetic/externe-daten",
            "SYNTH Externe Daten",
            FolderStatus::NotFound,
            true,
            918,
            918,
        ),
        (
            "/synthetic/export-fehler",
            "SYNTH Export mit Fehler",
            FolderStatus::Error,
            true,
            310,
            290,
        ),
        (
            "/synthetic/archiv-2018",
            "SYNTH Archiv 2018",
            FolderStatus::Idle,
            false,
            24,
            24,
        ),
    ];
    for (path, name, status, enabled, total, uploaded) in folders {
        let mut folder = BackupFolder::new(PathBuf::from(path));
        folder.name = name.to_owned();
        folder.status = status;
        folder.enabled = enabled;
        folder.total_files = total;
        folder.uploaded_files = uploaded;
        folder.last_updated = "01.01.2026 12:00".to_owned();
        state.store.folders.push(folder);
    }
}

/// A fake *active* run: this is what makes the real `target_card` draw its
/// spinner and disable removal of the folders taking part in the run.
fn add_synthetic_upload(state: &mut LiveState) {
    let paths: Vec<PathBuf> = state
        .store
        .folders
        .iter()
        .map(|folder| folder.path.clone())
        .collect();
    let mut upload = state.upload.lock().unwrap();
    upload.active = true;
    upload.total = 187;
    upload.done = 37;
    upload.failed = 1;
    upload.uploads_in_flight = 1;
    upload.current = "IMG_2043.HEIC".to_owned();
    for (index, path) in paths.iter().enumerate() {
        upload.folder_paths.insert(index, path.clone());
    }
    for (index, (done, total)) in [(0, (13, 63)), (1, (62, 62)), (2, (918, 918)), (3, (290, 310))]
    {
        upload.folder_done.insert(index, done);
        upload.folder_total.insert(index, total);
    }
    upload.queue = vec![
        queue_entry(0, "IMG_2043.HEIC", FileState::Uploading, 2_400_000, 600_000),
        queue_entry(1, "IMG_2044.HEIC", FileState::Done, 1_800_000, 1_800_000),
        queue_entry(0, "IMG_2045.HEIC", FileState::Waiting, 0, 0),
        queue_entry(3, "IMG_2046.HEIC", FileState::Failed, 900_000, 0),
    ];
}

fn queue_entry(
    folder: usize,
    name: &str,
    state: FileState,
    size: u64,
    bytes_sent: u64,
) -> QueueEntry {
    QueueEntry {
        folder,
        name: name.to_owned(),
        state,
        size,
        bytes_sent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rune_backup_live::test_support::{a11y_contains_text, find_a11y_node};
    use rune::{Env, Runtime};

    /// Tall enough that every target card is laid out and `presented` reports
    /// real boxes, including the last folder.
    fn live_runtime(scene: LiveFixtureScene) -> Runtime<LiveState> {
        let mut runtime = Runtime::new(
            fixture_state(scene, Appearance::Dark),
            view::live_view,
            (1400.0, 1800.0),
            Env::new(true, 1.0),
        );
        runtime.frame(0.0);
        runtime
    }

    fn drawn_rect(runtime: &Runtime<LiveState>, key: &str) -> [f32; 4] {
        runtime
            .presented(key)
            .unwrap_or_else(|| panic!("{key} is drawn"))
            .0
    }

    fn hover_track(runtime: &Runtime<LiveState>, key: &str) -> f32 {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("{key} is presented"))
            .hover
    }

    fn hover_over(runtime: &mut Runtime<LiveState>, key: &str) -> f32 {
        let (x, y) = runtime
            .find(key)
            .unwrap_or_else(|| panic!("missing Rune control {key:?}"));
        runtime.pointer_move(x, y);
        runtime.frame(1.0 / 60.0);
        runtime.frame(0.25);
        hover_track(runtime, key)
    }

    #[test]
    fn mixed_scene_renders_the_real_target_cards_with_fake_data() {
        let runtime = live_runtime(LiveFixtureScene::Mixed);
        let json = runtime.accessibility_json();

        // Keys of the production view, not the retired smoke parallel UI.
        assert!(find_a11y_node(&json, "live-targets").is_some());
        assert!(find_a11y_node(&json, "target-card-0").is_some());
        assert!(find_a11y_node(&json, "upload-progress").is_some());
        assert!(find_a11y_node(&json, "live-queue").is_some());

        // Real `target_card` statuses and synthetic paths/files.
        assert!(a11y_contains_text(&json, "Upload-Vorgang läuft"));
        assert!(a11y_contains_text(&json, "Abgeschlossen"));
        assert!(a11y_contains_text(&json, "Ordner nicht gefunden"));
        assert!(a11y_contains_text(&json, "/synthetic/photos-2024"));
        assert!(a11y_contains_text(&json, "IMG_2043.HEIC"));

        // Spinner comes from the live run in `target_card`, not from a fake card.
        let spinner = find_a11y_node(&json, "target-spinner-0").expect("spinner is accessible");
        assert_eq!(spinner["role"], "progress-indicator");
        assert!(runtime.find("target-spinner-0").is_some());
    }

    #[test]
    fn empty_scene_has_no_target_cards_and_other_scenes_switch_pages() {
        let empty = live_runtime(LiveFixtureScene::Empty);
        let json = empty.accessibility_json();
        assert!(find_a11y_node(&json, "empty-targets").is_some());
        assert!(find_a11y_node(&json, "target-card-0").is_none());

        for (scene, key) in [
            (LiveFixtureScene::Settings, "settings-content-root"),
            (LiveFixtureScene::About, "rune-about-page"),
            (LiveFixtureScene::Licenses, "rune-licenses-page"),
        ] {
            let runtime = live_runtime(scene);
            assert!(
                find_a11y_node(&runtime.accessibility_json(), key).is_some(),
                "{key} must be rendered"
            );
        }
    }

    #[test]
    fn fixture_cards_keep_right_aligned_switch_group_and_inert_hover() {
        let mut runtime = live_runtime(LiveFixtureScene::Mixed);
        let upload = drawn_rect(&runtime, "target-upload-0");
        let label = drawn_rect(&runtime, "target-enabled-0-label");
        let options = drawn_rect(&runtime, "target-edit-0");
        let remove = drawn_rect(&runtime, "target-remove-0");
        let switch = drawn_rect(&runtime, "target-enabled-0");
        let track = drawn_rect(&runtime, "target-enabled-0-switch");
        let card = drawn_rect(&runtime, "target-card-0");

        assert!(((switch[0] - (label[0] + label[2])) - 8.0).abs() <= 1.0);
        assert!(((label[1] + label[3] / 2.0) - (switch[1] + switch[3] / 2.0)).abs() <= 1.0);
        assert!(((switch[0] + switch[2]) - (card[0] + card[2] - 12.0)).abs() <= 1.0);
        assert!((track[0] - switch[0]).abs() <= 0.5);
        assert!(
            upload[0] + upload[2] < options[0]
                && options[0] + options[2] < remove[0]
                && remove[0] + remove[2] < label[0]
        );

        // NotFound folder 2 disables upload, switch and (in this live run)
        // remove; none of them may paint a hover overlay.
        for key in ["target-upload-2", "target-enabled-2", "target-remove-2"] {
            assert!(
                hover_over(&mut runtime, key) <= 0.001,
                "{key} must not hover while disabled"
            );
        }
        assert!(
            hover_over(&mut runtime, "target-edit-2") > 0.9,
            "an enabled neighbour must still hover"
        );
    }
}
