use crate::i18n::Lang;
use crate::tray::{self, Tray, TrayState, WakeupCallback};
use rune::Tx;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

const SYSTEM_THEME_UNSET: u8 = 0;
const SYSTEM_THEME_LIGHT: u8 = 1;
const SYSTEM_THEME_DARK: u8 = 2;

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
    last_icon_state: Option<IconKey>,
    system_theme: AtomicU8,
    was_busy: bool,
    last_outcome: LastOutcome,
    scheduled_done_at: Option<Instant>,
}

impl TrayRoute {
    pub(super) fn set_wakeup(&mut self, wakeup: WakeupCallback) {
        self.wakeup = Some(wakeup);
    }

    pub(super) fn is_active(&self) -> bool {
        self.tray.is_some()
    }

    /// Build and reconcile the startup tray while Rune's native event loop is
    /// active, returning whether it is available for a silent launch.
    pub(super) fn prepare(
        &mut self,
        has_pending_session: bool,
        paused: bool,
        busy: bool,
        language: Lang,
        runtime: &tokio::runtime::Runtime,
    ) -> bool {
        self.reconcile(has_pending_session, paused, busy, language, runtime);
        self.is_active()
    }

    /// Rune 53c7f60 exposes OS appearance through `Cx::system_dark()` in the
    /// view only. Cache it atomically for the main-thread tray reconcile and
    /// wake that reconcile once when the appearance changes.
    pub(super) fn observe_system_dark(&self, system_dark: bool) {
        let theme = system_theme(system_dark);
        let next = theme_code(theme);
        if self.system_theme.swap(next, Ordering::Relaxed) != next {
            if let Some(wakeup) = &self.wakeup {
                wakeup();
            }
        }
    }

    fn reconcile(
        &mut self,
        has_pending_session: bool,
        paused: bool,
        busy: bool,
        language: Lang,
        runtime: &tokio::runtime::Runtime,
    ) -> bool {
        if !has_pending_session {
            self.tray = None;
            self.build_failed = false;
            self.last_labels = None;
            self.last_icon_state = None;
            return false;
        }

        let theme = self.current_theme();
        if self.tray.is_none() && !self.build_failed {
            let Some(wakeup) = self.wakeup.clone() else {
                return false;
            };
            self.tray = tray::build(theme, wakeup);
            self.build_failed = self.tray.is_none();
            self.last_labels = None;
            self.last_icon_state = None;
        }

        let Some(tray) = self.tray.as_mut() else {
            return false;
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

        let since_done = self.last_outcome.done_at.map(|finished| finished.elapsed());
        let icon = icon_key(
            paused,
            busy,
            self.last_outcome.failed,
            since_done,
            theme,
        );
        let mut changed = false;
        if should_apply_icon(self.last_icon_state, icon) {
            tray.set_state(icon.state, icon.theme);
            self.last_icon_state = Some(icon);
            changed = true;
        }

        // Like eframe, only a currently visible Done glyph needs an expiry
        // wake. Paused/active priorities will reconcile on their own next
        // state change; there is no periodic tray polling.
        if let Some(delay) = done_wake_after(icon, since_done) {
            if let Some(done_at) = self.last_outcome.done_at {
                if self.scheduled_done_at != Some(done_at) {
                    if let Some(wakeup) = self.wakeup.clone() {
                        self.scheduled_done_at = Some(done_at);
                        drop(runtime.spawn(async move {
                            tokio::time::sleep(delay).await;
                            wakeup();
                        }));
                    }
                }
            }
        }
        changed
    }

    fn current_theme(&self) -> tray::Theme {
        theme_from_code(self.system_theme.load(Ordering::Relaxed))
    }

    fn observe_upload(
        &mut self,
        busy: bool,
        cancelled: bool,
        failed: bool,
        now: Instant,
    ) {
        if self.was_busy && !busy {
            self.last_outcome = finished_outcome(self.last_outcome, cancelled, failed, now);
        }
        self.was_busy = busy;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IconKey {
    state: TrayState,
    theme: tray::Theme,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LastOutcome {
    failed: bool,
    done_at: Option<Instant>,
}

fn system_theme(system_dark: bool) -> tray::Theme {
    if system_dark {
        tray::Theme::Dark
    } else {
        tray::Theme::Light
    }
}

fn theme_code(theme: tray::Theme) -> u8 {
    match theme {
        tray::Theme::Unknown => SYSTEM_THEME_UNSET,
        tray::Theme::Light => SYSTEM_THEME_LIGHT,
        tray::Theme::Dark => SYSTEM_THEME_DARK,
    }
}

fn theme_from_code(code: u8) -> tray::Theme {
    match code {
        SYSTEM_THEME_LIGHT => tray::Theme::Light,
        SYSTEM_THEME_DARK => tray::Theme::Dark,
        _ => tray::Theme::Unknown,
    }
}

fn finished_outcome(
    previous: LastOutcome,
    cancelled: bool,
    failed: bool,
    now: Instant,
) -> LastOutcome {
    if cancelled {
        previous
    } else {
        LastOutcome {
            failed,
            done_at: Some(now),
        }
    }
}

fn icon_key(
    paused: bool,
    busy: bool,
    last_run_failed: bool,
    since_done: Option<Duration>,
    theme: tray::Theme,
) -> IconKey {
    IconKey {
        state: tray::tray_state(paused, busy, last_run_failed, since_done),
        theme,
    }
}

fn should_apply_icon(previous: Option<IconKey>, next: IconKey) -> bool {
    previous != Some(next)
}

fn done_wake_after(icon: IconKey, since_done: Option<Duration>) -> Option<Duration> {
    if icon.state == TrayState::Done {
        since_done.and_then(|elapsed| tray::DONE_WINDOW.checked_sub(elapsed))
    } else {
        None
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

    let (busy, cancelled, failed) = {
        let upload = state.upload.lock().unwrap();
        (upload.active, upload.cancelled, upload.failed > 0)
    };
    state
        .tray_route
        .observe_upload(busy, cancelled, failed, Instant::now());
    let paused = *state.paused.borrow();
    let has_pending_session = state.has_pending_session();
    let runtime = state.runtime.clone();
    changed |= state.tray_route.reconcile(
        has_pending_session,
        paused,
        busy,
        crate::i18n::lang(),
        runtime.as_ref(),
    );
    changed
}

#[cfg(test)]
#[path = "tray_route/tests.rs"]
mod tests;
