mod prime_photos;
mod app_menu;
mod config;
mod countries;
mod dedup;
mod folders;
mod i18n;
mod icons;
mod login;
mod notify;
mod theme;
mod tray;
mod upload;
mod app;
mod backup;
mod backup_config;
mod settings;

use app::PrimePhotoApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Prime Photos",
        options,
        Box::new(|cc| Ok(Box::new(PrimePhotoApp::new(cc)))),
    )
}
