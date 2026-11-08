use crate::config::Appearance;
use crate::i18n;
use rune::tok::{self, Theme};
use rune::{El, col, el, row, text, text_input};

use super::SettingsHost;

pub(super) fn general_page<S: SettingsHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    col()
        .key("general-page")
        .w_full()
        .gap(22.0)
        .child(super::section(
            "appearance-section",
            "Darstellung",
            theme,
            [
                appearance_row(state, theme, colors),
                language_row(state, theme, colors),
            ],
        ))
        .child(super::section(
            "general-upload-section",
            "Uploads",
            theme,
            [
                album_row(state, theme, colors),
                super::toggle_row(
                    "notifications-row",
                    "Desktop-Benachrichtigungen (Upload fertig / Fehler)",
                    "",
                    state.config().notifications,
                    toggle_notifications::<S>,
                    theme,
                    colors,
                ),
                super::toggle_row(
                    "background-sync-row",
                    "Hintergrund-Synchronisierung",
                    "Sucht automatisch nach neuen Dateien in den aktiven Sicherungszielen. Läuft nur, solange Prime Photos geöffnet ist.",
                    state.config().background_sync,
                    toggle_background_sync::<S>,
                    theme,
                    colors,
                ),
                super::toggle_row(
                    "autostart-row",
                    "Nach dem Anmelden starten",
                    "Startet Prime Photos automatisch, wenn du dich am System anmeldest.",
                    state.autostart_enabled(),
                    toggle_autostart::<S>,
                    theme,
                    colors,
                ),
                super::toggle_row(
                    "slow-mode-row",
                    "Slow-Mode (1 Datei gleichzeitig)",
                    "Gilt nur bis zum nächsten App-Start; wird nicht gespeichert.",
                    state.slow_mode(),
                    toggle_slow_mode::<S>,
                    theme,
                    colors,
                ),
            ],
        ))
}

fn album_row<S: SettingsHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    setting_row(
        "album-row",
        "Album (optional):",
        "",
        theme,
        el().key("album-name-field")
            .size(240.0, 36.0)
            .padding(8.0)
            .radius(8.0)
            .fill(theme.bg)
            .border(1.0, colors.control_border)
            .child(
                text_input(state.album_edit(), tok::type_::BODY)
                    .key("album-name-input")
                    .single_line()
                    .on_edit(|state: &mut S| state.album_edit_mut())
                    .on_input_change(|state: &mut S, _| state.album_name_changed())
                    .color(theme.text),
            ),
    )
}

fn appearance_row<S: SettingsHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let appearance = state.config().appearance;
    setting_row(
        "appearance-row",
        "Darstellung",
        "",
        theme,
        appearance_segments::<S>(appearance, theme, colors),
    )
}

fn appearance_segments<S: SettingsHost + 'static>(
    current: Appearance,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let options = [
        (Appearance::Light, "Hell", "appearance-light"),
        (Appearance::Dark, "Dunkel", "appearance-dark"),
        (Appearance::System, "System", "appearance-system"),
    ];
    let mut segments = row()
        .key("appearance-segments")
        .gap(3.0)
        .padding(3.0)
        .fill(theme.control_track)
        .border(1.0, colors.control_border)
        .radius(9.0);
    for (appearance, label, key) in options {
        let selected = appearance == current;
        segments = segments.child(
            el().key(key)
                .role("radio-button")
                .label(if appearance == Appearance::System {
                    i18n::tr("System (automatisch)")
                } else {
                    i18n::tr(label)
                })
                .checked(selected)
                .focusable(true)
                .size(90.0, 32.0)
                .center()
                .radius(7.0)
                .fill(if selected {
                    colors.primary_fill
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(theme.hover)
                .on_press_tx(move |state: &mut S, tx| {
                    if state.config().appearance != appearance {
                        state.config_mut().appearance = appearance;
                        state.persist_settings();
                        tx.color_motion(tok::motion::CALM);
                    }
                })
                .child(
                    text(i18n::tr(label))
                        .font(tok::type_::LABEL)
                        .color(if selected {
                            colors.primary_foreground
                        } else {
                            theme.text_secondary
                        })
                        .a11y_hidden(),
                ),
        );
    }
    segments
}

fn language_row<S: SettingsHost + 'static>(
    state: &S,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    setting_row(
        "language-row",
        "Sprache",
        "",
        theme,
        language_segments::<S>(state.config().language.as_deref(), theme, colors),
    )
}

fn language_segments<S: SettingsHost + 'static>(
    current: Option<&str>,
    theme: Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let options: [(Option<&'static str>, &'static str, &'static str); 3] = [
        (None, "Automatisch (System)", "language-system"),
        (Some("de"), "Deutsch", "language-de"),
        (Some("en"), "English", "language-en"),
    ];
    let mut segments = row()
        .key("language-segments")
        .gap(3.0)
        .padding(3.0)
        .fill(theme.control_track)
        .border(1.0, colors.control_border)
        .radius(9.0);
    for (language, label, key) in options {
        let selected = current == language;
        segments = segments.child(
            el().key(key)
                .role("radio-button")
                .label(i18n::tr(label))
                .checked(selected)
                .focusable(true)
                .size(if language.is_none() { 150.0 } else { 94.0 }, 32.0)
                .center()
                .radius(7.0)
                .fill(if selected {
                    colors.primary_fill
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(theme.hover)
                .on_press(move |state: &mut S| {
                    if state.config().language.as_deref() != language {
                        state.set_language(language);
                    }
                })
                .child(
                    text(i18n::tr(label))
                        .font(tok::type_::LABEL)
                        .color(if selected {
                            colors.primary_foreground
                        } else {
                            theme.text_secondary
                        })
                        .a11y_hidden(),
                ),
        );
    }
    segments
}

fn setting_row(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    theme: Theme,
    control: El,
) -> El {
    row()
        .key(key)
        .w_full()
        .h(62.0)
        .px(14.0)
        .center_y()
        .justify_between()
        .child(
            text(i18n::tr(label))
                .key(format!("{key}-label"))
                .font(tok::type_::BODY)
                .color(theme.text),
        )
        .child(control)
        .description(i18n::tr(description))
}

fn toggle_notifications<S: SettingsHost>(state: &mut S) {
    let enabled = !state.config().notifications;
    state.config_mut().notifications = enabled;
    state.persist_settings();
}

fn toggle_background_sync<S: SettingsHost>(state: &mut S) {
    let enabled = !state.config().background_sync;
    state.config_mut().background_sync = enabled;
    state.persist_settings();
}

fn toggle_autostart<S: SettingsHost>(state: &mut S) {
    state.set_autostart_enabled(!state.autostart_enabled());
}

fn toggle_slow_mode<S: SettingsHost>(state: &mut S) {
    state.set_slow_mode(!state.slow_mode());
}
