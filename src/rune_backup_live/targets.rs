mod form;

use super::widgets::{
    action_button, disabled_action_button, disabled_switch, enabled_switch, enabled_switch_label,
    folder_status_text, upload_spinner,
};
use super::{LiveState, TargetForm};
use crate::folders::{BackupFolder, ConflictResolution, FolderStatus};
use crate::i18n::{tr, tr3};
use crate::upload::FileState;
use form::{edit_target_form, select_target_photos};
use rune::tok;
use rune::{El, FileDialogOptions, FileDialogResult, Tx, col, el, row, text};
use tok::Theme;

pub(super) fn target_card(
    state: &LiveState,
    index: usize,
    folder: &BackupFolder,
    theme: Theme,
    primary_fill: rune::Color,
    primary_foreground: rune::Color,
    alert_foreground: rune::Color,
) -> El {
    let status = folder_status_text(folder.status, folder.enabled);
    // `primary_fill` is the appearance-specific palette colour passed in by the
    // production view; recover the matching palette so the switch is styled
    // exactly like the one in Settings without changing this function's callers.
    let colors = crate::rune_theme::palette(
        primary_fill == crate::rune_theme::palette(true).primary_fill,
    );
    let not_found = folder.status == FolderStatus::NotFound;

    let (target_done, target_total, in_active_run, show_spinner) = {
        let upload = state.upload.lock().unwrap();
        let current = upload
            .folder_paths
            .iter()
            .find_map(|(plan_index, path)| (path == &folder.path).then_some(*plan_index));
        let in_active_run = upload.active && current.is_some();
        let show_spinner = in_active_run && current.is_some_and(|plan_index| {
            upload.queue.iter().any(|entry| {
                entry.folder == plan_index && entry.state == FileState::Uploading
            }) || upload.folder_done.get(&plan_index).copied().unwrap_or(0)
                < upload.folder_total.get(&plan_index).copied().unwrap_or(0)
        });
        let counts = current.map_or(
            (folder.uploaded_files, folder.total_files),
            |plan_index| {
                (
                    upload.folder_done.get(&plan_index).copied().unwrap_or(0),
                    upload.folder_total.get(&plan_index).copied().unwrap_or(0),
                )
            },
        );
        (counts.0, counts.1, in_active_run, show_spinner)
    };

    let enabled_label = if folder.enabled {
        tr("Aktiviert")
    } else {
        tr("Deaktiviert")
    };
    let mut actions = row().gap(6.0).center_y();
    let upload_label = if folder.status == FolderStatus::Error {
        tr("Erneut versuchen").to_owned()
    } else {
        tr("Jetzt sichern").to_owned()
    };
    actions = if not_found {
        actions.child(disabled_action_button(
            format!("target-upload-{index}"),
            upload_label,
            "Ordner nicht gefunden",
            theme,
        ))
    } else {
        actions.child(action_button(
            format!("target-upload-{index}"),
            upload_label,
            theme,
            primary_fill,
            primary_foreground,
            move |state: &mut LiveState, _| state.start_upload(vec![index]),
        ))
    };
    actions = actions.child(action_button(
        format!("target-edit-{index}"),
        tr("Optionen").to_owned(),
        theme,
        theme.control_track,
        theme.text,
        move |state: &mut LiveState, _| {
            if let Some(folder) = state.store.folders.get(index) {
                state.form = Some(edit_target_form(folder));
            }
        },
    ));
    let remove_label = tr("Entfernen").to_owned();
    actions = if in_active_run {
        actions.child(disabled_action_button(
            format!("target-remove-{index}"),
            remove_label,
            "Ordner wird gerade gesichert",
            theme,
        ))
    } else {
        actions.child(action_button(
            format!("target-remove-{index}"),
            remove_label,
            theme,
            theme.control_track,
            theme.text,
            move |state: &mut LiveState, _| {
                if index < state.store.folders.len() {
                    state.store.folders.remove(index);
                    state.persist_store_if_allowed();
                }
            },
        ))
    };

    let switch_key = format!("target-enabled-{index}");
    let switch = if not_found {
        disabled_switch(
            switch_key,
            folder.enabled,
            enabled_label,
            "Ordner nicht gefunden",
            theme,
            colors,
        )
    } else {
        enabled_switch(
            switch_key,
            folder.enabled,
            enabled_label,
            theme,
            colors,
            move |state: &mut LiveState, _| {
                if let Some(folder) = state.store.folders.get_mut(index) {
                    folder.enabled = !folder.enabled;
                }
                state.persist_store_if_allowed();
            },
        )
    };
    // UI-2: label + switch form one group at the right end of the actions row
    // (`Jetzt sichern · Optionen · Entfernen · [Spacer] · Aktiviert · [Toggle]`).
    // The spacer absorbs all slack so only the card's 12 pt padding remains to
    // the right; `S2` is the 8 pt step between the label and the switch track.
    actions = actions.child(el().grow(1.0)).child(
        row()
            .gap(tok::space::S2)
            .center_y()
            .child(enabled_switch_label(
                format!("target-enabled-{index}-label"),
                enabled_label,
                theme,
            ))
            .child(switch),
    );

    col()
        .key(format!("target-card-{index}"))
        .w_full()
        .padding(12.0)
        .gap(7.0)
        .fill(theme.card)
        .radius(10.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(text(folder.name.clone()).font(tok::type_::BODY).color(theme.text))
                .child(
                    row()
                        .gap(6.0)
                        .center_y()
                        .child(
                            text(status)
                                .font(tok::type_::LABEL)
                                .color(if folder.status == FolderStatus::Error {
                                    alert_foreground
                                } else {
                                    theme.text_secondary
                                }),
                        )
                        .children(
                            show_spinner.then(|| {
                                upload_spinner(format!("target-spinner-{index}"), theme, colors)
                            }),
                        ),
                ),
        )
        .child(
            text(folder.path.display().to_string())
                .font(tok::type_::LABEL)
                .color(theme.text_secondary)
                .wrap(),
        )
        .child(
            text(tr3(
                "{} / {} Dateien · {}",
                target_done,
                target_total,
                if folder.dedup {
                    tr("Duplikate vermeiden")
                } else {
                    tr("Duplikatprüfung aus")
                },
            ))
            .font(tok::type_::LABEL)
            .color(theme.text_secondary),
        )
        .child(actions)
}

pub(super) fn empty_targets(theme: Theme) -> El {
    col()
        .key("empty-targets")
        .w_full()
        .padding(22.0)
        .gap(8.0)
        .center()
        .fill(theme.card)
        .radius(12.0)
        .child(text(tr("Noch keine Sicherungsziele")).font(tok::type_::SECTION).color(theme.text))
        .child(
            text(tr("Füge einen Fotoordner hinzu, um deine Bilder für die Sicherung vorzubereiten."))
                .font(tok::type_::BODY)
                .color(theme.text_secondary)
                .wrap(),
        )
        .child(
            text(tr("Der Ordnerdialog wird nur auf ausdrückliche Auswahl geöffnet."))
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        )
}

pub(super) fn target_form(
    form: &TargetForm,
    theme: Theme,
    primary_fill: rune::Color,
    primary_foreground: rune::Color,
    alert_foreground: rune::Color,
) -> El {
    let path = form.path.clone();
    let label = if form.edit_path.is_some() {
        tr("Sicherungsoptionen").to_owned()
    } else {
        tr("Neues Sicherungsziel").to_owned()
    };
    let mut form_body = col()
        .key("target-form")
        .role("dialog")
        .label(label.clone())
        .modal(false, |state: &mut LiveState| state.form = None)
        .w_full()
        .max_w(560.0)
        .h_full()
        .max_h(720.0)
        .min_h(0.0)
        .scroll(rune::ScrollSpec { stick_bottom: false })
        .padding(16.0)
        .gap(9.0)
        .fill(theme.card)
        .radius(12.0)
        .child(text(label).font(tok::type_::SECTION).color(theme.text))
        .child(
            text(path.display().to_string())
                .font(tok::type_::LABEL)
                .color(theme.text_secondary)
                .wrap(),
        );
    if form.edit_path.is_none() {
        form_body = form_body.child(action_button(
            "choose-another-folder",
            tr("Anderen Ordner wählen …").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |_: &mut LiveState, tx: &mut Tx| {
                tx.open_folder(
                    FileDialogOptions {
                        title: tr("Sicherungsordner auswählen").to_owned(),
                        ..FileDialogOptions::default()
                    },
                    |state: &mut LiveState, result: FileDialogResult, _| {
                        if let Ok(Some(path)) = result {
                            if let Some(form) = state.form.as_mut() {
                                if form.edit_path.is_none() {
                                    form.path = path;
                                    form.error = None;
                                }
                            }
                        }
                    },
                );
            },
        ));
    }
    let photos_selected = form.content_filter.photos();
    let accessible_photo_label = format!(
        "{}: {}",
        tr("Fotos sichern"),
        if photos_selected {
            tr("Aktiviert")
        } else {
            tr("Deaktiviert")
        }
    );
    let visible_photo_label = format!(
        "{} {}",
        if photos_selected { "☑" } else { "☐" },
        tr("Fotos sichern")
    );
    form_body = form_body
        .child(
            el()
                .key("toggle-photos")
                .role("checkbox")
                .label(accessible_photo_label)
                .checked(photos_selected)
                .focusable(true)
                .h(38.0)
                .px(12.0)
                .center()
                .radius(8.0)
                .fill(if photos_selected {
                    primary_fill
                } else {
                    theme.control_track
                })
                .hover(theme.hover)
                .on_press_tx(|state: &mut LiveState, _| {
                    if let Some(form) = state.form.as_mut() {
                        let photos = !form.content_filter.photos();
                        let persisted = form.content_filter;
                        form.content_filter = select_target_photos(persisted, photos);
                    }
                })
                .child(
                    text(visible_photo_label)
                        .font(tok::type_::LABEL)
                        .color(if photos_selected {
                            primary_foreground
                        } else {
                            theme.text
                        })
                        .a11y_hidden(),
                ),
        )
        .child(action_button(
            "toggle-dedup",
            if form.dedup {
                tr("Duplikate vermeiden: ein")
            } else {
                tr("Duplikate vermeiden: aus")
            }
            .to_owned(),
            theme,
            primary_fill,
            primary_foreground,
            |state: &mut LiveState, _| {
                if let Some(form) = state.form.as_mut() {
                    form.dedup = !form.dedup;
                }
            },
        ))
        .child(text(tr("Bei Namenskonflikt:")).font(tok::type_::LABEL).color(theme.text_secondary));
    for (key, title, resolution) in [
        ("conflict-ask", tr("Fragen"), ConflictResolution::Ask),
        ("conflict-replace", tr("Ersetzen"), ConflictResolution::Replace),
        (
            "conflict-save-new",
            tr("Als neue Datei speichern"),
            ConflictResolution::SaveAsNew,
        ),
        ("conflict-ignore", tr("Ignorieren"), ConflictResolution::Ignore),
    ] {
        form_body = form_body.child(action_button(
            key,
            if form.conflict == resolution {
                format!("✓ {title}")
            } else {
                title.to_owned()
            },
            theme,
            if form.conflict == resolution {
                primary_fill
            } else {
                theme.control_track
            },
            if form.conflict == resolution {
                primary_foreground
            } else {
                theme.text
            },
            move |state: &mut LiveState, _| {
                if let Some(form) = state.form.as_mut() {
                    form.conflict = resolution;
                }
            },
        ));
    }
    if let Some(error) = &form.error {
        form_body = form_body.child(text(error.clone()).color(alert_foreground).wrap());
    }
    form_body
        .child(action_button(
            "save-target",
            tr("Speichern").to_owned(),
            theme,
            primary_fill,
            primary_foreground,
            |state: &mut LiveState, _| state.save_form(),
        ))
        .child(action_button(
            "cancel-target-form",
            tr("Abbrechen").to_owned(),
            theme,
            theme.control_track,
            theme.text,
            |state: &mut LiveState, _| state.form = None,
        ))
}

#[cfg(test)]
mod tests;
