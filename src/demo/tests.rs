//! Unit tests for the demo command line and the scenario table. The fixture
//! invariants live in `fixture_tests.rs` (the ratchet keeps every file small).

use super::*;
use std::path::PathBuf;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

/// The default interactive demo mode for `scenario`.
fn demo(scenario: Scenario) -> Mode {
    Mode::Demo {
        scenario,
        size: DemoSize::INTERACTIVE,
        language: None,
    }
}

/// A screenshot mode with the default screenshot size.
fn shot(scenario: Scenario, path: &str) -> Mode {
    Mode::Screenshot {
        scenario,
        path: PathBuf::from(path),
        size: DemoSize::SCREENSHOT,
        language: None,
    }
}

#[test]
fn no_arguments_means_the_normal_app() {
    assert_eq!(parse_args(&[]), Ok(Mode::Off));
}

#[test]
fn demo_without_a_value_is_the_mixed_scenario() {
    assert_eq!(parse_args(&args(&["--demo"])), Ok(demo(Scenario::Mixed)));
}

#[test]
fn demo_accepts_both_attached_and_separate_values() {
    assert_eq!(
        parse_args(&args(&["--demo=uploading"])),
        Ok(demo(Scenario::Uploading))
    );
    assert_eq!(
        parse_args(&args(&["--demo", "all-done"])),
        Ok(demo(Scenario::AllDone))
    );
    // Case-insensitive: a shell completion or a hand-typed id works.
    assert_eq!(
        parse_args(&args(&["--demo=Signed-Out"])),
        Ok(demo(Scenario::SignedOut))
    );
    // `--demo` followed by another flag must not consume that flag.
    assert_eq!(
        parse_args(&args(&["--demo", "--screenshot=/tmp/x.png"])),
        Ok(shot(Scenario::Mixed, "/tmp/x.png"))
    );
}

#[test]
fn screenshot_implies_the_demo_and_combines_with_a_scenario() {
    assert_eq!(
        parse_args(&args(&["--screenshot", "/tmp/shot.png"])),
        Ok(shot(Scenario::Mixed, "/tmp/shot.png"))
    );
    assert_eq!(
        parse_args(&args(&["--demo=failures", "--screenshot", "/tmp/f.png"])),
        Ok(shot(Scenario::Failures, "/tmp/f.png"))
    );
    // Order must not matter, and the attached form works too.
    assert_eq!(
        parse_args(&args(&["--screenshot=/tmp/g.png", "--demo=paused"])),
        Ok(shot(Scenario::Paused, "/tmp/g.png"))
    );
}

#[test]
fn size_accepts_both_forms_and_both_defaults() {
    // The screenshot default stays 1400x900, the interactive one 1200x800.
    assert_eq!(DemoSize::SCREENSHOT.points(), [1400.0, 900.0]);
    assert_eq!(DemoSize::INTERACTIVE.points(), [1200.0, 800.0]);

    let size = DemoSize {
        width: 760,
        height: 520,
    };
    assert_eq!(
        parse_args(&args(&["--demo=mixed", "--size", "760x520"])),
        Ok(Mode::Demo {
            scenario: Scenario::Mixed,
            size,
            language: None,
        })
    );
    // Attached form, uppercase separator and every combination order.
    assert_eq!(
        parse_args(&args(&["--size=760X520", "--demo=failures"])),
        Ok(Mode::Demo {
            scenario: Scenario::Failures,
            size,
            language: None,
        })
    );
    assert_eq!(
        parse_args(&args(&[
            "--size",
            "1000x700",
            "--demo",
            "mixed",
            "--screenshot=/tmp/s.png",
        ])),
        Ok(Mode::Screenshot {
            scenario: Scenario::Mixed,
            path: PathBuf::from("/tmp/s.png"),
            size: DemoSize {
                width: 1000,
                height: 700,
            },
            language: None,
        })
    );
    // A `--demo` followed by `--size` must not consume the flag as a scenario.
    assert_eq!(
        parse_args(&args(&["--demo", "--size", "800x600"])),
        Ok(Mode::Demo {
            scenario: Scenario::Mixed,
            size: DemoSize {
                width: 800,
                height: 600,
            },
            language: None,
        })
    );
}

#[test]
fn demo_language_is_optional_and_accepts_german_or_english() {
    assert_eq!(
        parse_args(&args(&["--demo", "--demo-language=en"])),
        Ok(Mode::Demo {
            scenario: Scenario::Mixed,
            size: DemoSize::INTERACTIVE,
            language: Some(crate::i18n::Lang::En),
        })
    );
    assert_eq!(
        parse_args(&args(&[
            "--demo-language",
            "DE",
            "--screenshot=/tmp/de.png",
        ])),
        Ok(Mode::Screenshot {
            scenario: Scenario::Mixed,
            path: PathBuf::from("/tmp/de.png"),
            size: DemoSize::SCREENSHOT,
            language: Some(crate::i18n::Lang::De),
        })
    );

    for invalid in ["fr", "en-US", ""] {
        let err = parse_args(&args(&["--demo", "--demo-language", invalid]))
            .expect_err("unsupported demo language");
        assert!(err.contains("erwartet de oder en"), "{err}");
    }
    let err = parse_args(&args(&["--demo-language=en"]))
        .expect_err("language without demo or screenshot");
    assert!(err.contains("nur mit --demo oder --screenshot"), "{err}");
    let err = parse_args(&args(&["--demo-list", "--demo-language=en"]))
        .expect_err("list cannot select a demo language");
    assert!(err.contains("--demo-list"), "{err}");
}

#[test]
fn size_is_validated_and_only_valid_for_demo_runs() {
    for invalid in ["760", "760x", "x520", "axb", "760x520x1", "760,520"] {
        let err = parse_args(&args(&["--demo", "--size", invalid]))
            .expect_err("invalid size must be rejected");
        assert!(
            err.contains("Ungültige Fenstergröße") && err.contains("Verwendung"),
            "{invalid}: {err}"
        );
    }
    // Below the app minimum is a usage error, not a silently clamped window.
    for too_small in ["759x520", "760x519", "0x0"] {
        let err = parse_args(&args(&["--demo", "--size", too_small]))
            .expect_err("size below the minimum must be rejected");
        assert!(err.contains("zu klein"), "{too_small}: {err}");
    }
    // Above the supported maximum is a typo, not a window-manager problem.
    for too_large in ["8193x520", "760x8193", "10000x10000"] {
        let err = parse_args(&args(&["--demo", "--size", too_large]))
            .expect_err("size above the maximum must be rejected");
        assert!(
            err.contains("zu groß") && err.contains("Maximum 8192x8192"),
            "{too_large}: {err}"
        );
    }
    // The boundary itself is accepted (no window is opened in a parse test).
    assert_eq!(
        parse_args(&args(&["--demo", "--size", "8192x8192"])),
        Ok(Mode::Demo {
            scenario: Scenario::Mixed,
            size: DemoSize {
                width: 8192,
                height: 8192,
            },
            language: None,
        })
    );
    // A missing value is a usage error too, not a panic.
    let err = parse_args(&args(&["--demo", "--size"])).expect_err("missing size");
    assert!(err.contains("erwartet WxH"), "{err}");
    // Malformed flags stay in our namespace and are reported.
    let err = parse_args(&args(&["--demo", "--sizes=760x520"])).expect_err("typo");
    assert!(err.contains("Unbekanntes Argument"), "{err}");
    // A normal start has no demo window: `--size` is not a real-app option.
    let err = parse_args(&args(&["--size", "760x520"])).expect_err("size without demo");
    assert!(err.contains("nur mit --demo oder --screenshot"), "{err}");
    // `--demo-list` takes no size.
    let err = parse_args(&args(&["--demo-list", "--size=760x520"])).expect_err("list + size");
    assert!(err.contains("--demo-list"), "{err}");
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
    assert!(demo(Scenario::Mixed).is_demo());
    assert!(shot(Scenario::Mixed, "/tmp/x.png").is_demo());
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
    assert_eq!(Scenario::ALL.len(), 10);
    assert!(seen.contains(&"settings-bandwidth"));
    assert_eq!(Scenario::parse("  MIXED "), Some(Scenario::Mixed));
    assert_eq!(
        Scenario::parse("settings-bandwidth"),
        Some(Scenario::SettingsBandwidth)
    );
    assert_eq!(Scenario::parse("nope"), None);
}
