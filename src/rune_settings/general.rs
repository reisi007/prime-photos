use crate::config::Appearance;
use crate::i18n;
use rune::tok::{self, Theme};
use rune::{El, col, el, row, text};

use super::SettingsState;

pub(super) fn general_page(state: &SettingsState, theme: Theme, accent: rune::Color) -> El {
    col()
        .key("general-page")
        .w_full()
        .gap(22.0)
        .child(super::section(
            "appearance-section",
            "Darstellung",
            theme,
            [
                appearance_row(state, theme, accent),
                language_row(state, theme, accent),
            ],
        ))
        .child(super::section(
            "general-upload-section",
            "Uploads",
            theme,
            [
                super::toggle_row(
                    "notifications-row",
                    "Desktop-Benachrichtigungen (Upload fertig / Fehler)",
                    "",
                    state.config.notifications,
                    toggle_notifications,
                    theme,
                    accent,
                ),
                super::toggle_row(
                    "background-sync-row",
                    "Hintergrund-Synchronisierung",
                    "Sucht automatisch nach neuen Dateien in den aktiven Sicherungszielen. Läuft nur, solange Prime Photos geöffnet ist.",
                    state.config.background_sync,
                    toggle_background_sync,
                    theme,
                    accent,
                ),
            ],
        ))
}

fn appearance_row(state: &SettingsState, theme: Theme, accent: rune::Color) -> El {
    let appearance = state.config.appearance;
    setting_row(
        "appearance-row",
        "Darstellung",
        "",
        theme,
        appearance_segments(appearance, theme, accent),
    )
}

fn appearance_segments(current: Appearance, theme: Theme, accent: rune::Color) -> El {
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
                    theme.indicator
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(theme.hover)
                .on_press_tx(move |state: &mut SettingsState, tx| {
                    if state.config.appearance != appearance {
                        state.config.appearance = appearance;
                        state.persist();
                        tx.color_motion(tok::motion::CALM);
                    }
                })
                .child(
                    text(i18n::tr(label))
                        .font(tok::type_::LABEL)
                        .color(if selected {
                            theme.text
                        } else {
                            theme.text_secondary
                        })
                        .a11y_hidden(),
                ),
        );
    }
    let _ = accent;
    segments
}

fn language_row(state: &SettingsState, theme: Theme, accent: rune::Color) -> El {
    setting_row(
        "language-row",
        "Sprache",
        "",
        theme,
        language_segments(state.config.language.as_deref(), theme, accent),
    )
}

fn language_segments(current: Option<&str>, theme: Theme, accent: rune::Color) -> El {
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
                    theme.indicator
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(theme.hover)
                .on_press(move |state: &mut SettingsState| {
                    if state.config.language.as_deref() != language {
                        state.set_language(language);
                    }
                })
                .child(
                    text(i18n::tr(label))
                        .font(tok::type_::LABEL)
                        .color(if selected {
                            theme.text
                        } else {
                            theme.text_secondary
                        })
                        .a11y_hidden(),
                ),
        );
    }
    let _ = accent;
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

fn toggle_notifications(state: &mut SettingsState) {
    state.config.notifications = !state.config.notifications;
    state.persist();
}

fn toggle_background_sync(state: &mut SettingsState) {
    state.config.background_sync = !state.config.background_sync;
    state.persist();
}
