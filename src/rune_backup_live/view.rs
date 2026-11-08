use super::account_identity;
use super::targets::{empty_targets, target_card, target_form};
use super::widgets::{action_button, file_state_text, section_heading};
use super::one_time::{backup_row, drop_zone};
use super::login_panel::marketplace_picker;
use super::{HEADER_HEIGHT, LivePage, LiveState};
use crate::backup;
use crate::i18n::{tr, tr2};
use crate::login;
use crate::platform::app_shared::ONE_TIME_IDX;
use crate::upload::{FileState, calculate_pct};
use rune::tok;
use rune::{Cx, El, FileDialogOptions, FileDialogResult, ScrollSpec, Tx, col, el, row, text};

pub(super) fn live_view(state: &LiveState, cx: &mut Cx<LiveState>) -> El {
    let system_dark = cx.system_dark();
    state.tray_route.observe_system_dark(system_dark);
    let dark = crate::rune_settings::effective_dark(state.config.appearance, system_dark);
    let theme = tok::theme(dark);
    let colors = crate::rune_theme::palette(dark);
    cx.foreground(theme.text);
    cx.focus_ring(2.0, 2.0, colors.focus_ring);
    let inset = cx.titlebar_inset();
    let active = state.upload.lock().unwrap().active;
    let paused = *state.paused.borrow();

    let header_action = match state.page {
        LivePage::Backup => row()
            .gap(8.0)
            .center_y()
            .child(action_button(
                "open-settings",
                tr("Einstellungen").to_owned(),
                theme,
                theme.control_track,
                theme.text,
                |state: &mut LiveState, _| state.page = super::LivePage::Settings,
            ))
            .child(super::about::about_entry_button::<LiveState>(theme, colors))
            .child(action_button(
                "add-target",
                tr("Ordner hinzufügen …").to_owned(),
                theme,
                colors.primary_fill,
                colors.primary_foreground,
                move |_: &mut LiveState, tx: &mut Tx| {
                    tx.open_folder(
                        FileDialogOptions {
                            title: tr("Sicherungsordner auswählen").to_owned(),
                            ..FileDialogOptions::default()
                        },
                        |state: &mut LiveState, answer: FileDialogResult, _| match answer {
                            Ok(Some(path)) => state.open_create_form(path),
                            Ok(None) => {}
                            Err(error) => {
                                log::warn!("Rune-Ordnerauswahl fehlgeschlagen: {error}");
                                state.status =
                                    Some(tr("Der Ordner konnte nicht ausgewählt werden.").to_owned());
                            }
                        },
                    );
                },
            )),
        LivePage::Settings => action_button(
            "back-to-backup",
            tr("Zur Datensicherung").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut LiveState, _| state.page = super::LivePage::Backup,
        ),
        LivePage::About | LivePage::Licenses => el(),
    };

    let header = row()
        .key("rune-backup-header")
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
                    text(match state.page {
                        LivePage::Backup => tr("Datensicherung"),
                        LivePage::Settings => tr("Einstellungen"),
                        LivePage::About => tr("Über Prime Photos"),
                        LivePage::Licenses => tr("Drittanbieterlizenzen"),
                    })
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .child(header_action);

    let mut targets = col()
        .key("live-targets")
        .w_full()
        .gap(10.0)
        .child(section_heading(tr("Sicherungsziele"), theme));
    for (index, folder) in state.store.folders.iter().enumerate() {
        targets = targets.child(target_card(
            state,
            index,
            folder,
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            colors.alert_foreground,
        ));
    }
    if state.store.folders.is_empty() {
        targets = targets.child(empty_targets(theme));
    }

    let (done, total, current, queue, pending_conflict, pending_remember, pending_idx, pending_path) = {
        let upload = state.upload.lock().unwrap();
        (
            upload.done,
            upload.total,
            upload.current.clone(),
            upload
                .queue
                .iter()
                .map(|item| {
                    (
                        item.folder,
                        item.name.clone(),
                        item.state,
                        item.size,
                        item.bytes_sent,
                    )
                })
                .collect::<Vec<_>>(),
            upload.pending_conflict.clone(),
            upload.conflict_remember,
            upload.pending_conflict_idx,
            upload.pending_conflict_path.clone(),
        )
    };
    let pause_label = if paused {
        tr("Alle Vorgänge fortsetzen")
    } else {
        tr("Alle Vorgänge anhalten")
    };
    let progress = col()
        .key("upload-progress")
        .w_full()
        .padding(14.0)
        .gap(8.0)
        .fill(theme.card)
        .radius(12.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(section_heading(tr("Gesamtfortschritt"), theme))
                .child(
                    text(if active && paused {
                        tr("Angehalten")
                    } else if active {
                        tr("Upload-Vorgang läuft")
                    } else {
                        tr("Bereit")
                    })
                    .font(tok::type_::LABEL)
                    .color(theme.text_secondary),
                ),
        )
        .child(
            text(tr2("{} von {} Dateien abgeschlossen", done, total))
                .font(tok::type_::BODY)
                .color(theme.text),
        )
        .child(if current.is_empty() {
            text(tr("Keine Datei wird gerade übertragen."))
                .font(tok::type_::LABEL)
                .color(theme.text_secondary)
        } else {
            text(current)
                .font(tok::type_::LABEL)
                .color(theme.text_secondary)
        })
        .child(action_button(
            "pause-resume",
            pause_label.to_owned(),
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            |state: &mut LiveState, _| {
                let next = !*state.paused.borrow();
                state.paused.send_replace(next);
                state.config.paused = next;
                state.persist_config_if_allowed();
            },
        ));

    let mut queue_column = col()
        .key("live-queue")
        .w_full()
        .gap(6.0)
        .child(section_heading(tr("Warteschlange"), theme));
    if queue.is_empty() {
        queue_column = queue_column.child(
            text(tr("Noch keine Upload-Dateien eingeplant."))
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        );
    } else {
        for (position, (folder, name, file_state, size, bytes_sent)) in queue.iter().enumerate() {
            let target_path = state
                .upload
                .lock()
                .unwrap()
                .folder_paths
                .get(folder)
                .cloned();
            let target_name = if *folder == ONE_TIME_IDX {
                tr("Einmalige Sicherung").to_owned()
            } else {
                target_path
                    .as_ref()
                    .and_then(|path| state.store.folders.iter().find(|target| target.path == *path))
                    .map(|target| target.name.clone())
                    .unwrap_or_else(|| tr("Sicherungsziel").to_owned())
            };
            let progress_text = if *file_state == FileState::Uploading && *size > 0 {
                format!("{}%", calculate_pct(*size, *bytes_sent))
            } else {
                file_state_text(*file_state).to_owned()
            };
            let retry_index = target_path
                .as_ref()
                .and_then(|path| state.store.folders.iter().position(|target| target.path == *path));
            let mut queue_row = row()
                .key(format!("queue-item-{position}"))
                .w_full()
                .h(36.0)
                .px(12.0)
                .center_y()
                .justify_between()
                .fill(theme.card)
                .radius(8.0)
                .child(
                    text(format!("{name} · {target_name}"))
                        .font(tok::type_::LABEL)
                        .color(theme.text),
                )
                .child(
                    text(progress_text)
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary),
                );
            if *file_state == FileState::Failed {
                if let Some(retry_index) = retry_index {
                    queue_row = queue_row.child(action_button(
                        format!("queue-retry-{position}"),
                        tr("Erneut versuchen").to_owned(),
                        theme,
                        colors.primary_fill,
                        colors.primary_foreground,
                        move |state: &mut LiveState, _| state.start_upload(vec![retry_index]),
                    ));
                }
            }
            queue_column = queue_column.child(queue_row);
        }
    }

    let auth_notice = if state.logged_in() {
        account_identity::signed_in_notice(state, theme)
    } else if state.has_pending_session() {
        text(tr("Sitzung wird wiederhergestellt …")).color(theme.text_secondary)
    } else {
        text(tr("Nicht angemeldet – die Anmeldung startet erst nach deiner Auswahl."))
            .color(theme.text_secondary)
    };
    let login_button = if state.login.running {
        action_button(
            "cancel-login",
            tr("Anmeldung abbrechen").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut LiveState, tx: &mut Tx| {
                state.login.cancel();
                state.status = Some(tr("Anmeldung abgebrochen.").to_owned());
                tx.set_title("Prime Photos");
                crate::wry_host::close();
            },
        )
    } else if !state.sign_in_available() {
        el()
    } else {
        action_button(
            "sign-in",
            tr("Bei Amazon anmelden").to_owned(),
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            start_login,
        )
    };
    let mut login_panel = col()
        .key("login-status")
        .w_full()
        .gap(10.0)
        .child(auth_notice);
    if !state.login.running && !state.logged_in() {
        if let Some(error) = state.login.error.as_ref() {
            login_panel = login_panel.child(
                text(error.clone())
                    .key("login-error")
                    .font(tok::type_::LABEL)
                    .color(colors.alert_foreground)
                    .wrap(),
            );
        }
    }
    if state.sign_in_available() {
        login_panel = login_panel.child(marketplace_picker(state, theme, colors));
    }
    let login_panel = login_panel
        .child(row().w_full().gap(10.0).center_y().child(login_button))
        .child(if state.config_error.is_some() {
            text(tr("Einstellungen konnten nicht geladen oder gespeichert werden.")).color(colors.alert_foreground)
        } else {
            el()
        });

    let remember_label = match (pending_idx, pending_path.as_deref()) {
        (Some(index), Some(path)) => backup::conflict_remember_label(&state.store, index, path),
        _ => tr("Nicht noch einmal fragen").to_owned(),
    };
    let conflict = pending_conflict.map(|name| {
        super::conflict::conflict_panel(
            name,
            pending_remember,
            remember_label,
            theme,
            colors.primary_fill,
            colors.primary_foreground,
            colors.alert_foreground,
            colors.danger_fill,
            colors.danger_foreground,
        )
    });
    let status = state.status.as_ref().map(|value| {
        text(value.clone())
            .key("route-status")
            .font(tok::type_::LABEL)
            .color(theme.text_secondary)
    });
    let store_warning = state.store_error.as_ref().map(|_| {
        text(tr("Sicherungsziele konnten nicht geladen werden. Änderungen werden nicht gespeichert."))
            .key("store-load-error")
            .font(tok::type_::LABEL)
            .color(colors.alert_foreground)
            .wrap()
    });
    let main_content = if state.page == LivePage::Settings {
        crate::rune_settings::embedded_page(state, theme, colors)
    } else if matches!(state.page, LivePage::About | LivePage::Licenses) {
        super::about::route_content(state, theme, colors)
    } else if state.login.running {
        col()
            .key("production-backup-login-webview-region")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .fill(theme.bg)
            .center()
            .child(
                text(tr("Anmeldeseite wird geladen …"))
                    .font(tok::type_::BODY)
                    .color(theme.text_secondary),
            )
            .on_layout(|state: &mut LiveState, rect| {
                let bounds = crate::wry_host::bounds_from_layout(rect);
                state.child_bounds = Some(bounds);
                if state.login.running {
                    state
                        .login
                        .set_bounds(login::rect(bounds[0], bounds[1], bounds[2], bounds[3]));
                }
            })
    } else {
        col()
            .key("backup-content-viewport")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(
                col()
                    .key("backup-content")
                    .w_full()
                    .gap(12.0)
                    .padding(20.0)
                    .child(login_panel)
                    .child(store_warning.unwrap_or_else(|| el()))
                    .child(status.unwrap_or_else(|| el()))
                    .child(drop_zone(state, theme, colors))
                    .child(backup_row(state, theme, colors))
                    .child(progress)
                    .child(targets)
                    .child(queue_column)
                    .child(col().grow(1.0)),
            )
    };
    let mut root = col()
        .key("rune-backup-production")
        .role("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(header)
        .child(main_content);
    // A pending upload decision owns the modal layer. Keep an edit form mounted in
    // state, but never expose it behind the conflict decision.
    if let Some(form) = state.form.as_ref().filter(|_| conflict.is_none()) {
        root = root.child(
            col()
                .key("target-form-overlay")
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .center()
                .padding(24.0)
                .fill(rune::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: if dark { 0.72 } else { 0.42 },
                })
                .child(target_form(
                    form,
                    theme,
                    colors.primary_fill,
                    colors.primary_foreground,
                    colors.alert_foreground,
                )),
        );
    }
    if let Some(conflict) = conflict {
        root = root.child(
            col()
                .key("conflict-overlay")
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .center()
                .padding(24.0)
                .fill(rune::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: if dark { 0.72 } else { 0.42 },
                })
                .child(conflict),
        );
    }
    root
}

fn start_login(state: &mut LiveState, tx: &mut Tx) {
    state.status = None;
    let Some(native_window) = state.native_window.clone() else {
        state.status = Some(tr("Das Prime-Photos-Fenster ist noch nicht bereit.").to_owned());
        return;
    };
    let Some(bounds) = state.child_bounds else {
        state.status = Some(tr("Der Anmeldebereich ist noch nicht bereit.").to_owned());
        return;
    };
    match native_window.with_raw_handle(|raw| {
        login::start_device_login(
            &mut state.login,
            &mut state.config,
            &raw,
            login::rect(bounds[0], bounds[1], bounds[2], bounds[3]),
        );
    }) {
        Ok(()) if state.login.running => tx.set_title("Login – Prime Photos"),
        Ok(()) => {}
        Err(error) => {
            log::warn!("Rune-Sicherungsroute: Fensterhandle für Anmeldung fehlgeschlagen: {error}");
            state.status = Some(tr("Anmeldeseite konnte nicht geöffnet werden.").to_owned());
        }
    }
}
