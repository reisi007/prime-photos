use crate::backup::answer_pending_conflict;
use crate::folders::ConflictResolution;
use crate::i18n::{tr, tr1};
use rune::tok;
use rune::{El, col, text};
use tok::Theme;

use super::widgets::action_button;
use super::LiveState;

pub(super) fn conflict_panel(
    file_name: String,
    remember: bool,
    remember_label: String,
    theme: Theme,
    primary_fill: rune::Color,
    primary_foreground: rune::Color,
    alert_foreground: rune::Color,
    danger_fill: rune::Color,
    danger_foreground: rune::Color,
) -> El {
    col()
        .key("conflict-decision")
        .w_full()
        .padding(14.0)
        .gap(8.0)
        .fill(theme.card)
        .radius(12.0)
        .child(text(tr("Dateiname-Konflikt")).font(tok::type_::SECTION).color(alert_foreground))
        .child(
            text(tr1("„{}“ existiert bereits in Amazon Photos.", &file_name))
                .font(tok::type_::BODY)
                .color(theme.text),
        )
        .child(action_button(
            "remember-conflict",
            if remember {
                format!("✓ {remember_label}")
            } else {
                remember_label
            },
            theme,
            theme.control_track,
            theme.text,
            |state: &mut LiveState, _| {
                let mut upload = state.upload.lock().unwrap();
                upload.conflict_remember = !upload.conflict_remember;
            },
        ))
        .child(action_button(
            "conflict-ignore",
            tr("Ignorieren").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut LiveState, _| answer_conflict(state, ConflictResolution::Ignore),
        ))
        .child(action_button(
            "conflict-save-new",
            tr("Als neue Datei speichern").to_owned(),
            theme,
            primary_fill,
            primary_foreground,
            |state: &mut LiveState, _| answer_conflict(state, ConflictResolution::SaveAsNew),
        ))
        .child(action_button(
            "conflict-replace",
            tr("Ersetzen").to_owned(),
            theme,
            danger_fill,
            danger_foreground,
            |state: &mut LiveState, _| answer_conflict(state, ConflictResolution::Replace),
        ))
}

fn answer_conflict(state: &mut LiveState, answer: ConflictResolution) {
    let remember = state.upload.lock().unwrap().conflict_remember;
    answer_pending_conflict(
        &mut state.store,
        &state.upload,
        answer,
        remember,
        state.persist_store,
    );
}
