//! In-place adaptation of Rune's existing standard About item.

use std::cell::RefCell;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::sel;
use objc2_app_kit::{NSApplication, NSMenuItem};

thread_local! {
    /// Rune owns its main menu; keep only a handle to its adapted standard About item.
    static ITEM: RefCell<Option<Retained<NSMenuItem>>> = const { RefCell::new(None) };
}

/// Adapt Rune's existing standard About item, once it exists. Rune creates the
/// generic item when an accessory app is promoted, which can be after `setup`;
/// the production Rune poll retries this cheap lookup until it succeeds.
pub(super) fn install_about_item() {
    if super::UI_WAKEUP.get().is_none() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    if ITEM.with(|slot| slot.borrow().is_some()) {
        return;
    }

    let about_label = super::about_label(crate::i18n::lang());
    let app = NSApplication::sharedApplication(mtm);
    // Rune's standard About item is in the first (app) submenu and keeps
    // AppKit's nil-target responder behavior until this exact item is adapted.
    let Some(root) = app.mainMenu() else {
        return;
    };
    let Some(app_menu) = root.itemAtIndex(0).and_then(|item| item.submenu()) else {
        return;
    };
    let about_item = (0..app_menu.numberOfItems())
        .filter_map(|index| app_menu.itemAtIndex(index))
        .find(|item| item.action() == Some(sel!(orderFrontStandardAboutPanel:)));
    let Some(about_item) = about_item else {
        return;
    };

    let target = super::ABOUT_TARGET.get_or_init(super::AboutTarget::new);
    super::set_title(&about_item, about_label);
    // Keep the standard item in place (and therefore preserve Rune's Edit and
    // Window menus); only About is routed to Prime Photos' app-level callback.
    // SAFETY: `showAbout:` is implemented by this live target, which `ABOUT_TARGET`
    // retains for the process lifetime. AppKit invokes it on the main thread.
    unsafe {
        about_item.setAction(Some(sel!(showAbout:)));
        about_item.setTarget(Some(target));
    }
    ITEM.with(|slot| *slot.borrow_mut() = Some(about_item));
}

/// Refresh the Rune About label from the current language, including the first
/// call that might discover Rune's item after its accessory menu is promoted.
pub(super) fn refresh_label(label: &str) {
    install_about_item();
    ITEM.with(|slot| {
        if let Some(item) = slot.borrow().as_ref() {
            super::set_title(item, label);
        }
    });
}
