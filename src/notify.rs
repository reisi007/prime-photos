//! Cross-platform desktop notifications for finished upload runs (PORT-2).
//!
//! Uses `notify-rust`, which maps to the native backend per OS (macOS
//! Notification Center, Windows toast, Linux libnotify).
//!
//! On macOS the notification is **not** posted inline: the backend sends it
//! synchronously and pumps the run loop (`-[NSRunLoop runUntilDate:]`) while
//! doing so. `run_finished` is called from egui's `update`, so that nested run
//! loop re-enters winit's event handler — a queued closure runs inside the ObjC
//! callback, where unwinding is forbidden, so a panic there aborts the process
//! (SIGABRT crash report, 2026-10-14). The fix is the one already used for the
//! cookie probe in `login.rs`: hand the notification to the main dispatch
//! queue, so it is only sent once `update` has returned.
//!
//! Off macOS there is no run-loop re-entrancy problem; the notification is
//! posted directly on the calling (UI) thread, exactly as before.

use crate::i18n::{tr, tr1, tr2};
#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;
use log::warn;

/// Summary and body of the notification for a finished upload run.
///
/// Pure (no OS, no I/O), so the branch and the counters can be tested. `done` /
/// `failed` / `total` are the counters of the run that just ended.
fn notification_texts(done: usize, failed: usize, total: usize) -> (String, String) {
    let summary = tr("Prime Photos").to_string();
    let body = if failed > 0 {
        tr2(
            "Upload mit Fehlern beendet: {} von {} Datei(en) fehlgeschlagen.",
            failed,
            total,
        )
    } else {
        tr1("Upload abgeschlossen: {} Datei(en).", done)
    };
    (summary, body)
}

/// Show one desktop notification for a finished upload run.
///
/// Never panics: a failed notification is only logged.
pub fn run_finished(done: usize, failed: usize, total: usize) {
    let (summary, body) = notification_texts(done, failed, total);
    deliver(summary, body);
}

/// macOS: schedule the notification on the main dispatch queue, i.e. outside
/// winit's event handler (avoids the run-loop re-entrancy described above). The
/// block — including the handle's `Drop`, which is what actually sends the
/// notification — runs after `update` returned.
#[cfg(target_os = "macos")]
fn deliver(summary: String, body: String) {
    DispatchQueue::main().exec_async(move || post(&summary, &body));
}

/// Non-macOS: there is no run-loop re-entrancy problem, so post directly on the
/// calling (UI) thread.
#[cfg(not(target_os = "macos"))]
fn deliver(summary: String, body: String) {
    post(&summary, &body);
}

/// Post the notification with the native backend. Never panics: a failure is
/// only logged.
fn post(summary: &str, body: &str) {
    if let Err(e) = notify_rust::Notification::new()
        .appname(tr("Prime Photos"))
        .summary(summary)
        .body(body)
        .show()
    {
        warn!("Benachrichtigung konnte nicht angezeigt werden: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The assertions compare against the same `tr*` helpers the code under test
    // uses, so they hold in either UI language (the global language is not
    // touched here — other tests run in parallel) while still pinning the
    // branch taken and every substituted counter.

    #[test]
    fn texts_report_a_clean_run_with_the_done_count() {
        let (summary, body) = notification_texts(12, 0, 30);
        assert_eq!(summary, tr("Prime Photos"));
        assert_eq!(body, tr1("Upload abgeschlossen: {} Datei(en).", 12));
        assert!(body.contains("12"), "done count missing: {body}");
    }

    #[test]
    fn texts_report_a_single_file_like_the_existing_wording() {
        // The stored wording is count-neutral ("Datei(en)"), so one file keeps
        // the same key and only substitutes 1 — the existing behaviour.
        let (_, body) = notification_texts(1, 0, 1);
        assert_eq!(body, tr1("Upload abgeschlossen: {} Datei(en).", 1));
        assert!(body.contains('1'), "done count missing: {body}");
    }

    #[test]
    fn texts_report_a_run_with_failures_from_failed_and_total() {
        let (summary, body) = notification_texts(30, 2, 32);
        assert_eq!(summary, tr("Prime Photos"));
        assert_eq!(
            body,
            tr2(
                "Upload mit Fehlern beendet: {} von {} Datei(en) fehlgeschlagen.",
                2,
                32
            )
        );
        assert!(
            body.contains('2') && body.contains("32"),
            "failed/total counters missing: {body}"
        );
    }

    #[test]
    fn texts_switch_between_success_and_error() {
        let (_, ok) = notification_texts(5, 0, 5);
        let (_, err) = notification_texts(5, 1, 6);
        assert_ne!(ok, err, "the failure wording must differ");
    }
}
