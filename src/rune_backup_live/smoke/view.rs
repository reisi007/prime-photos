use super::{QueueStatus, SmokeQueueItem, SmokeState, SmokeTarget, SmokeScene, TargetStatus};
use super::super::{HEADER_HEIGHT, LivePage};
use crate::i18n::{tr, tr1, tr2};
use rune::tok;
use rune::{Cx, El, ScrollSpec, col, row, text};

pub(super) fn smoke_view(state: &SmokeState, cx: &mut Cx<SmokeState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);
    let colors = crate::rune_theme::palette(dark);
    cx.foreground(theme.text);
    cx.focus_ring(2.0, 2.0, colors.focus_ring);
    let inset = cx.titlebar_inset();
    let (page_title, page_title_key) = match state.page {
        LivePage::Backup => (tr("Datensicherung · Rune-Review"), "smoke-title-backup"),
        LivePage::Settings => (tr("Einstellungen"), "smoke-title-settings"),
        LivePage::About => (tr("Über Prime Photos"), "smoke-title-about"),
        LivePage::Licenses => (tr("Drittanbieterlizenzen"), "smoke-title-licenses"),
    };
    let content = if matches!(state.page, LivePage::About | LivePage::Licenses) {
        super::super::about::route_content(state, theme, colors)
    } else {
        backup_content(state, theme, colors)
    };
    let header = row()
        .key("smoke-header")
        .w_full()
        .h(HEADER_HEIGHT)
        .pl(inset + 24.0)
        .pr(24.0)
        .center_y()
        .justify_between()
        .fill(theme.card)
        .window_drag()
        .child(
            col()
                .gap(2.0)
                .child(text("Prime Photos").font(tok::type_::TITLE).color(theme.text))
                .child(
                    text(page_title)
                        .key(page_title_key)
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    text(format!("Rune · Szene: {}", state.scene.id()))
                        .key("smoke-scene-name")
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .child(
            text("SYNTHETISCHE DATEN")
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        );
    col()
        .key("rune-backup-smoke")
        .role("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(header)
        .child(content)
}

fn backup_content(
    state: &SmokeState,
    theme: tok::Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let mut targets = col()
        .key("smoke-targets")
        .w_full()
        .gap(10.0)
        .child(section_heading(tr("Sicherungsziele"), theme));
    for (index, target) in state.targets.iter().enumerate() {
        targets = targets.child(target_card(state, index, target, theme, colors));
    }
    if state.targets.is_empty() {
        targets = targets.child(
            text(tr("Noch keine Sicherungsziele in dieser synthetischen Ansicht."))
                .key("smoke-empty-targets")
                .font(tok::type_::BODY)
                .color(theme.text_secondary)
                .wrap(),
        );
    }

    let mut queue = col()
        .key("smoke-queue")
        .w_full()
        .gap(6.0)
        .child(section_heading(
            &tr1("Warteschlange ({})", state.queue.len()),
            theme,
        ));
    for (index, item) in state.queue.iter().enumerate() {
        queue = queue.child(queue_row(state, index, item, theme, colors));
    }
    if state.queue.is_empty() {
        queue = queue.child(
            text(tr("Die synthetische Warteschlange ist leer."))
                .key("smoke-empty-queue")
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        );
    }

    let controls = row()
        .key("smoke-controls")
        .w_full()
        .gap(8.0)
        .child(super::super::widgets::action_button(
            "smoke-pause",
            if state.paused {
                tr("Fortsetzen")
            } else {
                tr("Anhalten")
            }
            .to_owned(),
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            |state: &mut SmokeState, _| {
                state.paused = !state.paused;
                state.notice = if state.paused {
                    "Nur der lokale Fixture-Zustand wurde pausiert."
                } else {
                    "Nur der lokale Fixture-Zustand wurde fortgesetzt."
                }
                .to_owned();
            },
        ))
        .child(super::super::widgets::action_button(
            "smoke-advance",
            tr("Demo-Schritt").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut SmokeState, _| {
                if !state.paused {
                    state.progress_done = (state.progress_done + 1).min(state.progress_total);
                    if let Some(item) = state
                        .queue
                        .iter_mut()
                        .find(|item| item.status == QueueStatus::Uploading)
                    {
                        item.status = QueueStatus::Done;
                    }
                    state.notice = "Synthetischer Fortschritt wurde lokal verschoben.".to_owned();
                }
            },
        ))
        .child(super::super::widgets::action_button(
            "smoke-add-target",
            tr("Beispielziel hinzufügen").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut SmokeState, _| {
                let number = state.targets.len() + 1;
                state.targets.push(SmokeTarget {
                    name: format!("Fixture-Ordner {number}"),
                    completed: 0,
                    total: 0,
                    status: TargetStatus::NotScanned,
                });
                state.notice = "Synthetisches Ziel hinzugefügt; keine Datei wurde gelesen."
                    .to_owned();
            },
        ))
        .child(super::super::widgets::action_button(
            "smoke-scene",
            tr("Ansicht wechseln").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut SmokeState, _| {
                let next = state.scene.next_backup();
                state.show_scene(next);
            },
        ))
        .child(super::super::widgets::action_button(
            "smoke-retry",
            tr("Fehler erneut versuchen").to_owned(),
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            |state: &mut SmokeState, _| {
                state.retry_count += 1;
                if let Some(target) = state
                    .targets
                    .iter_mut()
                    .find(|target| target.status == TargetStatus::Error)
                {
                    target.status = TargetStatus::Waiting;
                }
                if let Some(item) = state
                    .queue
                    .iter_mut()
                    .find(|item| item.status == QueueStatus::Failed)
                {
                    item.status = QueueStatus::Waiting;
                }
                state.notice = format!(
                    "Lokaler Retry-Klick {} – kein Upload gestartet.",
                    state.retry_count
                );
            },
        ));

    let progress_status = if state.paused {
        tr("Angehalten")
    } else if state.progress_total > 0 {
        tr("Upload-Vorgang läuft")
    } else if state.scene == SmokeScene::AllDone {
        tr("Alles aktuell")
    } else if state.scene == SmokeScene::Failures {
        tr("Fehler")
    } else {
        tr("Bereit")
    };
    col()
        .key("smoke-content")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .w_full()
                .gap(14.0)
                .padding(20.0)
                .child(
                    col()
                        .key("smoke-progress")
                        .w_full()
                        .padding(14.0)
                        .gap(6.0)
                        .fill(theme.card)
                        .radius(12.0)
                        .child(
                            row()
                                .w_full()
                                .justify_between()
                                .child(section_heading(tr("Gesamtfortschritt"), theme))
                                .child(
                                    text(progress_status)
                                        .key("smoke-progress-status")
                                        .font(tok::type_::LABEL)
                                        .color(theme.text_secondary),
                                ),
                        )
                        .child(
                            text(tr2(
                                "{} von {} Dateien abgeschlossen",
                                state.progress_done,
                                state.progress_total,
                            ))
                            .key("smoke-progress-count")
                            .font(tok::type_::BODY)
                            .color(theme.text),
                        )
                        .child(
                            text(state.notice.clone())
                                .key("smoke-notice")
                                .font(tok::type_::LABEL)
                                .color(if state.scene == SmokeScene::Failures {
                                    colors.alert_foreground
                                } else {
                                    theme.text_secondary
                                })
                                .wrap(),
                        ),
                )
                .child(controls)
                .child(targets)
                .child(queue)
                .child(col().grow(1.0)),
        )
}

fn target_card(
    state: &SmokeState,
    index: usize,
    target: &SmokeTarget,
    theme: tok::Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let status = match target.status {
        TargetStatus::Uploading if state.paused => tr("Angehalten"),
        TargetStatus::Uploading => tr("Upload-Vorgang läuft"),
        TargetStatus::Done => tr("Abgeschlossen"),
        TargetStatus::Waiting => tr("Warten"),
        TargetStatus::Error => tr("Fehler"),
        TargetStatus::NotScanned => "",
        TargetStatus::Missing => tr("Ordner nicht gefunden"),
    };
    let status_color = if target.status == TargetStatus::Error {
        colors.alert_foreground
    } else {
        theme.text_secondary
    };
    col()
        .key(format!("smoke-target-{index}"))
        .w_full()
        .padding(13.0)
        .gap(6.0)
        .fill(theme.card)
        .radius(10.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(text(target.name.clone()).font(tok::type_::BODY).color(theme.text))
                .child(
                    text(status)
                        .key(format!("smoke-target-status-{index}"))
                        .font(tok::type_::LABEL)
                        .color(status_color),
                ),
        )
        .child(
            text(tr2(
                "{} von {} Dateien abgeschlossen",
                target.completed,
                target.total,
            ))
            .font(tok::type_::LABEL)
            .color(theme.text_secondary),
        )
}

fn queue_row(
    state: &SmokeState,
    index: usize,
    item: &SmokeQueueItem,
    theme: tok::Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let (label, color) = match item.status {
        QueueStatus::Done => (tr("Abgeschlossen").to_owned(), theme.text_secondary),
        QueueStatus::Uploading if state.paused => (tr("Angehalten").to_owned(), theme.text_secondary),
        QueueStatus::Uploading => (format!("{} %", item.percent), theme.text_secondary),
        QueueStatus::Waiting => (tr("Warten").to_owned(), theme.text_secondary),
        QueueStatus::Failed => (tr("Fehler").to_owned(), colors.alert_foreground),
    };
    row()
        .key(format!("smoke-queue-{index}"))
        .w_full()
        .h(36.0)
        .px(12.0)
        .center_y()
        .justify_between()
        .fill(theme.card)
        .radius(8.0)
        .child(
            text(format!("{} · {}", item.name, item.folder))
                .font(tok::type_::LABEL)
                .color(theme.text),
        )
        .child(text(label).font(tok::type_::LABEL).color(color))
}

fn section_heading(title: &str, theme: tok::Theme) -> El {
    text(title.to_owned())
        .font(tok::type_::SECTION)
        .color(theme.text)
}
