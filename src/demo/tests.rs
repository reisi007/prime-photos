//! Unit tests for the demo command line and the scenario table. The fixture
//! invariants live in `fixture_tests.rs` (the ratchet keeps every file small).

use super::*;
use std::path::PathBuf;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

#[test]
fn no_arguments_means_the_normal_app() {
    assert_eq!(parse_args(&[]), Ok(Mode::Off));
}

#[test]
fn demo_without_a_value_is_the_mixed_scenario() {
    assert_eq!(
        parse_args(&args(&["--demo"])),
        Ok(Mode::Demo(Scenario::Mixed))
    );
}

#[test]
fn demo_accepts_both_attached_and_separate_values() {
    assert_eq!(
        parse_args(&args(&["--demo=uploading"])),
        Ok(Mode::Demo(Scenario::Uploading))
    );
    assert_eq!(
        parse_args(&args(&["--demo", "all-done"])),
        Ok(Mode::Demo(Scenario::AllDone))
    );
    // Case-insensitive: a shell completion or a hand-typed id works.
    assert_eq!(
        parse_args(&args(&["--demo=Signed-Out"])),
        Ok(Mode::Demo(Scenario::SignedOut))
    );
    // `--demo` followed by another flag must not consume that flag.
    assert_eq!(
        parse_args(&args(&["--demo", "--screenshot=/tmp/x.png"])),
        Ok(Mode::Screenshot {
            scenario: Scenario::Mixed,
            path: PathBuf::from("/tmp/x.png"),
        })
    );
}

#[test]
fn screenshot_implies_the_demo_and_combines_with_a_scenario() {
    assert_eq!(
        parse_args(&args(&["--screenshot", "/tmp/shot.png"])),
        Ok(Mode::Screenshot {
            scenario: Scenario::Mixed,
            path: PathBuf::from("/tmp/shot.png"),
        })
    );
    assert_eq!(
        parse_args(&args(&["--demo=failures", "--screenshot", "/tmp/f.png"])),
        Ok(Mode::Screenshot {
            scenario: Scenario::Failures,
            path: PathBuf::from("/tmp/f.png"),
        })
    );
    // Order must not matter, and the attached form works too.
    assert_eq!(
        parse_args(&args(&["--screenshot=/tmp/g.png", "--demo=paused"])),
        Ok(Mode::Screenshot {
            scenario: Scenario::Paused,
            path: PathBuf::from("/tmp/g.png"),
        })
    );
}

#[test]
fn a_relative_screenshot_path_is_rejected() {
    let err = parse_args(&args(&["--screenshot", "shot.png"])).expect_err("relative path");
    assert!(
        err.contains("absolut"),
        "the error must say the path has to be absolute: {err}"
    );
    let err = parse_args(&args(&["--screenshot=./shot.png"])).expect_err("relative path");
    assert!(err.contains("absolut"));
}

#[test]
fn unknown_arguments_and_scenarios_report_the_usage() {
    // In demo mode a stray argument is a typo and must be reported.
    let err = parse_args(&args(&["--demo", "--wat"])).expect_err("unknown argument");
    assert!(err.contains("Unbekanntes Argument"));
    assert!(err.contains("Verwendung"), "usage missing: {err}");

    // Our own namespace is never silently ignored, not even for a normal start.
    let err = parse_args(&args(&["--demoo"])).expect_err("malformed demo flag");
    assert!(err.contains("Unbekanntes Argument"));

    let err = parse_args(&args(&["--demo=does-not-exist"])).expect_err("unknown scenario");
    assert!(err.contains("Unbekanntes Szenario"));
    assert!(err.contains("Verwendung"));

    // A missing screenshot value is a usage error too, not a panic.
    let err = parse_args(&args(&["--screenshot"])).expect_err("missing path");
    assert!(err.contains("erwartet einen Pfad"));
    assert!(err.contains("Verwendung"));
}

#[test]
fn launcher_arguments_still_start_the_normal_app() {
    // macOS injects `-psn_0_…` on a fresh bundle start; debuggers/IDEs inject
    // their own flags. A normal start must ignore everything that is not ours.
    assert_eq!(parse_args(&args(&["-psn_0_1234567"])), Ok(Mode::Off));
    assert_eq!(
        parse_args(&args(&["-NSDocumentRevisionsDebugMode", "YES"])),
        Ok(Mode::Off)
    );
    assert_eq!(parse_args(&args(&["--wat"])), Ok(Mode::Off));
}

#[test]
fn demo_list_prints_the_scenarios_and_rejects_extra_arguments() {
    assert_eq!(parse_args(&args(&["--demo-list"])), Ok(Mode::List));

    // Anything extra is an error instead of a silent half-list.
    let err = parse_args(&args(&["--demo-list", "--bogus"])).expect_err("extra argument");
    assert!(
        err.contains("--demo-list") && err.contains("Verwendung"),
        "{err}"
    );
    let err = parse_args(&args(&["--demo-list", "--demo=mixed"])).expect_err("combination");
    assert!(err.contains("--demo-list"), "{err}");
    let err =
        parse_args(&args(&["--screenshot=/tmp/x.png", "--demo-list"])).expect_err("combination");
    assert!(err.contains("--demo-list"), "{err}");
    let err = parse_args(&args(&["--demo-list=yes"])).expect_err("malformed list flag");
    assert!(err.contains("Unbekanntes Argument"), "{err}");
}

#[test]
fn every_explicit_mode_is_recognised_by_mode_predicates() {
    assert!(!Mode::Off.is_demo());
    assert!(!Mode::List.is_demo());
    assert_eq!(Mode::List.into_demo(), None);
    assert!(Mode::Demo(Scenario::Mixed).is_demo());
    assert!(
        Mode::Screenshot {
            scenario: Scenario::Mixed,
            path: PathBuf::from("/tmp/x.png"),
        }
        .is_demo()
    );
}

#[test]
fn every_scenario_round_trips_through_its_id() {
    let mut seen = Vec::new();
    for scenario in Scenario::ALL {
        assert_eq!(Scenario::parse(scenario.id()), Some(scenario));
        assert!(!scenario.description().is_empty(), "{}", scenario.id());
        assert!(
            !seen.contains(&scenario.id()),
            "duplicate id {}",
            scenario.id()
        );
        seen.push(scenario.id());
    }
    assert_eq!(Scenario::ALL.len(), 9);
    assert_eq!(Scenario::parse("  MIXED "), Some(Scenario::Mixed));
    assert_eq!(Scenario::parse("nope"), None);
}
