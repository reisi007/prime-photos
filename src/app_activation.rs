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
//! **The Dock icon follows the window** (original parity, user decision
//! 2026-10-25 — `SPEC_OS_INTEGRATION.md` §3.4): the bundle sets `LSUIElement`
//! and the runtime policy changes between `Regular` (open window, Dock icon +
//! app menu) and `Accessory` (closed-to-tray, no Dock icon or app menu). The
//! previous `NSApplication::hide` call is removed: a live test on 2026-10-25
//! showed that the hidden state kept the Dock tile around even after the policy
//! had correctly changed to `Accessory`. [`step_back`] now changes the policy
//! and calls `deactivate()` to give up activation, so a later activation via
//! `open -a` or Finder can post `NSApplicationDidBecomeActiveNotification` and
//! bring the window back; without deactivation that observer-based return path
//! would be dead. The tray's "Fenster anzeigen" entry calls `show_main_window`
//! directly instead. [`come_forward`]
//! restores `Regular` **first** and still unhides a hidden app before activating
//! it (the unhide also protects the ⌘H case). The observer mechanism itself is
//! unchanged. While the window is open the app is `Regular`, so a Dock click
//! activates it; while closed-to-tray there is no Dock icon, and the return
//! paths are the tray (a direct `show_main_window` call), a relaunch (through
//! the activation observer), and a second start.
//!
//! **Silent login-item start** (user decision 2026-10-18, original behaviour —
//! `SPEC_OS_INTEGRATION.md` §3.5): the retained launch-notification observer
//! reads `currentAppleEvent` while `NSApplicationDidFinishLaunchingNotification`
//! is delivered and caches whether `oapp` carries `prdt=lgit`. The app creator
//! consumes only that cache; it does not re-query the transient current event.
//! No Apple-event handler or application delegate is registered.
//!
//! **Why closing steps the app back (`step_back` / `come_forward`).**
//! Reactivating an app that is *already* the frontmost one activates nothing:
//! AppKit posts no `NSApplicationDidBecomeActiveNotification`, so the observer
//! above would never fire and the way back would look dead until the user
//! focused some other app first. [`step_back`] therefore switches to `Accessory`
//! and calls `deactivate()` to give up activation, without setting the hidden
//! state: a live finding on 2026-10-25 showed that `hide` kept the Dock tile
//! visible despite the `Accessory` policy. A later relaunch from Finder,
//! Spotlight, or `open -a` can then post the notification and bring the window
//! back; the tray entry calls `show_main_window` directly. [`come_forward`] is
//! the counterpart: restore `Regular` first, then unhide (if hidden, e.g. by
//! ⌘H) and activate.

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
    NSApplicationDidFinishLaunchingNotification,
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

/// Written by the main-thread launch observer before eframe's initial dispatch.
#[cfg(target_os = "macos")]
static LAUNCH_NOTIFICATION_SEEN: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static LAUNCHED_AS_LOGIN_ITEM: AtomicBool = AtomicBool::new(false);

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
            log::debug!("App-Aktivierungsdiagnose: NSApplicationDidBecomeActiveNotification empfangen");
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
    static LAUNCH_DIAGNOSTIC_OBSERVER: OnceCell<Retained<LaunchDiagnosticObserver>> = const { OnceCell::new() };
}

#[cfg(target_os = "macos")]
objc2::define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct LaunchDiagnosticObserver;

    impl LaunchDiagnosticObserver {
        #[unsafe(method(didFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            const OAPP: AEEventID = 0x6F61_7070;
            const PRDT: AEKeyword = 0x7072_6474;
            const LGIT: OSType = 0x6C67_6974;

            let manager = NSAppleEventManager::sharedAppleEventManager();
            let event = manager.currentAppleEvent();
            let event_id = event.as_ref().map(|event| event.eventID());
            let property = event
                .as_ref()
                .and_then(|event| event.paramDescriptorForKeyword(PRDT));
            let enum_code = property.as_ref().map(|descriptor| descriptor.enumCodeValue());
            let login_item = event_id == Some(OAPP) && enum_code == Some(LGIT);
            LAUNCHED_AS_LOGIN_ITEM.store(login_item, Ordering::Relaxed);
            LAUNCH_NOTIFICATION_SEEN.store(true, Ordering::Release);
            log::debug!(
                "Login-Item-Startdiagnose: launch_notification_seen=true, cached_login_item={}, currentAppleEvent={}, eventID={}, prdt={}, prdt.enumCode={}, oapp={}, lgit={}",
                login_item,
                event.is_some(),
                event_id.map_or_else(|| "none".to_owned(), |value| format!("0x{value:08x}")),
                property.is_some(),
                enum_code.map_or_else(|| "none".to_owned(), |value| format!("0x{value:08x}")),
                event_id == Some(OAPP),
                enum_code == Some(LGIT),
            );
        }
    }
);

#[cfg(target_os = "macos")]
impl LaunchDiagnosticObserver {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { objc2::msg_send![super(this), init] }
    }
}

/// Register the retained launch-event observer before eframe starts AppKit.
#[cfg(target_os = "macos")]
pub fn install_launch_diagnostic() {
    static INSTALL_ONCE: Once = Once::new();
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    INSTALL_ONCE.call_once(|| {
        let observer = LaunchDiagnosticObserver::new(mtm);
        let center = NSNotificationCenter::defaultCenter();
        unsafe {
            center.addObserver_selector_name_object(
                &observer,
                objc2::sel!(didFinishLaunching:),
                Some(NSApplicationDidFinishLaunchingNotification),
                None,
            );
        }
        let _ = LAUNCH_DIAGNOSTIC_OBSERVER.with(|slot| slot.set(observer));
    });
}

#[cfg(not(target_os = "macos"))]
pub fn install_launch_diagnostic() {}

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
/// Returns the launch classification captured in the early AppKit notification.
/// If that notification was not observed, the initialized-false cache safely
/// falls back to a visible/manual start.
#[cfg(target_os = "macos")]
pub fn launched_as_log_in_item() -> bool {
    let seen = LAUNCH_NOTIFICATION_SEEN.load(Ordering::Acquire);
    let login_item = LAUNCHED_AS_LOGIN_ITEM.load(Ordering::Relaxed);
    log::debug!("Login-Item-Cache: launch_notification_seen={seen}, cached_login_item={login_item}");
    login_item
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
/// The original uses the dynamic activation policy (`SPEC_OS_INTEGRATION.md`
/// §3.4): switch to `Accessory` without hiding the app, then deactivate it so a
/// later activation can notify the observer and bring the window back. Hiding
/// first kept the Dock tile alive after the policy change (live finding,
/// 2026-10-25). A `false` return from `setActivationPolicy` is only logged, not
/// treated as an error. Main-thread-bound: returns early without a
/// [`MainThreadMarker`] (AppKit may only be messaged from the main thread).
#[cfg(target_os = "macos")]
pub fn step_back() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    if !app.setActivationPolicy(NSApplicationActivationPolicy::Accessory) {
        log::debug!("setActivationPolicy(Accessory) meldete false");
    }
    app.deactivate();
}

/// macOS: the counterpart to [`step_back`] — restore `Regular` first, then undo
/// a hide (if any, e.g. from ⌘H) and activate the app, so the restored window
/// also comes to the front. `Regular` **first** is load-bearing: without the
/// Dock icon/policy back, the following `activate` does not take effect.
/// Main-thread-bound like [`step_back`].
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
