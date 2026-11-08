use super::{lifecycle, LiveState};
use crate::login::WebviewLogin;
use rune::{AppHandle, Tx, WindowHandle};

#[derive(Default)]
pub(super) struct LifecycleRoute {
    last_dock_icon: Option<bool>,
}

trait LifecycleHost {
    fn hide(&mut self);
    fn quit(&mut self);
    fn dock_icon(&mut self, visible: bool);
    fn open(&mut self) -> bool;
}

impl LifecycleRoute {
    fn set_dock_icon(&mut self, host: &mut impl LifecycleHost, visible: bool) {
        if self.last_dock_icon != Some(visible) {
            host.dock_icon(visible);
            self.last_dock_icon = Some(visible);
        }
    }

    fn close(&mut self, host: &mut impl LifecycleHost, tray_active: bool) {
        if tray_active {
            host.hide();
            self.set_dock_icon(host, false);
        } else {
            host.quit();
        }
    }

    fn show(&mut self, host: &mut impl LifecycleHost) -> bool {
        self.set_dock_icon(host, true);
        host.open()
    }

    fn reopen(&mut self, host: &mut impl LifecycleHost, has_visible_windows: bool) -> bool {
        if has_visible_windows {
            false
        } else {
            self.show(host);
            true
        }
    }

    fn quit(&mut self, host: &mut impl LifecycleHost) {
        host.quit();
    }
}

struct LiveLifecycleHost<'a> {
    native_window: &'a Option<WindowHandle>,
    pending_show_request: &'a mut bool,
    app_handle: &'a Option<AppHandle<LiveState>>,
    login: &'a mut WebviewLogin,
    tx: &'a mut Tx,
}

impl LifecycleHost for LiveLifecycleHost<'_> {
    fn hide(&mut self) {
        if let Some(window) = self.native_window {
            window.hide();
        }
    }

    fn quit(&mut self) {
        self.login.cancel();
        crate::wry_host::close();
        self.tx.quit();
    }

    fn dock_icon(&mut self, visible: bool) {
        if let Some(app) = self.app_handle {
            app.set_dock_icon(visible);
        }
    }

    fn open(&mut self) -> bool {
        *self.pending_show_request = true;
        lifecycle::deliver_pending_show_request(
            self.pending_show_request,
            self.native_window.as_ref(),
            WindowHandle::open,
        )
    }
}

impl LiveState {
    pub(super) fn show_window(&mut self, tx: &mut Tx) -> bool {
        let Self {
            lifecycle_route,
            native_window,
            pending_show_request,
            app_handle,
            login,
            ..
        } = self;
        lifecycle_route.show(&mut LiveLifecycleHost {
            native_window,
            pending_show_request,
            app_handle,
            login,
            tx,
        })
    }

    pub(super) fn quit(&mut self, tx: &mut Tx) {
        let Self {
            lifecycle_route,
            native_window,
            pending_show_request,
            app_handle,
            login,
            ..
        } = self;
        lifecycle_route.quit(&mut LiveLifecycleHost {
            native_window,
            pending_show_request,
            app_handle,
            login,
            tx,
        });
    }
}

pub(super) fn close_live(state: &mut LiveState, tx: &mut Tx) {
    let tray_active = state.tray_route.is_active();
    let LiveState {
        lifecycle_route,
        native_window,
        pending_show_request,
        app_handle,
        login,
        ..
    } = state;
    lifecycle_route.close(
        &mut LiveLifecycleHost {
            native_window,
            pending_show_request,
            app_handle,
            login,
            tx,
        },
        tray_active,
    );
}

pub(super) fn reopen_live(
    state: &mut LiveState,
    has_visible_windows: bool,
    tx: &mut Tx,
) -> bool {
    let LiveState {
        lifecycle_route,
        native_window,
        pending_show_request,
        app_handle,
        login,
        ..
    } = state;
    lifecycle_route.reopen(
        &mut LiveLifecycleHost {
            native_window,
            pending_show_request,
            app_handle,
            login,
            tx,
        },
        has_visible_windows,
    )
}

#[cfg(test)]
mod tests {
    use super::{LifecycleHost, LifecycleRoute};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Action {
        Hide,
        Quit,
        Dock(bool),
        Open,
    }

    struct FakeHost {
        actions: Vec<Action>,
        session_alive: bool,
        login_running: bool,
    }

    impl FakeHost {
        fn new() -> Self {
            Self {
                actions: Vec::new(),
                session_alive: true,
                login_running: true,
            }
        }
    }

    impl LifecycleHost for FakeHost {
        fn hide(&mut self) {
            self.actions.push(Action::Hide);
        }

        fn quit(&mut self) {
            self.actions.push(Action::Quit);
            self.session_alive = false;
            self.login_running = false;
        }

        fn dock_icon(&mut self, visible: bool) {
            self.actions.push(Action::Dock(visible));
        }

        fn open(&mut self) -> bool {
            self.actions.push(Action::Open);
            true
        }
    }

    #[test]
    fn close_with_active_tray_hides_without_quitting_or_cancelling_work() {
        let mut route = LifecycleRoute::default();
        let mut host = FakeHost::new();

        route.close(&mut host, true);

        assert_eq!(host.actions, [Action::Hide, Action::Dock(false)]);
        assert!(host.session_alive);
        assert!(host.login_running);
    }

    #[test]
    fn close_without_active_tray_quits() {
        let mut route = LifecycleRoute::default();
        let mut host = FakeHost::new();

        route.close(&mut host, false);

        assert_eq!(host.actions, [Action::Quit]);
        assert!(!host.session_alive);
        assert!(!host.login_running);
    }

    #[test]
    fn show_restores_dock_once_and_opens_each_time() {
        let mut route = LifecycleRoute::default();
        let mut host = FakeHost::new();

        assert!(route.show(&mut host));
        assert!(route.show(&mut host));

        assert_eq!(host.actions, [Action::Dock(true), Action::Open, Action::Open]);
    }

    #[test]
    fn reopen_opens_only_when_no_windows_are_visible() {
        let mut route = LifecycleRoute::default();
        let mut host = FakeHost::new();

        assert!(route.reopen(&mut host, false));
        assert_eq!(host.actions, [Action::Dock(true), Action::Open]);

        host.actions.clear();
        assert!(!route.reopen(&mut host, true));
        assert!(host.actions.is_empty());
    }
}
