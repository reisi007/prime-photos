use crate::app::{PrimePhotoApp, SettingsTab};
use crate::app_menu;
use crate::folders::ConflictResolution;
use crate::i18n::{self, tr};
use crate::theme;
use eframe::egui;

impl PrimePhotoApp {
    pub(crate) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.settings_tab, SettingsTab::General, tr("Allgemeines"));
            ui.selectable_value(
                &mut self.settings_tab,
                SettingsTab::Bandwidth,
                tr("Bandbreite"),
            );
        });
        ui.separator();

        match self.settings_tab {
            SettingsTab::General => self.general_settings(ui),
            SettingsTab::Bandwidth => self.bandwidth_settings(ui),
        }
    }

    pub(crate) fn general_settings(&mut self, ui: &mut egui::Ui) {
        // Every change is persisted immediately (user decision 2026-10-16: no
        // "Speichern" button anymore). One `changed` flag per tab saves **once**
        // per frame at the end — a slider drag or text edit fires `changed()`
        // on many frames, and per-widget saves would write the file each time.
        let mut changed = false;
        // Language: applied immediately, persisted below.
        ui.horizontal(|ui| {
            ui.label(tr("Sprache"));
            let selected = match self.config.language.as_deref().and_then(i18n::Lang::from_code) {
                Some(i18n::Lang::De) => "Deutsch".to_string(),
                Some(i18n::Lang::En) => "English".to_string(),
                None => tr("Automatisch (System)").to_string(),
            };
            let before = self.config.language.clone();
            egui::ComboBox::from_id_salt("settings-language")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.config.language,
                        None,
                        tr("Automatisch (System)"),
                    );
                    ui.selectable_value(
                        &mut self.config.language,
                        Some(i18n::Lang::De.code().to_string()),
                        "Deutsch",
                    );
                    ui.selectable_value(
                        &mut self.config.language,
                        Some(i18n::Lang::En.code().to_string()),
                        "English",
                    );
                });
            if self.config.language != before {
                let lang = self
                    .config
                    .language
                    .as_deref()
                    .and_then(i18n::Lang::from_code)
                    .unwrap_or_else(i18n::system_lang);
                i18n::set_lang(lang);
                // Refresh the native menu immediately; it is already on screen
                // and would otherwise keep the old language (Issue #8).
                app_menu::refresh_labels();
                self.apply_tray_labels();
                changed = true;
            }
        });
        // No sign-in block here: the login is our own device flow, started
        // automatically by the start gate; sign-out lives in the sidebar's
        // account block. The old text described the removed cookie path.
        ui.separator();
        ui.heading(tr("Upload"));
        ui.horizontal(|ui| {
            ui.label(tr("Album (optional):"));
            changed |= ui.text_edit_singleline(&mut self.album_name).changed();
        });
        ui.add_space(8.0);
        changed |= ui
            .checkbox(
                &mut self.config.notifications,
                tr("Desktop-Benachrichtigungen (Upload fertig / Fehler)"),
            )
            .changed();
        ui.add_space(8.0);
        changed |= ui
            .checkbox(
                &mut self.config.background_sync,
                tr("Hintergrund-Synchronisierung"),
            )
            .changed();
        ui.label(theme::muted(tr(
            "Sucht automatisch nach neuen Dateien in den aktiven Sicherungszielen. Läuft nur, solange Prime Photos geöffnet ist.",
        )));
        ui.add_space(8.0);
        let autostart_resp =
            ui.checkbox(&mut self.autostart_enabled, tr("Nach dem Anmelden starten"));
        if autostart_resp.changed() {
            self.set_autostart(self.autostart_enabled);
            changed = true;
        }
        ui.label(theme::muted(tr(
            "Startet Prime Photos automatisch, wenn du dich am System anmeldest.",
        )));
        // `slow_mode` stays transient on purpose (documented below the checkbox):
        // it must never reach `Config`, so it does not set `changed`.
        ui.add_space(8.0);
        ui.checkbox(&mut self.slow_mode, tr("Slow-Mode (1 Datei gleichzeitig)"));
        ui.label(theme::muted(tr("Gilt nur bis zum nächsten App-Start; wird nicht gespeichert.")));

        // No "Erweitert (Konto)" section anymore (user decision 2026-10-16):
        // the marketplace is chosen at sign-in only, and zone / user agent are
        // protocol internals derived from it — nothing to configure here.
        if changed {
            self.save_config();
        }
    }

    pub(crate) fn bandwidth_settings(&mut self, ui: &mut egui::Ui) {
        // Same immediate-persistence pattern as the general tab: one save per
        // frame at the end, never one per widget (see `general_settings`).
        let mut changed = false;
        ui.heading(tr("Uploads"));
        changed |= ui
            .add(
                egui::Slider::new(&mut self.config.concurrency, 1..=8)
                    .text(tr("Gleichzeitige Uploads"))
                    .integer(),
            )
            .changed();
        ui.add_space(8.0);
        ui.label(tr("Upload-Rate pro Datei"));
        changed |= ui
            .radio_value(&mut self.config.limit_enabled, false, tr("Nicht begrenzen"))
            .changed();
        ui.horizontal(|ui| {
            changed |= ui
                .radio_value(&mut self.config.limit_enabled, true, tr("Begrenzen auf"))
                .changed();
            changed |= ui
                .add_enabled(
                    self.config.limit_enabled,
                    egui::DragValue::new(&mut self.config.limit_kbps).speed(10.0),
                )
                .changed();
            ui.label(tr("KB/Sekunde"));
        });
        ui.label(theme::muted(tr(
            "Hinweis: Die Begrenzung wird pro Datei nach dem Upload angewendet.",
        )));

        // Global dedupe default (user decision 2026-10-15): the preselection of
        // a new backup target and the policy of the one-time (drag & drop)
        // queue — the same role the conflict default below plays, which is why
        // the two are shown as one block and share the closing note. Persisted
        // immediately like every other setting here.
        ui.add_space(8.0);
        ui.label(theme::muted(tr("Standard für neue Upload-Ziele")));
        changed |= ui
            .checkbox(&mut self.config.dedup_default, tr("Duplikate überspringen"))
            .changed();
        ui.label(theme::muted(tr(
            "Dateien, die sich bereits in meinem Konto befinden, nicht sichern",
        )));

        // Application default for the name-conflict policy (user decision
        // 2026-10-15): the preselection of a new backup target and the policy
        // of the one-time (drag & drop) queue. Persisted immediately.
        ui.add_space(8.0);
        ui.label(theme::muted(tr("Standard bei Dateikonflikten")));
        changed |= ui
            .radio_value(
                &mut self.config.conflict_resolution_default,
                ConflictResolution::Ask,
                tr("Jedes Mal nachfragen"),
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut self.config.conflict_resolution_default,
                ConflictResolution::Ignore,
                tr("Ignorieren"),
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut self.config.conflict_resolution_default,
                ConflictResolution::SaveAsNew,
                tr("Neue Datei automatisch umbenennen"),
            )
            .changed();
        changed |= ui
            .radio_value(
                &mut self.config.conflict_resolution_default,
                ConflictResolution::Replace,
                tr("Ersetzen"),
            )
            .changed();
        ui.label(theme::muted(tr(
            "Gilt für neue Upload-Ziele und für Drag&Drop.",
        )));

        ui.separator();
        ui.heading(tr("Downloads"));
        ui.label(theme::muted(tr(
            "Downloads sind in diesem Port noch nicht implementiert.",
        )));

        if changed {
            self.save_config();
        }
    }
}
