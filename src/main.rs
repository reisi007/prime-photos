mod prime_photos;
mod about;
mod app_activation;
mod app_menu;
mod auth;
mod config;
mod countries;
mod dedup;
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

    // Only one instance at a time: a second start asks the running instance to
    // show its window (see `src/single_instance.rs`) and exits instead of
    // opening a second window. Runs before `notify::init()` so that a start
    // which immediately gives up never builds the notification binding.
    if !single_instance::acquire() {
        log::info!("Prime Photos läuft bereits — Fenster der laufenden Instanz angefordert");
        return Ok(());
    }

    notify::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([760.0, 520.0])
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Prime Photos",
        options,
        Box::new(|cc| Ok(Box::new(PrimePhotoApp::new(cc)))),
    )
}
