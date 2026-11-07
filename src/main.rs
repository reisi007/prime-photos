mod prime_photos;
mod about;
mod app_activation;
mod app_menu;
mod auth;
mod config;
mod countries;
mod dedup;
#[cfg(debug_assertions)]
mod demo;
mod destination;
mod folders;
mod i18n;
mod icons;
mod login;
mod logging;
mod notify;
mod platform;
mod text;
mod theme;
mod tray;
mod upload;
mod urls;
mod app;
mod backup;
mod backup_config;
mod settings;
mod single_instance;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_settings;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_login;

const RUNE_LOGIN_FIXTURE_FLAG: &str = "--rune-login-fixture";
const RUNE_HEADLESS_FLAGS: [&str; 3] = ["--snap", "--tree", "--script"];

use app::PrimePhotoApp;
use eframe::egui;

/// Window/dock icon, from the build-time 1024-px raster of `assets/logo.svg`
/// (see `build.rs`). The asset is compile-time mandatory, so a decode failure
/// is a hard failure — never a silently icon-less window.
fn window_icon() -> egui::IconData {
    const LOGO_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo.png"));
    let image = image::load_from_memory(LOGO_PNG)
        .expect("Prime Photos: eingebettetes Logo konnte nicht dekodiert werden")
        .to_rgba8();
    let (width, height) = (image.width(), image.height());
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result<()> {
    logging::init();

    // Developer-only demo/screenshot mode (issue #29). Parsed before anything
    // else so `--demo-list` and argument errors never touch the app's state.
    // `args_os` + a UTF-8 filter instead of `std::env::args()`: the latter
    // panics on a non-UTF-8 argument, and a launcher may hand us one. Such an
    // argument cannot be one of our flags, so dropping it is the same as "not
    // ours".
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .filter_map(|arg| arg.into_string().ok())
        .collect();

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    if let Some(arg) = args.iter().find(|arg| is_malformed_rune_fixture_arg(arg)) {
        eprintln!(
            "Malformed Rune fixture/tooling argument: {arg}. Use --snap FILE, --tree FILE, or --script FILE."
        );
        std::process::exit(2);
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    if args.iter().any(|arg| arg == RUNE_LOGIN_FIXTURE_FLAG) {
        let rune_args = strip_rune_login_fixture_arg(args.clone());
        if !rune_login::has_headless_action(&rune_args) {
            eprintln!(
                "Usage: prime-photos {RUNE_LOGIN_FIXTURE_FLAG} (--snap FILE | --tree FILE | --script FILE)"
            );
            std::process::exit(2);
        }
        if let Err(message) = rune_login::run(rune_args) {
            eprintln!("Prime Photos Rune login fixture: {message}");
            std::process::exit(1);
        }
        return Ok(());
    }

    #[cfg(all(debug_assertions, not(feature = "rune-ui-spike")))]
    if let Some(arg) = args.iter().find(|arg| is_rune_fixture_or_tooling_arg(arg)) {
        eprintln!(
            "Rune fixture/tooling options are unavailable without a debug build with `--features rune-ui-spike`: {arg}"
        );
        std::process::exit(2);
    }

    // Rune remains a headless-only migration fixture; it is not an
    // interactive preview or an application start mode.
    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    match rune_settings_fixture_args(args.clone()) {
        Ok(Some(rune_args)) => {
            if let Err(message) = rune_settings::run(rune_args) {
                eprintln!("Prime Photos Rune spike: {message}");
                std::process::exit(1);
            }
            return Ok(());
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("Prime Photos Rune spike: {message}");
            std::process::exit(2);
        }
    }

    #[cfg(debug_assertions)]
    let mode = match demo::parse_args(&args) {
        Ok(mode) => mode,
        Err(message) => {
            // Usage errors are a developer-facing start failure: stderr and a
            // nonzero exit, never a half-started window.
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    #[cfg(debug_assertions)]
    if mode == demo::Mode::List {
        for scenario in demo::Scenario::ALL {
            println!("{:<18} {}", scenario.id(), scenario.description());
        }
        return Ok(());
    }

    #[cfg(debug_assertions)]
    if mode.is_demo() {
        return run_eframe_demo(mode);
    }

    #[cfg(not(debug_assertions))]
    reject_release_demo_args(&args);

    // Temporary legacy production host: it remains the sole normal user UI
    // until Rune covers login, backup, settings, dialogs, and the app shell.
    // Do not add eframe-only user flows; remove this host after the direct Rune
    // replacement passes the gates in docs/features/rune-migration.md.
    if !single_instance::acquire() {
        log::info!("Prime Photos läuft bereits — Fenster der laufenden Instanz angefordert");
        return Ok(());
    }
    notify::init();
    app_activation::install_launch_diagnostic();

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1000.0, 700.0])
        .with_min_inner_size([760.0, 520.0])
        .with_icon(window_icon())
        .with_visible(false);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Prime Photos",
        options,
        Box::new(|cc| Ok(Box::new(PrimePhotoApp::new(cc)))),
    )
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn has_rune_headless_args(args: &[String]) -> bool {
    let args = rune::tooling::Args::parse_from(args.to_vec());
    args.headless() && (args.snap.is_some() || args.tree.is_some() || args.script.is_some())
}

fn rune_headless_flag(arg: &str) -> Option<&'static str> {
    RUNE_HEADLESS_FLAGS
        .iter()
        .copied()
        .find(|flag| arg.starts_with(flag))
}

fn is_rune_fixture_or_tooling_arg(arg: &str) -> bool {
    arg.starts_with(RUNE_LOGIN_FIXTURE_FLAG) || rune_headless_flag(arg).is_some()
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn is_malformed_rune_fixture_arg(arg: &str) -> bool {
    is_rune_fixture_or_tooling_arg(arg)
        && ((arg.starts_with(RUNE_LOGIN_FIXTURE_FLAG) && arg != RUNE_LOGIN_FIXTURE_FLAG)
            || rune_headless_flag(arg).is_some_and(|flag| arg != flag))
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn validate_rune_headless_options(args: &[String]) -> Result<bool, &'static str> {
    let mut found_option = false;
    let mut i = 0;

    while i < args.len() {
        if let Some(flag) = rune_headless_flag(&args[i]) {
            if args[i] != flag {
                return Err("malformed Rune tooling option");
            }

            found_option = true;
            let Some(value) = args.get(i + 1) else {
                return Err("Rune tooling option requires a file path");
            };
            if value.is_empty() || value.starts_with("--") {
                return Err("Rune tooling option requires a file path");
            }
            i += 1;
        }
        i += 1;
    }

    Ok(found_option)
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn rune_settings_fixture_args(args: Vec<String>) -> Result<Option<Vec<String>>, &'static str> {
    if args.iter().any(|arg| is_malformed_rune_fixture_arg(arg)) {
        return Err("malformed Rune fixture/tooling option");
    }

    if !validate_rune_headless_options(&args)? {
        return Ok(None);
    }
    if !has_rune_headless_args(&args) {
        return Err("malformed Rune headless action");
    }

    let filtered_args = filter_launcher_args(args);
    if has_rune_headless_args(&filtered_args) {
        Ok(Some(filtered_args))
    } else {
        Err("launcher argument filtering removed the Rune headless action")
    }
}

/// macOS launchers may inject process-serialization arguments.
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn filter_launcher_args(args: Vec<String>) -> Vec<String> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg.starts_with("-psn_") {
            continue;
        }
        if arg == "-NSDocumentRevisionsDebugMode" {
            let _ = args.next();
            continue;
        }
        filtered.push(arg);
    }
    filtered
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn strip_rune_login_fixture_arg(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .filter(|arg| arg != RUNE_LOGIN_FIXTURE_FLAG)
        .collect()
}

#[cfg(debug_assertions)]
fn run_eframe_demo(mode: demo::Mode) -> eframe::Result<()> {
    log::info!("Prime Photos startet im Debug-Demo-Modus (keine echten Daten)");

    // Observe AppKit's first launch notification before eframe runs its launch
    // path, without taking over the application delegate or Apple-event handler.
    app_activation::install_launch_diagnostic();

    // The demo window has a fixed size so a screenshot does not depend on the
    // developer's screen; `--size WxH` picks a different one (Issue #30),
    // defaulting to the historical values. The normal app keeps its preferred
    // start size.
    let inner_size = match &mode {
        demo::Mode::Screenshot { size, .. } | demo::Mode::Demo { size, .. } => size.points(),
        // `List` already returned above; keep the normal start size for it so
        // the match stays exhaustive without a wildcard.
        demo::Mode::Off | demo::Mode::List => [1000.0, 700.0],
    };
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(inner_size)
        .with_min_inner_size([760.0, 520.0])
        .with_icon(window_icon());
    // A demo run is meant to be looked at (and captured) — no hidden start.
    let viewport = viewport.with_visible(true);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Prime Photos",
        options,
        Box::new(|cc| Ok(Box::new(PrimePhotoApp::new_with_mode(cc, mode)))),
    )
}

#[cfg(not(debug_assertions))]
fn reject_release_demo_args(args: &[String]) {
    let Some(arg) = args.iter().find(|arg| {
        arg.starts_with("--demo")
            || arg.starts_with("--screenshot")
            || arg.starts_with("--size")
            || is_rune_fixture_or_tooling_arg(arg)
    }) else {
        return;
    };

    if is_rune_fixture_or_tooling_arg(arg) {
        eprintln!("Rune fixture/tooling options are unavailable in release builds: {arg}");
        std::process::exit(2);
    }

    i18n::set_lang(i18n::system_lang());
    eprintln!(
        "{}",
        i18n::tr1(
            "Demo- und Screenshot-Argumente sind in Release-Builds nicht verfügbar: {}",
            arg
        )
    );
    std::process::exit(2);
}

#[cfg(all(test, debug_assertions, feature = "rune-ui-spike"))]
mod rune_login_routing_tests {
    use super::{RUNE_LOGIN_FIXTURE_FLAG, strip_rune_login_fixture_arg};

    #[test]
    fn fixture_routing_strips_only_its_own_marker() {
        let args = vec![
            RUNE_LOGIN_FIXTURE_FLAG.to_owned(),
            "--snap".to_owned(),
            "login.png".to_owned(),
            "--scene".to_owned(),
            "error".to_owned(),
        ];

        assert_eq!(
            strip_rune_login_fixture_arg(args),
            ["--snap", "login.png", "--scene", "error"]
        );
    }
}

#[cfg(all(test, debug_assertions, feature = "rune-ui-spike"))]
mod rune_settings_routing_tests {
    use super::{has_rune_headless_args, is_malformed_rune_fixture_arg, rune_settings_fixture_args};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn settings_routes_for_each_parsed_headless_action() {
        for action in ["--snap", "--tree", "--script"] {
            assert!(
                has_rune_headless_args(&args(&[action, "fixture-output"])),
                "{action} should route to the Settings fixture"
            );
        }
    }

    #[test]
    fn settings_does_not_route_for_unparsed_or_non_headless_actions() {
        for values in [&[][..], &["--scene", "login"][..], &["--native"][..]] {
            assert!(
                !has_rune_headless_args(&args(values)),
                "{values:?} should not route to the Settings fixture"
            );
            assert_eq!(
                rune_settings_fixture_args(args(values)),
                Ok(None),
                "{values:?} should remain on the ordinary app path"
            );
        }

        assert_eq!(
            rune_settings_fixture_args(args(&["--native", "--script", "fixture-output"])),
            Err("malformed Rune headless action")
        );
        assert_eq!(
            rune_settings_fixture_args(args(&["--snap"])),
            Err("Rune tooling option requires a file path")
        );
    }

    #[test]
    fn filtered_launcher_args_must_retain_the_headless_action() {
        let injected_args = args(&["-NSDocumentRevisionsDebugMode", "--snap", "out.png"]);
        assert!(has_rune_headless_args(&injected_args));
        assert_eq!(
            rune_settings_fixture_args(injected_args),
            Err("launcher argument filtering removed the Rune headless action")
        );

        assert_eq!(
            rune_settings_fixture_args(args(&["--snap", "out.png"])),
            Ok(Some(args(&["--snap", "out.png"])))
        );
        assert_eq!(
            rune_settings_fixture_args(args(&["-psn_0_123", "--snap", "out.png"])),
            Ok(Some(args(&["--snap", "out.png"])))
        );
    }

    #[test]
    fn malformed_rune_fixture_options_are_rejected_instead_of_falling_through() {
        assert_eq!(
            rune_settings_fixture_args(args(&["--snap=fixture-output"])),
            Err("malformed Rune fixture/tooling option")
        );
        assert_eq!(
            rune_settings_fixture_args(args(&["--rune-login-fixture=login"])),
            Err("malformed Rune fixture/tooling option")
        );
        assert!(is_malformed_rune_fixture_arg("--rune-login-fixture=login"));
    }

    #[test]
    fn unrelated_arguments_do_not_route_to_the_settings_fixture() {
        assert_eq!(
            rune_settings_fixture_args(args(&["--foreign-launcher-arg", "value"])),
            Ok(None)
        );
    }
}
