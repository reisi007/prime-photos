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
//! instance instead of starting a second one) or switches back to it while it
//! is `Regular` (e.g. a Dock click with an open window). That covers the "come
//! back from close-to-tray" moment, so no delegate is needed for it.
//!
//! Like [`crate::app_menu`], the AppKit callback must never touch the UI
//! directly: it only records the request in a flag and wakes the render loop.
//! `PrimePhotoApp::update` drains the flag via [`take_become_active_request`]
//! and then shows and focuses the window.
//!
//! **The Dock icon follows the window** (user decision 2026-10-18, original
//! behaviour — `SPEC_OS_INTEGRATION.md` §3.4): while a window is open the app is
//! `Regular` (Dock icon + app menu); once the window is hidden (close-to-tray)
//! it becomes `Accessory` (no Dock icon, no app menu), exactly like the
//! original's dynamic `WindowManager` policy. [`step_back`] drops to
//! `Accessory`, [`come_forward`] restores `Regular` **first** (otherwise the
//! following `activate` would not take effect). A relaunch from
//! Finder/Spotlight/`open` activates the running app, which brings the hidden
//! window back and focuses it — that is what this module is for. (While a
//! window is open the app is `Regular`, so a Dock click also activates it; in
//! the hidden `Accessory` state there is no Dock icon, and the return paths are
//! the tray, a relaunch, and a second start.)
//!
//! **Silent login-item start** (user decision 2026-10-18, original behaviour —
//! `SPEC_OS_INTEGRATION.md` §3.5): [`launched_as_log_in_item`] reads the Apple
//! event that is current at launch. An `OpenApplication` (`oapp`) event whose
//! `prdt` property-data parameter carries `lgit` means the app was started as a
//! login item (autostart), so it starts hidden with no Dock icon (tray only).
//! `None` (no current event) falls back to a normal start — never an invisible,
//! unreachable app. Only the event is **read**; no handler is registered (that
//! would replace AppKit's own handler).
//!
//! **Why closing steps the app back (`step_back` / `come_forward`).**
//! Reactivating an app that is *already* the frontmost one activates nothing:
//! AppKit posts no `NSApplicationDidBecomeActiveNotification`, so the observer
//! above would never fire and the way back would look dead until the user
//! focused some other app first. Hiding the window alone leaves the app in
//! exactly that frontmost-but-inactive state. [`step_back`] therefore calls
//! `NSApplication::hide` when the window is closed: AppKit hides the app **and
//! activates the next app**, so our app is no longer frontmost and a later
//! activation (a relaunch from Finder/Spotlight/`open`) fires the notification
//! and brings the window back. It then also drops to `Accessory`, so the Dock
//! icon disappears with the window. [`come_forward`] is the counterpart on the
//! way back: `Regular` first, then unhide (if hidden) and activate.

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
use objc2_foundation::{NSAppleEventManager, NSNotification, NSNotificationCenter};

// Four-char-code aliases matching the argument types of the Apple-event API
// (`AEEventID` / `AEKeyword` / `OSType`). The `objc2-core-services` names are not
// nameable from here (private re-export / `pub(crate)`), so they are spelled out
// as the same `u32` four-char code. The `kAE*` constants below are not in any of
// our crates either, so they are written out as literals with their character
// form.
#[cfg(target_os = "macos")]
type AEEventID = u32;
#[cfg(target_os = "macos")]
type AEKeyword = u32;
#[cfg(target_os = "macos")]
type OSType = u32;

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

/// macOS: whether the app was started **as a login item** (autostart) rather
/// than by the user.
///
/// Reads the Apple event that is current at launch: an `OpenApplication` event
/// (`kAEOpenApplication` = `'oapp'`) whose `prdt` property-data parameter
/// carries `kAELaunchedAsLogInItem` (`'lgit'`). That is exactly how the original
/// classifies `AppStartupFlags.AutoStart` (`SPEC_OS_INTEGRATION.md` §3.5). The
/// Apple-event manager is only **read** — no handler is registered (that would
/// replace AppKit's own). `None` (no current event) ⇒ `false`, i.e. "treat it
/// like a manual start and show the window": never an invisible, unreachable
/// app. Main-thread-bound like the other functions; returns `false` without a
/// [`MainThreadMarker`].
#[cfg(target_os = "macos")]
pub fn launched_as_log_in_item() -> bool {
    // Four-char codes, each with its character form:
    // `kAEOpenApplication` = 'oapp' — the "open application" event Launch
    // Services sends when the app is launched; `keyAEPropData` = 'prdt' — its
    // property-data parameter; `kAELaunchedAsLogInItem` = 'lgit' — the value
    // `prdt` carries when the launch came from a login item (autostart).
    const K_AE_OPEN_APPLICATION: AEEventID = 0x6F61_7070; // 'oapp'
    const KEY_AE_PROP_DATA: AEKeyword = 0x7072_6474; // 'prdt'
    const K_AE_LAUNCHED_AS_LOG_IN_ITEM: OSType = 0x6C67_6974; // 'lgit'

    let Some(_mtm) = MainThreadMarker::new() else {
        return false;
    };
    let manager = NSAppleEventManager::sharedAppleEventManager();
    let event = manager.currentAppleEvent();
    let log_in = event.as_ref().is_some_and(|ev| {
        ev.eventID() == K_AE_OPEN_APPLICATION
            && ev
                .paramDescriptorForKeyword(KEY_AE_PROP_DATA)
                .is_some_and(|prop| prop.enumCodeValue() == K_AE_LAUNCHED_AS_LOG_IN_ITEM)
    });
    // Permanent diagnostic (live test): distinguishes "no Apple event at all"
    // from "an event that is not a login-item launch", so a live run proves the
    // event was actually read (not merely the `None` fallback).
    log::info!(
        "Apple-Event beim Start: vorhanden={}, als Login-Item gestartet={log_in}",
        event.is_some()
    );
    log_in
}

/// macOS: switch to `Accessory` — no Dock icon and no app menu (the window is
/// hidden / not open). Main-thread-bound like the other functions. The `bool`
/// return value of `setActivationPolicy` is **not** an error signal (see
/// [`become_regular`]).
#[cfg(target_os = "macos")]
pub fn become_accessory() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if !NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Accessory)
    {
        log::debug!("setActivationPolicy(Accessory) meldete false");
    }
}

/// macOS: switch back to `Regular` — the Dock icon and the app menu return with
/// the window. Main-thread-bound like the other functions. The `bool` return
/// value of `setActivationPolicy` is **not** an error signal: in the live test
/// of 2026-10-18 it returned `false` although the policy verifiably switched to
/// `Regular` (external check: `activationPolicy` changes, the window comes
/// back). The assignment itself is therefore unconditional and a `false` is
/// only logged, at debug level.
#[cfg(target_os = "macos")]
pub fn become_regular() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if !NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Regular)
    {
        log::debug!("setActivationPolicy(Regular) meldete false");
    }
}

/// macOS: step the app out of the foreground right before the window is hidden.
///
/// `NSApplication::hide` hides the app **and activates the next app**, so our
/// app is no longer frontmost. That restores the precondition for the observer
/// above: an app is only activated when it is not already frontmost (see the
/// module docs), and the activation then fires
/// `NSApplicationDidBecomeActiveNotification` and brings the window back. It
/// then drops to `Accessory`, so the Dock icon (and the app menu) disappear with
/// the window — the original's dynamic policy (`SPEC_OS_INTEGRATION.md` §3.4).
/// Main-thread-bound: returns early without a [`MainThreadMarker`] (AppKit may
/// only be messaged from the main thread).
#[cfg(target_os = "macos")]
pub fn step_back() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.hide(None);
    if !app.setActivationPolicy(NSApplicationActivationPolicy::Accessory) {
        log::debug!("setActivationPolicy(Accessory) meldete false");
    }
}

/// macOS: the counterpart to [`step_back`] — restore `Regular` first, then undo
/// a previous hide (if any) and activate the app, so the restored window also
/// comes to the front. `Regular` **first** is load-bearing: without the Dock
/// icon/policy back, the following `activate` does not take effect. Main-thread-
/// bound like [`step_back`].
#[cfg(target_os = "macos")]
pub fn come_forward() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    if !app.setActivationPolicy(NSApplicationActivationPolicy::Regular) {
        log::debug!("setActivationPolicy(Regular) meldete false");
    }
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

/// Non-macOS: there is no Apple event to detect a login-item start.
#[cfg(not(target_os = "macos"))]
pub fn launched_as_log_in_item() -> bool {
    false
}

/// Non-macOS: there is no AppKit activation policy to switch.
#[cfg(not(target_os = "macos"))]
pub fn become_accessory() {}

/// Non-macOS: there is no AppKit activation policy to switch.
#[cfg(not(target_os = "macos"))]
pub fn become_regular() {}

/// Non-macOS: there is no activation observer to install.
#[cfg(not(target_os = "macos"))]
pub fn install(_ctx: &eframe::egui::Context) {}

/// Non-macOS: the activation path only exists on macOS.
#[cfg(not(target_os = "macos"))]
pub fn take_become_active_request() -> bool {
    false
}
