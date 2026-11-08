use super::{BackupConfigForm, ConfigFormAction, ConfigFormMode};
use crate::app::{FolderPickerTarget, PrimePhotoApp};
use crate::folders::{ConflictResolution, ContentFilter};
use crate::i18n::{tr, tr1};
use crate::theme::eframe_ui as theme;
use eframe::egui::{self, RichText};

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
        let change = ui.add_enabled(
            !editing,
            theme::secondary_button(tr("Ändern …"), ui.visuals().dark_mode),
        );
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
        ui.colored_label(theme::palette(ui.visuals().dark_mode).red, err);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.horizontal(|ui| {
        if ui
            .add(theme::primary_button(
                tr("Speichern"),
                ui.visuals().dark_mode,
            ))
            .clicked()
        {
            action = Some(ConfigFormAction::Save);
        }
        if ui
            .add(theme::secondary_button(
                tr("Abbrechen"),
                ui.visuals().dark_mode,
            ))
            .clicked()
        {
            action = Some(ConfigFormAction::Cancel);
        }
    });

    action
}

/// Applies a Save/Cancel from the configuration window. Validation failures
/// keep the window open with an inline error; a valid Save closes it.
pub(crate) fn handle_config_form_action(
    app: &mut PrimePhotoApp,
    action: ConfigFormAction,
    ctx: &egui::Context,
) {
    // Defence in depth: the demo never opens this form (no folder dialog,
    // no row edit), and a Save here would write the fixture store to disk
    // and start a real upload.
    if app.is_demo() {
        app.demo_skip("Datensicherungseinstellungen speichern");
        return;
    }
    match action {
        ConfigFormAction::Cancel => app.config_form = None,
        ConfigFormAction::PickFolder => {
            app.spawn_folder_picker(FolderPickerTarget::ConfigForm, ctx);
        }
        ConfigFormAction::Save => {
            let Some((mode, path, dedup, conflict, content_filter)) =
                app.config_form.as_ref().map(|f| {
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
                if let Some(f) = app.config_form.as_mut() {
                    f.error = Some(tr("Lokaler Pfad ist nicht verfügbar").to_string());
                }
                return;
            }

            match mode {
                ConfigFormMode::Create => {
                    if app.store.folders.iter().any(|f| f.path == path) {
                        if let Some(f) = app.config_form.as_mut() {
                            f.error = Some(
                                tr("Datensicherung für lokalen Pfad existiert bereits")
                                    .to_string(),
                            );
                        }
                        return;
                    }
                    app.store.add(path.clone());
                    let idx = app.store.folders.len() - 1;
                    if let Some(f) = app.store.folders.get_mut(idx) {
                        f.dedup = dedup;
                        f.conflict_resolution = conflict;
                        f.content_filter = content_filter;
                    }
                    let _ = app.store.save();
                    app.log(tr1(
                        "Ordner hinzugefügt: {}",
                        crate::text::nfc(&path.display().to_string()),
                    ));
                    app.config_form = None;
                    app.start_folder_upload(vec![idx]);
                }
                ConfigFormMode::Edit(edit_path) => {
                    match app
                        .store
                        .folders
                        .iter()
                        .position(|f| f.path == edit_path)
                    {
                        Some(idx) => {
                            if let Some(f) = app.store.folders.get_mut(idx) {
                                f.dedup = dedup;
                                f.conflict_resolution = conflict;
                                f.content_filter = content_filter;
                            }
                            let _ = app.store.save();
                            app.config_form = None;
                        }
                        None => {
                            if let Some(f) = app.config_form.as_mut() {
                                f.error = Some(tr("Ordner ist nicht mehr vorhanden").to_string());
                            }
                        }
                    }
                }
            }
        }
    }
}
