//! Headless-only backup-page fixture for the Rune migration (Issue #49).
//!
//! This is synthetic UI test evidence only. It deliberately has no connection
//! to configuration, folders, filesystems, account data, persistence, or the
//! upload pipeline.

use crate::i18n::{self, Lang};
use rune::tok;
use rune::{App, AppSpec, Cx, El, LiveRegion, col, el, row, text};

#[cfg(test)]
mod tests;
mod scenes;

const WINDOW_SIZE: (f32, f32) = (1120.0, 780.0);
const ADD_TARGET_NOTICE: &str =
    "Fixture-only: Ziel hinzufügen ist hier nur eine lokale Demo.";
const RETRY_NOTICE: &str = "Fixture-only: Wiederholen startet keinen Upload.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scene {
    #[default]
    Empty,
    Uploading,
    Failure,
}

#[derive(Default)]
struct BackupState {
    scene: Scene,
    paused: bool,
    notice: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TargetFixture {
    key: &'static str,
    name: &'static str,
    completed_files: usize,
    total_files: usize,
    status: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QueueFixture {
    key: &'static str,
    name: &'static str,
    status: &'static str,
    paused_status: Option<&'static str>,
}

const UPLOADING_TARGETS: [TargetFixture; 2] = [
    TargetFixture {
        key: "target-family",
        name: "Familienfotos",
        completed_files: 3,
        total_files: 12,
        status: "Upload-Vorgang läuft",
    },
    TargetFixture {
        key: "target-trips",
        name: "Reisebilder",
        completed_files: 0,
        total_files: 6,
        status: "Warten",
    },
];

const UPLOADING_QUEUE: [QueueFixture; 3] = [
    QueueFixture {
        key: "queue-row-1",
        name: "Beispiel_001.jpg",
        status: "Wird übertragen",
        paused_status: Some("Angehalten"),
    },
    QueueFixture {
        key: "queue-row-2",
        name: "Beispiel_002.jpg",
        status: "Warten",
        paused_status: None,
    },
    QueueFixture {
        key: "queue-row-3",
        name: "Beispiel_003.png",
        status: "Warten",
        paused_status: None,
    },
];

const FAILED_TARGET: TargetFixture = TargetFixture {
    key: "target-failed",
    name: "Archiv-Beispiele",
    completed_files: 2,
    total_files: 5,
    status: "Fehler",
};

pub(crate) fn has_headless_action(args: &[String]) -> bool {
    let args = rune::tooling::Args::parse_from(args.to_vec());
    args.headless() && (args.snap.is_some() || args.tree.is_some() || args.script.is_some())
}

pub(crate) fn run(args: Vec<String>) -> Result<(), String> {
    if !has_headless_action(&args) {
        return Err(
            "usage: --rune-backup-fixture requires --snap FILE, --tree FILE, or --script FILE"
                .to_owned(),
        );
    }

    // A fixed language makes fixture content independent of the user's locale.
    i18n::set_lang(Lang::De);

    let spec = AppSpec {
        scenes: &[
            ("empty", empty_scene),
            ("uploading", uploading_scene),
            ("failure", failure_scene),
        ],
        ..AppSpec::new("Prime Photos", WINDOW_SIZE, BackupState::default, view)
    };

    App::new(spec)
        .init(BackupState::default)
        .args(args)
        .try_run()
}

fn empty_scene(state: &mut BackupState) {
    *state = BackupState {
        scene: Scene::Empty,
        ..BackupState::default()
    };
}

fn uploading_scene(state: &mut BackupState) {
    *state = BackupState {
        scene: Scene::Uploading,
        ..BackupState::default()
    };
}

fn failure_scene(state: &mut BackupState) {
    *state = BackupState {
        scene: Scene::Failure,
        ..BackupState::default()
    };
}

fn view(state: &BackupState, cx: &mut Cx<BackupState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    let accent = tok::accent(0, dark);
    let alert = tok::accent(2, dark);

    let content = match state.scene {
        Scene::Empty => scenes::empty_content(theme, accent),
        Scene::Uploading => scenes::uploading_content(state, theme, accent),
        Scene::Failure => scenes::failure_content(theme, accent, alert),
    };

    let mut footer = row()
        .key("fixture-footer")
        .w_full()
        .h(30.0)
        .px(28.0)
        .center_y()
        .child(styled_text(
            "Headless-Testfixture · ausschließlich synthetische Daten",
            tok::type_::LABEL,
            theme.text_secondary,
        ));
    if let Some(notice) = state.notice {
        footer = footer.child(
            keyed_text("fixture-notice", notice, tok::type_::LABEL, alert)
                .role("status")
                .live_region(LiveRegion::Polite),
        );
    }

    col()
        .key("screen")
        .role("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(
            row()
                .key("page-header")
                .w_full()
                .h(72.0)
                .px(28.0)
                .center_y()
                .justify_between()
                .child(
                    col()
                        .gap(3.0)
                        .child(keyed_text(
                            "page-title",
                            "Datensicherung",
                            tok::type_::TITLE,
                            theme.text,
                        ))
                        .child(keyed_text(
                            "page-subtitle",
                            "Ziele und Upload-Warteschlange",
                            tok::type_::LABEL,
                            theme.text_secondary,
                        )),
                )
                .child(keyed_text(
                    "fixture-badge",
                    "TESTANSICHT",
                    tok::type_::LABEL,
                    theme.text_secondary,
                )),
        )
        .child(el().key("header-divider").w_full().h(1.0).fill(theme.separator))
        .child(
            col()
                .key("backup-content")
                .grow(1.0)
                .min_h(0.0)
                .w_full()
                .padding(24.0)
                .gap(14.0)
                .child(content),
        )
        .child(footer)
}

fn queue_item_status(item: QueueFixture, paused: bool) -> &'static str {
    if paused {
        item.paused_status.unwrap_or(item.status)
    } else {
        item.status
    }
}

fn styled_text(value: impl Into<String>, font: tok::TextStyle, color: rune::Color) -> El {
    text(value).font(font).color(color)
}

fn keyed_text(
    key: impl Into<String>,
    value: impl Into<String>,
    font: tok::TextStyle,
    color: rune::Color,
) -> El {
    styled_text(value, font, color).key(key)
}

fn show_add_target_notice(state: &mut BackupState) {
    state.notice = Some(ADD_TARGET_NOTICE);
}

fn toggle_pause(state: &mut BackupState) {
    state.paused = !state.paused;
}

fn show_retry_notice(state: &mut BackupState) {
    state.notice = Some(RETRY_NOTICE);
}
