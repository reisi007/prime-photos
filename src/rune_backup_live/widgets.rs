use crate::folders::FolderStatus;
use crate::i18n::tr;
use crate::rune_theme::Palette;
use crate::upload::FileState;
use rune::tok;
use rune::{El, Icon, Tx, el, icon, row, text};
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

/// Same control as [`action_button`], but inert: no press handler at all (Rune's
/// `disabled(true)` only changes accessibility and focus, it does **not** block
/// pointer presses) and a neutral fill that does not react to hover. The key,
/// role and label stay identical so tests and readers see the same control.
pub(super) fn disabled_action_button(
    key: impl Into<String>,
    label: String,
    description: &'static str,
    theme: Theme,
) -> El {
    let mut button = el()
        .key(key)
        .role("button")
        .label(label.clone())
        .focusable(false)
        .disabled(true)
        .h(38.0)
        .px(12.0)
        .center()
        .radius(8.0)
        .fill(theme.control_track)
        .hover(theme.control_track);
    if !description.is_empty() {
        button = button.description(tr(description));
    }
    button.child(
        text(label)
            .font(tok::type_::LABEL)
            .color(theme.text_secondary)
            .a11y_hidden(),
    )
}

/// Switch in the style of `rune_settings::components::toggle_control`: a
/// visible state label plus a 38×22 track with an 18 pt knob. The state is
/// exposed through `checked`; pressing runs `action` (toggle + persist at the
/// caller).
pub(super) fn enabled_switch<S: 'static>(
    key: impl Into<String>,
    checked: bool,
    label: &'static str,
    theme: Theme,
    colors: Palette,
    action: impl Fn(&mut S, &mut Tx) + 'static,
) -> El {
    let key = key.into();
    switch_base(&key, checked, label, "", theme, colors, true).on_press_tx(action)
}

/// Inert sibling of [`enabled_switch`] for a folder whose source is missing.
/// Keeps the switch role, state and label readable, but has no press handler.
pub(super) fn disabled_switch(
    key: impl Into<String>,
    checked: bool,
    label: &'static str,
    description: &'static str,
    theme: Theme,
    colors: Palette,
) -> El {
    let key = key.into();
    switch_base(&key, checked, label, description, theme, colors, false).disabled(true)
}

fn switch_base(
    key: &str,
    checked: bool,
    label: &'static str,
    description: &'static str,
    theme: Theme,
    colors: Palette,
    interactive: bool,
) -> El {
    let mut node = row()
        .key(key.to_owned())
        .role("switch")
        .label(tr(label))
        .checked(checked)
        .focusable(interactive)
        .h(38.0)
        .px(12.0)
        .center_y()
        // `justify_end` keeps the label next to the track (instead of
        // spreading them apart when the row has slack); `S2` is the same
        // 8 pt step the surrounding action buttons use. Together they make
        // label + track one right-aligned group without widening the gap to
        // the following button.
        .gap(tok::space::S2)
        .justify_end();
    if !description.is_empty() {
        node = node.description(tr(description));
    }
    node = if interactive {
        node.hover(theme.hover).press(theme.press)
    } else {
        node.hover(theme.control_track)
    };
    node.child(
        text(tr(label))
            .key(format!("{key}-label"))
            .font(tok::type_::LABEL)
            .color(theme.text)
            .align(rune::Align::End)
            .a11y_hidden(),
    )
    .child(switch_track(key, checked, theme, colors).a11y_hidden())
}

fn switch_track(key: &str, checked: bool, theme: Theme, colors: Palette) -> El {
    const WIDTH: f32 = 38.0;
    const HEIGHT: f32 = 22.0;
    const KNOB: f32 = 18.0;
    let inset = (HEIGHT - KNOB) / 2.0;
    el()
        .key(format!("{key}-switch"))
        .size(WIDTH, HEIGHT)
        .radius(HEIGHT / 2.0)
        .fill(if checked {
            colors.toggle_on
        } else {
            colors.toggle_off
        })
        .border(1.0, colors.control_border)
        .motion(tok::motion::SNAPPY)
        .child(
            el()
                .key(format!("{key}-knob"))
                .size(KNOB, KNOB)
                .absolute(if checked { WIDTH - KNOB - inset } else { inset }, inset)
                .radius(KNOB / 2.0)
                .fill(rune::Color::WHITE)
                .shadow(1.0, 3.0, theme.knob_shadow),
        )
}

static SPINNER_RING: Icon = Icon {
    name: "spinner-ring",
    view: 20.0,
    body: r#"<circle cx="10" cy="10" r="8" stroke="currentColor" stroke-width="2" fill="none"/>"#,
};

static SPINNER_ARC: Icon = Icon {
    name: "spinner-arc",
    view: 20.0,
    body: r#"<path d="M 10 2 A 8 8 0 0 1 18 10" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round"/>"#,
};

/// Indeterminate progress indicator for an active folder. Rune has no spinner
/// widget: a static ring plus an asymmetric arc that rotates forever. Both
/// icons are decoration; the keyed parent is the accessible progressbar.
pub(super) fn upload_spinner(key: impl Into<String>, theme: Theme, colors: Palette) -> El {
    let key = key.into();
    el()
        .key(key.clone())
        .role("progressbar")
        .label(tr("Upload-Vorgang läuft"))
        .size(20.0, 20.0)
        .child(
            icon(&SPINNER_RING, 20.0)
                .key(format!("{key}-ring"))
                .color(theme.control_off)
                .absolute(0.0, 0.0)
                .a11y_hidden(),
        )
        .child(
            icon(&SPINNER_ARC, 20.0)
                .key(format!("{key}-arc"))
                .color(colors.primary_fill)
                .absolute(0.0, 0.0)
                .looping(rune::LoopSpec::rotation(0.9))
                .a11y_hidden(),
        )
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
