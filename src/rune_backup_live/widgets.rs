use crate::folders::FolderStatus;
use crate::i18n::tr;
use crate::upload::FileState;
use rune::tok;
use rune::{El, Tx, el, text};
use tok::Theme;

pub(super) fn action_button<S: 'static>(
    key: impl Into<String>,
    label: String,
    theme: Theme,
    fill: rune::Color,
    foreground: rune::Color,
    action: impl Fn(&mut S, &mut Tx) + 'static,
) -> El {
    el()
        .key(key)
        .role("button")
        .label(label.clone())
        .focusable(true)
        .h(38.0)
        .px(12.0)
        .center()
        .radius(8.0)
        .fill(fill)
        .hover(theme.hover)
        .on_press_tx(action)
        .child(text(label).font(tok::type_::LABEL).color(foreground).a11y_hidden())
}

pub(super) fn section_heading(title: &str, theme: Theme) -> El {
    text(title.to_owned()).font(tok::type_::SECTION).color(theme.text)
}

pub(super) fn file_state_text(state: FileState) -> &'static str {
    match state {
        FileState::Waiting => tr("Warten"),
        FileState::Uploading => tr("Wird übertragen"),
        FileState::Done => tr("Abgeschlossen"),
        FileState::Failed => tr("Fehler"),
        FileState::Skipped => tr("Übersprungen"),
    }
}

/// Render rank of a file state in the live backup queue: files being uploaded
/// first, then failures, waiting files, completed files and finally skips.
pub(super) fn state_rank(state: FileState) -> u8 {
    match state {
        FileState::Uploading => 0,
        FileState::Failed => 1,
        FileState::Waiting => 2,
        FileState::Done => 3,
        FileState::Skipped => 4,
    }
}

/// Row indices `0..len` sorted stably by [`state_rank`], so the queue renders in
/// status order while each status bucket keeps its insertion order. The queue
/// itself stays untouched: workers address rows by their original index.
pub(super) fn queue_render_order(states: &[FileState]) -> Vec<usize> {
    let mut order = (0..states.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i| state_rank(states[i]));
    order
}

pub(super) fn folder_status_text(status: FolderStatus, enabled: bool) -> &'static str {
    if !enabled {
        return tr("Deaktiviert");
    }
    match status {
        FolderStatus::Idle => tr("Bereit"),
        FolderStatus::Uploading => tr("Upload-Vorgang läuft"),
        FolderStatus::Done => tr("Abgeschlossen"),
        FolderStatus::Paused => tr("Angehalten"),
        FolderStatus::NotFound => tr("Ordner nicht gefunden"),
        FolderStatus::Error => tr("Fehler"),
        FolderStatus::Scanning => tr("Wird eingelesen"),
    }
}
