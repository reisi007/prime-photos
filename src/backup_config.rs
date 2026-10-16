use crate::app::{FolderPickerTarget, PrimePhotoApp};
use crate::folders::{ConflictResolution, ContentFilter};
use crate::i18n::{tr, tr1};
use crate::theme;
use eframe::egui::{self, RichText};
use std::path::PathBuf;

/// Whether the "Datensicherungseinstellungen" window is configuring a new
/// backup or one that already exists. Edit mode stores the folder **path** (not
/// a row index) so a folder removed while the window is open cannot make Save
/// write to the wrong backup.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ConfigFormMode {
    Create,
    Edit(PathBuf),
}

/// Result of one rendered frame of the shared backup-configuration form body.
pub(crate) enum ConfigFormAction {
    Save,
    Cancel,
    /// The "Ändern …" button: ask the app to open the asynchronous native folder
    /// picker (Welle 5.1). The picked folder is applied back to this form via
    /// `poll_folder_picker`.
    PickFolder,
}

/// State of the "Datensicherungseinstellungen" window, shared by Create and Edit.
pub(crate) struct BackupConfigForm {
    pub(crate) path: PathBuf,
    pub(crate) dedup: bool,
    pub(crate) conflict_resolution: ConflictResolution,
    /// Content filter — the original's two "Fotos sichern"/"Videos sichern"
    /// checkboxes (`docs/original/SPEC_CONTENT_FILTER_DESTINATION.md` §2.1). `Photos` by
    /// default; the effective filter stays `is_supported_image`.
    pub(crate) content_filter: ContentFilter,
    pub(crate) mode: ConfigFormMode,
    /// Inline validation message; keeps the window open after an invalid Save.
    pub(crate) error: Option<String>,
}

impl PrimePhotoApp {
    /// The single shared form body for the "Datensicherungseinstellungen"
    /// window. Drawn once for both Create and Edit; returns the user's action.
    /// Functional fields: source folder (read-only when `editing`), dedup and the
    /// name-conflict policy. Destination, content filter and upload interval have
    /// no backend yet, so they are shown disabled with a hover hint.
    pub(crate) fn render_backup_config(
        ui: &mut egui::Ui,
        form: &mut BackupConfigForm,
        editing: bool,
    ) -> Option<ConfigFormAction> {
        let unavailable = tr("Noch nicht verfügbar");
        let mut action = None;

        ui.label(RichText::new(tr("Datensicherung")).size(16.0).strong());
        ui.add_space(10.0);

        // Source folder. Read-only when editing an existing backup
        // (original `CanChangeSource`); "Ändern …" only in Create mode.
        ui.label(tr("Gesicherter Ordner:"));
        ui.horizontal(|ui| {
            ui.label(theme::muted(crate::text::nfc(&form.path.display().to_string())));
            let change = ui.add_enabled(!editing, theme::secondary_button(tr("Ändern …")));
            if change.clicked() {
                // Do not block the UI with the native dialog (Welle 5.1): the
                // app opens it asynchronously and writes the picked folder back
                // into this form (`poll_folder_picker`).
                action = Some(ConfigFormAction::PickFolder);
            }
        });
        ui.add_space(10.0);

        // Destination is derived automatically: today the upload target is
        // resolved below Pictures/Web (`src/prime_photos.rs` looks up the two
        // folders). The original instead mirrors the source path into a cloud
        // node (`Backup/<Macname>/<path>`) behind a cloud picker — that is a
        // separate task (`docs/original/SPEC_UPLOAD_DESTINATION.md`), not built here.
        // Content filter: the original's two checkboxes ("Fotos sichern"/
        // "Videos sichern", `docs/original/SPEC_CONTENT_FILTER_DESTINATION.md` §2.1).
        // "Videos sichern" is visible but disabled — the app uploads photos
        // only, so the effective filter stays `is_supported_image`. This field
        // is parity + future, it changes no behaviour today.
        let mut photos = form.content_filter.photos();
        let mut videos = form.content_filter.videos();
        ui.checkbox(&mut photos, tr("Fotos sichern"));
        ui.add_enabled(false, egui::Checkbox::new(&mut videos, tr("Videos sichern")))
            .on_disabled_hover_text(tr("Videos werden noch nicht hochgeladen"));
        form.content_filter = ContentFilter::from_selection(photos, videos, form.content_filter);
        ui.add_space(10.0);

        // Upload interval (not implemented yet).
        ui.label(tr("Änderungen hochladen:"));
        ui.horizontal(|ui| {
            ui.add_enabled_ui(false, |ui| {
                egui::ComboBox::from_id_salt("backup-config-interval")
                    .selected_text(tr("Sofort"))
                    .show_ui(ui, |ui| {
                        ui.label(tr("Sofort"));
                    });
            })
            .response
            .on_disabled_hover_text(unavailable);
        });
        ui.add_space(10.0);

        // Weekly schedule (inert; no backend yet) — §4.5 "… aktiviert" + "um".
        ui.horizontal(|ui| {
            let mut weekly = true;
            let mut time = "14:00".to_string();
            ui.add_enabled(
                false,
                egui::Checkbox::new(&mut weekly, tr("Wöchentlich aktiviert")),
            )
            .on_disabled_hover_text(unavailable);
            ui.label(tr("um"));
            ui.add_enabled(
                false,
                egui::TextEdit::singleline(&mut time).desired_width(64.0),
            )
            .on_disabled_hover_text(unavailable);
        });
        ui.add_space(10.0);

        // Functional: dedup.
        ui.checkbox(
            &mut form.dedup,
            tr("Duplikate vermeiden: Keine Fotos und Videos sichern, die sich bereits in meinem Konto befinden"),
        );
        ui.add_space(10.0);

        // Functional: name-conflict policy.
        ui.label(tr("Umgang mit Dateinamenkonflikten:"));
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Ask,
            tr("Jedes Mal nachfragen"),
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Ignore,
            tr("Ignorieren"),
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::SaveAsNew,
            tr("Neue Datei automatisch umbenennen"),
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Replace,
            tr("Ersetzen"),
        );

        if let Some(err) = &form.error {
            ui.add_space(6.0);
            ui.colored_label(theme::RED, err);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.add(theme::primary_button(tr("Speichern"))).clicked() {
                action = Some(ConfigFormAction::Save);
            }
            if ui.add(theme::secondary_button(tr("Abbrechen"))).clicked() {
                action = Some(ConfigFormAction::Cancel);
            }
        });

        action
    }

    /// Applies a Save/Cancel from the configuration window. Validation failures
    /// keep the window open with an inline error; a valid Save closes it.
    pub(crate) fn handle_config_form_action(
        &mut self,
        action: ConfigFormAction,
        ctx: &egui::Context,
    ) {
        match action {
            ConfigFormAction::Cancel => self.config_form = None,
            ConfigFormAction::PickFolder => {
                self.spawn_folder_picker(FolderPickerTarget::ConfigForm, ctx);
            }
            ConfigFormAction::Save => {
                let Some((mode, path, dedup, conflict, content_filter)) =
                    self.config_form.as_ref().map(|f| {
                        (
                            f.mode.clone(),
                            f.path.clone(),
                            f.dedup,
                            f.conflict_resolution,
                            f.content_filter,
                        )
                    })
                else {
                    return;
                };

                if !path.exists() {
                    if let Some(f) = self.config_form.as_mut() {
                        f.error = Some(tr("Lokaler Pfad ist nicht verfügbar").to_string());
                    }
                    return;
                }

                match mode {
                    ConfigFormMode::Create => {
                        if self.store.folders.iter().any(|f| f.path == path) {
                            if let Some(f) = self.config_form.as_mut() {
                                f.error = Some(
                                    tr("Datensicherung für lokalen Pfad existiert bereits")
                                        .to_string(),
                                );
                            }
                            return;
                        }
                        self.store.add(path.clone());
                        let idx = self.store.folders.len() - 1;
                        if let Some(f) = self.store.folders.get_mut(idx) {
                            f.dedup = dedup;
                            f.conflict_resolution = conflict;
                            f.content_filter = content_filter;
                        }
                        let _ = self.store.save();
                        self.log(tr1(
                            "Ordner hinzugefügt: {}",
                            crate::text::nfc(&path.display().to_string()),
                        ));
                        self.config_form = None;
                        self.start_folder_upload(vec![idx]);
                    }
                    ConfigFormMode::Edit(edit_path) => {
                        match self
                            .store
                            .folders
                            .iter()
                            .position(|f| f.path == edit_path)
                        {
                            Some(idx) => {
                                if let Some(f) = self.store.folders.get_mut(idx) {
                                    f.dedup = dedup;
                                    f.conflict_resolution = conflict;
                                    f.content_filter = content_filter;
                                }
                                let _ = self.store.save();
                                self.config_form = None;
                            }
                            None => {
                                if let Some(f) = self.config_form.as_mut() {
                                    f.error =
                                        Some(tr("Ordner ist nicht mehr vorhanden").to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
