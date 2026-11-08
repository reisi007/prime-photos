use crate::app::{PrimePhotoApp, SettingsTab};
use crate::config::Appearance;
use crate::app_menu;
use crate::folders::ConflictResolution;
use crate::i18n::{self, tr};
use crate::theme::eframe_ui as theme;
use eframe::egui;

pub(crate) fn settings_ui(app: &mut PrimePhotoApp, ui: &mut egui::Ui) {
    app.settings_ui(ui);
}

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
        let previous_appearance = self.config.appearance;
        ui.horizontal(|ui| {
            let _ = add_appearance_combo(ui, &mut self.config.appearance);
        });
        if self.config.appearance != previous_appearance {
            theme::apply(ui.ctx(), self.config.appearance);
            changed = true;
        }
        // No sign-in block here: the login is our own device flow, started
        // automatically by the start gate; sign-out lives in the sidebar's
        // account block. The old text described the removed cookie path.
        ui.separator();
        ui.heading(tr("Upload"));
        ui.horizontal(|ui| {
            ui.label(tr("Album (optional):"));
            if ui.text_edit_singleline(&mut self.album_name).changed() {
                self.config.album_name = self.album_name.clone();
                changed = true;
            }
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
        // `slow_mode` stays transient on purpose (documented below the
        // checkbox): it must never reach `Config`, so it does not set
        // `changed`. `set_slow_mode` also pushes the switch into a live run.
        ui.add_space(8.0);
        let mut slow_mode = self.slow_mode;
        if ui
            .checkbox(&mut slow_mode, tr("Slow-Mode (1 Datei gleichzeitig)"))
            .changed()
        {
            self.set_slow_mode(slow_mode);
        }
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
        ui.horizontal(|ui| {
            let label = ui.label(tr("Gleichzeitige Uploads"));
            changed |= add_concurrency_slider(ui, &mut self.config.concurrency, label.id).changed();
            ui.label(self.config.concurrency.to_string());
        });
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

fn add_appearance_combo(ui: &mut egui::Ui, appearance: &mut Appearance) -> egui::Response {
    let label = ui.label(tr("Darstellung"));
    let selected = match appearance {
        Appearance::System => tr("System (automatisch)"),
        Appearance::Light => tr("Hell"),
        Appearance::Dark => tr("Dunkel"),
    };
    egui::ComboBox::from_id_salt("settings-appearance")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                appearance,
                Appearance::System,
                tr("System (automatisch)"),
            );
            ui.selectable_value(appearance, Appearance::Light, tr("Hell"));
            ui.selectable_value(appearance, Appearance::Dark, tr("Dunkel"));
        })
        .response
        .labelled_by(label.id)
}

/// Keep this as egui's native Slider so it supports pointer dragging, keyboard
/// navigation and its normal accessibility semantics.
fn add_concurrency_slider(
    ui: &mut egui::Ui,
    value: &mut usize,
    label_id: egui::Id,
) -> egui::Response {
    let colors = theme::palette(ui.visuals().dark_mode);
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.bg_fill = colors.slider_track;
        ui.visuals_mut().selection.bg_fill = colors.slider_active;
        ui.visuals_mut().slider_trailing_fill = true;
        ui.style_mut().spacing.slider_rail_height = 4.0;
        ui.add_sized(
            [280.0, 28.0],
            egui::Slider::new(value, 1..=8)
                .integer()
                .show_value(false)
                .trailing_fill(true),
        )
    })
    .inner
    .labelled_by(label_id)
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    fn frame(
        ctx: &egui::Context,
        value: &mut usize,
        events: Vec<egui::Event>,
        rect: &mut Option<egui::Rect>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 180.0),
            )),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let label = ui.label("Gleichzeitige Uploads");
                *rect = Some(super::add_concurrency_slider(ui, value, label.id).rect);
            });
        });
    }

    #[test]
    fn concurrency_slider_is_labelled_for_assistive_technology() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 180.0),
            )),
            ..Default::default()
        };
        let mut value = 3;
        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let label = ui.label("Gleichzeitige Uploads");
                let _ = super::add_concurrency_slider(ui, &mut value, label.id);
            });
        });

        let update = output
            .platform_output
            .accesskit_update
            .expect("AccessKit update");
        let label_id = update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Label
                    && node.value() == Some("Gleichzeitige Uploads")
            })
            .map(|(id, _)| *id)
            .expect("visible label node in the accessibility tree");
        let slider_node = update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == egui::accesskit::Role::Slider)
            .map(|(_, node)| node)
            .expect("slider node in the accessibility tree");
        assert_eq!(
            slider_node.labelled_by(),
            &[label_id],
            "the visible label should be the slider's accessible name"
        );
    }

    #[test]
    fn appearance_combo_is_labelled_for_assistive_technology() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500.0, 180.0),
            )),
            ..Default::default()
        };
        let mut appearance = crate::config::Appearance::System;
        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _ = super::add_appearance_combo(ui, &mut appearance);
            });
        });

        let update = output
            .platform_output
            .accesskit_update
            .expect("AccessKit update");
        let combo_node = update
            .nodes
            .iter()
            .find(|(_, node)| !node.labelled_by().is_empty())
            .map(|(_, node)| node)
            .expect("appearance combo node in the accessibility tree");
        assert_eq!(combo_node.labelled_by().len(), 1);
        let label_node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == combo_node.labelled_by()[0])
            .map(|(_, node)| node)
            .expect("visible appearance label node in the accessibility tree");
        assert_eq!(label_node.role(), egui::accesskit::Role::Label);
    }

    #[test]
    fn concurrency_slider_changes_value_when_dragged_with_the_pointer() {
        let ctx = egui::Context::default();
        crate::theme::eframe_ui::apply(&ctx, crate::config::Appearance::Light);
        let mut value = 3;
        let mut rect = None;
        frame(&ctx, &mut value, Vec::new(), &mut rect);
        let slider_rect = rect.expect("slider response rect");
        let start = egui::pos2(slider_rect.left() + 40.0, slider_rect.center().y);
        let end = egui::pos2(
            slider_rect.left() + slider_rect.width() * 0.75,
            slider_rect.center().y,
        );
        let mut later_rect = None;

        frame(
            &ctx,
            &mut value,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            &mut later_rect,
        );
        frame(
            &ctx,
            &mut value,
            vec![egui::Event::PointerMoved(end)],
            &mut later_rect,
        );
        frame(
            &ctx,
            &mut value,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
            &mut later_rect,
        );

        assert!(value > 3, "drag should change the exact integer value: {value}");
        assert!(value <= 8, "slider stays in its configured range: {value}");
    }
}
