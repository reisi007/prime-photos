//! Host-neutral helpers shared by the eframe and Rune application hosts.

/// Synthetic folder index of the transient one-time (drag & drop) upload
/// target: it is never in `store.folders`, so no persisted index can collide
/// with it and every run reports its counters under this one key.
pub(crate) const ONE_TIME_IDX: usize = usize::MAX;

/// App name used for the auto-launch (login item) registration.
const AUTOSTART_APP_NAME: &str = "Prime Photos";

/// Build the `auto-launch` registration for the current executable.
///
/// `None` means "autostart is unavailable here": the platform is not supported
/// or the executable path could not be determined. The app keeps running and
/// the toggle simply stays off.
///
/// The builder is used instead of the platform-specific `AutoLaunch::new`
/// constructors so this helper stays `#[cfg]`-free. `MacOSLaunchMode::SMAppService`
/// is spelled out (not the crate default `LaunchAgent`) because that is the mode
/// we want on macOS: `SMAppService.mainApp.register()` enters the app into the
/// **Anmeldeobjekte** (System Settings → General → Login Items) — a `LaunchAgent`
/// plist under `~/Library/LaunchAgents` never shows up there. Requires macOS 13+;
/// a freshly registered entry may first be `RequiresApproval` until the user
/// approves it in the login-items list.
fn build_autostart() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name(AUTOSTART_APP_NAME)
        .set_app_path(&exe.display().to_string())
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::SMAppService)
        .build()
        .ok()
}

/// Runtime-only autostart (login item) state.
///
/// `inner` is `None` when the platform is unsupported or the registration could
/// not be built. Every operation then answers "unsupported"/"disabled" rather
/// than panicking, so the settings toggle is safe on every platform.
pub(crate) struct Autostart {
    inner: Option<auto_launch::AutoLaunch>,
}

impl Autostart {
    pub(crate) fn new() -> Self {
        Self {
            inner: build_autostart(),
        }
    }

    #[cfg(any(test, all(debug_assertions, feature = "legacy-eframe")))]
    pub(crate) fn unavailable_for_test() -> Self {
        Self { inner: None }
    }

    pub(crate) fn enable(&self) -> auto_launch::Result<()> {
        match self.inner.as_ref() {
            Some(inner) => inner.enable(),
            None => Err(auto_launch::Error::UnsupportedOS),
        }
    }

    pub(crate) fn disable(&self) -> auto_launch::Result<()> {
        match self.inner.as_ref() {
            Some(inner) => inner.disable(),
            None => Err(auto_launch::Error::UnsupportedOS),
        }
    }

    /// Current OS state; `None` when unsupported or unreadable (treated as off).
    pub(crate) fn is_enabled(&self) -> Option<bool> {
        self.inner.as_ref().and_then(|inner| inner.is_enabled().ok())
    }
}

/// What the persisted autostart intent asks us to do at startup (PKG-3).
///
/// Pure decision, no OS access: derived from the config intent and the current
/// OS state so it is cheap to unit-test and can be used by either host.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum AutostartAction {
    /// Never decided before (`None`): turn the login item on and record
    /// `Some(true)`. The enable is a no-op when the OS state is already on.
    EnableAndRecord,
    /// Intent says "on" but the OS state is off: repair the login item
    /// (plist deleted, app moved).
    Enable,
    /// Nothing to do (already on, or the user vetoed with `Some(false)`).
    Nothing,
}

/// Derive the startup action from the persisted intent and the OS state.
///
/// `None` (never decided) ⇒ enable and record; `Some(true)` + OS off ⇒ repair;
/// `Some(false)` always ⇒ nothing (never fight a deliberate "off" — a login
/// item the user created by hand is not touched).
pub(crate) fn autostart_action(intent: Option<bool>, os_enabled: bool) -> AutostartAction {
    match intent {
        None => AutostartAction::EnableAndRecord,
        Some(true) if !os_enabled => AutostartAction::Enable,
        _ => AutostartAction::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::{AutostartAction, autostart_action};

    #[test]
    fn autostart_action_never_decided_enables_and_records() {
        // `None` ⇒ enable + record, regardless of the current OS state.
        assert_eq!(
            autostart_action(None, false),
            AutostartAction::EnableAndRecord
        );
        assert_eq!(
            autostart_action(None, true),
            AutostartAction::EnableAndRecord
        );
    }

    #[test]
    fn autostart_action_intent_on_repairs_only_when_the_os_is_off() {
        assert_eq!(
            autostart_action(Some(true), false),
            AutostartAction::Enable
        );
        assert_eq!(
            autostart_action(Some(true), true),
            AutostartAction::Nothing
        );
    }

    #[test]
    fn autostart_action_deliberate_off_is_a_veto() {
        assert_eq!(
            autostart_action(Some(false), false),
            AutostartAction::Nothing
        );
        assert_eq!(
            autostart_action(Some(false), true),
            AutostartAction::Nothing
        );
    }
}
