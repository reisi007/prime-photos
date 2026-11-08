use super::LiveState;
use crate::countries::{self, COUNTRIES};
use crate::i18n::tr;
use rune::tok;
use rune::{El, PopupMenu, ScrollSpec, Tx, col, el, row, text};

pub(super) fn marketplace_picker(
    state: &LiveState,
    theme: tok::Theme,
    colors: crate::rune_theme::Palette,
) -> El {
    let label = tr("Land / Amazon-Marktplatz:");
    let current_country = countries::country_label(&state.config.country);
    let selector = el()
        .key("marketplace-selector")
        .role("combobox")
        .label(current_country.clone())
        .focusable(true)
        .w_full()
        .h(46.0)
        .px(14.0)
        .center_y()
        .radius(9.0)
        .fill(theme.control_track)
        .border(1.0, colors.control_border)
        .hover(theme.hover)
        .on_press_tx(|state: &mut LiveState, tx: &mut Tx| {
            if rune::popup_menu_supported() {
                tx.popup_menu(
                    PopupMenu::picker(
                        COUNTRIES
                            .iter()
                            .map(|&(code, _)| (code, countries::country_label(code))),
                        &state.config.country,
                    ),
                    "marketplace-selector",
                    |state: &mut LiveState, picked, _| {
                        if let Some(country) = picked {
                            select_country(state, &country);
                        }
                    },
                );
            } else {
                state.marketplaces_open = !state.marketplaces_open;
            }
        })
        .child(
            row()
                .w_full()
                .center_y()
                .justify_between()
                .child(
                    text(current_country)
                        .key("marketplace-value")
                        .font(tok::type_::BODY)
                        .color(theme.text),
                )
                .child(text("⌄").font(tok::type_::BODY).color(theme.text_secondary)),
        );

    let mut marketplace_form = col()
        .key("marketplace-form")
        .w_full()
        .gap(8.0)
        .child(
            text(label)
                .key("marketplace-label")
                .font(tok::type_::LABEL)
                .color(theme.text),
        )
        .child(selector);
    if !rune::popup_menu_supported() && state.marketplaces_open {
        let mut options = col()
            .key("marketplace-options")
            .role("listbox")
            .label(label)
            .w_full()
            .h(176.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .padding(6.0)
            .gap(2.0)
            .fill(theme.control_track)
            .border(1.0, colors.control_border)
            .radius(9.0);
        for &(country, _) in COUNTRIES {
            let title = countries::country_label(country);
            let selected = country == state.config.country;
            options = options.child(
                el()
                    .key(format!("marketplace-option-{country}"))
                    .role("option")
                    .label(title.clone())
                    .checked(selected)
                    .focusable(true)
                    .w_full()
                    .h(30.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(if selected {
                        theme.indicator
                    } else {
                        rune::Color::TRANSPARENT
                    })
                    .hover(theme.hover)
                    .on_press(move |state: &mut LiveState| {
                        select_country(state, country);
                        state.marketplaces_open = false;
                    })
                    .child(text(title).font(tok::type_::LABEL).color(theme.text)),
            );
        }
        marketplace_form = marketplace_form.child(options);
    }

    marketplace_form
}

fn select_country(state: &mut LiveState, country: &str) {
    if update_country(&mut state.config.country, country) {
        state.persist_config_if_allowed();
    }
}

fn update_country(current: &mut String, selected: &str) -> bool {
    if current == selected || !COUNTRIES.iter().any(|&(code, _)| code == selected) {
        return false;
    }
    *current = selected.to_owned();
    true
}

#[cfg(test)]
mod tests {
    use super::update_country;

    #[test]
    fn selecting_a_valid_marketplace_updates_the_config_country() {
        let mut country = "de".to_owned();

        assert!(update_country(&mut country, "fr"));
        assert_eq!(country, "fr");
    }

    #[test]
    fn unsupported_or_unchanged_marketplaces_are_ignored() {
        let mut country = "de".to_owned();

        assert!(!update_country(&mut country, "invalid"));
        assert!(!update_country(&mut country, "de"));
        assert_eq!(country, "de");
    }
}
