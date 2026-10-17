//! macOS app activation: bring the (possibly hidden) window back.
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
//! **The Dock icon stays** (user decision 2026-10-17): the app is a normal
//! windowed app whose Dock icon — and app menu bar — remains for as long as the
//! app runs. Closing the window only hides it (the app keeps running in the
//! background), and the icon stays; it disappears only when the app is really
//! quit (normal macOS behaviour). An app activation — a Dock click, a relaunch
//! from Finder/Spotlight/`open`, or switching back to it — brings the hidden
//! window back and focuses it, which is what this module is for.
//!
//! **Why closing steps the app back (`step_back` / `come_forward`).** A Dock
//! click on the app that is *already* the frontmost one activates nothing:
//! AppKit posts no `NSApplicationDidBecomeActiveNotification`, so the observer
//! above would never fire and the way back would look dead until the user
//! focused some other app first. Hiding the window alone leaves the app in
//! exactly that frontmost-but-inactive state. [`step_back`] therefore calls
//! `NSApplication::hide` when the window is closed: AppKit hides the app **and
//! activates the next app**, so our app is no longer frontmost and the next
//! Dock click activates it again — the notification fires and the window comes
//! back. The activation policy is deliberately **not** touched, so the Dock icon
//! stays (see above). [`come_forward`] is the counterpart on the way back:
//! unhide (if hidden) and activate.

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
use objc2_app_kit::{NSApplication, NSApplicationDidBecomeActiveNotification};
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

/// macOS: step the app out of the foreground right before the window is hidden.
///
/// `NSApplication::hide` hides the app **and activates the next app**, so our
/// app is no longer frontmost. That restores the precondition for the observer
/// above: a Dock click only activates a non-frontmost app (see the module docs),
/// and the activation then fires `NSApplicationDidBecomeActiveNotification` and
/// brings the window back. The activation policy is deliberately left alone —
/// the Dock icon stays. Main-thread-bound: returns early without a
/// [`MainThreadMarker`] (AppKit may only be messaged from the main thread).
#[cfg(target_os = "macos")]
pub fn step_back() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    NSApplication::sharedApplication(mtm).hide(None);
}

/// macOS: the counterpart to [`step_back`] — undo a previous hide (if any) and
/// then activate the app, so the restored window also comes to the front. Main-
/// thread-bound like [`step_back`].
#[cfg(target_os = "macos")]
pub fn come_forward() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    if app.isHidden() {
        app.unhide(None);
    }
    app.activate();
}

/// Non-macOS: there is no AppKit app to step back.
#[cfg(not(target_os = "macos"))]
pub fn step_back() {}

/// Non-macOS: there is no AppKit app to bring forward.
#[cfg(not(target_os = "macos"))]
pub fn come_forward() {}

/// Non-macOS: there is no activation observer to install.
#[cfg(not(target_os = "macos"))]
pub fn install(_ctx: &eframe::egui::Context) {}

/// Non-macOS: the activation path only exists on macOS.
#[cfg(not(target_os = "macos"))]
pub fn take_become_active_request() -> bool {
    false
}
