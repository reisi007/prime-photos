//! A minimal native macOS main menu.
//!
//! A bare executable has no main menu, so AppKit never installs the standard
//! "Edit" menu. Without it the ⌘C / ⌘V / ⌘X key equivalents are never routed
//! through the responder chain — pasting into the embedded login webview then
//! only works via the right-click context menu. Installing this menu fixes ⌘V.
//!
//! The menu items use `nil` targets, so `paste:` / `copy:` / … are handled by
//! whatever is focused (the WKWebView), exactly like in a native app.

#[cfg(target_os = "macos")]
use std::sync::Once;

#[cfg(target_os = "macos")]
pub fn install() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    // Defensive re-entrancy guard: installing the menu twice would leave AppKit
    // with two main menus. `install()` is idempotent either way; the `Once`
    // makes that explicit and keeps it a no-op even if a future call path
    // forgets that.
    static INSTALL_ONCE: Once = Once::new();

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);

    INSTALL_ONCE.call_once(|| {
        build_main_menu(&app, mtm);
    });
}

#[cfg(target_os = "macos")]
fn build_main_menu(app: &objc2_app_kit::NSApplication, mtm: objc2::MainThreadMarker) {
    use objc2::sel;
    use objc2_app_kit::{NSMenu, NSMenuItem};
    use objc2_foundation::NSString;

    let main_menu = NSMenu::new(mtm);

    // ---- Application menu (the first item is the app menu) ----------------
    let app_item = NSMenuItem::new(mtm);
    let app_menu = NSMenu::new(mtm);
    app_item.setSubmenu(Some(&app_menu));
    main_menu.addItem(&app_item);
    app_menu.addItem(&item(
        mtm,
        "Prime Photos beenden",
        sel!(terminate:),
        "q",
    ));

    // ---- Edit menu: makes ⌘X / ⌘C / ⌘V / ⌘A work --------------------------
    let edit_item = NSMenuItem::new(mtm);
    edit_item.setTitle(&NSString::from_str("Bearbeiten"));
    let edit_menu = NSMenu::new(mtm);
    edit_item.setSubmenu(Some(&edit_menu));
    main_menu.addItem(&edit_item);

    for (title, action, key) in [
        ("Rückgängig", sel!(undo:), "z"),
        ("Wiederholen", sel!(redo:), "Z"),
        ("Ausschneiden", sel!(cut:), "x"),
        ("Kopieren", sel!(copy:), "c"),
        ("Einfügen", sel!(paste:), "v"),
        ("Alles auswählen", sel!(selectAll:), "a"),
    ] {
        edit_menu.addItem(&item(mtm, title, action, key));
    }

    app.setMainMenu(Some(&main_menu));
}

/// One menu item with a `nil` target (handled via the responder chain) and the
/// default ⌘ modifier.
#[cfg(target_os = "macos")]
fn item(
    mtm: objc2::MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
    key: &str,
) -> objc2::rc::Retained<objc2_app_kit::NSMenuItem> {
    use objc2_app_kit::NSMenuItem;
    use objc2_foundation::NSString;

    let item = NSMenuItem::new(mtm);
    item.setTitle(&NSString::from_str(title));
    // Safety: `action` is one of the standard responder-chain selectors.
    unsafe {
        item.setAction(Some(action));
    }
    item.setKeyEquivalent(&NSString::from_str(key));
    item
}

#[cfg(not(target_os = "macos"))]
pub fn install() {}
