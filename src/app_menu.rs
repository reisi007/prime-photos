//! A minimal native macOS main menu.
//!
//! A bare executable has no main menu, so AppKit never installs the standard
//! "Edit" menu. Without it the ⌘C / ⌘V / ⌘X key equivalents are never routed
//! through the responder chain — pasting into the embedded login webview then
//! only works via the right-click context menu. Installing this menu fixes ⌘V.
//!
//! The standard menu items use `nil` targets, so `paste:` / `copy:` / … are
//! handled by whatever is focused (the WKWebView), exactly like in a native app.
//!
//! The app menu additionally carries "Über Prime Photos" as its **first** entry
//! (original parity, `docs/SPEC_ABOUT.md` §5.2). AppKit's built-in items need no
//! target because the responder chain provides one; a **custom** selector does
//! not, so that item gets its own target object (see [`AboutTarget`]). The
//! callback must not touch the UI from inside AppKit — it only sets a flag and
//! wakes the render loop, which `PrimePhotoApp::update` drains via
//! [`take_about_request`], exactly like `src/tray.rs` does with its queue.

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Once, OnceLock};

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyObject, NSObject};

/// Set by the About menu item's target object; consumed once per frame by
/// `PrimePhotoApp::update` through [`take_about_request`].
#[cfg(target_os = "macos")]
static ABOUT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The egui context to wake when the About item is chosen. Stored at
/// [`install`] time; the AppKit callback only flags + repaints, never touches
/// the UI directly.
#[cfg(target_os = "macos")]
static UI_CTX: OnceLock<eframe::egui::Context> = OnceLock::new();

/// The AppKit target object of the About item. AppKit holds a menu item's
/// target **weakly**, so this `'static` owner is what keeps it alive — without
/// it the menu entry would be dead.
#[cfg(target_os = "macos")]
static ABOUT_TARGET: OnceLock<Retained<AboutTarget>> = OnceLock::new();

#[cfg(target_os = "macos")]
objc2::define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, we add no instance
    // variables, and the class implements no `Drop` — so it is trivially
    // `Send + Sync` (auto traits) and safe to keep in a `'static`.
    #[unsafe(super(NSObject))]
    struct AboutTarget;

    impl AboutTarget {
        /// AppKit action behind "Über Prime Photos". Runs on the main thread.
        ///
        /// It must not change the UI from here: it only records the request and
        /// wakes the render loop (mirrors `src/tray.rs`'s event queue).
        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: &AnyObject) {
            ABOUT_REQUESTED.store(true, Ordering::Relaxed);
            if let Some(ctx) = UI_CTX.get() {
                ctx.request_repaint();
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl AboutTarget {
    objc2::extern_methods!(
        /// `NSObject`'s inherited `+new`.
        #[unsafe(method(new))]
        fn new() -> Retained<Self>;
    );
}

/// Install the main menu once. `ctx` is remembered so the About item's target
/// object can wake the UI; passing it here (rather than a second setter) keeps
/// the wiring in one place.
#[cfg(target_os = "macos")]
pub fn install(ctx: &eframe::egui::Context) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    // Remember the context even before the (once-only) menu build, so the About
    // callback can always wake the UI.
    let _ = UI_CTX.set(ctx.clone());

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

/// Whether the About item was chosen since the last call. Drains the flag, so
/// the caller opens the window exactly once per activation.
#[cfg(target_os = "macos")]
pub fn take_about_request() -> bool {
    ABOUT_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Non-macOS: there is no app menu to install and no About request to drain.
#[cfg(not(target_os = "macos"))]
pub fn install(_ctx: &eframe::egui::Context) {}

/// Non-macOS: the About item only exists in the tray there.
#[cfg(not(target_os = "macos"))]
pub fn take_about_request() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn build_main_menu(app: &objc2_app_kit::NSApplication, mtm: objc2::MainThreadMarker) {
    use crate::i18n::tr;
    use objc2::sel;
    use objc2_app_kit::{NSMenu, NSMenuItem};
    use objc2_foundation::NSString;

    let main_menu = NSMenu::new(mtm);

    // ---- Application menu (the first item is the app menu) ----------------
    let app_item = NSMenuItem::new(mtm);
    let app_menu = NSMenu::new(mtm);
    app_item.setSubmenu(Some(&app_menu));
    main_menu.addItem(&app_item);

    // "Über Prime Photos" is the **first** entry (original parity) and carries
    // **no** key equivalent; a separator follows, then Quit (⌘Q). Unlike the
    // built-in items this one needs an explicit target object, kept alive in
    // `ABOUT_TARGET` (AppKit stores the target weakly).
    let target = AboutTarget::new();
    let about_item = NSMenuItem::new(mtm);
    about_item.setTitle(&NSString::from_str(tr("Über Prime Photos")));
    // Safety: `showAbout:` is the selector implemented by `AboutTarget`.
    unsafe { about_item.setAction(Some(sel!(showAbout:))) };
    // Safety: `target` is a live object of that class; `setTarget` does not
    // retain, which is why the object is moved into the `'static` below.
    unsafe { about_item.setTarget(Some(&target)) };
    app_menu.addItem(&about_item);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&item(mtm, "Prime Photos beenden", sel!(terminate:), "q"));

    let _ = ABOUT_TARGET.set(target);

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
