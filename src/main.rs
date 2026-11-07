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

    // Rune remains a headless-only migration fixture; it is not an
    // interactive preview or an application start mode.
    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    if has_rune_headless_args(&args) {
        if let Err(message) = rune_settings::run(filter_launcher_args(args.clone())) {
            eprintln!("Prime Photos Rune spike: {message}");
            std::process::exit(1);
        }
        return Ok(());
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
    args.iter().any(|arg| {
        arg == "--snap"
            || arg.starts_with("--snap=")
            || arg == "--tree"
            || arg.starts_with("--tree=")
            || arg == "--script"
            || arg.starts_with("--script=")
    })
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
        arg.starts_with("--demo") || arg.starts_with("--screenshot") || arg.starts_with("--size")
    }) else {
        return;
    };

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
