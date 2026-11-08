use super::*;
use crate::i18n::{tr, tr1, tr2, tr3};
use crate::theme::eframe_ui as theme;
use crate::upload::{calculate_pct, outcome_text, upload_progress_text};
use eframe::egui::{self, RichText};
use std::sync::{Arc, Mutex};

/// One queue-table cell, allocated at exactly `width` × `height` and aligned
/// inside it (`right_aligned` puts the widget at the cell's right edge). The
/// cell is its own `Ui`, so alignment does not depend on the widgets before it,
/// and a long value can never paint into the next column (Issue #30).
fn table_cell(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    right_aligned: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let layout = if right_aligned {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    ui.allocate_ui_with_layout(egui::vec2(width, height), layout, |ui| {
        // Reserve the full cell before adding the widget: a short label's own
        // rect would otherwise shrink the cell and shift every later column.
        ui.set_min_size(egui::vec2(width, height));
        add_contents(ui);
    });
}

/// Draw the fixed queue-table header and divider. Kept on the production path
/// so its actual egui allocation can be measured by the headless layout test.
fn draw_queue_table_header(ui: &mut egui::Ui, cols: QueueColumns) -> (egui::Rect, egui::Rect) {
    let header_row = ui.horizontal(|ui| {
        table_cell(ui, cols.folder_w, QUEUE_HEADER_H, false, |ui| {
            ui.label(RichText::new(tr("Ordner")).strong().size(11.0));
        });
        table_cell(ui, cols.name_w, QUEUE_HEADER_H, false, |ui| {
            ui.label(RichText::new(tr("Name")).strong().size(11.0));
        });
        table_cell(ui, cols.size_w, QUEUE_HEADER_H, true, |ui| {
            ui.label(RichText::new(tr("Größe")).strong().size(11.0));
        });
        table_cell(ui, cols.status_w, QUEUE_HEADER_H, false, |ui| {
            ui.label(RichText::new(tr("Status")).strong().size(11.0));
        });
    });

    // 1 px divider under the header, spanning exactly the table width (never
    // the reserved scroll-bar strip). The following ScrollArea supplies the
    // actual trailing item spacing before the first queue row.
    let (divider, _) =
        ui.allocate_exact_size(egui::vec2(cols.table_w, 1.0), egui::Sense::hover());
    ui.painter().hline(
        divider.x_range(),
        divider.center().y,
        egui::Stroke::new(1.0_f32, theme::palette(ui.visuals().dark_mode).border),
    );
    (header_row.response.rect, divider)
}

/// Draw the queue header and its production virtualized row area together, so
/// header height tests observe the same spacing and first-row position as the
/// live table.
pub(super) fn show_queue_table(
    ui: &mut egui::Ui,
    cols: QueueColumns,
    table_h: f32,
    row_count: usize,
    add_rows: impl FnOnce(&mut egui::Ui, std::ops::Range<usize>),
) -> (egui::Rect, egui::Rect) {
    let header_geometry = draw_queue_table_header(ui, cols);
    egui::ScrollArea::vertical()
        // Own scroll state (user decision 2026-10-16): egui keys a ScrollArea's
        // offset by `make_persistent_id("scroll_area")` when no `id_salt` is
        // given, so the queue and folder list would share one offset.
        .id_salt("queue")
        .max_height(table_h)
        .auto_shrink(false)
        // Honor the cap even when tiny: the default `min_scrolled_height`
        // (64 px) would otherwise overrule a smaller `max_height`.
        .min_scrolled_height(0.0)
        // Reserve the 18 px strip on every frame, not only while hovered; the
        // floating handle stays dormant until hover.
        .scroll_bar_visibility(egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible)
        // Every row is allocated at exactly `QUEUE_ROW_H`, so `show_rows`
        // advances by the same row height plus the theme's item spacing.
        .show_rows(ui, QUEUE_ROW_H, row_count, add_rows);
    header_geometry
}

pub(super) fn show_queue_content(
    ui: &mut egui::Ui,
    cols: QueueColumns,
    table_h: f32,
    free: f32,
    queue_expanded: bool,
    layout: &[usize],
    upload: &Arc<Mutex<UploadShared>>,
    folder_paths: &HashMap<usize, PathBuf>,
    ambiguous: &HashSet<String>,
    colors: theme::Palette,
) {
    if queue_expanded && layout.is_empty() {
        // Centred empty state (Welle 3.1) instead of an empty table. The
        // block may not be taller than the table it replaces (plus the
        // header it saves), or it would eat the list's promised height in a
        // very small window.
        let empty_h = (QUEUE_HEADER_TOTAL_H + table_h).min(free);
        ui.allocate_ui(egui::vec2(cols.table_w, empty_h), |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(theme::muted(tr("Keine Uploads in der Warteschlange")));
            });
        });
    } else if queue_expanded {
        // The production helper keeps the fixed header above the row area's
        // virtualized viewport and shares that exact path with its geometry
        // regression test.
        show_queue_table(ui, cols, table_h, layout.len(), |ui, range| {
            // Keep the content at the table width even while the
            // scroll-bar animation has not reserved its full strip yet;
            // the columns below are fixed widths anyway.
            ui.set_width(cols.table_w);
            // Lock the shared state once per frame for the visible slice.
            let s = upload.lock().unwrap();
            for &queue_idx in &layout[range] {
                let Some(e) = s.queue.get(queue_idx) else {
                    continue;
                };
                // Befund 11c: an `Uploading` row additionally shows
                // the real per-file percentage once bytes have gone
                // out, so progress is visible per file and not only
                // as the coarse state. The whole visible string is
                // one i18n key — never assembled from two parts.
                let (txt, col): (String, egui::Color32) = match e.state {
                    FileState::Waiting => (tr("Warten").to_string(), colors.muted),
                    FileState::Uploading => {
                        let pct = calculate_pct(e.size, e.bytes_sent);
                        if pct > 0 {
                            (tr1("In Bearbeitung · {}%", pct), colors.teal)
                        } else {
                            (tr("In Bearbeitung").to_string(), colors.teal)
                        }
                    }
                    FileState::Done => (tr("Abgeschlossen").to_string(), colors.green),
                    FileState::Failed => (tr("Fehlgeschlagen").to_string(), colors.red),
                    FileState::Skipped => (tr("Übersprungen").to_string(), colors.muted),
                };
                // A long name is truncated in the cell; the full
                // path shows on hover (§7.3).
                let full = e.path.display().to_string();
                // The row's backup folder (disambiguated when the
                // active run holds the name twice).
                let (folder_label, folder_tip) = if e.folder == ONE_TIME_IDX {
                    (tr("Einmalige Sicherung").to_string(), String::new())
                } else {
                    queue_folder_display(folder_paths, e.folder, ambiguous)
                        .unwrap_or_default()
                };
                // Allocated at exactly `QUEUE_ROW_H` so the row
                // advances by `QUEUE_ROW_H + item_spacing.y`,
                // matching what `show_rows` derives.
                ui.allocate_ui_with_layout(
                    egui::vec2(cols.table_w, QUEUE_ROW_H),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        table_cell(ui, cols.folder_w, QUEUE_ROW_H, false, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(folder_label).size(12.0))
                                    .truncate(),
                            )
                            .on_hover_text(folder_tip);
                        });
                        table_cell(ui, cols.name_w, QUEUE_ROW_H, false, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(crate::text::nfc(&e.name)).size(12.0),
                                )
                                .truncate(),
                            )
                            .on_hover_text(full);
                        });
                        table_cell(ui, cols.size_w, QUEUE_ROW_H, true, |ui| {
                            ui.label(
                                RichText::new(crate::text::format_size(e.size)).size(12.0),
                            );
                        });
                        table_cell(ui, cols.status_w, QUEUE_ROW_H, false, |ui| {
                            // Truncated as well: a longer translation
                            // must never paint over the page edge.
                            let status = ui.add(
                                egui::Label::new(RichText::new(txt).color(col).size(12.0))
                                    .truncate(),
                            );
                            // Optional byte detail while a transfer is
                            // live (Befund 11c).
                            if e.state == FileState::Uploading && e.size > 0 {
                                status.on_hover_text(tr2(
                                    "{} von {}",
                                    crate::text::format_size(e.bytes_sent),
                                    crate::text::format_size(e.size),
                                ));
                            }
                        });
                    },
                );
            }
        });
    }
}

pub(super) fn show_aggregate_progress(
    ui: &mut egui::Ui,
    cols: QueueColumns,
    upload: &Arc<Mutex<UploadShared>>,
    rows: &[Row],
) -> (bool, usize, String) {
    // Aggregate progress: **one** display under the queue. While a run is
    // live it shows only the run counters (including when its last target
    // row was removed); while idle it sums all target rows. The byte/ETA
    // detail only exists for a live run; outcome counters remain aggregated
    // over every target (kept for the footer line below).
    let (running, progress_text, outcome, run_done, run_total) = {
        let s = upload.lock().unwrap();
        let outcome = outcome_text(
            s.uploaded_new,
            s.conflict_skipped,
            s.unchanged_skipped,
            s.failed,
        );
        if s.active {
            (
                true,
                upload_progress_text(&s),
                outcome,
                s.done,
                s.total,
            )
        } else {
            (false, String::new(), outcome, 0, 0)
        }
    };
    let (sum_done, sum_total) = displayed_progress(running, run_done, run_total, rows);
    // Reserve the complete progress zone in every branch: 6 px gap + the
    // fixed 26 px row + egui's 8 px trailing item spacing (40 px total).
    ui.add_space(6.0);
    ui.allocate_ui_with_layout(
        egui::vec2(cols.table_w, QUEUE_PROGRESS_ROW_H),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            // An empty `allocate_ui_with_layout` shrinks to ~13 px; the
            // explicit minimum keeps the row at exactly 26 px, so the zone
            // stays deterministic even without a progress display.
            ui.set_min_size(egui::vec2(cols.table_w, QUEUE_PROGRESS_ROW_H));
            if running && sum_total == 0 {
                // An all-current scan is active only while its no-op run is
                // being finalized. Show the same completion wording as the
                // idle state, without a meaningless 0/0 bar or spinner.
                ui.label(progress_text);
            } else if running || sum_total > 0 {
                let frac = if sum_total > 0 {
                    (sum_done as f32 / sum_total as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                // The bar gets an explicit width: the full row without a
                // spinner, the row minus the spinner and the item spacing
                // with one (clamped at 0). It spans the table width, not
                // the full page, so it ends on the same right edge as the
                // header, rows and cards.
                let row_w = cols.table_w;
                // A spinner belongs to this one display and only while a
                // run is active — never a second, separate indicator.
                if running {
                    ui.spinner();
                }
                let text = if running {
                    // The run's file counts plus its byte/ETA detail from
                    // `upload_progress_text` (kept verbatim).
                    tr3(
                        "Gesamt: {} von {} Dateien · {}",
                        sum_done,
                        sum_total,
                        progress_text,
                    )
                } else if sum_total > 0 && sum_done >= sum_total {
                    tr("Alles aktuell").to_string()
                } else {
                    tr2("Gesamt: {} von {} Dateien", sum_done, sum_total)
                };
                // The spinner is a square of `interact_size.y` (26 px,
                // theme); the layout adds `item_spacing.x` between them.
                let bar_w = if running {
                    (row_w
                        - ui.spacing().item_spacing.x
                        - ui.style().spacing.interact_size.y)
                        .max(0.0)
                } else {
                    row_w
                };
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(bar_w)
                        .desired_height(20.0)
                        .text(text),
                );
            }
        },
    );
    (running, sum_total, outcome)
}
