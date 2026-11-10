use super::super::live_view;
use crate::rune_backup_live::test_support::signed_out_state;
use crate::rune_backup_live::{LivePage, LiveState};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn production_runtime(state: LiveState, size: (f32, f32)) -> Runtime<LiveState> {
    let mut runtime = Runtime::new(state, live_view, size, Env::new(false, 1.0));
    runtime.frame(0.0);
    runtime
}

fn synthetic_signed_in_state() -> LiveState {
    let mut state = signed_out_state();
    state.config.session_id = "synthetic-session".to_owned();
    state.config.cookie_x_acb = "synthetic-cookie".to_owned();
    state.account_identity.name = Some("Synthetic backup account".to_owned());
    state
}

fn synthetic_restoring_state() -> LiveState {
    let state = signed_out_state();
    state.session.lock().unwrap().refresh_token = Some("synthetic-refresh-token".to_owned());
    state
}

fn live_view_element(runtime: &Runtime<LiveState>, key: &str) -> rune::runtime::ElementInfo {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("missing production Rune view element {key:?}"))
}

fn assert_live_view_bounds(runtime: &Runtime<LiveState>, size: (f32, f32), keys: &[&str]) {
    for key in keys {
        let element = live_view_element(runtime, key);
        let [x, y, width, height] = element.rect;
        let [visible_x, visible_y, visible_width, visible_height] = element.visible;
        assert!(
            x >= -0.5
                && y >= -0.5
                && width > 0.0
                && height > 0.0
                && x + width <= size.0 + 0.5
                && y + height <= size.1 + 0.5,
            "{key} bounds {:?} extend outside viewport {size:?}",
            element.rect
        );
        assert!(
            (visible_x - x).abs() <= 0.5
                && (visible_y - y).abs() <= 0.5
                && (visible_width - width).abs() <= 0.5
                && (visible_height - height).abs() <= 0.5,
            "{key} is clipped: rect={:?}, visible={:?}",
            element.rect,
            element.visible
        );
    }
}

fn find_a11y_node<'a>(node: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    if node["key"] == key {
        return Some(node);
    }
    node["children"]
        .as_array()?
        .iter()
        .find_map(|child| find_a11y_node(child, key))
}

fn a11y_children(node: &serde_json::Value) -> &[serde_json::Value] {
    node["children"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn a11y_contains_key(node: &serde_json::Value, key: &str) -> bool {
    find_a11y_node(node, key).is_some()
}

fn a11y_count_role(node: &serde_json::Value, role: &str) -> usize {
    usize::from(node["role"] == role)
        + a11y_children(node)
            .iter()
            .map(|child| a11y_count_role(child, role))
            .sum::<usize>()
}

fn a11y_contains_text(node: &serde_json::Value, text: &str) -> bool {
    node["name"] == text
        || node["value"] == text
        || a11y_children(node)
            .iter()
            .any(|child| a11y_contains_text(child, text))
}

fn a11y_focused_key(node: &serde_json::Value) -> Option<String> {
    if node["states"]
        .as_array()
        .is_some_and(|states| states.iter().any(|state| state.as_str() == Some("focused")))
    {
        return node["key"].as_str().map(str::to_owned);
    }
    a11y_children(node).iter().find_map(a11y_focused_key)
}

fn assert_a11y_focused(runtime: &Runtime<LiveState>, key: &str) {
    let accessibility = runtime.accessibility_json();
    let node = find_a11y_node(&accessibility, key)
        .unwrap_or_else(|| panic!("{key} is present in the AccessKit tree"));
    assert!(
        node["states"]
            .as_array()
            .is_some_and(|states| states.iter().any(|state| state.as_str() == Some("focused"))),
        "{key} holds keyboard focus: {accessibility}"
    );
}

fn assert_live_sidebar(
    runtime: &Runtime<LiveState>,
    active_page: LivePage,
    signed_in: bool,
    expected_status: Option<&str>,
) {
    assert!(runtime.find("rune-backup-sidebar").is_some());
    assert!(runtime.find("sidebar-brand-name").is_some());
    let active_key = match active_page {
        LivePage::Settings => "sidebar-nav-settings",
        _ => "sidebar-nav-backup",
    };
    let accessibility = runtime.accessibility_json();
    let navigation = find_a11y_node(&accessibility, "sidebar-navigation")
        .expect("sidebar navigation landmark in the AccessKit tree");
    assert_eq!(navigation["role"], "navigation");
    assert_eq!(
        navigation["name"],
        crate::i18n::tr("Hauptnavigation"),
        "the navigation landmark is labelled"
    );
    assert!(
        a11y_contains_key(navigation, "sidebar-tablist"),
        "the navigation landmark contains the tablist: {accessibility}"
    );
    assert!(
        !a11y_contains_key(navigation, "sidebar-brand-name")
            && !a11y_contains_key(navigation, "sidebar-account"),
        "brand and account stay outside the navigation landmark: {accessibility}"
    );

    let tablist = find_a11y_node(&accessibility, "sidebar-tablist")
        .expect("sidebar tablist in the AccessKit tree");
    assert_eq!(tablist["role"], "tab-list");
    assert_eq!(tablist["name"], crate::i18n::tr("Hauptbereiche"));
    assert_eq!(
        a11y_count_role(tablist, "tab"),
        2,
        "the tablist holds exactly the two navigation tabs: {accessibility}"
    );
    assert!(
        !a11y_contains_key(tablist, "sign-out"),
        "sign-out is a button outside the tablist: {accessibility}"
    );

    let account = find_a11y_node(&accessibility, "sidebar-account")
        .expect("account group in the AccessKit tree");
    assert_eq!(account["role"], "group");
    assert_eq!(account["name"], crate::i18n::tr("Konto"));
    assert!(
        !a11y_contains_key(account, "sidebar-account-heading"),
        "the duplicate visible account heading is hidden from assistive tech"
    );

    let marketplace = find_a11y_node(&accessibility, "sidebar-marketplace")
        .expect("marketplace group in the AccessKit tree");
    assert_eq!(marketplace["name"], crate::i18n::tr("Marktplatz"));
    assert!(
        !a11y_contains_key(marketplace, "sidebar-marketplace-label"),
        "the duplicate visible marketplace caption is hidden from assistive tech"
    );

    for (key, label) in [
        ("sidebar-nav-backup", crate::i18n::tr("Datensicherung")),
        ("sidebar-nav-settings", crate::i18n::tr("Einstellungen")),
    ] {
        let item = live_view_element(runtime, key);
        assert_eq!(item.role, "tab", "{key} is a tab in the sidebar tablist");
        assert_eq!(
            item.label.as_deref(),
            Some(label),
            "{key} has an accessible label"
        );
        assert!(item.focusable, "{key} stays keyboard focusable");
        let node = find_a11y_node(&accessibility, key)
            .unwrap_or_else(|| panic!("{key} is present in the AccessKit tree"));
        assert_eq!(node["role"], "tab", "{key} keeps its tab role in AccessKit");
        let selected_state = if key == active_key {
            "selected"
        } else {
            "unselected"
        };
        assert!(
            node["states"].as_array().is_some_and(|states| {
                states
                    .iter()
                    .any(|state| state.as_str() == Some(selected_state))
            }),
            "{key} should be {selected_state}: {accessibility}"
        );
    }
    let active_marker = format!("{active_key}-active");
    assert!(
        runtime.find(&active_marker).is_some(),
        "active marker {active_marker}"
    );
    assert_eq!(
        runtime.find(if active_key == "sidebar-nav-settings" {
            "sidebar-nav-backup-active"
        } else {
            "sidebar-nav-settings-active"
        }),
        None,
        "only the current navigation item is marked active"
    );
    assert!(runtime.find("sidebar-account").is_some());
    assert!(runtime.find("sidebar-marketplace-value").is_some());

    if signed_in {
        let name = live_view_element(runtime, "account-name");
        assert_eq!(name.text.as_deref(), Some("Synthetic backup account"));
        assert!(runtime.find("account-avatar-placeholder").is_some());
        let sign_out = live_view_element(runtime, "sign-out");
        assert_eq!(sign_out.role, "button");
        assert!(sign_out.focusable);
        let sign_out_a11y = find_a11y_node(&accessibility, "sign-out")
            .expect("sign-out is present in the AccessKit tree");
        assert_eq!(
            sign_out_a11y["role"], "button",
            "sign-out stays a button and never becomes a navigation tab"
        );
        let account_identity = find_a11y_node(&accessibility, "account-identity")
            .expect("signed-in profile group in the AccessKit tree");
        assert!(
            !a11y_contains_text(account_identity, "S"),
            "the decorative avatar initial is hidden from assistive tech: {accessibility}"
        );
    } else {
        let status = live_view_element(runtime, "sidebar-account-status");
        assert_eq!(status.role, "status");
        assert_eq!(status.text.as_deref(), expected_status);
        assert!(runtime.find("account-identity").is_none());
        assert!(runtime.find("account-name").is_none());
        assert!(runtime.find("sign-out").is_none());
    }
}

fn click_live_view(runtime: &mut Runtime<LiveState>, key: &str) {
    let (x, y) = runtime
        .find(key)
        .unwrap_or_else(|| panic!("missing production Rune view control {key:?}"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

#[test]
fn production_live_sidebar_is_accessible_and_fits_minimum_and_desktop_viewports() {
    for size in [(760.0, 520.0), (1180.0, 820.0)] {
        for (state, signed_in, expected_status) in [
            (
                signed_out_state(),
                false,
                Some(crate::i18n::tr("Nicht angemeldet")),
            ),
            (
                synthetic_restoring_state(),
                false,
                Some(crate::i18n::tr("Sitzung wird wiederhergestellt …")),
            ),
            (synthetic_signed_in_state(), true, None),
        ] {
            let runtime = production_runtime(state, size);
            assert_live_sidebar(&runtime, LivePage::Backup, signed_in, expected_status);
            let mut keys = vec![
                "rune-backup-sidebar",
                "sidebar-brand-name",
                "sidebar-nav-backup",
                "sidebar-nav-settings",
                "sidebar-account-footer",
                "sidebar-account",
                "sidebar-marketplace-value",
                "live-page-title",
                "open-about",
                "add-target",
            ];
            if signed_in {
                keys.extend([
                    "account-identity",
                    "account-avatar-placeholder",
                    "account-name",
                    "sign-out",
                ]);
            } else {
                keys.push("sidebar-account-status");
            }
            assert_live_view_bounds(&runtime, size, &keys);
        }

        let mut state = signed_out_state();
        state.page = LivePage::Settings;
        let runtime = production_runtime(state, size);
        assert_live_sidebar(
            &runtime,
            LivePage::Settings,
            false,
            Some(crate::i18n::tr("Nicht angemeldet")),
        );
        assert!(runtime.find("settings-content-root").is_some());
        assert!(runtime.find("open-settings").is_none());
        assert!(runtime.find("back-to-backup").is_none());
        assert_live_view_bounds(
            &runtime,
            size,
            &[
                "rune-backup-sidebar",
                "sidebar-brand-name",
                "sidebar-nav-backup",
                "sidebar-nav-settings",
                "sidebar-account-status",
                "sidebar-marketplace-value",
                "live-page-title",
                "open-about",
            ],
        );
    }
}

#[test]
fn production_sidebar_accessibility_tracks_selected_page_without_mutating_auth_state() {
    for signed_in in [false, true] {
        for page in [
            LivePage::Backup,
            LivePage::Settings,
            LivePage::About,
            LivePage::Licenses,
        ] {
            let mut state = if signed_in {
                synthetic_signed_in_state()
            } else {
                signed_out_state()
            };
            state.page = page;

            let runtime = production_runtime(state, (760.0, 520.0));
            assert_live_sidebar(
                &runtime,
                page,
                signed_in,
                (!signed_in).then(|| crate::i18n::tr("Nicht angemeldet")),
            );
            assert_eq!(
                runtime.state.page, page,
                "rendering does not change the route"
            );
            assert_eq!(
                runtime.state.config.session_id,
                if signed_in { "synthetic-session" } else { "" },
                "rendering does not change session state"
            );
            assert_eq!(
                runtime.state.config.cookie_x_acb,
                if signed_in { "synthetic-cookie" } else { "" },
                "rendering does not change authentication state"
            );
        }
    }
}

#[test]
fn production_live_sidebar_navigation_switches_backup_settings_and_keeps_about_routes_mounted() {
    let mut runtime = production_runtime(signed_out_state(), (760.0, 520.0));

    click_live_view(&mut runtime, "sidebar-nav-settings");
    assert_eq!(runtime.state.page, LivePage::Settings);
    assert!(runtime.find("settings-content-root").is_some());
    assert_live_sidebar(
        &runtime,
        LivePage::Settings,
        false,
        Some(crate::i18n::tr("Nicht angemeldet")),
    );

    click_live_view(&mut runtime, "sidebar-nav-backup");
    assert_eq!(runtime.state.page, LivePage::Backup);
    assert!(runtime.find("live-targets").is_some());
    assert_live_sidebar(
        &runtime,
        LivePage::Backup,
        false,
        Some(crate::i18n::tr("Nicht angemeldet")),
    );

    click_live_view(&mut runtime, "open-about");
    assert_eq!(runtime.state.page, LivePage::About);
    assert!(runtime.find("rune-about-page").is_some());
    assert_live_sidebar(
        &runtime,
        LivePage::About,
        false,
        Some(crate::i18n::tr("Nicht angemeldet")),
    );

    click_live_view(&mut runtime, "license-open");
    assert_eq!(runtime.state.page, LivePage::Licenses);
    assert!(runtime.find("rune-licenses-page").is_some());
    assert_live_sidebar(
        &runtime,
        LivePage::Licenses,
        false,
        Some(crate::i18n::tr("Nicht angemeldet")),
    );

    click_live_view(&mut runtime, "sidebar-nav-settings");
    assert_eq!(runtime.state.page, LivePage::Settings);
    assert!(runtime.find("settings-content-root").is_some());
    assert_live_sidebar(
        &runtime,
        LivePage::Settings,
        false,
        Some(crate::i18n::tr("Nicht angemeldet")),
    );
}

#[test]
fn production_live_sidebar_tablist_supports_keyboard_navigation() {
    let mut runtime = production_runtime(signed_out_state(), (760.0, 520.0));

    // Tab from no focus enters the tablist at its selected item.
    runtime.key(KeyEvent {
        key: Key::Tab,
        mods: Mods::default(),
    });
    runtime.frame(1.0 / 60.0);
    assert_a11y_focused(&runtime, "sidebar-nav-backup");

    // Inside the tablist an arrow moves focus and activates the tab it lands on.
    runtime.key(KeyEvent {
        key: Key::Right,
        mods: Mods::default(),
    });
    runtime.frame(1.0 / 60.0);
    assert_eq!(runtime.state.page, LivePage::Settings);
    assert_a11y_focused(&runtime, "sidebar-nav-settings");
    assert_live_sidebar(
        &runtime,
        LivePage::Settings,
        false,
        Some(crate::i18n::tr("Nicht angemeldet")),
    );

    runtime.key(KeyEvent {
        key: Key::Left,
        mods: Mods::default(),
    });
    runtime.frame(1.0 / 60.0);
    assert_eq!(runtime.state.page, LivePage::Backup);
    assert_a11y_focused(&runtime, "sidebar-nav-backup");

    // The tablist is one Tab stop: Tab moves focus out of it.
    runtime.key(KeyEvent {
        key: Key::Tab,
        mods: Mods::default(),
    });
    runtime.frame(1.0 / 60.0);
    let focused = a11y_focused_key(&runtime.accessibility_json());
    assert!(
        focused.as_deref() != Some("sidebar-nav-backup")
            && focused.as_deref() != Some("sidebar-nav-settings"),
        "Tab leaves the tablist as a whole, focus is on {focused:?}"
    );
}
