use super::{
    IconKey, LastOutcome, MenuAction, TrayActionHost, TrayRoute, action_for_id, dispatch_action,
    done_wake_after, finished_outcome, icon_key, should_apply_icon, system_theme,
};
use crate::tray::{self, Theme, TrayState};
use std::time::{Duration, Instant};

#[test]
fn label_refresh_invalidates_cached_labels_and_runs_wakeup() {
    let mut route = TrayRoute {
        last_labels: Some((crate::i18n::Lang::En, false)),
        ..TrayRoute::default()
    };
    let wakeups = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = wakeups.clone();
    route.set_wakeup(std::sync::Arc::new(move || {
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));

    route.request_label_refresh();

    assert!(route.last_labels.is_none());
    assert_eq!(wakeups.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn tray_state_preserves_pause_busy_error_and_done_priority() {
    assert_eq!(
        icon_key(true, true, true, Some(Duration::ZERO), Theme::Light).state,
        TrayState::Paused
    );
    assert_eq!(
        icon_key(false, true, true, Some(Duration::ZERO), Theme::Light).state,
        TrayState::Active
    );
    assert_eq!(
        icon_key(false, false, true, Some(Duration::ZERO), Theme::Light).state,
        TrayState::Error
    );
    assert_eq!(
        icon_key(
            false,
            false,
            false,
            Some(tray::DONE_WINDOW - Duration::from_millis(1)),
            Theme::Light,
        )
        .state,
        TrayState::Done
    );
    assert_eq!(
        icon_key(
            false,
            false,
            false,
            Some(tray::DONE_WINDOW),
            Theme::Light,
        )
        .state,
        TrayState::Idle
    );
    assert_eq!(
        icon_key(false, false, true, Some(Duration::from_secs(300)), Theme::Light).state,
        TrayState::Error,
        "the last failure remains visible beyond the Done window"
    );
}

#[test]
fn done_policy_returns_only_the_remaining_expiry_delay() {
    let elapsed = tray::DONE_WINDOW - Duration::from_secs(2);
    let done = icon_key(false, false, false, Some(elapsed), Theme::Light);
    assert_eq!(done.state, TrayState::Done);
    assert_eq!(done_wake_after(done, Some(elapsed)), Some(Duration::from_secs(2)));

    let idle = icon_key(
        false,
        false,
        false,
        Some(tray::DONE_WINDOW),
        Theme::Light,
    );
    assert_eq!(done_wake_after(idle, Some(tray::DONE_WINDOW)), None);
    assert_eq!(
        done_wake_after(
            icon_key(true, false, false, Some(elapsed), Theme::Light),
            Some(elapsed),
        ),
        None,
        "a paused glyph needs no Done-expiry timer"
    );
}

#[test]
fn cancelled_runs_keep_the_prior_outcome_and_success_starts_a_new_done_window() {
    let finished_at = Instant::now();
    let previous = LastOutcome {
        failed: true,
        done_at: Some(finished_at),
    };
    assert_eq!(
        finished_outcome(
            previous,
            true,
            false,
            finished_at + Duration::from_secs(1),
        ),
        previous
    );

    let success = finished_outcome(
        previous,
        false,
        false,
        finished_at + Duration::from_secs(2),
    );
    assert!(!success.failed);
    assert_eq!(success.done_at, Some(finished_at + Duration::from_secs(2)));
    assert_eq!(
        icon_key(false, false, success.failed, Some(Duration::ZERO), Theme::Light).state,
        TrayState::Done
    );

    let later_failure = finished_outcome(
        success,
        false,
        true,
        finished_at + Duration::from_secs(3),
    );
    assert_eq!(
        icon_key(
            false,
            false,
            later_failure.failed,
            Some(Duration::from_secs(300)),
            Theme::Light,
        )
        .state,
        TrayState::Error
    );
}

#[test]
fn system_appearance_changes_the_icon_key_without_a_state_change() {
    let light = IconKey {
        state: TrayState::Idle,
        theme: system_theme(false),
    };
    let dark = IconKey {
        state: TrayState::Idle,
        theme: system_theme(true),
    };
    assert_eq!(light.theme, Theme::Light);
    assert_eq!(dark.theme, Theme::Dark);
    assert!(should_apply_icon(Some(light), dark));
    assert!(!should_apply_icon(Some(light), light));
}

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
