//! macOS app activation: bring the (possibly hidden) window back.
//!
//! **Why there is no `NSApplicationDelegate` here.** Rune's native host owns the
//! application delegate; replacing it would break the host's own launch and
//! event handling. This module therefore never calls
//! `NSApplication::setDelegate`.
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
//! directly: it only records the request in a flag and wakes the active host's
//! render loop. The host drains the flag via [`take_become_active_request`] and
//! then shows and focuses the window.
//!
//! **Login-item launch diagnostic** (`SPEC_OS_INTEGRATION.md` §3.5): the
//! retained launch-notification observer reads `currentAppleEvent` while
//! `NSApplicationDidFinishLaunchingNotification` is delivered and caches
//! whether `oapp` carries `prdt=lgit`. The Rune production host uses that cache
//! for its silent-start policy. No Apple-event handler or application delegate
//! is registered.

#[cfg(target_os = "macos")]
use std::cell::OnceCell;
#[cfg(any(target_os = "macos", test))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Once, OnceLock};
use std::sync::Arc;

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::NSObject;
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplicationDidBecomeActiveNotification, NSApplicationDidFinishLaunchingNotification,
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

/// Set by the activation observer; consumed by the active host through
/// [`take_become_active_request`].
#[cfg(target_os = "macos")]
static BECOME_ACTIVE_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Written by the main-thread launch observer during AppKit's launch event.
#[cfg(target_os = "macos")]
static LAUNCH_NOTIFICATION_SEEN: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static LAUNCHED_AS_LOGIN_ITEM: AtomicBool = AtomicBool::new(false);

/// The active host's render-loop wake callback. Stored at [`install`] time; the
/// AppKit callback only flags + wakes, never touches the UI directly.
#[cfg(target_os = "macos")]
static UI_WAKEUP: OnceLock<WakeupCallback> = OnceLock::new();

/// UI-neutral callback used to wake whichever host owns the render loop.
pub(crate) type WakeupCallback = Arc<dyn Fn() + Send + Sync>;

#[cfg(any(target_os = "macos", test))]
fn signal_become_active_request(requested: &AtomicBool, wakeup: Option<&WakeupCallback>) {
    requested.store(true, Ordering::Relaxed);
    if let Some(wakeup) = wakeup {
        wakeup();
    }
}

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
            signal_become_active_request(&BECOME_ACTIVE_REQUESTED, UI_WAKEUP.get());
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

/// Register the retained launch-event observer before the native host starts
/// AppKit's launch path.
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

/// Install the activation observer once with a UI-neutral render-loop waker.
#[cfg(target_os = "macos")]
pub(crate) fn install(wakeup: WakeupCallback) {
    // Remember the waker even before the (once-only) observer setup, so the
    // callback can always wake the active host.
    let _ = UI_WAKEUP.set(wakeup);

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

/// Non-macOS: there is no Apple event to detect a login-item start.
#[cfg(not(target_os = "macos"))]
pub fn launched_as_log_in_item() -> bool {
    false
}

/// Non-macOS: there is no activation observer to install.
#[cfg(not(target_os = "macos"))]
pub(crate) fn install(_wakeup: WakeupCallback) {}

/// Non-macOS: the activation path only exists on macOS.
#[cfg(not(target_os = "macos"))]
pub fn take_become_active_request() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{WakeupCallback, signal_become_active_request};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn activation_request_flags_and_invokes_the_host_waker() {
        let requested = AtomicBool::new(false);
        let wakes = Arc::new(AtomicUsize::new(0));
        let wake_count = wakes.clone();
        let wakeup: WakeupCallback = Arc::new(move || {
            wake_count.fetch_add(1, Ordering::Relaxed);
        });

        signal_become_active_request(&requested, Some(&wakeup));

        assert!(requested.load(Ordering::Relaxed));
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn activation_request_is_flagged_even_without_a_host_waker() {
        let requested = AtomicBool::new(false);

        signal_become_active_request(&requested, None);

        assert!(requested.load(Ordering::Relaxed));
    }
}
