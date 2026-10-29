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
//! (original parity, `docs/original/SPEC_ABOUT.md` §5.2). AppKit's built-in items need no
//! target because the responder chain provides one; a **custom** selector does
//! not, so that item gets its own target object (see [`AboutTarget`]). The
//! callback must not touch the UI from inside AppKit — it only sets a flag and
//! wakes the render loop, which `PrimePhotoApp::update` drains via
//! [`take_about_request`], exactly like `src/tray.rs` does with its queue.

#[cfg(target_os = "macos")]
use std::cell::RefCell;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Once, OnceLock};

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyObject, NSObject};
#[cfg(target_os = "macos")]
use objc2_app_kit::NSMenuItem;

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

/// Every visible label of the native main menu for one UI language. This is the
/// single source used by both the initial build and [`refresh_labels`]; the
/// tests exercise it directly for German and English.
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
struct MenuLabels {
    about: &'static str,
    quit: &'static str,
    edit: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
}

/// Resolve the native menu labels for `lang`. The "Wiederholen" Edit-menu item
/// uses the contextual lookup: the same German key means "Retry" in the backup
/// list but "Redo" here (`docs/dev/I18N.md`, `docs/features/languages.md`).
#[cfg(target_os = "macos")]
fn menu_labels(lang: crate::i18n::Lang) -> MenuLabels {
    use crate::i18n::{CONTEXT_NATIVE_MENU, tr_ctx_for, tr_for};

    MenuLabels {
        about: tr_for(lang, "Über Prime Photos"),
        quit: tr_for(lang, "Prime Photos beenden"),
        edit: tr_for(lang, "Bearbeiten"),
        undo: tr_for(lang, "Rückgängig"),
        redo: tr_ctx_for(lang, CONTEXT_NATIVE_MENU, "Wiederholen"),
        cut: tr_for(lang, "Ausschneiden"),
        copy: tr_for(lang, "Kopieren"),
        paste: tr_for(lang, "Einfügen"),
        select_all: tr_for(lang, "Alles auswählen"),
    }
}

/// Handles to the items installed into the AppKit main menu. AppKit objects are
/// main-thread-only, so these live in a thread-local instead of a `static`
/// (`Retained<NSMenuItem>` is not `Sync`). The main menu itself keeps all items
/// alive; these handles exist only to re-title them in place.
#[cfg(target_os = "macos")]
struct MenuItems {
    about: Retained<NSMenuItem>,
    quit: Retained<NSMenuItem>,
    edit_menu: Retained<NSMenuItem>,
    edit: [Retained<NSMenuItem>; 6],
}

#[cfg(target_os = "macos")]
impl MenuItems {
    fn apply_labels(&self, labels: &MenuLabels) {
        set_title(&self.about, labels.about);
        set_title(&self.quit, labels.quit);
        set_title(&self.edit_menu, labels.edit);
        let edit_labels = [
            labels.undo,
            labels.redo,
            labels.cut,
            labels.copy,
            labels.paste,
            labels.select_all,
        ];
        for (item, label) in self.edit.iter().zip(edit_labels) {
            set_title(item, label);
        }
    }
}

#[cfg(target_os = "macos")]
thread_local! {
    /// The installed menu's label handles; `None` before [`install`].
    static MENU_ITEMS: RefCell<Option<MenuItems>> = const { RefCell::new(None) };
}

/// Apply `title` to an AppKit menu item.
#[cfg(target_os = "macos")]
fn set_title(item: &NSMenuItem, title: &str) {
    item.setTitle(&objc2_foundation::NSString::from_str(title));
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

/// Re-apply every visible menu label from the current UI language, in place.
///
/// Idempotent and cheap, macOS-only: called immediately after `i18n::set_lang`
/// in the settings path so an already-installed menu does not stay in the old
/// language. Before [`install`] has built the menu — or when called off the
/// main thread — this is a silent no-op, never a panic.
#[cfg(target_os = "macos")]
pub fn refresh_labels() {
    use objc2::MainThreadMarker;

    if MainThreadMarker::new().is_none() {
        return;
    }
    let labels = menu_labels(crate::i18n::lang());
    MENU_ITEMS.with(|slot| {
        if let Some(items) = slot.borrow().as_ref() {
            items.apply_labels(&labels);
        }
    });
}

/// Non-macOS: no native app menu exists, so the refresh is a harmless no-op.
#[cfg(not(target_os = "macos"))]
pub fn refresh_labels() {}

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
    use objc2::sel;
    use objc2_app_kit::NSMenu;
    use objc2_foundation::NSString;

    // Labels are resolved before the menu is shown, from the language that
    // `PrimePhotoApp::new` set right before this call.
    let labels = menu_labels(crate::i18n::lang());
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
    about_item.setTitle(&NSString::from_str(labels.about));
    // Safety: `showAbout:` is the selector implemented by `AboutTarget`.
    unsafe { about_item.setAction(Some(sel!(showAbout:))) };
    // Safety: `target` is a live object of that class; `setTarget` does not
    // retain, which is why the object is moved into the `'static` below.
    unsafe { about_item.setTarget(Some(&target)) };
    app_menu.addItem(&about_item);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    let quit_item = item(mtm, labels.quit, sel!(terminate:), "q");
    app_menu.addItem(&quit_item);

    let _ = ABOUT_TARGET.set(target);

    // ---- Edit menu: makes ⌘X / ⌘C / ⌘V / ⌘A work --------------------------
    let edit_item = NSMenuItem::new(mtm);
    edit_item.setTitle(&NSString::from_str(labels.edit));
    let edit_menu = NSMenu::new(mtm);
    edit_item.setSubmenu(Some(&edit_menu));
    main_menu.addItem(&edit_item);

    // The titles come from `labels`, but the responder-chain selectors and key
    // equivalents are fixed and must not change with the language.
    let edit_items = [
        item(mtm, labels.undo, sel!(undo:), "z"),
        item(mtm, labels.redo, sel!(redo:), "Z"),
        item(mtm, labels.cut, sel!(cut:), "x"),
        item(mtm, labels.copy, sel!(copy:), "c"),
        item(mtm, labels.paste, sel!(paste:), "v"),
        item(mtm, labels.select_all, sel!(selectAll:), "a"),
    ];
    for edit in &edit_items {
        edit_menu.addItem(edit);
    }

    app.setMainMenu(Some(&main_menu));

    // Keep handles for the in-place language refresh. The menu itself retains
    // every item; moving the `Retained`s in here additionally keeps them alive
    // for `refresh_labels` even if AppKit replaces the menu.
    MENU_ITEMS.with(|slot| {
        *slot.borrow_mut() = Some(MenuItems {
            about: about_item,
            quit: quit_item,
            edit_menu: edit_item,
            edit: edit_items,
        });
    });
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{MenuLabels, menu_labels};
    use crate::i18n::{Lang, tr_for};

    /// The production label source for the complete visible native menu, in
    /// both shipped languages. A missing EN table row would leave a German
    /// label and fail here (the `tr` fallback), so this pins every entry
    /// without launching AppKit.
    #[test]
    fn native_menu_labels_are_complete_in_german_and_english() {
        assert_eq!(
            menu_labels(Lang::De),
            MenuLabels {
                about: "Über Prime Photos",
                quit: "Prime Photos beenden",
                edit: "Bearbeiten",
                undo: "Rückgängig",
                redo: "Wiederholen",
                cut: "Ausschneiden",
                copy: "Kopieren",
                paste: "Einfügen",
                select_all: "Alles auswählen",
            }
        );
        assert_eq!(
            menu_labels(Lang::En),
            MenuLabels {
                about: "About Prime Photos",
                quit: "Quit Prime Photos",
                edit: "Edit",
                undo: "Undo",
                redo: "Redo",
                cut: "Cut",
                copy: "Copy",
                paste: "Paste",
                select_all: "Select All",
            }
        );

        // No label may silently fall back to German in English — including the
        // brand-carrying About/Quit entries, whose German form differs.
        let de = menu_labels(Lang::De);
        let en = menu_labels(Lang::En);
        for (de_label, en_label) in [
            (de.about, en.about),
            (de.quit, en.quit),
            (de.edit, en.edit),
            (de.undo, en.undo),
            (de.redo, en.redo),
            (de.cut, en.cut),
            (de.copy, en.copy),
            (de.paste, en.paste),
            (de.select_all, en.select_all),
        ] {
            assert_ne!(de_label, en_label, "menu label fell back to German");
        }
    }

    /// "Wiederholen" stays the German Edit-menu wording, but its English menu
    /// label is "Redo" — not the backup list's "Retry" from the plain table.
    #[test]
    fn edit_redo_is_not_backup_retry() {
        assert_eq!(menu_labels(Lang::De).redo, "Wiederholen");
        assert_eq!(menu_labels(Lang::En).redo, "Redo");
        assert_eq!(tr_for(Lang::En, "Wiederholen"), "Retry");
        assert_ne!(menu_labels(Lang::En).redo, tr_for(Lang::En, "Wiederholen"));
    }
}
