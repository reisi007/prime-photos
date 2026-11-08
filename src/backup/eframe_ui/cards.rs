use super::*;
use crate::icons::eframe_ui;
use crate::i18n::{tr, tr1};
use crate::theme::eframe_ui as theme;
use crate::upload::{folder_progress_text, outcome_text, status_text, StatusTone};
use eframe::egui::{self, RichText};

/// Render a shared semantic status tone with the eframe palette.
fn status_color(tone: StatusTone, dark: bool) -> egui::Color32 {
    let colors = theme::palette(dark);
    match tone {
        StatusTone::Info => colors.teal,
        StatusTone::Success => colors.green,
        StatusTone::Warning => colors.amber,
        StatusTone::Error => colors.red,
        StatusTone::Normal => colors.muted,
    }
}

/// The card's small meta line: path · progress · last updated (each present part
/// exactly once, empty parts omitted). Shared by the card draw and its labels.
fn folder_card_meta(r: &Row) -> String {
    let mut line = crate::text::nfc(&r.path.display().to_string());
    let progress = folder_progress_text(r.status, r.done, r.total);
    if !progress.is_empty() {
        line.push_str(" · ");
        line.push_str(&progress);
    }
    if !r.last_updated.is_empty() {
        line.push_str(" · ");
        line.push_str(&tr1("Zuletzt aktualisiert: {}", &r.last_updated));
    }
    line
}

/// One text block in a folder card's left column.
struct CardLabel {
    text: String,
    font: egui::FontId,
    /// `theme::muted` (meta/outcome); the name uses the strong 15 pt style.
    muted: bool,
}

/// The left column of a folder card: name, meta line and — **only when the run
/// produced a result for it** — the outcome line.
fn folder_card_labels(r: &Row) -> Vec<CardLabel> {
    let mut labels = vec![
        CardLabel {
            text: crate::text::nfc(&r.name),
            font: egui::FontId::proportional(15.0),
            muted: false,
        },
        CardLabel {
            text: folder_card_meta(r),
            font: egui::FontId::proportional(11.0),
            muted: true,
        },
    ];
    let outcome = outcome_text(r.new, r.existing, r.unchanged, r.failed);
    if !outcome.is_empty() {
        labels.push(CardLabel {
            text: outcome,
            font: egui::FontId::proportional(11.0),
            muted: true,
        });
    }
    labels
}

impl PrimePhotoApp {
    /// Status word and row actions at a folder card's right edge.
    ///
    /// `with_popup == false` is the invisible measuring pass: the popup is an
    /// overlay and contributes no row width, and skipping it prevents the
    /// probe from creating a second popup area next to the real one.
    fn folder_card_side(
        ui: &mut egui::Ui,
        r: &Row,
        status_txt: &str,
        status_col: egui::Color32,
        running: bool,
        with_popup: bool,
        actions: &mut Vec<FolderAction>,
    ) {
        // The one-time target is transient: no kebab/refresh action icons
        // (nothing to edit, reveal or stop) and no pause toggle.
        if r.idx != ONE_TIME_IDX {
            // Row actions. `egui::menu_button` cannot carry an image (0.31 has
            // no `From<Image> for WidgetText`), so the kebab is a
            // `Button::image` and the popup is driven by hand via
            // `toggle_popup` + `popup_below_widget`. The tint follows the theme
            // text colour (the raster is white).
            let kebab = ui
                .add(
                    egui::Button::image(eframe_ui::kebab_icon(ui))
                        .image_tint_follows_text_color(true)
                        .frame(false),
                )
                .on_hover_text(tr("Aktionen"));
            let popup_id = ui.make_persistent_id(("folder_row_actions", r.idx));
            if kebab.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup_id));
            }
            if with_popup {
                egui::popup::popup_below_widget(
                    ui,
                    popup_id,
                    &kebab,
                    egui::popup::PopupCloseBehavior::CloseOnClick,
                    |ui| {
                        // popup_below_widget sizes the popup to its parent
                        // widget — here the ~26 px kebab icon — which would wrap
                        // every entry character by character.
                        ui.set_min_width(220.0);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        if ui.button(tr("Im Dateimanager anzeigen")).clicked() {
                            actions.push(FolderAction::OpenFinder(r.idx));
                        }
                        if ui.button(tr("Wiederholen")).clicked() {
                            actions.push(FolderAction::Retry(r.idx));
                        }
                        if ui.button(tr("Bearbeiten")).clicked() {
                            actions.push(FolderAction::Edit(r.idx));
                        }
                        ui.separator();
                        if ui.button(tr("Datensicherung stoppen")).clicked() {
                            actions.push(FolderAction::Remove(r.idx));
                        }
                    },
                );
            }
            // The rescan is its own, always visible button — no longer hidden
            // inside the popup.
            if ui
                .add(
                    egui::Button::image(eframe_ui::refresh_icon(ui))
                        .image_tint_follows_text_color(true)
                        .frame(false),
                )
                .on_hover_text(tr("Jetzt scannen und hochladen"))
                .clicked()
            {
                actions.push(FolderAction::Upload(r.idx));
            }
            if ui
                .add_enabled(
                    running,
                    egui::Button::new(
                        RichText::new(if r.enabled { "⏸" } else { "▶" }).size(16.0),
                    )
                    .frame(false),
                )
                .clicked()
            {
                actions.push(FolderAction::ToggleEnabled(r.idx));
            }
        }
        ui.label(RichText::new(status_txt).color(status_col).size(12.0));
    }
}

pub(super) fn measure_folder_card_side_widths(
    ui: &mut egui::Ui,
    rows: &[Row],
    card_content_w: f32,
    running: bool,
) -> Vec<f32> {
    rows
        .iter()
        .map(|r| {
            let (status_txt, status_tone) = status_text(r.status, r.total);
            let status_col = status_color(status_tone, ui.visuals().dark_mode);
            let mut ignored = Vec::new();
            let side_w = {
                let mut probe = ui.new_child(
                    egui::UiBuilder::new()
                        .invisible()
                        .max_rect(egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(card_content_w, 40.0),
                        )),
                );
                let side = probe.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        PrimePhotoApp::folder_card_side(
                            ui,
                            r,
                            &status_txt,
                            status_col,
                            running,
                            false,
                            &mut ignored,
                        );
                    },
                );
                side.response.rect.width()
            };
            side_w
        })
        .collect()
}

pub(super) fn show_folder_cards(
    ui: &mut egui::Ui,
    draw_folders: bool,
    cols: QueueColumns,
    list_h: f32,
    rows: &[Row],
    side_ws: &[f32],
    card_content_w: f32,
    running: bool,
    dropping_files: bool,
    store_empty: bool,
    actions: &mut Vec<FolderAction>,
    colors: theme::Palette,
) {
    // Omit the folder ScrollArea when no folder viewport is allocated
    // (stable collapsed or opening transition). Keep it for the final
    // closing frame to consume its already allocated height. Even
    // `max_height(0)` consumes the trailing item spacing.
    if draw_folders {
        // Cap against the actual remaining space, including on the final
        // closing frame, so the footer cannot be pushed past the bottom if
        // preceding chrome leaves less than the planned list viewport.
        let list_h = list_h.min((ui.available_height() - QUEUE_FOOTER_H).max(0.0));
        // The card labels keep the theme's vertical spacing; only the gap
        // *between* cards is compacted to `FOLDER_CARD_GAP` inside the list.
        let label_spacing_y = ui.spacing().item_spacing.y;
        egui::ScrollArea::vertical()
        // Own scroll state, for the same reason as the queue's salt above:
        // without it both areas share one offset and scroll together.
        .id_salt("folders")
        .max_height(list_h)
        .auto_shrink(false)
        // Honor a shrinking cap: `ScrollArea`'s default `min_scrolled_height`
        // (64 px) would otherwise override a smaller `max_height` and push the
        // footer below off the bottom edge in a very small window.
        .min_scrolled_height(0.0)
        // Always reserve the 18 px strip: the card's right border must stay
        // visible and both lists must end on the same edge (Issue #30).
        .scroll_bar_visibility(
            egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible,
        )
        .show(ui, |ui| {
            // The cards use the table width — not the scroll area's
            // animated content width — so their right edge is constant and
            // matches the queue table above.
            ui.set_width(cols.table_w);
            // The documented 6 px gap between cards (Issue #33).
            ui.spacing_mut().item_spacing.y = FOLDER_CARD_GAP;
            // While a file drag hovers the window, the backup page keeps
            // its hands off: no clicks out from under the overlay.
            if dropping_files {
                ui.disable();
            }
            if store_empty {
                ui.add_space(8.0);
                ui.label(tr(
                    "Wählen Sie einen Ordner aus, um die Sicherung zu starten.",
                ));
                ui.add_space(6.0);
                ui.label(theme::muted(tr(
                    "Tipp: Ordner werden dauerhaft gemerkt und bei jedem Start wieder gesichert.",
                )));
            }
            for (i, r) in rows.iter().enumerate() {
                let (status_txt, status_tone) = status_text(r.status, r.total);
                let status_col = status_color(status_tone, ui.visuals().dark_mode);
                egui::Frame::new()
                    .fill(colors.card_bg)
                    .stroke(egui::Stroke::new(1.0_f32, colors.border))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(
                        theme::PAGE_MARGIN_X,
                        FOLDER_CARD_INNER_Y,
                    ))
                    .outer_margin(egui::Margin::symmetric(0, FOLDER_CARD_OUTER_Y))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // The left column wraps inside what the right
                            // block leaves, so no line can run under the
                            // icons at the card's right edge. `side_ws[i]`
                            // was measured with the same probe and card
                            // width, so labels never overlap the controls.
                            // `allocate_new_ui` (not `allocate_ui_with_layout`
                            // with height 0): the latter would center the
                            // zero-height frame in the row and push the
                            // first line down.
                            let gap = ui.spacing().item_spacing.x;
                            let left_w = (card_content_w - side_ws[i] - gap).max(0.0);
                            ui.allocate_new_ui(
                                egui::UiBuilder::new()
                                    .max_rect(egui::Rect::from_min_size(
                                        ui.cursor().min,
                                        egui::vec2(left_w, ui.available_height()),
                                    ))
                                    .layout(egui::Layout::top_down(egui::Align::Min)),
                                |ui| {
                                    ui.set_min_width(left_w);
                                    // Keep the card's internal label gap at
                                    // the theme spacing; the list gap above
                                    // is the compacted one.
                                    ui.spacing_mut().item_spacing.y = label_spacing_y;
                                    // Name (strong 15 pt), meta (muted 11 pt) and the outcome only
                                    // when the run produced one for this target (user decision 2026-10-14).
                                    for label in folder_card_labels(r) {
                                        let rich = if label.muted {
                                            theme::muted(label.text).size(label.font.size)
                                        } else {
                                            RichText::new(label.text)
                                                .strong()
                                                .size(label.font.size)
                                        };
                                        ui.label(rich);
                                    }
                                },
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    PrimePhotoApp::folder_card_side(
                                        ui,
                                        r,
                                        &status_txt,
                                        status_col,
                                        running,
                                        true,
                                        actions,
                                    );
                                },
                            );
                        });
                    });
            }
        });
        ui.add_space(POST_LIST_GAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_tones_keep_the_legacy_light_and_dark_palette_mapping() {
        for dark in [false, true] {
            let colors = theme::palette(dark);
            for (tone, expected) in [
                (StatusTone::Info, colors.teal),
                (StatusTone::Success, colors.green),
                (StatusTone::Warning, colors.amber),
                (StatusTone::Error, colors.red),
                (StatusTone::Normal, colors.muted),
            ] {
                assert_eq!(status_color(tone, dark), expected, "dark={dark}, {tone:?}");
            }
        }
    }
}
