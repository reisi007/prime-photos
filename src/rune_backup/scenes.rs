use super::{
    BackupState, FAILED_TARGET, QueueFixture, TargetFixture, UPLOADING_QUEUE, UPLOADING_TARGETS,
    keyed_text, queue_item_status, show_add_target_notice, show_retry_notice, styled_text,
    toggle_pause,
};
use rune::tok::{self, Theme};
use rune::{El, col, el, row, text};

pub(super) fn empty_content(theme: Theme, colors: crate::rune_theme::Palette) -> El {
    col()
        .key("targets")
        .w_full()
        .grow(1.0)
        .gap(12.0)
        .child(section_heading("Sicherungsziele", theme))
        .child(
            col()
                .key("empty-state")
                .grow(1.0)
                .min_h(0.0)
                .w_full()
                .center()
                .fill(theme.card)
                .radius(16.0)
                .child(
                    col()
                        .w(560.0)
                        .gap(14.0)
                        .center()
                        .child(keyed_text(
                            "empty-icon",
                            "＋",
                            tok::TextStyle::new(34.0, 400.0),
                            colors.primary_foreground,
                        ))
                        .child(keyed_text(
                            "empty-title",
                            "Noch keine Sicherungsziele",
                            tok::type_::SECTION,
                            theme.text,
                        ))
                        .child(
                            keyed_text(
                                "empty-description",
                                "Füge einen Fotoordner hinzu, um deine Bilder für die Sicherung vorzubereiten.",
                                tok::type_::BODY,
                                theme.text_secondary,
                            )
                            .wrap(),
                        )
                        .child(action_button(
                            "add-target-cta",
                            "Ordner hinzufügen",
                            theme,
                            colors.primary_fill,
                            colors.primary_foreground,
                            show_add_target_notice,
                        )),
                ),
        )
}

pub(super) fn uploading_content(
    state: &BackupState,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let global_total = UPLOADING_TARGETS
        .iter()
        .map(|target| target.total_files)
        .sum::<usize>();
    let global_completed = UPLOADING_TARGETS
        .iter()
        .map(|target| target.completed_files)
        .sum::<usize>();
    let global_progress = global_progress(
        global_completed,
        global_total,
        state.paused,
        theme,
        colors.toggle_on,
    );

    let targets = UPLOADING_TARGETS.iter().fold(
        col()
            .key("targets")
            .w_full()
            .gap(8.0)
            .child(
                row()
                    .w_full()
                    .h(36.0)
                    .center_y()
                    .justify_between()
                    .child(section_heading("Sicherungsziele", theme))
                    .child(action_button(
                        "pause-resume",
                        if state.paused { "Fortsetzen" } else { "Anhalten" },
                        theme,
                        colors.primary_fill,
                        colors.primary_foreground,
                        toggle_pause,
                    )),
            ),
        |list, target| list.child(target_card(*target, state.paused, theme, None)),
    );

    let queue = UPLOADING_QUEUE.iter().fold(
        col()
            .key("queue")
            .w_full()
            .gap(6.0)
            .child(section_heading("Warteschlange · 15 Dateien", theme)),
        |list, item| list.child(queue_row(*item, state.paused, theme, None)),
    );

    col()
        .key("uploading-scene")
        .w_full()
        .grow(1.0)
        .gap(12.0)
        .child(global_progress)
        .child(targets)
        .child(queue)
}

pub(super) fn failure_content(theme: Theme, colors: crate::rune_theme::Palette) -> El {
    let targets = col()
        .key("targets")
        .w_full()
        .gap(8.0)
        .child(section_heading("Sicherungsziele", theme))
        .child(target_card(
            FAILED_TARGET,
            false,
            theme,
            Some(colors.alert_foreground),
        ));

    let failed_row = col()
        .key("queue")
        .w_full()
        .gap(8.0)
        .child(section_heading("Warteschlange", theme))
        .child(
            row()
                .key("failed-queue-row")
                .w_full()
                .h(62.0)
                .px(14.0)
                .gap(12.0)
                .center_y()
                .fill(theme.card)
                .radius(10.0)
                .child(
                    col()
                        .grow(1.0)
                        .gap(3.0)
                        .child(
                            text("Beispiel_003.png")
                                .key("failed-file-name")
                                .font(tok::type_::BODY)
                                .color(theme.text),
                        )
                        .child(
                            text("Upload fehlgeschlagen")
                                .key("failed-file-status")
                                .font(tok::type_::LABEL)
                                .color(colors.alert_foreground),
                        ),
                )
                .child(action_button(
                    "retry-failed",
                    "Erneut versuchen",
                    theme,
                    colors.primary_fill,
                    colors.primary_foreground,
                    show_retry_notice,
                )),
        );

    col()
        .key("failure-scene")
        .w_full()
        .gap(14.0)
        .child(keyed_text(
            "failure-summary",
            "Ein Sicherungsziel benötigt Aufmerksamkeit.",
            tok::type_::BODY,
            theme.text_secondary,
        ))
        .child(targets)
        .child(failed_row)
}

fn global_progress(
    completed: usize,
    total: usize,
    paused: bool,
    theme: Theme,
    accent: rune::Color,
) -> El {
    let mut meter = row()
        .key("global-progress-meter")
        .w_full()
        .h(8.0)
        .gap(3.0);
    for index in 0..total {
        meter = meter.child(
            el()
                .grow(1.0)
                .h(8.0)
                .fill(if index < completed {
                    accent
                } else {
                    theme.control_track
                })
                .radius(4.0),
        );
    }

    col()
        .key("global-progress")
        .w_full()
        .padding(14.0)
        .gap(10.0)
        .fill(theme.card)
        .radius(12.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(styled_text(
                    "Gesamtfortschritt",
                    tok::type_::BODY,
                    theme.text,
                ))
                .child(keyed_text(
                    "global-progress-status",
                    if paused {
                        "Angehalten"
                    } else {
                        "Upload-Vorgang läuft"
                    },
                    tok::type_::LABEL,
                    theme.text_secondary,
                )),
        )
        .child(meter)
        .child(keyed_text(
            "global-progress-count",
            format!(
                "{completed} von {total} Dateien abgeschlossen · Fortschritt nach Dateien gezählt"
            ),
            tok::type_::LABEL,
            theme.text_secondary,
        ))
}

fn section_heading(title: &'static str, theme: Theme) -> El {
    text(title)
        .font(tok::type_::SECTION)
        .color(theme.text)
}

fn target_card(
    target: TargetFixture,
    paused: bool,
    theme: Theme,
    alert: Option<rune::Color>,
) -> El {
    let status = if paused && target.status == "Upload-Vorgang läuft" {
        "ANGEHALTEN"
    } else {
        target.status
    };
    let status_color = alert.unwrap_or(theme.text_secondary);

    col()
        .key(target.key)
        .w_full()
        .padding(12.0)
        .gap(6.0)
        .fill(theme.card)
        .radius(10.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(keyed_text(
                    format!("{}-name", target.key),
                    target.name,
                    tok::type_::BODY,
                    theme.text,
                ))
                .child(keyed_text(
                    format!("{}-status", target.key),
                    status,
                    tok::type_::LABEL,
                    status_color,
                )),
        )
        .child(keyed_text(
            format!("{}-progress", target.key),
            format!(
                "{} von {} Dateien abgeschlossen",
                target.completed_files, target.total_files
            ),
            tok::type_::LABEL,
            theme.text_secondary,
        ))
}

fn queue_row(
    item: QueueFixture,
    paused: bool,
    theme: Theme,
    status_color: Option<rune::Color>,
) -> El {
    row()
        .key(item.key)
        .w_full()
        .h(38.0)
        .px(14.0)
        .center_y()
        .justify_between()
        .fill(theme.card)
        .radius(8.0)
        .child(keyed_text(
            format!("{}-name", item.key),
            item.name,
            tok::type_::LABEL,
            theme.text,
        ))
        .child(keyed_text(
            format!("{}-status", item.key),
            queue_item_status(item, paused),
            tok::type_::LABEL,
            status_color.unwrap_or(theme.text_secondary),
        ))
}

fn action_button(
    key: &'static str,
    label: &'static str,
    theme: Theme,
    fill: rune::Color,
    foreground: rune::Color,
    action: fn(&mut BackupState),
) -> El {
    el()
        .key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .h(44.0)
        .px(18.0)
        .center()
        .radius(9.0)
        .fill(fill)
        .hover(theme.hover)
        .on_press(action)
        .child(styled_text(label, tok::TextStyle::new(14.0, 650.0), foreground).a11y_hidden())
}
