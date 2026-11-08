use super::*;
use crate::i18n::tr;
use crate::theme::eframe_ui as theme;
use eframe::egui;

pub(super) fn section_is_open(ui: &egui::Ui, id: egui::Id, default_open: bool) -> bool {
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open)
        .is_open()
}


/// Draw a standard egui disclosure control and persist its open state. The
/// toggle is keyboard-focusable and announces its localized action and section.
///
/// `other_open` enforces the hard invariant: a section may only be closed while
/// the other one is open. If both persisted states are closed, this header
/// reopens itself before painting, so the icon and the announced action always
/// match the real state.
pub(super) fn show_section_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    default_open: bool,
    other_open: bool,
    show_label: &str,
    hide_label: &str,
    add_header: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> (bool, f32) {
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        default_open,
    );
    let corrected = keep_one_section_open(state.is_open(), other_open);
    if corrected != state.is_open() {
        state.set_open(corrected);
        state.store(ui.ctx());
    }
    // The only open section cannot be collapsed. Expose that as disabled to
    // assistive technology and remove click semantics instead of swallowing an
    // apparently enabled action.
    let can_toggle = section_toggle_enabled(ui.is_enabled(), other_open);
    let header = ui.horizontal(|ui| {
        let item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;
        // A custom toggle (not `show_toggle_button`) keeps the localized action
        // label and supports a genuinely disabled state for the last section.
        let size = egui::vec2(ui.spacing().indent, ui.spacing().icon_width);
        let (_auto_id, disclosure_rect) = ui.allocate_space(size);
        ui.spacing_mut().item_spacing = item_spacing;
        let label_response = add_header(ui);
        // One interaction and one id cover both icon and label, keeping a
        // single keyboard focus target, accessibility action and focus ring.
        let header_rect = disclosure_rect.union(label_response.rect);
        let sense = if can_toggle {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let toggle = ui.interact(header_rect, id, sense);
        let toggle = if can_toggle {
            toggle.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            toggle
        };
        if let Some(next) = section_toggle_after_click(
            toggle.clicked(),
            can_toggle,
            state.is_open(),
            other_open,
        ) {
            state.set_open(next);
            state.store(ui.ctx());
            ui.ctx().request_repaint();
        }
        let accessible_label = if state.is_open() {
            hide_label
        } else {
            show_label
        };
        toggle.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, can_toggle, accessible_label)
        });
        let openness = state.openness(ui.ctx());
        let (mut icon_rect, _) = ui.spacing().icon_rectangles(disclosure_rect);
        icon_rect.set_center(egui::pos2(
            disclosure_rect.left() + ui.spacing().indent / 2.0,
            disclosure_rect.center().y,
        ));
        let icon_response = toggle.clone().with_new_rect(icon_rect);
        egui::collapsing_header::paint_default_icon(ui, openness, &icon_response);
        // `Sense::click` makes this shared control keyboard-focusable, but the
        // custom interaction does not paint egui's usual focus outline.
        // Expand outside the icon and label so the ring cannot cover either.
        if can_toggle && toggle.has_focus() {
            ui.painter().rect_stroke(
                header_rect.expand(2.0),
                egui::CornerRadius::same(3),
                egui::Stroke::new(
                    1.5_f32,
                    crate::theme::eframe_ui::palette(ui.visuals().dark_mode).focus,
                ),
                egui::StrokeKind::Outside,
            );
        }
    });
    state.store(ui.ctx());
    (state.is_open(), header.response.rect.height())
}

/// The production folder disclosure heading, including its exact localized
/// label and toggle action. Sharing this helper lets the headless geometry test
/// measure the same rendered row rather than a stand-in.
pub(super) fn show_folder_section_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    queue_open: bool,
) -> (bool, f32, f32) {
    let mut label_h = 0.0;
    let (is_open, row_h) = show_section_header(
        ui,
        id,
        true,
        queue_open,
        tr("Automatisch gesicherte Ordner ausklappen"),
        tr("Automatisch gesicherte Ordner einklappen"),
        |ui| {
            let label = ui.label(theme::muted(tr("Automatisch gesicherte Ordner:")).size(14.0));
            label_h = label.rect.height();
            label
        },
    );
    (is_open, row_h, label_h)
}

/// Draw the accessible drag handle between the two open scroll panes.
/// Up/down arrows adjust the same ratio for keyboard users.
pub(super) fn show_backup_splitter(ui: &mut egui::Ui, ratio: f32, splittable_h: f32) -> (f32, bool) {
    ui.add_space(SPLITTER_GAP);
    let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), SPLITTER_H));
    let response = ui.interact(
        rect,
        ui.make_persistent_id("backup-layout-splitter"),
        egui::Sense::click_and_drag(),
    );
    if response.clicked() || response.drag_started() || response.is_pointer_button_down_on() {
        response.request_focus();
    }
    let ratio = sanitize_backup_queue_ratio(ratio);
    let enabled = ui.is_enabled();
    let accessible_label = tr("Höhe von Warteschlange und Ordnern anpassen");
    response.widget_info(|| egui::WidgetInfo::slider(enabled, f64::from(ratio), accessible_label));
    let response = response.on_hover_text(tr(
        "Mit den Pfeiltasten nach oben oder unten anpassen",
    ));

    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    let colors = theme::palette(ui.visuals().dark_mode);
    let grip_color = if response.dragged() || response.has_focus() {
        colors.focus
    } else if response.hovered() {
        colors.muted
    } else {
        colors.border
    };
    let grip = egui::Rect::from_center_size(rect.center(), egui::vec2(48.0, 2.0));
    ui.painter().rect_filled(grip, 1.0, grip_color);

    let mut next_ratio = adjusted_backup_queue_ratio(ratio, response.drag_delta().y, splittable_h);
    let keyboard_delta = if response.has_focus() {
        let step = if ui.input(|input| input.modifiers.shift) {
            0.10
        } else {
            0.03
        };
        ui.input(|input| {
            if input.key_pressed(egui::Key::ArrowUp) {
                -step
            } else if input.key_pressed(egui::Key::ArrowDown) {
                step
            } else {
                0.0
            }
        })
    } else {
        0.0
    };
    if keyboard_delta != 0.0 {
        next_ratio = sanitize_backup_queue_ratio(next_ratio + keyboard_delta);
    }
    ui.add_space(SPLITTER_GAP);
    let should_save = response.drag_stopped() || keyboard_delta != 0.0;
    (next_ratio, should_save)
}
