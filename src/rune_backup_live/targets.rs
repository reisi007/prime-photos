use super::widgets::{action_button, folder_status_text};
use super::{LiveState, TargetForm};
use crate::folders::{self, BackupFolder, ConflictResolution, ContentFilter, FolderStatus};
use crate::i18n::{tr, tr3};
use rune::tok;
use rune::{El, FileDialogOptions, FileDialogResult, Tx, col, el, row, text};
use std::path::PathBuf;
use tok::Theme;

fn create_target_form(path: PathBuf, dedup: bool, conflict: ConflictResolution) -> TargetForm {
    TargetForm {
        path,
        dedup,
        conflict,
        content_filter: ContentFilter::Photos,
        edit_path: None,
        error: None,
    }
}

fn edit_target_form(folder: &BackupFolder) -> TargetForm {
    TargetForm {
        path: folder.path.clone(),
        dedup: folder.dedup,
        conflict: folder.conflict_resolution,
        content_filter: folder.content_filter,
        edit_path: Some(folder.path.clone()),
        error: None,
    }
}

fn apply_target_options(
    folder: &mut BackupFolder,
    dedup: bool,
    conflict: ConflictResolution,
    content_filter: ContentFilter,
) {
    folder.dedup = dedup;
    folder.conflict_resolution = conflict;
    folder.content_filter = content_filter;
}

fn select_target_photos(persisted: ContentFilter, photos: bool) -> ContentFilter {
    ContentFilter::from_selection(photos, persisted.videos(), persisted)
}

impl LiveState {
    pub(super) fn open_create_form(&mut self, path: PathBuf) {
        let (dedup, conflict) = folders::default_options(&self.config);
        self.form = Some(create_target_form(path, dedup, conflict));
        self.status = None;
    }

    pub(super) fn save_form(&mut self) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let path = form.path.clone();
        let dedup = form.dedup;
        let conflict = form.conflict;
        let content_filter = form.content_filter;
        let edit_path = form.edit_path.clone();
        if !path.is_dir() {
            if let Some(form) = self.form.as_mut() {
                form.error = Some(tr("Lokaler Pfad ist nicht verfügbar").to_owned());
            }
            return;
        }
        if let Some(edit_path) = edit_path {
            let Some(index) = self
                .store
                .folders
                .iter()
                .position(|folder| folder.path == edit_path)
            else {
                if let Some(form) = self.form.as_mut() {
                    form.error = Some(tr("Ordner ist nicht mehr vorhanden").to_owned());
                }
                return;
            };
            if let Some(folder) = self.store.folders.get_mut(index) {
                apply_target_options(folder, dedup, conflict, content_filter);
            }
            self.persist_store_if_allowed();
            self.form = None;
        } else {
            if self.store.folders.iter().any(|folder| folder.path == path) {
                if let Some(form) = self.form.as_mut() {
                    form.error = Some(tr("Datensicherung für lokalen Pfad existiert bereits").to_owned());
                }
                return;
            }
            if !self.store.add(path.clone()) {
                return;
            }
            let index = self.store.folders.len() - 1;
            if let Some(folder) = self.store.folders.get_mut(index) {
                apply_target_options(folder, dedup, conflict, content_filter);
            }
            self.persist_store_if_allowed();
            self.form = None;
            if self.logged_in() {
                self.start_upload(vec![index]);
            } else {
                self.status = Some(
                    tr("Sicherungsziel hinzugefügt. Melde dich an und starte den Upload manuell.")
                        .to_owned(),
                );
            }
        }
    }
}

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
    let mut actions = row().gap(6.0).center_y();
    actions = actions.child(action_button(
        format!("target-upload-{index}"),
        if folder.status == FolderStatus::Error {
            tr("Erneut versuchen").to_owned()
        } else {
            tr("Jetzt sichern").to_owned()
        },
        theme,
        primary_fill,
        primary_foreground,
        move |state: &mut LiveState, _| state.start_upload(vec![index]),
    ));
    actions = actions.child(action_button(
        format!("target-enabled-{index}"),
        if folder.enabled {
            tr("Aktiviert")
        } else {
            tr("Deaktiviert")
        }
        .to_owned(),
        theme,
        theme.control_track,
        theme.text,
        move |state: &mut LiveState, _| {
            if let Some(folder) = state.store.folders.get_mut(index) {
                folder.enabled = !folder.enabled;
            }
            state.persist_store_if_allowed();
        },
    ));
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
    actions = actions.child(action_button(
        format!("target-remove-{index}"),
        tr("Entfernen").to_owned(),
        theme,
        theme.control_track,
        theme.text,
        move |state: &mut LiveState, _| {
            if index < state.store.folders.len() {
                state.store.folders.remove(index);
                state.persist_store_if_allowed();
            }
        },
    ));

    let current = state
        .upload
        .lock()
        .unwrap()
        .folder_paths
        .iter()
        .find_map(|(plan_index, path)| (path == &folder.path).then_some(*plan_index));
    let (target_done, target_total) = current.map_or(
        (folder.uploaded_files, folder.total_files),
        |plan_index| {
            let upload = state.upload.lock().unwrap();
            (
                upload.folder_done.get(&plan_index).copied().unwrap_or(0),
                upload.folder_total.get(&plan_index).copied().unwrap_or(0),
            )
        },
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
                    text(status)
                        .font(tok::type_::LABEL)
                        .color(if folder.status == FolderStatus::Error {
                            alert_foreground
                        } else {
                            theme.text_secondary
                        }),
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
        .w_full()
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
