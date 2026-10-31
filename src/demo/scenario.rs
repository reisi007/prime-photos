//! The inspectable UI states of the demo data source.

/// One inspectable UI state. Every scenario is a complete, self-consistent
/// fixture set (folders, queue, counters, flags) — never a partial overlay on
/// top of the previous state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scenario {
    /// A running upload next to finished and missing folders — the everyday
    /// picture from the reference screenshot.
    Mixed,
    /// Several files transferring at once, plus done and waiting rows.
    Uploading,
    /// The same live picture with the global pause switched on.
    Paused,
    /// Nothing left to do: every folder settled, empty queue.
    AllDone,
    /// A failed folder, failed queue rows and a config load error.
    Failures,
    /// Signed in, but no backup target at all (both empty states).
    Empty,
    /// Twelve targets to exercise the folder list's scrolling.
    ManyFolders,
    /// The settings page instead of the backup page.
    Settings,
    /// No session: the sign-in screen (rendered by our own UI, no webview).
    SignedOut,
}

impl Scenario {
    /// Every scenario, in the order `--demo-list` prints them.
    pub(crate) const ALL: [Scenario; 9] = [
        Scenario::Mixed,
        Scenario::Uploading,
        Scenario::Paused,
        Scenario::AllDone,
        Scenario::Failures,
        Scenario::Empty,
        Scenario::ManyFolders,
        Scenario::Settings,
        Scenario::SignedOut,
    ];

    /// Stable CLI identifier (used by `--demo=<id>` and `--demo-list`).
    pub(crate) fn id(self) -> &'static str {
        match self {
            Scenario::Mixed => "mixed",
            Scenario::Uploading => "uploading",
            Scenario::Paused => "paused",
            Scenario::AllDone => "all-done",
            Scenario::Failures => "failures",
            Scenario::Empty => "empty",
            Scenario::ManyFolders => "many-folders",
            Scenario::Settings => "settings",
            Scenario::SignedOut => "signed-out",
        }
    }

    /// One German line for `--demo-list` (the UI language of the fixtures).
    pub(crate) fn description(self) -> &'static str {
        match self {
            Scenario::Mixed => "Gemischter Alltag: laufender Upload, fertige und fehlende Ordner",
            Scenario::Uploading => "Upload läuft, mehrere Dateien gleichzeitig in Bearbeitung",
            Scenario::Paused => "Angehaltener Upload (Pause-Schaltfläche aktiv)",
            Scenario::AllDone => "Alles gesichert, keine offenen Dateien",
            Scenario::Failures => "Fehler im Ordner, fehlgeschlagene Dateien, Ladefehler",
            Scenario::Empty => "Angemeldet, aber keine Sicherungsziele",
            Scenario::ManyFolders => "Zwölf Sicherungsziele (Liste scrollen)",
            Scenario::Settings => "Einstellungsseite, Tab Allgemeines",
            Scenario::SignedOut => "Anmeldebildschirm ohne Sitzung",
        }
    }

    /// Parse a scenario id; case-insensitive, surrounding whitespace ignored.
    pub(crate) fn parse(value: &str) -> Option<Scenario> {
        let value = value.trim().to_ascii_lowercase();
        Scenario::ALL
            .into_iter()
            .find(|scenario| scenario.id() == value)
    }
}
