//! macOS app activation: bring the (possibly hidden) window back and keep the
//! Dock presence in sync.
//!
//! **Why there is no `NSApplicationDelegate` here.** winit 0.30 registers an
//! `NSApplicationDelegate` of its own (`WinitApplicationDelegate`, see
//! `winit/src/platform_impl/macos/event_loop.rs`) and `ApplicationDelegate::get`
//! panics hard as soon as `NSApp.delegate` is not that object — "tried to get a
//! delegate that was not the one Winit has registered". Replacing it, which is
//! the escape hatch winit's documentation still advertises ("Winit guarantees
//! that it will not register an application delegate"), is therefore a pure
//! **runtime** failure that `cargo build` cannot see: the winit docs are out of
//! date for 0.30. This module never calls `NSApplication::setDelegate`.
//!
//! Instead it observes `NSApplicationDidBecomeActiveNotification` through the
//! **target-action** form of `addObserver:selector:name:object:` (no blocks:
//! the observer is an object defined with `objc2::define_class!`, the same
//! pattern as [`crate::app_menu`]'s `AboutTarget`). That notification fires
//! whenever the app becomes the active one: the user relaunches it from
//! Finder/Spotlight/`open` (Launch Services activates the already running
//! instance instead of starting a second one), clicks its Dock icon, or
//! switches back to it. That is exactly the "come back from close-to-tray"
//! moment, so no delegate is needed for it.
//!
//! Like [`crate::app_menu`], the AppKit callback must never touch the UI
//! directly: it only records the request in a flag and wakes the render loop.
//! `PrimePhotoApp::update` drains the flag via [`take_become_active_request`]
//! and then shows and focuses the window.
//!
//! [`set_dock_icon`] controls the Dock presence: the original is a menu-bar app
//! that only "borrows" a Dock icon while a window is open (original parity,
//! `docs/original/SPEC_OS_INTEGRATION.md` §3.4), so a hidden window means
//! `Accessory` and a shown one `Regular`.

#[cfg(target_os = "macos")]
use std::cell::OnceCell;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Once, OnceLock};

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::NSObject;
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDidBecomeActiveNotification,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSNotification, NSNotificationCenter};

/// Set by the activation observer; consumed once per frame by
/// `PrimePhotoApp::update` through [`take_become_active_request`].
#[cfg(target_os = "macos")]
static BECOME_ACTIVE_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The egui context to wake when the app is activated. Stored at [`install`]
/// time; the AppKit callback only flags + repaints, never touches the UI
/// directly.
#[cfg(target_os = "macos")]
static UI_CTX: OnceLock<eframe::egui::Context> = OnceLock::new();

#[cfg(target_os = "macos")]
objc2::define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, we add no instance
    // variables and implement no `Drop`. The class is `MainThreadOnly` because
    // AppKit only ever posts this notification — and therefore only ever
    // messages this observer — on the main thread, which is also where
    // [`install`] runs.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct ActivationObserver;

    impl ActivationObserver {
        /// AppKit action behind `NSApplicationDidBecomeActiveNotification`.
        /// Runs on the main thread.
        ///
        /// It must not change the UI from here: it only records the request and
        /// wakes the render loop (mirrors `src/tray.rs`'s event queue).
        #[unsafe(method(didBecomeActive:))]
        fn did_become_active(&self, _notification: &NSNotification) {
            BECOME_ACTIVE_REQUESTED.store(true, Ordering::Relaxed);
            if let Some(ctx) = UI_CTX.get() {
                ctx.request_repaint();
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl ActivationObserver {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: `NSObject`'s `init` has the signature our subclass needs; the
        // class has no instance variables, so the unit ivars are complete.
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { objc2::msg_send![super(this), init] }
    }
}

// Keeps the observer alive: `NSNotificationCenter` does **not** retain its
// observers, so without an owner the object would be deallocated and an
// activation would reach nobody (the same reason `app_menu`'s `ABOUT_TARGET`
// exists).
//
// `ActivationObserver` is `MainThreadOnly`, i.e. `!Send + !Sync`, so a
// `Retained<_>` of it cannot live in a global `OnceLock` — it is pinned to the
// main thread, where [`install`] runs and where AppKit messages it. The
// thread-local therefore lives exactly as long as the app.
#[cfg(target_os = "macos")]
thread_local! {
    static OBSERVER: OnceCell<Retained<ActivationObserver>> = const { OnceCell::new() };
}

/// Install the activation observer once. `ctx` is remembered so the observer
/// can wake the UI; passing it here (rather than a second setter) keeps the
/// wiring in one place.
#[cfg(target_os = "macos")]
pub fn install(ctx: &eframe::egui::Context) {
    // Remember the context even before the (once-only) observer setup, so the
    // callback can always wake the UI.
    let _ = UI_CTX.set(ctx.clone());

    // Defensive re-entrancy guard: a second observer would be registered twice
    // and a single activation would flag the request twice. `install()` is
    // idempotent either way; the `Once` makes that explicit.
    static INSTALL_ONCE: Once = Once::new();

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    INSTALL_ONCE.call_once(|| {
        let observer = ActivationObserver::new(mtm);
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: `didBecomeActive:` is the selector `ActivationObserver`
        // implements and `NSApplicationDidBecomeActiveNotification` is the
        // AppKit name constant. `object: nil` observes every sender (only the
        // application posts this notification).
        unsafe {
            center.addObserver_selector_name_object(
                &observer,
                objc2::sel!(didBecomeActive:),
                Some(NSApplicationDidBecomeActiveNotification),
                None,
            );
        }
        // The center holds the observer unretained; `OBSERVER` is what keeps it
        // alive (see above).
        let _ = OBSERVER.with(|slot| slot.set(observer));
    });
}

/// Whether the app became active since the last call. Drains the flag, so the
/// caller shows the window exactly once per activation.
#[cfg(target_os = "macos")]
pub fn take_become_active_request() -> bool {
    BECOME_ACTIVE_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Show or hide the Dock icon (and the app menu bar with it).
///
/// `false` switches the activation policy to `Accessory` and deactivates the
/// app: a window hidden in the tray leaves no Dock icon behind, exactly like
/// the original menu-bar app (`docs/original/SPEC_OS_INTEGRATION.md` §3.4).
/// `true` goes back to `Regular` and activates the app.
///
/// Must run on the main thread (AppKit); a call from anywhere else is a no-op.
#[cfg(target_os = "macos")]
pub fn set_dock_icon(visible: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    if visible {
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        app.activate();
    } else {
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        app.deactivate();
    }
}

/// Non-macOS: there is no activation observer to install.
#[cfg(not(target_os = "macos"))]
pub fn install(_ctx: &eframe::egui::Context) {}

/// Non-macOS: the activation path only exists on macOS.
#[cfg(not(target_os = "macos"))]
pub fn take_become_active_request() -> bool {
    false
}

/// Non-macOS: the Dock presence only exists on macOS.
#[cfg(not(target_os = "macos"))]
pub fn set_dock_icon(_visible: bool) {}
