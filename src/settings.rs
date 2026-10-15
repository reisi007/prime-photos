use crate::app::{PrimePhotoApp, SettingsTab};
use crate::countries::{country_label, COUNTRIES};
use crate::folders::ConflictResolution;
use crate::i18n::{self, tr, tr1};
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
        // Language: applied immediately, persisted by "Speichern".
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
                self.apply_tray_labels();
            }
        });
        ui.separator();

        ui.heading(tr("Anmeldung"));
        ui.label(theme::muted(tr(
            "Melde dich bei Amazon Photos an. Die Cookies werden automatisch übernommen.",
        )));
        ui.horizontal(|ui| {
            if ui.add(theme::primary_button(tr("Mit Amazon anmelden"))).clicked() {
                self.pending_login = true;
            }
            if self.login.running {
                ui.spinner();
            }
        });
        if let Some(e) = &self.login.error {
            ui.colored_label(theme::RED, e);
        }

        ui.separator();
        ui.heading(tr("Upload"));
        ui.horizontal(|ui| {
            ui.label(tr("Album (optional):"));
            ui.text_edit_singleline(&mut self.album_name);
        });
        ui.checkbox(
            &mut self.config.notifications,
            tr("Desktop-Benachrichtigungen (Upload fertig / Fehler)"),
        );
        ui.checkbox(
            &mut self.config.background_sync,
            tr("Hintergrund-Synchronisierung"),
        );
        ui.label(theme::muted(tr(
            "Sucht automatisch nach neuen Dateien in den aktiven Sicherungszielen. Läuft nur, solange Prime Photos geöffnet ist.",
        )));
        let autostart_resp =
            ui.checkbox(&mut self.autostart_enabled, tr("Nach dem Anmelden starten"));
        if autostart_resp.changed() {
            self.set_autostart(self.autostart_enabled);
        }
        ui.label(theme::muted(tr(
            "Startet Prime Photos automatisch, wenn du dich am System anmeldest.",
        )));
        ui.checkbox(&mut self.slow_mode, tr("Slow-Mode (1 Datei gleichzeitig)"));
        ui.label(theme::muted(tr("Gilt nur bis zum nächsten App-Start; wird nicht gespeichert.")));

        ui.separator();
        ui.collapsing(tr("Erweitert (Konto)"), |ui| {
            ui.label(theme::muted(tr1(
                "Konfiguration: {}",
                crate::text::nfc(&self.config_path),
            )));
            egui::Grid::new("cfg").num_columns(2).show(ui, |ui| {
                ui.label(tr("Amazon-Marktplatz"));
                let current = self.config.country.clone();
                egui::ComboBox::from_id_salt("settings-country")
                    .selected_text(country_label(&current))
                    .show_ui(ui, |ui| {
                        for &(code, _) in COUNTRIES {
                            ui.selectable_value(
                                &mut self.config.country,
                                code.to_string(),
                                country_label(code),
                            );
                        }
                    });
                ui.end_row();
                ui.label(tr("Zone"));
                ui.text_edit_singleline(&mut self.config.zone);
                ui.end_row();
                ui.label(tr("User-Agent"));
                ui.text_edit_singleline(&mut self.config.user_agent);
                ui.end_row();
            });
        });

        ui.add_space(10.0);
        if ui.add(theme::primary_button(tr("Speichern"))).clicked() {
            self.save_config();
        }
    }

    pub(crate) fn bandwidth_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("Uploads"));
        ui.add(
            egui::Slider::new(&mut self.concurrency, 1..=8)
                .text(tr("Gleichzeitige Uploads"))
                .integer(),
        );
        ui.add_space(8.0);
        ui.label(tr("Upload-Rate pro Datei"));
        ui.radio_value(&mut self.limit_enabled, false, tr("Nicht begrenzen"));
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.limit_enabled, true, tr("Begrenzen auf"));
            ui.add_enabled(
                self.limit_enabled,
                egui::DragValue::new(&mut self.limit_kbps).speed(10.0),
            );
            ui.label(tr("KB/Sekunde"));
        });
        ui.add_space(6.0);
        ui.label(theme::muted(tr(
            "Hinweis: Die Begrenzung wird pro Datei nach dem Upload angewendet.",
        )));

        // Global dedupe default (user decision 2026-10-15): the preselection of
        // a new backup target and the policy of the one-time (drag & drop)
        // queue — the same role the conflict default below plays, which is why
        // the two are shown as one block and share the closing note. Persisted
        // with "Speichern".
        ui.add_space(6.0);
        ui.label(theme::muted(tr("Standard für neue Upload-Ziele")));
        ui.checkbox(&mut self.config.dedup_default, tr("Duplikate überspringen"));
        ui.label(theme::muted(tr(
            "Dateien, die sich bereits in meinem Konto befinden, nicht sichern",
        )));

        // Application default for the name-conflict policy (user decision
        // 2026-10-15): the preselection of a new backup target and the policy
        // of the one-time (drag & drop) queue. Persisted with "Speichern".
        ui.add_space(6.0);
        ui.label(theme::muted(tr("Standard bei Dateikonflikten")));
        ui.radio_value(
            &mut self.config.conflict_resolution_default,
            ConflictResolution::Ask,
            tr("Jedes Mal nachfragen"),
        );
        ui.radio_value(
            &mut self.config.conflict_resolution_default,
            ConflictResolution::Ignore,
            tr("Ignorieren"),
        );
        ui.radio_value(
            &mut self.config.conflict_resolution_default,
            ConflictResolution::SaveAsNew,
            tr("Neue Datei automatisch umbenennen"),
        );
        ui.radio_value(
            &mut self.config.conflict_resolution_default,
            ConflictResolution::Replace,
            tr("Ersetzen"),
        );
        ui.add_space(6.0);
        ui.label(theme::muted(tr(
            "Gilt für neue Upload-Ziele und für Drag&Drop.",
        )));

        ui.separator();
        ui.heading(tr("Downloads"));
        ui.label(theme::muted(tr(
            "Downloads sind in diesem Port noch nicht implementiert.",
        )));

        ui.add_space(10.0);
        if ui.add(theme::primary_button(tr("Speichern"))).clicked() {
            self.save_config();
        }
    }
}
