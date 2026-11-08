mod cards;
pub(crate) mod dialogs;
mod queue;
mod sections;

use super::*;
use crate::app::PrimePhotoApp;
use crate::platform::app_shared::ONE_TIME_IDX;
use crate::theme::eframe_ui as theme;
use eframe::egui::{self, RichText};

pub(crate) fn backup_ui(app: &mut PrimePhotoApp, ui: &mut egui::Ui) {
    app.backup_ui(ui);
}

impl PrimePhotoApp {
    pub(crate) fn backup_ui(&mut self, ui: &mut egui::Ui) {
        let colors = theme::palette(ui.visuals().dark_mode);
        // Background sync (PORT-5): starts the supervising timer once we are
        // signed in; from then on this is a cheap no-op.
        self.ensure_background_sync();

        ui.horizontal(|ui| {
            if ui
                .add(theme::primary_button(
                    tr("Datensicherung hinzufügen …"),
                    ui.visuals().dark_mode,
                ))
                .clicked()
            {
                dialogs::add_backup_folder(self, ui.ctx());
            }
            let all_paused = *self.paused.borrow();
            let label = if all_paused {
                tr("Alle Sicherungsvorgänge fortsetzen")
            } else {
                tr("Alle Sicherungsvorgänge anhalten")
            };
            // Only the live "is a run active" flag stays here; the aggregated
            // progress display moved to the footer below (user decision
            // 2026-10-14): one display over *all* targets, not just this run.
            let running = self.upload.lock().unwrap().active;
            if ui
                .add_enabled(
                    // Pausable in the idle state too (user decision 2026-10-14):
                    // with no run active it blocks the background sync, and a
                    // pure drag & drop run has no persisted folders either.
                    !self.store.folders.is_empty() || running,
                    theme::secondary_button(label, ui.visuals().dark_mode),
                )
                .clicked()
            {
                let next = !all_paused;
                self.paused.send_replace(next);
                // Persist the new state (user decision 2026-10-14): the pause
                // survives a restart. `save_config` is the existing path — it
                // reports a failure through `config_error` as usual.
                self.config.paused = next;
                self.save_config();
            }
        });

        // Snapshot rows for read-only rendering. Built **before** the queue table
        // (Welle 3.1/3.3): the aggregate progress next to the queue and the live
        // per-folder status both read them, and neither may hold the shared lock
        // twice.
        let rows: Vec<Row> = {
            let s = self.upload.lock().unwrap();
            let mut rows: Vec<Row> = self
                .store
                .folders
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let plan_idx = plan_index_for(&s.folder_paths, &f.path);
                    let in_run = s.active
                        && plan_idx.is_some_and(|plan_idx| s.folder_total.contains_key(&plan_idx));
                    let total = row_total(
                        in_run,
                        plan_idx.and_then(|plan_idx| s.folder_total.get(&plan_idx).copied()),
                        f.total_files,
                    );
                    let done = row_done(
                        in_run,
                        plan_idx.and_then(|plan_idx| s.folder_done.get(&plan_idx).copied()),
                        f.uploaded_files,
                    );
                    // Cap the completed count (Welle 5.11): a legacy folder whose
                    // `total_files < uploaded_files` must not read above 100 % in
                    // the row's progress text or the idle aggregate.
                    let done = capped_done(done, total);
                    // Live status (Welle 3.3): while the run counts this target in
                    // `folder_total` it is *uploading*, regardless of the (stale)
                    // persisted status — the persisted outcome only applies once
                    // the run settled and `active` is false again. A background
                    // run writes its result into a fresh `FolderStore::load()`
                    // (`finalize_background_store`), never into `self.store`, so
                    // without this the row kept showing the previous outcome.
                    let status = if in_run {
                        FolderStatus::Uploading
                    } else if f.enabled {
                        // The missing-source state is a *live* condition (user
                        // decisions 2026-10-16, 2026-10-17): the detached
                        // supervisor writes a missing source to `folders.toml`,
                        // which the UI's in-memory store never reloads — so the
                        // row applies the very same rule (`source_status`) itself.
                        // Same helper, no second logic; a returned drive flips
                        // the row back on its own. A demo fixture supplies the
                        // answer (no directory exists for its fake paths).
                        source_status(
                            self.demo_source_exists(i)
                                .unwrap_or_else(|| f.path.is_dir()),
                            f.status,
                        )
                    } else {
                        f.status
                    };
                    Row {
                        idx: i,
                        name: f.name.clone(),
                        path: f.path.clone(),
                        enabled: f.enabled,
                        status,
                        total,
                        done,
                        new: plan_idx
                            .and_then(|plan_idx| s.folder_new.get(&plan_idx).copied())
                            .unwrap_or(0),
                        existing: plan_idx
                            .and_then(|plan_idx| s.folder_conflict.get(&plan_idx).copied())
                            .unwrap_or(0),
                        unchanged: plan_idx
                            .and_then(|plan_idx| s.folder_unchanged.get(&plan_idx).copied())
                            .unwrap_or(0),
                        failed: plan_idx
                            .and_then(|plan_idx| s.folder_failed.get(&plan_idx).copied())
                            .unwrap_or(0),
                        last_updated: f.last_updated.clone(),
                    }
                })
                .collect();
            // The transient one-time target is a single extra row after the
            // persisted folders — same chrome, but it has no row-action icons
            // (kebab/refresh) and no pause toggle (nothing to configure,
            // nothing to stop).
            if let Some(one) = self.one_time.as_ref() {
                let in_run = s.active && s.folder_total.contains_key(&ONE_TIME_IDX);
                let total = row_total(
                    in_run,
                    s.folder_total.get(&ONE_TIME_IDX).copied(),
                    one.total_files,
                );
                let done = row_done(
                    in_run,
                    s.folder_done.get(&ONE_TIME_IDX).copied(),
                    one.uploaded_files,
                );
                // Same cap as the persisted rows (Welle 5.11).
                let done = capped_done(done, total);
                // Same live-status rule as the persisted folders (Welle 3.3).
                let status = if in_run {
                    FolderStatus::Uploading
                } else {
                    one.status
                };
                rows.push(Row {
                    idx: ONE_TIME_IDX,
                    name: tr("Einmalige Sicherung").to_string(),
                    path: one.path.clone(),
                    enabled: true,
                    status,
                    total,
                    done,
                    new: s.folder_new.get(&ONE_TIME_IDX).copied().unwrap_or(0),
                    existing: s.folder_conflict.get(&ONE_TIME_IDX).copied().unwrap_or(0),
                    unchanged: s.folder_unchanged.get(&ONE_TIME_IDX).copied().unwrap_or(0),
                    failed: s.folder_failed.get(&ONE_TIME_IDX).copied().unwrap_or(0),
                    last_updated: one.last_updated.clone(),
                });
            }
            rows
        };

        let mut actions: Vec<FolderAction> = Vec::new();

        // Live per-file queue as one flat, virtualised table (Welle 3.1,
        // docs/original/SPEC_UPLOAD_PROGRESS.md §7.3/§7.8): permanent main-area
        // content with fixed columns. The rows are sorted globally by file
        // status — no folder grouping and no group headers (user decision
        // 2026-10-26, see [`status_rank`]). The layout only carries queue
        // indices, so no file name is cloned per frame. The Ordner column keeps
        // the run's captured plan paths; same-named folders are disambiguated
        // with "Name (parent)" (user decision 2026-10-16).
        let (folder_paths, remaining, layout) = {
            let s = self.upload.lock().unwrap();
            let remaining = s
                .queue
                .iter()
                .filter(|e| matches!(e.state, FileState::Waiting | FileState::Uploading))
                .count();
            (s.folder_paths.clone(), remaining, queue_layout(&s.queue))
        };
        let ambiguous = ambiguous_folder_names_for_paths(&folder_paths);

        ui.add_space(12.0);
        let queue_section_id = ui.make_persistent_id("backup-queue-section");
        let folders_section_id = ui.make_persistent_id("backup-folders-section");
        let (queue_expanded, _) = sections::show_section_header(
            ui,
            queue_section_id,
            true,
            sections::section_is_open(ui, folders_section_id, true),
            tr("Warteschlange ausklappen"),
            tr("Warteschlange einklappen"),
            |ui| {
                ui.label(
                    RichText::new(tr1("Warteschlange ({})", remaining))
                        .strong()
                        .size(14.0),
                )
            },
        );
        let folders_expanded_for_layout = sections::section_is_open(ui, folders_section_id, true);

        // Shared geometry: header and rows use the same `QueueColumns`, so the
        // four columns line up exactly (Issue #30). The scroll-bar strip is
        // reserved with the floating style too: `theme/eframe_ui.rs` sets
        // `floating_allocated_width = bar_width + 8`, so `allocated_width()`
        // yields 18, not 0. The table therefore ends `bar_w` before the page
        // gutter, and `AlwaysVisible` keeps that right edge constant.
        let content_w = ui.available_width();
        let spacing_x = ui.spacing().item_spacing.x;
        let bar_w = ui.spacing().scroll.allocated_width();
        let cols = queue_columns(content_w, spacing_x, bar_w);

        // Measure the action block once per row so the left text column never
        // runs underneath its controls. The cards scroll inside the user-sized
        // viewport, so their content height no longer constrains the split.
        let running = self.upload.lock().unwrap().active;
        let card_content_w = (cols.table_w - 2.0 * f32::from(theme::PAGE_MARGIN_X) - 2.0).max(1.0);
        let side_ws = cards::measure_folder_card_side_widths(ui, &rows, card_content_w, running);

        // Decide both viewport sizes before drawing either scroll area. With
        // both sections open, the saved ratio splits the flexible height; with
        // one closed, the open section receives all of it. The folder value is
        // intentionally this pre-header snapshot: on close, its old viewport
        // remains allocated for the folder list's final visible frame.
        let free = ui.available_height();
        let mut ratio = self.config.backup_queue_ratio();
        let (table_h, list_h) = page_heights_for_sections(
            free,
            queue_expanded,
            folders_expanded_for_layout,
            ratio,
        );

        queue::show_queue_content(
            ui,
            cols,
            table_h,
            free,
            queue_expanded,
            &layout,
            &self.upload,
            &folder_paths,
            &ambiguous,
            colors,
        );

        let (running, sum_total, outcome) =
            queue::show_aggregate_progress(ui, cols, &self.upload, &rows);

        if queue_expanded && folders_expanded_for_layout {
            let (next_ratio, should_save) =
                sections::show_backup_splitter(ui, ratio, backup_split_splittable(free));
            ratio = next_ratio;
            // Only the both-open state owns the saved ratio; a collapsed section
            // leaves it untouched so reopening restores the previous split.
            self.config.backup_queue_ratio = ratio;
            if should_save {
                self.save_config();
            }
        } else {
            ui.add_space(SECTION_GAP);
        }
        let (folders_expanded, _, _) =
            sections::show_folder_section_header(ui, folders_section_id, queue_expanded);

        // The header toggled *after* the split was computed. On open, defer the
        // list until the next frame applies the both-open split, avoiding
        // content outside this frame's queue-only allocation. On close, retain
        // the list for this final frame in the old both-open allocation.
        let draw_folders = folder_list_draws(folders_expanded, folders_expanded_for_layout);

        // Air between the heading above and the card list below. The heading
        // zone and this gap are part of `QUEUE_BELOW_H`.
        ui.add_space(if draw_folders { FOLDER_HEADING_GAP } else { 0.0 });

        cards::show_folder_cards(
            ui,
            draw_folders,
            cols,
            list_h,
            &rows,
            &side_ws,
            card_content_w,
            running,
            self.dropping_files,
            self.store.folders.is_empty(),
            &mut actions,
            colors,
        );

        for a in actions {
            self.apply_action(a);
        }

        // Footer: only the run's outcome breakdown over **all** targets is left
        // here — the aggregate progress display moved up next to the queue
        // (Welle 3.1). This is a result line, not a second progress bar, and it
        // stays hidden while every counter is 0 (before any run, or freshly
        // reset) and while there is no live run and no real total — no empty or
        // misleading line.
        if (running || sum_total > 0) && !outcome.is_empty() {
            // With an expanded list, `POST_LIST_GAP` precedes this gap; the
            // reserved footer area is `QUEUE_TABLE_TRAILING_H` +
            // `POST_LIST_GAP` + `FOOTER_GAP` + `FOOTER_LINE_H`.
            ui.add_space(FOOTER_GAP);
            ui.label(RichText::new(outcome).color(colors.ink).size(11.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_layout_header_chrome_matches_headless_egui_measurements() {
        fn measure_themed_ui(draw: impl FnOnce(&mut egui::Ui) -> f32) -> f32 {
            let ctx = egui::Context::default();
            let mut draw = Some(draw);
            theme::apply(&ctx, crate::config::Appearance::System);
            let mut measured = None;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    measured = Some(
                        draw.take()
                            .expect("headless UI draw closure runs once")(ui),
                    );
                });
            });
            measured.expect("headless CentralPanel rendered")
        }

        let queue_geometry = std::cell::Cell::new(None);
        let first_queue_row_top = std::cell::Cell::new(None);
        let queue_header_start = std::cell::Cell::new(None);
        let queue_header_h = measure_themed_ui(|ui| {
            let header_top = ui.cursor().min.y;
            queue_header_start.set(Some(header_top));
            let cols = queue_columns(
                ui.available_width(),
                ui.spacing().item_spacing.x,
                ui.spacing().scroll.allocated_width(),
            );
            let geometry = queue::show_queue_table(ui, cols, 100.0, 1, |ui, range| {
                assert_eq!(range, 0..1, "the headless table contains one visible row");
                first_queue_row_top.set(Some(ui.cursor().min.y));
            });
            queue_geometry.set(Some(geometry));
            first_queue_row_top
                .get()
                .expect("the production queue row renderer ran")
                - header_top
        });
        let folder_row_h = std::cell::Cell::new(0.0);
        let folder_label_h = std::cell::Cell::new(0.0);
        let folder_heading_h = measure_themed_ui(|ui| {
            let heading_top = ui.cursor().min.y;
            let id = ui.make_persistent_id("headless-folder-section");
            let (is_open, rendered_row_h, rendered_label_h) =
                sections::show_folder_section_header(ui, id, true);
            assert!(is_open);
            folder_row_h.set(rendered_row_h);
            folder_label_h.set(rendered_label_h);
            // Measure where the next production-layout item would begin. This
            // captures egui's trailing item spacing instead of adding 8 px as
            // a formula or copying the disclosure's drawing code into a test.
            let next_item = ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover());
            next_item.rect.top() - heading_top
        });

        let (header_rect, divider_rect) = queue_geometry
            .get()
            .expect("the production queue header rendered");
        let queue_header_start = queue_header_start
            .get()
            .expect("the production queue header has a starting cursor");
        let first_queue_row_top = first_queue_row_top
            .get()
            .expect("the production queue row renderer ran");
        assert!((header_rect.height() - 20.0).abs() < 0.01);
        assert!((header_rect.top() - queue_header_start - 3.0).abs() < 0.01);
        assert!((divider_rect.top() - queue_header_start - 31.0).abs() < 0.01);
        assert!((divider_rect.height() - 1.0).abs() < 0.01);
        assert!((divider_rect.top() - header_rect.bottom() - 8.0).abs() < 0.01);
        assert!(
            (first_queue_row_top - divider_rect.bottom() - 8.0).abs() < 0.01,
            "first row top {first_queue_row_top}, divider bottom {}",
            divider_rect.bottom()
        );
        assert!(
            (queue_header_h - 40.0).abs() < 0.01,
            "headless queue header geometry includes its 3 px top inset, 20 px row, two 8 px gaps, and divider, measured {queue_header_h}px"
        );
        assert!(
            (queue_header_h - QUEUE_HEADER_TOTAL_H).abs() < 0.01,
            "production queue header + divider + row spacing measured {queue_header_h}px, budget is {QUEUE_HEADER_TOTAL_H}px"
        );
        assert!(
            (folder_label_h.get() - 16.0).abs() < 0.01,
            "folder label response measured {}px",
            folder_label_h.get()
        );
        assert!(
            (folder_row_h.get() - 26.0).abs() < 0.01,
            "folder horizontal layout row measured {}px",
            folder_row_h.get()
        );
        assert!((folder_heading_h - 34.0).abs() < 0.01);
        assert!(
            (folder_heading_h - FOLDER_HEADING_ZONE_H).abs() < 0.01,
            "production folder disclosure + trailing spacing measured {folder_heading_h}px, budget is {FOLDER_HEADING_ZONE_H}px"
        );
        assert!((folder_heading_h - folder_row_h.get() - 8.0).abs() < 0.01);
    }
}
