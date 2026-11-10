//! Icon implementations grouped by UI backend.

#[cfg(feature = "legacy-eframe")]
pub(crate) mod eframe_ui;
#[cfg(feature = "rune-backup")]
pub(crate) mod rune_ui;
