use crate::app::{FolderPickerTarget, PrimePhotoApp};
use crate::folders::ConflictResolution;
use crate::i18n::{tr, tr1};
use crate::theme::eframe_ui as theme;
use eframe::egui;

use super::super::{answer_pending_conflict, conflict_remember_label};

/// Ask for a folder for a **new** backup target. The native dialog is opened
/// asynchronously (Welle 5.1) — the UI keeps running; `poll_folder_picker`
/// opens the Create form once a folder was picked.
pub(crate) fn add_backup_folder(app: &mut PrimePhotoApp, ctx: &egui::Context) {
    // Demo fixtures are read-only: no native dialog, no store write.
    if app.is_demo() {
        app.demo_skip("Ordner-Dialog öffnen");
        return;
    }
    app.spawn_folder_picker(FolderPickerTarget::NewBackup, ctx);
}

/// Modal prompt for a name conflict (`ConflictResolution::Ask`,
/// docs/original/SPEC_DEDUP.md §4.1). Rendered from `update` independently of the
/// current page; the answer is written into the shared state, where the
/// waiting worker picks it up. Drawn only while a conflict is pending.
pub(crate) fn conflict_dialog(app: &mut PrimePhotoApp, ctx: &egui::Context) {
    // Snapshot the prompt *and* its provenance under one lock. The worker
    // clears all three together once it takes the answer, so reading them in
    // one go is what keeps the persisted folder consistent with the name
    // the user actually saw.
    let (file_name, pending_idx, pending_path) = {
        let s = app.upload.lock().unwrap();
        (
            s.pending_conflict.clone(),
            s.pending_conflict_idx,
            s.pending_conflict_path.clone(),
        )
    };
    let Some(file_name) = file_name else {
        return;
    };
    // The worker only polls the shared state; it does not paint frames, so
    // keep the dialog (and the repaint that shows it) alive from here.
    ctx.request_repaint_after(std::time::Duration::from_millis(200));

    let mut remember = app.upload.lock().unwrap().conflict_remember;
    let mut answer: Option<ConflictResolution> = None;
    // Name the folder the answer is persisted into — through
    // `conflict_remember_label`, i.e. the exact guard
    // `remember_conflict_in_folder` writes with: the transient drag & drop
    // queue (`ONE_TIME_IDX`) has no `folders.toml` entry, an index the
    // store no longer holds must not borrow a neighbouring folder's name,
    // and a file outside the folder path means nothing would be written
    // there either. All three fall back to the neutral text and stay
    // run-only, exactly like the write refuses them.
    let remember_label = match (pending_idx, pending_path.as_deref()) {
        (Some(idx), Some(path)) => conflict_remember_label(&app.store, idx, path),
        _ => super::super::remember_checkbox_label(None),
    };
    // NOTE: egui 0.31 has no `Window::modal`; the dialog is centred and
    // covers the page it matters for. A second upload run is still
    // impossible while it waits (`upload_busy()`).
    egui::Window::new(tr("Dateiname-Konflikt"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(tr1(
                "„{}“ existiert bereits in Amazon Photos.",
                &file_name,
            ));
            ui.checkbox(&mut remember, remember_label.as_str());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add(theme::primary_button(
                        tr("Ignorieren"),
                        ui.visuals().dark_mode,
                    ))
                    .clicked()
                {
                    answer = Some(ConflictResolution::Ignore);
                }
                if ui.button(tr("Als neue Datei speichern")).clicked() {
                    answer = Some(ConflictResolution::SaveAsNew);
                }
                if ui.button(tr("Ersetzen")).clicked() {
                    answer = Some(ConflictResolution::Replace);
                }
            });
        });
    if let Some(a) = answer {
        answer_pending_conflict(&mut app.store, &app.upload, a, remember, true);
    } else {
        // Remember the checkbox even before the user commits to an option.
        app.upload.lock().unwrap().conflict_remember = remember;
    }
}
