//! Cross-platform desktop notifications for finished upload runs (PORT-2).
//!
//! Uses `notify-rust`, which maps to the native backend per OS (macOS
//! Notification Center, Windows toast, Linux libnotify). The notification is
//! shown synchronously on the calling thread: `update` runs on the main thread,
//! which is what macOS' backend requires.

use crate::i18n::{tr, tr1, tr2};
use log::warn;

/// Show one desktop notification for a finished upload run.
///
/// `done` / `failed` / `total` are the counters of the run that just ended.
/// Never panics: a failed notification is only logged.
pub fn run_finished(done: usize, failed: usize, total: usize) {
    let summary = tr("Prime Photos");
    let body = if failed > 0 {
        tr2(
            "Upload mit Fehlern beendet: {} von {} Datei(en) fehlgeschlagen.",
            failed,
            total,
        )
    } else {
        tr1("Upload abgeschlossen: {} Datei(en).", done)
    };

    if let Err(e) = notify_rust::Notification::new()
        .appname(tr("Prime Photos"))
        .summary(summary)
        .body(&body)
        .show()
    {
        warn!("Benachrichtigung konnte nicht angezeigt werden: {e}");
    }
}
