use super::*;
use rune::{Env, Runtime};

fn runtime() -> Runtime<LoginState> {
    let mut runtime = Runtime::new(LoginState::default(), view, WINDOW_SIZE, Env::default());
    runtime.frame(0.0);
    runtime
}

fn click(runtime: &mut Runtime<LoginState>, key: &str) {
    let (x, y) = runtime
        .find(key)
        .unwrap_or_else(|| panic!("missing Rune login element {key:?}"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

#[test]
fn marketplace_selector_lists_countries_and_updates_selection() {
    let mut runtime = runtime();

    assert_eq!(runtime.state.country, "de");
    for key in [
        "login-screen",
        "login-title",
        "login-subtitle",
        "marketplace-label",
        "marketplace-selector",
        "marketplace-value",
        "sign-in-cta",
    ] {
        assert!(runtime.find(key).is_some(), "missing keyed node {key:?}");
    }

    click(&mut runtime, "marketplace-selector");
    if rune::popup_menu_supported() {
        let menu = runtime
            .popup_menu()
            .expect("marketplace picker should open a Rune popup menu");
        for &(country, _) in COUNTRIES {
            assert!(
                menu.find(country).is_some(),
                "marketplace picker is missing {country:?}"
            );
        }
        assert_eq!(
            menu.find("uk").expect("UK option").title(),
            countries::country_label("uk")
        );

        runtime.resolve_popup(Some("uk")).unwrap();
        runtime.frame(1.0 / 60.0);
    } else {
        assert!(runtime.find("marketplace-options").is_some());
        for &(country, _) in COUNTRIES {
            assert!(
                runtime
                    .find(&format!("marketplace-option-{country}"))
                    .is_some(),
                "inline marketplace selector is missing {country:?}"
            );
        }
        click(&mut runtime, "marketplace-option-uk");
    }

    assert_eq!(runtime.state.country, "uk");
    assert!(runtime.find("marketplace-value").is_some());
}

#[test]
fn sign_in_cta_only_displays_the_local_fixture_error() {
    let mut runtime = runtime();
    assert_eq!(runtime.state.error, None);
    assert!(runtime.find("fixture-error").is_none());

    click(&mut runtime, "sign-in-cta");

    assert_eq!(runtime.state.error, Some(FIXTURE_ERROR));
    assert!(runtime.find("fixture-error").is_some());
    assert!(runtime.find("sign-in-cta").is_some());
    assert!(FIXTURE_ERROR.contains("Fixture-only"));
}

#[test]
fn error_scene_is_deterministic() {
    let mut state = LoginState::default();
    error_scene(&mut state);
    let mut runtime = Runtime::new(state, view, WINDOW_SIZE, Env::default());
    runtime.frame(0.0);

    assert_eq!(runtime.state.error, Some(FIXTURE_ERROR));
    assert!(runtime.find("fixture-error").is_some());
}

#[test]
fn fixture_run_rejects_interactive_invocation() {
    assert!(run(Vec::new()).is_err());
}

#[test]
fn fixture_requires_a_parser_recognized_headless_action() {
    assert!(!has_headless_action(&[
        "--rune-login-fixture".to_owned(),
    ]));
    assert!(!has_headless_action(&[
        "--scene".to_owned(),
        "--snap".to_owned(),
    ]));
    assert!(!has_headless_action(&["--snap=login.png".to_owned()]));
    assert!(!has_headless_action(&[
        "--native".to_owned(),
        "--script".to_owned(),
        "fixture.json".to_owned(),
    ]));

    for action in ["--snap", "--tree", "--script"] {
        assert!(has_headless_action(&[
            action.to_owned(),
            "fixture-output".to_owned(),
        ]));
    }
}
