//! In-place localization of Rune's standard menu titles.
//!
//! Rune installs its Edit and Window menus (and the app menu's standard items)
//! with fixed English titles. This module re-titles those **existing** items in
//! place: it never builds a menu, never adds or removes an item and never
//! touches a key equivalent or action, so Rune's routing stays exactly as it is.
//!
//! Items are found through their stable action selectors, not through their
//! titles: the titles are what changes here, so a title lookup would break the
//! switch back to English. [`localized`] is the pure English-title → label
//! mapping on top, exercised for both languages in the tests.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};

use crate::i18n::{Lang, tr_for};

/// Rune's English item title → our German key (the i18n source language).
///
/// The English titles are the ones Rune's `standard::ITEMS` and its app-menu
/// builder install, so `tr_for(Lang::En, key)` returns the entry's title again.
const TITLES: &[(&str, &str)] = &[
    ("Edit", "Bearbeiten"),
    ("Undo", "Rückgängig"),
    ("Redo", "Wiederholen"),
    ("Cut", "Ausschneiden"),
    ("Copy", "Kopieren"),
    ("Paste", "Einfügen"),
    ("Select All", "Alles auswählen"),
    ("Window", "Fenster"),
    ("Minimize", "Minimieren"),
    ("Close", "Schließen"),
    // The app menu: its title is the product name (the same in both
    // languages); its entries carry the product name in English.
    ("Prime Photos", "Prime Photos"),
    ("About Prime Photos", "Über Prime Photos"),
    ("Quit Prime Photos", "Prime Photos beenden"),
    ("Hide Prime Photos", "Prime Photos ausblenden"),
    ("Hide Others", "Andere ausblenden"),
    ("Show All", "Alle einblenden"),
    ("Services", "Dienste"),
];

/// Stable selectors of the items we rename, paired with their English title.
///
/// Titles are left out on purpose: Rune derives the app-menu titles from the
/// process name ("Quit prime-photos" in a dev binary), so the selector is the
/// reliable anchor. The Services item has no action and is found by its title
/// instead.
const ACTIONS: &[(&str, &str)] = &[
    ("undo:", "Undo"),
    ("redo:", "Redo"),
    ("cut:", "Cut"),
    ("copy:", "Copy"),
    ("paste:", "Paste"),
    ("selectAll:", "Select All"),
    ("performMiniaturize:", "Minimize"),
    ("performClose:", "Close"),
    ("orderFrontStandardAboutPanel:", "About Prime Photos"),
    ("terminate:", "Quit Prime Photos"),
    ("hide:", "Hide Prime Photos"),
    ("hideOtherApplications:", "Hide Others"),
    ("unhideAllApplications:", "Show All"),
];

/// The localized label for one of Rune's English menu titles, or `None` for a
/// title this module does not own.
fn localized(lang: Lang, english: &'static str) -> Option<&'static str> {
    TITLES
        .iter()
        .find(|(title, _)| *title == english)
        .map(|(_, key)| tr_for(lang, key))
}

/// Rename every standard item that exists. Idempotent: an unchanged title is
/// not written, a missing menu (before Rune promotes its app menu, or on a
/// window without one) is skipped, and the labels follow the current language.
pub(super) fn localize(lang: Lang) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(root) = app.mainMenu() else {
        return;
    };
    let services = app.servicesMenu();

    for index in 0..root.numberOfItems() {
        let Some(item) = root.itemAtIndex(index) else {
            continue;
        };
        // The first top-level item is the app menu; its title is the product
        // name rather than a translatable sentence.
        if index == 0 {
            set_if_changed(&item, tr_for(lang, "Prime Photos"));
        }
        let Some(menu) = item.submenu() else {
            continue;
        };
        if has_action(&menu, "undo:") {
            apply_title(&item, lang, "Edit");
        } else if has_action(&menu, "performMiniaturize:") {
            apply_title(&item, lang, "Window");
        }
        for entry in (0..menu.numberOfItems()).filter_map(|i| menu.itemAtIndex(i)) {
            if let Some(action) = entry.action() {
                if let Some((_, english)) = ACTIONS
                    .iter()
                    .find(|(selector, _)| action.name().to_str().ok() == Some(*selector))
                {
                    apply_title(&entry, lang, *english);
                }
            } else if entry
                .submenu()
                .is_some_and(|submenu| is_services_menu(&submenu, services.as_ref()))
            {
                apply_title(&entry, lang, "Services");
            }
        }
    }
}

/// Whether `menu` is the application's Services menu. Its item has no action
/// and its title is what this module changes, so the object identity (set once
/// by Rune through `setServicesMenu`) is the stable anchor.
fn is_services_menu(menu: &Retained<NSMenu>, services: Option<&Retained<NSMenu>>) -> bool {
    services
        .is_some_and(|services| std::ptr::eq(Retained::as_ptr(menu), Retained::as_ptr(services)))
}

/// Whether `menu` holds an item with this action selector.
fn has_action(menu: &NSMenu, selector: &str) -> bool {
    (0..menu.numberOfItems())
        .filter_map(|index| menu.itemAtIndex(index))
        .any(|item| {
            item.action()
                .is_some_and(|action| action.name().to_str().ok() == Some(selector))
        })
}

/// Set one item's title through the pure mapping when the mapping knows it.
fn apply_title(item: &NSMenuItem, lang: Lang, english: &'static str) {
    if let Some(label) = localized(lang, english) {
        set_if_changed(item, label);
    }
}

/// Apply `title` unless the item already carries it.
fn set_if_changed(item: &NSMenuItem, title: &str) {
    if item.title().to_string() != title {
        super::set_title(item, title);
    }
}

#[cfg(test)]
mod tests {
    use super::{TITLES, localized};
    use crate::i18n::Lang;

    #[test]
    fn every_standard_title_maps_to_its_german_label() {
        for (english, german) in [
            ("Edit", "Bearbeiten"),
            ("Undo", "Rückgängig"),
            ("Redo", "Wiederholen"),
            ("Cut", "Ausschneiden"),
            ("Copy", "Kopieren"),
            ("Paste", "Einfügen"),
            ("Select All", "Alles auswählen"),
            ("Window", "Fenster"),
            ("Minimize", "Minimieren"),
            ("Close", "Schließen"),
            ("Prime Photos", "Prime Photos"),
            ("About Prime Photos", "Über Prime Photos"),
            ("Quit Prime Photos", "Prime Photos beenden"),
            ("Hide Prime Photos", "Prime Photos ausblenden"),
            ("Hide Others", "Andere ausblenden"),
            ("Show All", "Alle einblenden"),
            ("Services", "Dienste"),
        ] {
            assert_eq!(localized(Lang::De, english), Some(german), "{english}");
            assert_eq!(localized(Lang::En, english), Some(english), "{english}");
        }
    }

    #[test]
    fn english_round_trip_keeps_rune_titles() {
        // The English table entry is the very title Rune installed, so a
        // language switch back to English restores the original menu.
        for &(english, _) in TITLES {
            assert_eq!(localized(Lang::En, english), Some(english), "{english}");
        }
    }

    #[test]
    fn unknown_titles_are_not_renamed() {
        for title in ["File", "Help", "Open Recent", ""] {
            assert_eq!(localized(Lang::De, title), None, "{title}");
            assert_eq!(localized(Lang::En, title), None, "{title}");
        }
    }

    #[test]
    fn german_labels_differ_from_their_english_source() {
        // Guards against an English string leaking into a German label by
        // accident (the product name is the one deliberate exception).
        for &(english, german) in TITLES {
            if english == "Prime Photos" {
                continue;
            }
            assert_ne!(
                localized(Lang::De, english),
                Some(english),
                "{german} must be German"
            );
        }
    }
}
