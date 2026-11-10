mod prime_photos;
mod about;
mod app_activation;
mod app_menu;
mod auth;
mod config;
mod countries;
mod dedup;
#[cfg(all(debug_assertions, feature = "legacy-eframe"))]
mod demo;
mod destination;
mod folders;
mod i18n;
#[cfg(any(feature = "legacy-eframe", feature = "rune-backup"))]
mod icons;
mod login;
mod logging;
mod notify;
mod platform;
mod text;
#[cfg(feature = "legacy-eframe")]
mod theme;
#[cfg(feature = "rune-backup")]
mod rune_theme;
mod tray;
mod upload;
mod urls;
#[cfg(feature = "legacy-eframe")]
mod app;
mod backup;
#[cfg(feature = "legacy-eframe")]
mod backup_config;
#[cfg(feature = "legacy-eframe")]
mod settings;
mod single_instance;
#[cfg(feature = "rune-backup")]
mod rune_settings;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_login;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_backup;
#[cfg(feature = "rune-backup")]
mod rune_backup_live;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_production_login;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_wry_smoke;
mod rune_fixture;
mod wry_host;

#[cfg(feature = "legacy-eframe")]
use app::PrimePhotoApp;
#[cfg(feature = "legacy-eframe")]
use eframe::egui;
#[cfg(not(all(debug_assertions, feature = "rune-ui-spike")))]
use rune_fixture::is_rune_route_or_tooling_arg;
#[cfg(not(all(debug_assertions, feature = "rune-ui-spike")))]
use rune_fixture::RUNE_BACKUP_FLAG;
#[cfg(all(
    feature = "rune-backup",
    not(all(debug_assertions, feature = "rune-ui-spike"))
))]
use rune_fixture::rune_production_backup_route_args;

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
use rune_fixture::{
    RUNE_BACKUP_FIXTURE_FLAG, RUNE_LOGIN_FIXTURE_FLAG, RUNE_PRODUCTION_LOGIN_FLAG,
    RUNE_WRY_SMOKE_FLAG, RUNE_BACKUP_FLAG, RUNE_BACKUP_SMOKE_FLAG,
    is_malformed_rune_fixture_arg, rune_backup_fixture_args, rune_interactive_route_args,
    rune_login_fixture_args, rune_settings_fixture_args,
};

#[cfg(feature = "legacy-eframe")]
type MainResult = eframe::Result<()>;
#[cfg(not(feature = "legacy-eframe"))]
type MainResult = Result<(), std::convert::Infallible>;

/// Window/dock icon, from the build-time 1024-px raster of `assets/logo.svg`
/// (see `build.rs`). The asset is compile-time mandatory, so a decode failure
/// is a hard failure — never a silently icon-less window.
#[cfg(feature = "legacy-eframe")]
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

fn main() -> MainResult {
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
    if args.iter().any(|arg| arg == RUNE_LOGIN_FIXTURE_FLAG)
        && args.iter().any(|arg| arg == RUNE_BACKUP_FIXTURE_FLAG)
    {
        eprintln!("Rune fixture markers cannot be combined.");
        std::process::exit(2);
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    {
        let login_args = match rune_interactive_route_args(&args, RUNE_PRODUCTION_LOGIN_FLAG) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("Prime Photos Rune login route: {message}");
                std::process::exit(2);
            }
        };
        let smoke_args = match rune_interactive_route_args(&args, RUNE_WRY_SMOKE_FLAG) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("Prime Photos Wry smoke route: {message}");
                std::process::exit(2);
            }
        };
        let backup_args = match rune_interactive_route_args(&args, RUNE_BACKUP_FLAG) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("Prime Photos Rune backup route: {message}");
                std::process::exit(2);
            }
        };
        let backup_smoke_args = match rune_interactive_route_args(&args, RUNE_BACKUP_SMOKE_FLAG) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("Prime Photos Rune backup smoke route: {message}");
                std::process::exit(2);
            }
        };
        let selected_routes = [
            login_args.is_some(),
            smoke_args.is_some(),
            backup_args.is_some(),
            backup_smoke_args.is_some(),
        ]
        .into_iter()
        .filter(|selected| *selected)
        .count();
        if selected_routes > 1 {
            eprintln!("Rune interactive routes cannot be combined.");
            std::process::exit(2);
        }
        match (login_args, smoke_args, backup_args, backup_smoke_args) {
            (Some(route_args), None, None, None) => {
                if let Err(message) = rune_production_login::run(route_args) {
                    eprintln!("Prime Photos Rune login route: {message}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            (None, Some(route_args), None, None) => {
                if let Err(message) = rune_wry_smoke::run(route_args) {
                    eprintln!("Prime Photos Wry smoke route: {message}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            (None, None, Some(route_args), None) => {
                if let Err(message) = rune_backup_live::run_production(route_args) {
                    eprintln!("Prime Photos Rune backup route: {message}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            (None, None, None, Some(route_args)) => {
                if let Err(message) = rune_backup_live::run_smoke(route_args) {
                    eprintln!("Prime Photos Rune backup smoke route: {message}");
                    std::process::exit(1);
                }
                return Ok(());
            }
            (None, None, None, None) => {}
            _ => unreachable!("interactive route count was validated"),
        }
    }

    // The production backup host is independently opt-in and can be built in
    // release without enabling the debug-only migration fixtures and smoke
    // routes. The normal no-argument path below remains eframe.
    #[cfg(all(
        feature = "rune-backup",
        not(all(debug_assertions, feature = "rune-ui-spike"))
    ))]
    match rune_production_backup_route_args(&args) {
        Ok(Some(route_args)) => {
            if let Err(message) = rune_backup_live::run_production(route_args) {
                eprintln!("Prime Photos Rune backup route: {message}");
                std::process::exit(1);
            }
            return Ok(());
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("Prime Photos Rune backup route: {message}");
            std::process::exit(2);
        }
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    match rune_backup_fixture_args(args.clone()) {
        Ok(Some(rune_args)) => {
            if let Err(message) = rune_backup::run(rune_args) {
                eprintln!("Prime Photos Rune backup fixture: {message}");
                std::process::exit(1);
            }
            return Ok(());
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("Prime Photos Rune backup fixture: {message}");
            std::process::exit(2);
        }
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    match rune_login_fixture_args(args.clone()) {
        Ok(Some(rune_args)) => {
            if let Err(message) = rune_login::run(rune_args) {
                eprintln!("Prime Photos Rune login fixture: {message}");
                std::process::exit(1);
            }
            return Ok(());
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("Prime Photos Rune login fixture: {message}");
            std::process::exit(2);
        }
    }

    #[cfg(not(all(debug_assertions, feature = "rune-ui-spike")))]
    if let Some(arg) = args.iter().find(|arg| is_rune_route_or_tooling_arg(arg)) {
        if arg.as_str() == RUNE_BACKUP_FLAG
            || arg
                .strip_prefix(RUNE_BACKUP_FLAG)
                .is_some_and(|suffix| suffix.starts_with('='))
        {
            eprintln!("The Rune production backup route requires `--features rune-backup`: {arg}");
        } else {
            eprintln!(
                "Rune fixtures, smoke routes, login routes, and tooling require a debug build with `--features rune-ui-spike`: {arg}"
            );
        }
        std::process::exit(2);
    }

    // This remaining Rune Settings migration fixture is headless-only;
    // interactive login and Wry verification routes are dispatched above.
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

    #[cfg(all(debug_assertions, feature = "legacy-eframe"))]
    let mode = match demo::parse_args(&args) {
        Ok(mode) => mode,
        Err(message) => {
            // Usage errors are a developer-facing start failure: stderr and a
            // nonzero exit, never a half-started window.
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    #[cfg(all(debug_assertions, feature = "legacy-eframe"))]
    if mode == demo::Mode::List {
        for scenario in demo::Scenario::ALL {
            println!("{:<18} {}", scenario.id(), scenario.description());
        }
        return Ok(());
    }

    #[cfg(all(debug_assertions, feature = "legacy-eframe"))]
    if mode.is_demo() {
        return run_eframe_demo(mode);
    }

    #[cfg(not(debug_assertions))]
    reject_release_demo_args(&args);

    #[cfg(feature = "legacy-eframe")]
    {
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
    #[cfg(not(feature = "legacy-eframe"))]
    {
        eprintln!(
            "This build excludes the legacy eframe host; select an explicit Rune route or enable `legacy-eframe`."
        );
        std::process::exit(2)
    }
}

#[cfg(all(debug_assertions, feature = "legacy-eframe"))]
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
            || is_rune_route_or_tooling_arg(arg)
    }) else {
        return;
    };

    if is_rune_route_or_tooling_arg(arg) {
        if arg.as_str() == RUNE_BACKUP_FLAG
            || arg
                .strip_prefix(RUNE_BACKUP_FLAG)
                .is_some_and(|suffix| suffix.starts_with('='))
        {
            eprintln!("The Rune production backup route requires `--features rune-backup`: {arg}");
        } else {
            eprintln!(
                "Rune fixtures, smoke routes, login routes, and tooling are unavailable in release builds: {arg}"
            );
        }
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
