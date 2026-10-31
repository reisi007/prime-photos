mod prime_photos;
mod about;
mod app_activation;
mod app_menu;
mod auth;
mod config;
mod countries;
mod dedup;
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
    let mode = match demo::parse_args(&args) {
        Ok(mode) => mode,
        Err(message) => {
            // Usage errors are a developer-facing start failure: stderr and a
            // nonzero exit, never a half-started window.
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    if mode == demo::Mode::List {
        for scenario in demo::Scenario::ALL {
            println!("{:<14} {}", scenario.id(), scenario.description());
        }
        return Ok(());
    }

    // A demo run never takes the single-instance slot (a running real instance
    // would swallow it) and never replaces that instance's notification
    // binding — it has no tray and never uploads in the background.
    if mode.is_demo() {
        log::info!("Prime Photos startet im Demo-Modus (keine echten Daten)");
    } else {
        // Only one instance at a time: a second start asks the running instance
        // to show its window (see `src/single_instance.rs`) and exits instead of
        // opening a second window. Runs before `notify::init()` so that a start
        // which immediately gives up never builds the notification binding.
        if !single_instance::acquire() {
            log::info!("Prime Photos läuft bereits — Fenster der laufenden Instanz angefordert");
            return Ok(());
        }

        notify::init();
    }

    // Observe AppKit's first launch notification before eframe runs its launch
    // path, without taking over the application delegate or Apple-event handler.
    app_activation::install_launch_diagnostic();

    // The demo window has a fixed size so a screenshot does not depend on the
    // developer's screen; the normal app keeps its preferred start size.
    let inner_size = match &mode {
        demo::Mode::Screenshot { .. } => [1400.0, 900.0],
        demo::Mode::Demo(_) => [1200.0, 800.0],
        // `List` already returned above; keep the normal start size for it so
        // the match stays exhaustive without a wildcard.
        demo::Mode::Off | demo::Mode::List => [1000.0, 700.0],
    };
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(inner_size)
        .with_min_inner_size([760.0, 520.0])
        .with_icon(window_icon());
    let viewport = if mode.is_demo() {
        // A demo run is meant to be looked at (and captured) — no hidden start.
        viewport.with_visible(true)
    } else {
        // The window starts invisible — this only avoids the first-frame
        // flash. It does **not** keep the window hidden: eframe shows it
        // itself (unconditionally) after the first frame, so the start
        // decision in `PrimePhotoApp::new` (silent login-item start vs.
        // manual start) holds the hidden state with an explicit
        // `ViewportCommand::Visible(false)`.
        viewport.with_visible(false)
    };

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Prime Photos",
        options,
        Box::new(|cc| Ok(Box::new(PrimePhotoApp::new(cc, mode)))),
    )
}
