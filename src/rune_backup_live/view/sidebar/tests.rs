use super::super::live_view;
use crate::rune_backup_live::test_support::{
    a11y_contains_key, a11y_contains_text, a11y_count_role, a11y_focused_key, find_a11y_node,
    signed_out_state,
};
use crate::rune_backup_live::{LivePage, LiveState};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

fn production_runtime(state: LiveState, size: (f32, f32)) -> Runtime<LiveState> {
    let mut runtime = Runtime::new(state, live_view, size, Env::new(false, 1.0));
    runtime.frame(0.0);
    runtime
}

fn synthetic_signed_in_state() -> LiveState {
    signed_in_state(Some("Synthetic backup account"))
}

/// A signed-in state whose profile name is either already loaded (`Some`) or
/// still in flight (`None`), without a profile picture in both cases.
fn signed_in_state(name: Option<&str>) -> LiveState {
    let mut state = signed_out_state();
    state.config.session_id = "synthetic-session".to_owned();
    state.config.cookie_x_acb = "synthetic-cookie".to_owned();
    state.account_identity.name = name.map(str::to_owned);
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
    assert!(
        runtime.find("sidebar-brand-logo").is_some(),
        "the brand row carries the app logo next to the title"
    );
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
            && !a11y_contains_key(navigation, "sidebar-account")
            && !a11y_contains_key(&accessibility, "sidebar-brand-logo"),
        "brand, logo and account stay outside the navigation landmark: {accessibility}"
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
    for key in ["sidebar-nav-backup", "sidebar-nav-settings"] {
        let icon = live_view_element(runtime, &format!("{key}-icon"));
        assert_eq!(
            icon.text, None,
            "{key} draws the raster icon instead of a Unicode glyph"
        );
        assert_eq!(
            runtime.find(&format!("{key}-active")),
            None,
            "the old 3-px marker is gone; the active row is marked by its fill"
        );
        assert!(
            !a11y_contains_key(&accessibility, &format!("{key}-icon")),
            "{key} icon stays decorative in the AccessKit tree"
        );
    }
    for glyph in ["▤", "⚙"] {
        assert!(!a11y_contains_text(&accessibility, glyph), "retired glyph {glyph:?}");
    }
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
                "sidebar-brand-logo",
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
                "sidebar-brand-logo",
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
fn production_live_sidebar_brand_navigation_and_account_share_one_left_edge() {
    for size in [(760.0, 520.0), (1180.0, 820.0)] {
        // 0.0 is the headless/full-screen case; 67.0 is Chrome::titlebar_inset
        // with AppKit's default traffic-light placement.
        for inset in [0.0, 67.0] {
            let mut runtime = production_runtime(signed_out_state(), size);
            if inset > 0.0 {
                runtime.set_titlebar_inset(inset);
                for _ in 0..4 {
                    runtime.frame(1.0 / 60.0);
                }
            }

            let logo_left = live_view_element(&runtime, "sidebar-brand-logo").rect[0];
            for key in [
                "sidebar-nav-backup-icon",
                "sidebar-nav-settings-icon",
                "sidebar-account-heading",
            ] {
                let left = live_view_element(&runtime, key).rect[0];
                assert!(
                    (left - logo_left).abs() <= 2.0,
                    "{key} left edge {left} does not share the brand logo's {logo_left} \
                     at {size:?} with titlebar inset {inset}"
                );
            }
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

/// Vertical geometry of the bottom-anchored account block. Every line the
/// profile details gain grows the block upwards, moving the avatar, the
/// heading and the divider above the marketplace.
fn account_block_geometry(runtime: &Runtime<LiveState>) -> Vec<(&'static str, [f32; 4])> {
    [
        "sidebar-account-footer",
        "sidebar-account-heading",
        "account-identity",
        "account-details",
        "sidebar-marketplace",
        "sign-out",
    ]
    .into_iter()
    .map(|key| (key, live_view_element(runtime, key).rect))
    .collect()
}

fn assert_same_account_block_geometry(
    pending: &Runtime<LiveState>,
    loaded: &Runtime<LiveState>,
    context: &str,
) {
    for ((key, before), (_, after)) in account_block_geometry(pending)
        .into_iter()
        .zip(account_block_geometry(loaded))
    {
        assert!(
            (before[1] - after[1]).abs() <= 0.5 && (before[3] - after[3]).abs() <= 0.5,
            "{context}: {key} moved from {before:?} to {after:?}"
        );
    }
}

/// A decoded 2×2 avatar, standing in for the profile picture that arrives on
/// the image worker after the name.
fn synthetic_avatar() -> rune::Image {
    let waker: rune::Waker = std::sync::Arc::new(|| {});
    let cache = rune::ImageCache::new(waker);
    let image = cache.rgba(2, 2, [0x40u8, 0x80, 0xc0, 0xff].repeat(4));
    image.wait().expect("synthetic avatar decodes");
    image
}

/// The identity is fetched after the first frame. The state before its name
/// arrives renders the same two-line structure with a localized placeholder,
/// so the bottom-anchored block never changes height once the name lands —
/// not for a short name, not for one that would wrap.
#[test]
fn production_sidebar_account_block_does_not_jump_when_the_identity_arrives() {
    for size in [(1180.0, 820.0), (760.0, 520.0)] {
        let pending = production_runtime(signed_in_state(None), size);
        assert!(
            pending.find("account-avatar-placeholder").is_some(),
            "the pending state draws the avatar placeholder"
        );
        assert!(
            pending.find("sign-out").is_some(),
            "signing out stays available while the identity loads"
        );
        let loading = live_view_element(&pending, "account-name");
        assert_eq!(
            loading.text.as_deref(),
            Some(crate::i18n::tr("Wird geladen …")),
            "the pending name line is the localized loading placeholder, not a fake account name"
        );

        for name in [
            "Anna",
            "Synthetic backup account",
            "Synthetic backup account with a considerably longer display name",
        ] {
            let loaded = production_runtime(signed_in_state(Some(name)), size);
            let context = format!("{name:?} at {size:?}");
            assert_same_account_block_geometry(&pending, &loaded, &context);

            let placeholder = live_view_element(&pending, "account-avatar-placeholder").rect;
            let loaded_avatar = live_view_element(&loaded, "account-avatar-placeholder").rect;
            assert_eq!(
                placeholder, loaded_avatar,
                "{context}: the avatar moved when the name arrived"
            );

            assert_eq!(
                live_view_element(&loaded, "account-name").text.as_deref(),
                Some(name),
                "{context}: the accessibility text keeps the whole name"
            );
        }
    }
}

/// Placeholder and decoded picture share one box and one corner radius, so the
/// swap of the pixels cannot move or reshape anything either.
#[test]
fn production_sidebar_avatar_swap_keeps_the_same_size_and_radius() {
    let size = (1180.0, 820.0);
    let placeholder = production_runtime(signed_in_state(Some("Anna")), size);

    let mut state = signed_in_state(Some("Anna"));
    state.account_identity.image = Some(synthetic_avatar());
    let picture = production_runtime(state, size);
    assert!(
        picture.find("account-avatar").is_some(),
        "the ready image replaces the placeholder"
    );

    let before = live_view_element(&placeholder, "account-avatar-placeholder");
    let after = live_view_element(&picture, "account-avatar");
    assert_eq!(
        before.rect, after.rect,
        "placeholder and picture occupy the same box"
    );
    assert_eq!(
        before.radius, after.radius,
        "placeholder and picture clip to the same radius"
    );
    let [_, _, width, height] = after.rect;
    assert!(width > 0.0 && (width - height).abs() <= 0.5);
    assert!(
        (after.radius - width / 2.0).abs() <= 0.5,
        "the avatar is a circle, radius {} of a {} pt box",
        after.radius,
        width
    );
    assert_same_account_block_geometry(&placeholder, &picture, "picture swap");
}
