//! The native macOS standard menu labels and About item.
//!
//! A bare executable has no main menu, so AppKit never installs the standard
//! "Edit" menu, and ⌘V into the embedded login webview would only work via the
//! right-click context menu. Rune installs its own standard menus; this module
//! adapts them in place: [`rune_standard`] re-titles the standard English menu
//! and item labels, and [`rune_about`] routes Rune's About item to Prime
//! Photos. Neither builds, adds or removes a menu item.
//!
//! The app menu carries "Über Prime Photos" (original parity,
//! `docs/original/SPEC_ABOUT.md` §5.2). AppKit's built-in items need no target
//! because the responder chain provides one; a **custom** selector does not, so
//! that item gets its own target object (see [`AboutTarget`]). The callback must
//! not touch the UI from inside AppKit — it only sets a flag and wakes the
//! render loop, which Rune's `poll` drains via [`take_about_request`], exactly
//! like `src/tray.rs` does with its queue.

use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
#[cfg(target_os = "macos")]
mod rune_about;
#[cfg(target_os = "macos")]
mod rune_standard;

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyObject, NSObject};
#[cfg(target_os = "macos")]
use objc2_app_kit::NSMenuItem;

/// Thread-safe callback used to wake the UI without coupling the menu to a UI
/// framework.
pub(crate) type WakeupCallback = Arc<dyn Fn() + Send + Sync>;

/// Set by the About menu item's target object; consumed once per frame by Rune's
/// `poll` through [`take_about_request`].
#[cfg(target_os = "macos")]
static ABOUT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The UI wakeup to invoke when the About item is chosen. Stored at
/// menu-install time; the AppKit callback only flags + invokes it, never
/// touches the UI directly.
#[cfg(target_os = "macos")]
static UI_WAKEUP: OnceLock<WakeupCallback> = OnceLock::new();

#[cfg(target_os = "macos")]
fn register_wakeup_callback(slot: &OnceLock<WakeupCallback>, wakeup: WakeupCallback) {
    let _ = slot.set(wakeup);
}

#[cfg(target_os = "macos")]
fn request_about(requested: &AtomicBool, wakeup: Option<&WakeupCallback>) {
    requested.store(true, Ordering::Relaxed);
    if let Some(wakeup) = wakeup {
        wakeup();
    }
}

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
            request_about(&ABOUT_REQUESTED, UI_WAKEUP.get());
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

/// The localized About label.
#[cfg(target_os = "macos")]
fn about_label(lang: crate::i18n::Lang) -> &'static str {
    crate::i18n::tr_for(lang, "Über Prime Photos")
}

/// Apply `title` to an AppKit menu item.
#[cfg(target_os = "macos")]
fn set_title(item: &NSMenuItem, title: &str) {
    item.setTitle(&objc2_foundation::NSString::from_str(title));
}

/// Register the Rune About callback and adapt Rune's existing standard About
/// item in place when it is available. This never replaces or adds to Rune's
/// main menu; [`ensure_rune_about_installed`] retries after Rune promotes its
/// accessory app menu.
#[cfg(target_os = "macos")]
pub fn install_rune_about(wakeup: WakeupCallback) {
    register_wakeup_callback(&UI_WAKEUP, wakeup);
    rune_about::install_about_item();
    rune_standard::localize(crate::i18n::lang());
}

/// Retry the in-place Rune About adaptation after Rune has had a chance to
/// create its standard item, and re-apply the standard menu labels once Rune's
/// Edit/Window/app menus exist. Safe to call once per UI poll; already-correct
/// titles are not written again.
#[cfg(target_os = "macos")]
pub(crate) fn ensure_rune_about_installed() {
    rune_about::install_about_item();
    rune_standard::localize(crate::i18n::lang());
}

/// Re-apply every visible menu label from the current UI language, in place.
///
/// Idempotent and cheap, macOS-only: called immediately after `i18n::set_lang`
/// in the settings path so an already-installed menu does not stay in the old
/// language. It re-titles Rune's standard Edit/Window/app menus and the adapted
/// About item. Before the menu is available — or when called off the main
/// thread — this is a silent no-op.
#[cfg(target_os = "macos")]
pub fn refresh_labels() {
    use objc2::MainThreadMarker;

    if MainThreadMarker::new().is_none() {
        return;
    }
    let lang = crate::i18n::lang();
    rune_about::refresh_label(about_label(lang));
    rune_standard::localize(lang);
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

/// Rune has no native macOS About item on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn install_rune_about(_wakeup: WakeupCallback) {}

/// Non-macOS: there is no Rune AppKit menu to adapt.
#[cfg(not(target_os = "macos"))]
pub(crate) fn ensure_rune_about_installed() {}

/// Non-macOS: the About item only exists in the tray there.
#[cfg(not(target_os = "macos"))]
pub fn take_about_request() -> bool {
    false
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{WakeupCallback, register_wakeup_callback, request_about};
    use std::sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn about_request_invokes_the_registered_wakeup_callback() {
        let called = Arc::new(AtomicBool::new(false));
        let called_by_callback = called.clone();
        let callback: WakeupCallback = Arc::new(move || {
            called_by_callback.store(true, Ordering::Relaxed);
        });
        let slot = OnceLock::new();
        register_wakeup_callback(&slot, callback);
        let requested = AtomicBool::new(false);

        request_about(&requested, slot.get());

        assert!(requested.load(Ordering::Relaxed));
        assert!(called.load(Ordering::Relaxed));
    }

    #[test]
    fn about_request_without_a_wakeup_still_records_the_request() {
        let requested = AtomicBool::new(false);

        request_about(&requested, None);

        assert!(requested.load(Ordering::Relaxed));
    }
}
