use super::super::{LiveState, TargetForm};
use crate::folders::{self, BackupFolder, ConflictResolution, ContentFilter};
use crate::i18n::tr;
use std::path::PathBuf;

pub(super) fn create_target_form(
    path: PathBuf,
    dedup: bool,
    conflict: ConflictResolution,
) -> TargetForm {
    TargetForm {
        path,
        dedup,
        conflict,
        content_filter: ContentFilter::Photos,
        edit_path: None,
        error: None,
    }
}

pub(super) fn edit_target_form(folder: &BackupFolder) -> TargetForm {
    TargetForm {
        path: folder.path.clone(),
        dedup: folder.dedup,
        conflict: folder.conflict_resolution,
        content_filter: folder.content_filter,
        edit_path: Some(folder.path.clone()),
        error: None,
    }
}

pub(super) fn apply_target_options(
    folder: &mut BackupFolder,
    dedup: bool,
    conflict: ConflictResolution,
    content_filter: ContentFilter,
) {
    folder.dedup = dedup;
    folder.conflict_resolution = conflict;
    folder.content_filter = content_filter;
}

pub(super) fn select_target_photos(persisted: ContentFilter, photos: bool) -> ContentFilter {
    ContentFilter::from_selection(photos, persisted.videos(), persisted)
}

impl LiveState {
    pub(in crate::rune_backup_live) fn open_create_form(&mut self, path: PathBuf) {
        let (dedup, conflict) = folders::default_options(&self.config);
        self.form = Some(create_target_form(path, dedup, conflict));
        self.status = None;
    }

    pub(in crate::rune_backup_live) fn save_form(&mut self) {
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
