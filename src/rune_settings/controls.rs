use crate::folders::ConflictResolution;
use crate::i18n;
use rune::tok::{self, Theme};
use rune::{Drag, El, Phase, col, el, row, text, text_input};

use super::{SLIDER_HEIGHT, SLIDER_KNOB, SLIDER_WIDTH, SettingsState};

pub(super) fn bandwidth_page(state: &SettingsState, theme: Theme, accent: rune::Color) -> El {
    col()
        .key("bandwidth-page")
        .w_full()
        .gap(22.0)
        .child(super::section(
            "uploads-section",
            "Uploads",
            theme,
            [
                concurrency_row(state, theme, accent),
                upload_rate_row(state, theme, accent),
                super::toggle_row(
                    "dedup-default-row",
                    "Duplikate überspringen",
                    "Dateien, die sich bereits in meinem Konto befinden, nicht sichern",
                    state.config.dedup_default,
                    super::toggle_dedup_default,
                    theme,
                    accent,
                ),
                conflict_resolution_row(state, theme, accent),
            ],
        ))
        .child(super::section(
            "downloads-section",
            "Downloads",
            theme,
            [text(i18n::tr(
                "Downloads sind in diesem Port noch nicht implementiert.",
            ))
            .key("downloads-placeholder")
            .font(tok::type_::BODY)
            .color(theme.text_secondary)
            .wrap()
            .padding(14.0)],
        ))
}

fn concurrency_row(state: &SettingsState, theme: Theme, accent: rune::Color) -> El {
    row()
        .key("concurrency-row")
        .w_full()
        .h(64.0)
        .px(14.0)
        .center_y()
        .justify_between()
        .child(
            text(i18n::tr("Gleichzeitige Uploads"))
                .key("concurrency-label")
                .font(tok::type_::BODY)
                .color(theme.text),
        )
        .child(concurrency_slider(
            state.config.concurrency(),
            theme,
            accent,
        ))
        .child(
            text(state.config.concurrency().to_string())
                .key("concurrency-value")
                .font(tok::type_::MONO_CAPTION)
                .color(theme.text_secondary)
                .w(34.0)
                .align(rune::Align::End),
        )
}

fn concurrency_slider(value: usize, theme: Theme, accent: rune::Color) -> El {
    let knob_center = slider_knob_center(value);
    let track_start = SLIDER_KNOB / 2.0;
    let track_width = SLIDER_WIDTH - SLIDER_KNOB;
    let fill_width = (knob_center - track_start).max(0.0);
    el().key("concurrency-slider")
        .role("slider")
        .label(i18n::tr("Gleichzeitige Uploads"))
        .range_value(value as f64, 1.0, 8.0)
        .range_step(1.0)
        .size(SLIDER_WIDTH, SLIDER_HEIGHT)
        .on_step(|state: &mut SettingsState, steps, _| {
            set_concurrency(
                state,
                (state.config.concurrency() as i32 + steps).clamp(1, 8) as usize,
            );
        })
        .on_step_limit(|state: &mut SettingsState, maximum, _| {
            set_concurrency(state, if maximum { 8 } else { 1 });
        })
        .on_drag(
            |state: &mut SettingsState, drag: &mut Drag, _| match drag.phase {
                Phase::Down => {
                    state.slider_origin = state.config.concurrency();
                    let current_knob = slider_knob_center(state.slider_origin);
                    drag.data = if (drag.x - current_knob).abs() <= SLIDER_KNOB / 2.0 + 3.0 {
                        drag.x - current_knob
                    } else {
                        0.0
                    };
                    set_concurrency(state, slider_value_at(drag.x - drag.data));
                }
                Phase::Move => set_concurrency(state, slider_value_at(drag.x - drag.data)),
                Phase::Up if drag.cancelled() => set_concurrency(state, state.slider_origin),
                Phase::Up => {}
            },
        )
        .child(
            el().key("concurrency-track")
                .size(track_width, 4.0)
                .absolute(track_start, (SLIDER_HEIGHT - 4.0) / 2.0)
                .radius(2.0)
                .fill(theme.control_off)
                .a11y_hidden(),
        )
        .child(
            el().key("concurrency-fill")
                .size(fill_width, 4.0)
                .absolute(track_start, (SLIDER_HEIGHT - 4.0) / 2.0)
                .radius(2.0)
                .fill(accent)
                .a11y_hidden(),
        )
        .child(
            el().key("concurrency-knob")
                .size(SLIDER_KNOB, SLIDER_KNOB)
                .absolute(
                    knob_center - SLIDER_KNOB / 2.0,
                    (SLIDER_HEIGHT - SLIDER_KNOB) / 2.0,
                )
                .radius(SLIDER_KNOB / 2.0)
                .fill(theme.knob)
                .shadow(1.0, 3.0, theme.knob_shadow)
                .a11y_hidden(),
        )
}

fn slider_knob_center(value: usize) -> f32 {
    let fraction = (value.clamp(1, 8) - 1) as f32 / 7.0;
    SLIDER_KNOB / 2.0 + fraction * (SLIDER_WIDTH - SLIDER_KNOB)
}

fn slider_value_at(x: f32) -> usize {
    let fraction = ((x - SLIDER_KNOB / 2.0) / (SLIDER_WIDTH - SLIDER_KNOB)).clamp(0.0, 1.0);
    1 + (fraction * 7.0).round() as usize
}

fn set_concurrency(state: &mut SettingsState, value: usize) {
    let value = value.clamp(1, 8);
    if state.config.concurrency != value {
        state.config.concurrency = value;
        state.persist();
    }
}

fn upload_rate_row(state: &SettingsState, theme: Theme, accent: rune::Color) -> El {
    col()
        .key("upload-rate-row")
        .w_full()
        .px(14.0)
        .py(12.0)
        .gap(10.0)
        .child(
            text(i18n::tr("Upload-Rate pro Datei"))
                .key("upload-rate-label")
                .font(tok::type_::BODY)
                .color(theme.text),
        )
        .child(
            row()
                .key("upload-rate-controls")
                .w_full()
                .center_y()
                .gap(8.0)
                .child(rate_option(
                    "limit-unlimited",
                    "Nicht begrenzen",
                    false,
                    state.config.limit_enabled,
                    theme,
                    accent,
                ))
                .child(rate_option(
                    "limit-enabled",
                    "Begrenzen auf",
                    true,
                    state.config.limit_enabled,
                    theme,
                    accent,
                ))
                .child(rune::spacer())
                .child(rate_input(state, theme))
                .child(
                    text(i18n::tr("KB/Sekunde"))
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .child(
            text(i18n::tr(
                "Hinweis: Die Begrenzung wird pro Datei nach dem Upload angewendet.",
            ))
            .font(tok::type_::LABEL)
            .color(theme.text_secondary)
            .wrap(),
        )
}

fn rate_option(
    key: &'static str,
    label: &'static str,
    enabled: bool,
    current: bool,
    theme: Theme,
    accent: rune::Color,
) -> El {
    let selected = enabled == current;
    el().key(key)
        .role("radio-button")
        .label(i18n::tr(label))
        .checked(selected)
        .focusable(true)
        .h(34.0)
        .px(10.0)
        .center_y()
        .radius(8.0)
        .fill(if selected {
            theme.indicator
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(theme.hover)
        .on_press(move |state: &mut SettingsState| set_limit_enabled(state, enabled))
        .child(
            text(i18n::tr(label))
                .font(tok::type_::LABEL)
                .color(if selected {
                    accent
                } else {
                    theme.text_secondary
                })
                .a11y_hidden(),
        )
}

fn set_limit_enabled(state: &mut SettingsState, enabled: bool) {
    if state.config.limit_enabled != enabled {
        state.config.limit_enabled = enabled;
        state.persist();
    }
}

fn rate_input(state: &SettingsState, theme: Theme) -> El {
    el().key("limit-kbps-field")
        .size(104.0, 36.0)
        .padding(8.0)
        .radius(8.0)
        .fill(theme.bg)
        .border(1.0, theme.card_border)
        .child(
            text_input(&state.limit_edit, tok::type_::BODY)
                .key("limit-kbps-input")
                .single_line()
                .on_edit(|state: &mut SettingsState| &mut state.limit_edit)
                .on_input_change(|state: &mut SettingsState, _| state.set_limit_from_edit())
                .disabled(!state.config.limit_enabled)
                .color(theme.text),
        )
}

fn conflict_resolution_row(state: &SettingsState, theme: Theme, accent: rune::Color) -> El {
    col()
        .key("conflict-default-row")
        .w_full()
        .px(14.0)
        .py(12.0)
        .gap(8.0)
        .child(
            text(i18n::tr("Standard bei Dateikonflikten"))
                .key("conflict-default-label")
                .font(tok::type_::BODY)
                .color(theme.text),
        )
        .child(
            row()
                .w_full()
                .gap(8.0)
                .child(conflict_option(
                    "conflict-ask",
                    "Jedes Mal nachfragen",
                    ConflictResolution::Ask,
                    state.config.conflict_resolution_default,
                    theme,
                    accent,
                ))
                .child(conflict_option(
                    "conflict-ignore",
                    "Ignorieren",
                    ConflictResolution::Ignore,
                    state.config.conflict_resolution_default,
                    theme,
                    accent,
                )),
        )
        .child(
            row()
                .w_full()
                .gap(8.0)
                .child(conflict_option(
                    "conflict-save-as-new",
                    "Neue Datei automatisch umbenennen",
                    ConflictResolution::SaveAsNew,
                    state.config.conflict_resolution_default,
                    theme,
                    accent,
                ))
                .child(conflict_option(
                    "conflict-replace",
                    "Ersetzen",
                    ConflictResolution::Replace,
                    state.config.conflict_resolution_default,
                    theme,
                    accent,
                )),
        )
        .child(
            text(i18n::tr("Gilt für neue Upload-Ziele und für Drag&Drop."))
                .font(tok::type_::LABEL)
                .color(theme.text_secondary),
        )
}

fn conflict_option(
    key: &'static str,
    label: &'static str,
    value: ConflictResolution,
    current: ConflictResolution,
    theme: Theme,
    accent: rune::Color,
) -> El {
    let selected = value == current;
    el().key(key)
        .role("radio-button")
        .label(i18n::tr(label))
        .checked(selected)
        .focusable(true)
        .grow(1.0)
        .h(36.0)
        .px(10.0)
        .center_y()
        .radius(8.0)
        .fill(if selected {
            theme.indicator
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(theme.hover)
        .on_press(move |state: &mut SettingsState| {
            if state.config.conflict_resolution_default != value {
                state.config.conflict_resolution_default = value;
                state.persist();
            }
        })
        .child(
            text(i18n::tr(label))
                .font(tok::type_::LABEL)
                .color(if selected {
                    accent
                } else {
                    theme.text_secondary
                })
                .a11y_hidden(),
        )
}
