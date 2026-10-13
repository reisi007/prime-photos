use crate::app::PrimePhotoApp;
use crate::folders::ConflictResolution;
use crate::theme;
use eframe::egui::{self, RichText};
use rfd::FileDialog;
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
}

/// State of the "Datensicherungseinstellungen" window, shared by Create and Edit.
pub(crate) struct BackupConfigForm {
    pub(crate) path: PathBuf,
    pub(crate) dedup: bool,
    pub(crate) conflict_resolution: ConflictResolution,
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
        const UNAVAILABLE: &str = "Noch nicht verfügbar";
        let mut action = None;

        ui.label(RichText::new("Datensicherung").size(16.0).strong());
        ui.add_space(10.0);

        // Source folder. Read-only when editing an existing backup
        // (original `CanChangeSource`); "Ändern …" only in Create mode.
        ui.label("Gesicherter Ordner:");
        ui.horizontal(|ui| {
            ui.label(theme::muted(form.path.display().to_string()));
            let change = ui.add_enabled(!editing, theme::secondary_button("Ändern …"));
            if change.clicked() {
                if let Some(p) = FileDialog::new().pick_folder() {
                    form.path = p;
                    form.error = None;
                }
            }
        });
        ui.add_space(10.0);

        // Destination (not implemented yet).
        ui.label("Ziel für Datensicherung:");
        ui.horizontal(|ui| {
            ui.add_enabled(false, theme::secondary_button("Ändern …"))
                .on_disabled_hover_text(UNAVAILABLE);
        });
        ui.add_space(10.0);

        // Content filter (not implemented yet).
        ui.label("Was soll gesichert werden:");
        ui.horizontal(|ui| {
            for (i, label) in ["Alles", "Fotos und Videos", "Fotos"].iter().enumerate() {
                ui.add_enabled(false, egui::Button::new(*label).selected(i == 0))
                    .on_disabled_hover_text(UNAVAILABLE);
            }
        });
        ui.add_space(10.0);

        // Upload interval (not implemented yet).
        ui.label("Änderungen hochladen:");
        ui.horizontal(|ui| {
            ui.add_enabled_ui(false, |ui| {
                egui::ComboBox::from_id_salt("backup-config-interval")
                    .selected_text("Sofort")
                    .show_ui(ui, |ui| {
                        ui.label("Sofort");
                    });
            })
            .response
            .on_disabled_hover_text(UNAVAILABLE);
        });
        ui.add_space(10.0);

        // Weekly schedule (inert; no backend yet) — §4.5 "… aktiviert" + "um".
        ui.horizontal(|ui| {
            let mut weekly = true;
            let mut time = "14:00".to_string();
            ui.add_enabled(
                false,
                egui::Checkbox::new(&mut weekly, "Wöchentlich aktiviert"),
            )
            .on_disabled_hover_text(UNAVAILABLE);
            ui.label("um");
            ui.add_enabled(
                false,
                egui::TextEdit::singleline(&mut time).desired_width(64.0),
            )
            .on_disabled_hover_text(UNAVAILABLE);
        });
        ui.add_space(10.0);

        // Functional: dedup.
        ui.checkbox(
            &mut form.dedup,
            "Duplikate vermeiden: Keine Fotos und Videos sichern, die sich bereits in meinem Konto befinden",
        );
        ui.add_space(10.0);

        // Functional: name-conflict policy.
        ui.label("Umgang mit Dateinamenkonflikten:");
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Ask,
            "Jedes Mal nachfragen",
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Ignore,
            "Ignorieren",
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::SaveAsNew,
            "Als neue Dateien speichern",
        );
        ui.radio_value(
            &mut form.conflict_resolution,
            ConflictResolution::Replace,
            "Ersetzen",
        );

        if let Some(err) = &form.error {
            ui.add_space(6.0);
            ui.colored_label(theme::RED, err);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.add(theme::primary_button("Speichern")).clicked() {
                action = Some(ConfigFormAction::Save);
            }
            if ui.add(theme::secondary_button("Abbrechen")).clicked() {
                action = Some(ConfigFormAction::Cancel);
            }
        });

        action
    }

    /// Applies a Save/Cancel from the configuration window. Validation failures
    /// keep the window open with an inline error; a valid Save closes it.
    pub(crate) fn handle_config_form_action(&mut self, action: ConfigFormAction) {
        match action {
            ConfigFormAction::Cancel => self.config_form = None,
            ConfigFormAction::Save => {
                let Some((mode, path, dedup, conflict)) =
                    self.config_form.as_ref().map(|f| {
                        (f.mode.clone(), f.path.clone(), f.dedup, f.conflict_resolution)
                    })
                else {
                    return;
                };

                if !path.exists() {
                    if let Some(f) = self.config_form.as_mut() {
                        f.error = Some("Lokaler Pfad ist nicht verfügbar".to_string());
                    }
                    return;
                }

                match mode {
                    ConfigFormMode::Create => {
                        if self.store.folders.iter().any(|f| f.path == path) {
                            if let Some(f) = self.config_form.as_mut() {
                                f.error = Some(
                                    "Datensicherung für lokalen Pfad existiert bereits"
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
                        }
                        let _ = self.store.save();
                        self.log(format!("Ordner hinzugefügt: {}", path.display()));
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
                                }
                                let _ = self.store.save();
                                self.config_form = None;
                            }
                            None => {
                                if let Some(f) = self.config_form.as_mut() {
                                    f.error =
                                        Some("Ordner ist nicht mehr vorhanden".to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
