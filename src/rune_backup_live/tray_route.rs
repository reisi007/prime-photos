use crate::i18n::Lang;
use crate::tray::{self, Tray, TrayState, WakeupCallback};
use rune::Tx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuAction {
    ShowWindow,
    TogglePause,
    About,
    Quit,
}

#[derive(Default)]
pub(super) struct TrayRoute {
    tray: Option<Tray>,
    wakeup: Option<WakeupCallback>,
    build_failed: bool,
    last_labels: Option<(Lang, bool)>,
    last_icon_state: Option<TrayState>,
}

impl TrayRoute {
    pub(super) fn set_wakeup(&mut self, wakeup: WakeupCallback) {
        self.wakeup = Some(wakeup);
    }

    pub(super) fn is_active(&self) -> bool {
        self.tray.is_some()
    }

    fn reconcile(
        &mut self,
        has_pending_session: bool,
        paused: bool,
        busy: bool,
        language: Lang,
    ) {
        if !has_pending_session {
            self.tray = None;
            self.build_failed = false;
            self.last_labels = None;
            self.last_icon_state = None;
            return;
        }

        if self.tray.is_none() && !self.build_failed {
            let Some(wakeup) = self.wakeup.clone() else {
                return;
            };
            self.tray = tray::build(tray::Theme::Unknown, wakeup);
            self.build_failed = self.tray.is_none();
            self.last_labels = None;
            self.last_icon_state = None;
        }

        let Some(tray) = self.tray.as_mut() else {
            return;
        };

        let labels = (language, paused);
        if self.last_labels != Some(labels) {
            match self.last_labels {
                Some((last_language, _)) if last_language == language => {
                    tray.set_paused(paused);
                }
                _ => tray.set_labels(paused),
            }
            self.last_labels = Some(labels);
        }

        // `Theme::Unknown` selects per-state template glyphs on macOS and the
        // logo fallback elsewhere; done/error tray reporting is not ported.
        let icon_state = tray::tray_state(paused, busy, false, None);
        if self.last_icon_state != Some(icon_state) {
            tray.set_state(icon_state, tray::Theme::Unknown);
            self.last_icon_state = Some(icon_state);
        }
    }

    fn action_for_id(&self, id: &tray_icon::menu::MenuId) -> Option<MenuAction> {
        let tray = self.tray.as_ref()?;
        action_for_id(
            id,
            tray.show_item.id(),
            tray.pause_item.id(),
            tray.about_item.id(),
            tray.quit_item.id(),
        )
    }
}

trait TrayActionHost {
    fn show_window(&mut self);
    fn toggle_pause(&mut self);
    fn open_about(&mut self);
    fn quit(&mut self);
}

fn dispatch_action(host: &mut impl TrayActionHost, action: MenuAction) {
    match action {
        MenuAction::ShowWindow => host.show_window(),
        MenuAction::TogglePause => host.toggle_pause(),
        MenuAction::About => host.open_about(),
        MenuAction::Quit => host.quit(),
    }
}

fn action_for_id<T: PartialEq>(
    id: &T,
    show_id: &T,
    pause_id: &T,
    about_id: &T,
    quit_id: &T,
) -> Option<MenuAction> {
    if id == show_id {
        Some(MenuAction::ShowWindow)
    } else if id == pause_id {
        Some(MenuAction::TogglePause)
    } else if id == about_id {
        Some(MenuAction::About)
    } else if id == quit_id {
        Some(MenuAction::Quit)
    } else {
        None
    }
}

struct LiveTrayActionHost<'a> {
    state: &'a mut super::LiveState,
    tx: &'a mut Tx,
}

impl TrayActionHost for LiveTrayActionHost<'_> {
    fn show_window(&mut self) {
        self.state.show_window(self.tx);
    }

    fn toggle_pause(&mut self) {
        let paused = !*self.state.paused.borrow();
        self.state.paused.send_replace(paused);
        self.state.config.paused = paused;
        self.state.persist_config_if_allowed();
    }

    fn open_about(&mut self) {
        self.state.page = super::LivePage::About;
        self.show_window();
    }

    fn quit(&mut self) {
        self.state.quit(self.tx);
    }
}

pub(super) fn poll(state: &mut super::LiveState, tx: &mut Tx) -> bool {
    let mut changed = false;
    for event in tray::drain_events() {
        let Some(action) = state.tray_route.action_for_id(&event.id) else {
            continue;
        };
        dispatch_action(&mut LiveTrayActionHost { state, tx }, action);
        changed = true;
    }

    let paused = *state.paused.borrow();
    let busy = state.upload.lock().unwrap().active;
    state.tray_route.reconcile(
        state.has_pending_session(),
        paused,
        busy,
        crate::i18n::lang(),
    );
    changed
}

#[cfg(test)]
mod tests {
    use super::{action_for_id, dispatch_action, MenuAction, TrayActionHost};

    #[test]
    fn menu_ids_route_to_each_action() {
        let show = "show";
        let pause = "pause";
        let about = "about";
        let quit = "quit";

        assert_eq!(
            action_for_id(&show, &show, &pause, &about, &quit),
            Some(MenuAction::ShowWindow)
        );
        assert_eq!(
            action_for_id(&pause, &show, &pause, &about, &quit),
            Some(MenuAction::TogglePause)
        );
        assert_eq!(
            action_for_id(&about, &show, &pause, &about, &quit),
            Some(MenuAction::About)
        );
        assert_eq!(
            action_for_id(&quit, &show, &pause, &about, &quit),
            Some(MenuAction::Quit)
        );
        assert_eq!(
            action_for_id(&"unknown", &show, &pause, &about, &quit),
            None
        );
    }

    #[derive(Default)]
    struct FixtureHost {
        actions: Vec<MenuAction>,
    }

    impl TrayActionHost for FixtureHost {
        fn show_window(&mut self) {
            self.actions.push(MenuAction::ShowWindow);
        }

        fn toggle_pause(&mut self) {
            self.actions.push(MenuAction::TogglePause);
        }

        fn open_about(&mut self) {
            self.actions.push(MenuAction::About);
        }

        fn quit(&mut self) {
            self.actions.push(MenuAction::Quit);
        }
    }

    #[test]
    fn shared_dispatcher_routes_every_action_to_its_host_operation() {
        let mut host = FixtureHost::default();
        for action in [
            MenuAction::ShowWindow,
            MenuAction::TogglePause,
            MenuAction::About,
            MenuAction::Quit,
        ] {
            dispatch_action(&mut host, action);
        }

        assert_eq!(
            host.actions,
            [
                MenuAction::ShowWindow,
                MenuAction::TogglePause,
                MenuAction::About,
                MenuAction::Quit,
            ]
        );
    }
}
