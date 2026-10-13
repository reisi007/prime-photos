use crate::app::{PrimePhotoApp, SettingsTab};
use crate::countries::{country_label, COUNTRIES};
use crate::theme;
use eframe::egui;

impl PrimePhotoApp {
    pub(crate) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.settings_tab, SettingsTab::General, "Allgemeines");
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Bandwidth, "Bandbreite");
        });
        ui.separator();

        match self.settings_tab {
            SettingsTab::General => self.general_settings(ui),
            SettingsTab::Bandwidth => self.bandwidth_settings(ui),
        }
    }

    pub(crate) fn general_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Anmeldung");
        ui.label(theme::muted(
            "Melde dich bei Amazon Photos an. Die Cookies werden automatisch übernommen.",
        ));
        ui.horizontal(|ui| {
            if ui.add(theme::primary_button("Mit Amazon anmelden")).clicked() {
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
        ui.heading("Upload");
        ui.horizontal(|ui| {
            ui.label("Album (optional):");
            ui.text_edit_singleline(&mut self.album_name);
        });
        ui.checkbox(
            &mut self.config.notifications,
            "Desktop-Benachrichtigungen (Upload fertig / Fehler)",
        );

        ui.separator();
        ui.collapsing("Erweitert (Konto)", |ui| {
            ui.label(theme::muted(format!("Konfiguration: {}", self.config_path)));
            egui::Grid::new("cfg").num_columns(2).show(ui, |ui| {
                ui.label("Amazon-Marktplatz");
                let current = self.config.country.clone();
                egui::ComboBox::from_id_salt("settings-country")
                    .selected_text(country_label(&current))
                    .show_ui(ui, |ui| {
                        for (code, label) in COUNTRIES {
                            ui.selectable_value(&mut self.config.country, code.to_string(), *label);
                        }
                    });
                ui.end_row();
                ui.label("Zone");
                ui.text_edit_singleline(&mut self.config.zone);
                ui.end_row();
                ui.label("User-Agent");
                ui.text_edit_singleline(&mut self.config.user_agent);
                ui.end_row();
            });
        });

        ui.add_space(10.0);
        if ui.add(theme::primary_button("Speichern")).clicked() {
            self.save_config();
        }
    }

    pub(crate) fn bandwidth_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Uploads");
        ui.add(
            egui::Slider::new(&mut self.concurrency, 1..=8)
                .text("Gleichzeitige Uploads")
                .integer(),
        );
        ui.add_space(8.0);
        ui.label("Upload-Rate pro Datei");
        ui.radio_value(&mut self.limit_enabled, false, "Nicht begrenzen");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.limit_enabled, true, "Begrenzen auf");
            ui.add_enabled(
                self.limit_enabled,
                egui::DragValue::new(&mut self.limit_kbps).speed(10.0),
            );
            ui.label("KB/Sekunde");
        });
        ui.add_space(6.0);
        ui.label(theme::muted(
            "Hinweis: Die Begrenzung wird pro Datei nach dem Upload angewendet.",
        ));

        ui.separator();
        ui.heading("Downloads");
        ui.label(theme::muted(
            "Downloads sind in diesem Port noch nicht implementiert.",
        ));

        ui.add_space(10.0);
        if ui.add(theme::primary_button("Speichern")).clicked() {
            self.save_config();
        }
    }
}
