//! Central internationalization (i18n) layer.
//!
//! German is the reference/source language: the German UI text **is** the key.
//! `Lang::De` returns a key unchanged, `Lang::En` looks it up in the table
//! below (a missing entry falls back to the German key, so a gap degrades to
//! German instead of breaking). Call sites therefore read naturally — the text
//! the user sees in the reference language is right there.
//!
//! The current language is a process-global (`CURRENT`) because the upload
//! workers log from background threads and the tray is built on the main
//! thread. It is set once at startup and again when the user changes it in the
//! settings (`Config.language`).

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

/// The languages the UI ships in. `De` is the reference language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lang {
    De,
    En,
}

impl Lang {
    /// Stable code used for persistence (`Config.language`).
    pub(crate) fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    /// Parse a persisted language code; `None` for anything unknown.
    pub(crate) fn from_code(code: &str) -> Option<Lang> {
        match code.trim().to_ascii_lowercase().as_str() {
            "de" => Some(Lang::De),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

/// Current language as an `AtomicU8` (0 = `De`, 1 = `En`). Safe to read from
/// any thread.
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Set the process-global UI language.
pub(crate) fn set_lang(lang: Lang) {
    CURRENT.store(
        match lang {
            Lang::De => 0,
            Lang::En => 1,
        },
        Ordering::Relaxed,
    );
}

/// The process-global UI language.
pub(crate) fn lang() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::De
    }
}

/// Translate a German source string (the key) into the current language.
///
/// `Lang::De` returns the key unchanged; `Lang::En` returns the English entry,
/// or the German key if the table has no entry.
pub(crate) fn tr(key: &'static str) -> &'static str {
    match lang() {
        Lang::De => key,
        Lang::En => EN
            .iter()
            .find(|(de, _)| *de == key)
            .map(|(_, en)| *en)
            .unwrap_or(key),
    }
}

/// Replace the `{}` placeholders in `tr(key)` with `args`, left to right.
fn subst(key: &'static str, args: &[String]) -> String {
    let mut out = tr(key).to_string();
    for arg in args {
        if let Some(i) = out.find("{}") {
            out.replace_range(i..i + 2, arg);
        }
    }
    out
}

/// `tr` for a message with one `{}` placeholder.
pub(crate) fn tr1(key: &'static str, a: impl Display) -> String {
    subst(key, &[a.to_string()])
}

/// `tr` for a message with two `{}` placeholders.
pub(crate) fn tr2(key: &'static str, a: impl Display, b: impl Display) -> String {
    subst(key, &[a.to_string(), b.to_string()])
}

/// `tr` for a message with three `{}` placeholders.
pub(crate) fn tr3(key: &'static str, a: impl Display, b: impl Display, c: impl Display) -> String {
    subst(key, &[a.to_string(), b.to_string(), c.to_string()])
}

/// Count-dependent text: `singular` for exactly one, `plural` otherwise, with
/// the literal token `{n}` in the plural replaced by the count.
pub(crate) fn tr_n(n: usize, singular: &'static str, plural: &'static str) -> String {
    let key = if n == 1 { singular } else { plural };
    tr(key).replace("{n}", &n.to_string())
}

/// Best-effort system language from the environment. `de*` → German; anything
/// else (including `C`/`POSIX`/unset) → English. No OS API, no extra crate.
pub(crate) fn system_lang() -> Lang {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            if value.trim().to_ascii_lowercase().starts_with("de") {
                return Lang::De;
            }
        }
    }
    Lang::En
}

/// German source → English translation, grouped by feature module and sorted
/// within each group (umlauts ordered by their base letter). Keys shared by
/// several modules live in the "common" group. A missing entry falls back to
/// the German key in [`tr`].
const EN: &[(&str, &str)] = &[
    // -- common / shared ------------------------------------------------
    ("Abbrechen", "Cancel"),
    ("Ändern …", "Change …"),
    ("Datensicherung", "Backup"),
    ("Datensicherungseinstellungen", "Backup settings"),
    ("Mit Amazon anmelden", "Sign in with Amazon"),
    // Shared by the settings and by the backup-configuration dialog: the label
    // of the `SaveAsNew` option there is deliberately **not** the dialog button's
    // "Als neue Datei speichern" (the original names the same option differently
    // in the two places).
    (
        "Neue Datei automatisch umbenennen",
        "Rename the new file automatically",
    ),
    ("Prime Photos", "Prime Photos"),
    ("Speichern", "Save"),
    // -- app shell / navigation -----------------------------------------
    ("Abmelden", "Sign out"),
    ("Angemeldet", "Signed in"),
    ("Angemeldet als:", "Signed in as:"),
    ("Bereit", "Ready"),
    ("Einstellungen", "Settings"),
    (
        "Die Einstellungen konnten nicht gespeichert werden.",
        "The settings could not be saved.",
    ),
    (
        "Die Einstellungen konnten nicht geladen werden.",
        "The settings could not be loaded.",
    ),
    ("Letzter Upload: {} erfolgreich", "Last upload: {} successful"),
    ("Login – Prime Photos", "Sign in – Prime Photos"),
    ("Nicht angemeldet", "Not signed in"),
    ("Upload läuft: {} · {} · {}", "Upload running: {} · {} · {}"),
    // -- backup list ----------------------------------------------------
    ("Aktionen", "Actions"),
    ("Angehalten bei {} %", "Paused at {} %"),
    ("ANGEHALTEN", "PAUSED"),
    ("Alle Sicherungsvorgänge anhalten", "Pause all backups"),
    ("Alle Sicherungsvorgänge fortsetzen", "Resume all backups"),
    ("Alles aktuell", "Everything up to date"),
    ("Automatisch gesicherte Ordner:", "Automatically backed up folders:"),
    ("Bearbeiten", "Edit"),
    ("Bitte zuerst anmelden (Einstellungen).", "Please sign in first (Settings)."),
    ("Datensicherung abgeschlossen", "Backup complete"),
    ("Datensicherung hinzufügen …", "Add backup …"),
    ("Datensicherung stoppen", "Stop backup"),
    ("Datensicherung gestoppt: {}", "Backup stopped: {}"),
    ("Einmalige Sicherung", "One-time backup"),
    ("Es läuft bereits ein Upload.", "An upload is already running."),
    (
        "FOTOS PER DRAG & DROP VERSCHIEBEN, UM SIE HOCHZULADEN",
        "DRAG & DROP PHOTOS TO UPLOAD",
    ),
    ("Gesamt: {} von {} Dateien", "Total: {} of {} files"),
    (
        "Gesamt: {} von {} Dateien · {}",
        "Total: {} of {} files · {}",
    ),
    // Plattformneutral (PORT-7: open -R / explorer /select / xdg-open) —
    // "Finder" ist nur macOS.
    ("Im Dateimanager anzeigen", "Show in file manager"),
    ("Jetzt scannen und hochladen", "Scan and upload now"),
    (
        "Keine Fotos oder Videos in diesem Ordner gefunden.",
        "No photos or videos found in this folder.",
    ),
    ("Konfiguration wird durchgeführt", "Configuration in progress"),
    ("Ordner", "Folder"),
    (
        "Tipp: Ordner werden dauerhaft gemerkt und bei jedem Start wieder gesichert.",
        "Tip: folders are remembered and backed up again on every start.",
    ),
    (
        "Wählen Sie einen Ordner aus, um die Sicherung zu starten.",
        "Choose a folder to start the backup.",
    ),
    ("Warten", "Waiting"),
    ("Wiederholen", "Retry"),
    ("Upload-Vorgang läuft", "Upload in progress"),
    ("Vollständig", "Complete"),
    ("Zuletzt aktualisiert: {}", "Last updated: {}"),
    (
        "Zur einmaligen Sicherung mit der Maus hierher ziehen",
        "Drag here for a one-time backup",
    ),
    ("{} Datei(en) werden hochgeladen …", "{} file(s) being uploaded …"),
    ("{}% hochgeladen", "{}% uploaded"),
    // -- backup configuration dialog ------------------------------------
    (
        "Datensicherung für lokalen Pfad existiert bereits",
        "A backup for this local path already exists",
    ),
    (
        "Duplikate vermeiden: Keine Fotos und Videos sichern, die sich bereits in meinem Konto befinden",
        "Avoid duplicates: do not back up photos and videos already in my account",
    ),
    ("Ersetzen", "Replace"),
    ("Fotos sichern", "Back up photos"),
    ("Gesicherter Ordner:", "Backed up folder:"),
    ("Ignorieren", "Ignore"),
    ("Jedes Mal nachfragen", "Ask every time"),
    ("Lokaler Pfad ist nicht verfügbar", "Local path is not available"),
    ("Noch nicht verfügbar", "Not available yet"),
    ("Ordner hinzugefügt: {}", "Folder added: {}"),
    ("Ordner ist nicht mehr vorhanden", "Folder no longer exists"),
    ("Sofort", "Immediately"),
    ("Umgang mit Dateinamenkonflikten:", "Handling of file name conflicts:"),
    ("Videos sichern", "Back up videos"),
    (
        "Videos werden noch nicht hochgeladen",
        "Videos are not uploaded yet",
    ),
    ("Wöchentlich aktiviert", "Weekly enabled"),
    ("Änderungen hochladen:", "Upload changes:"),
    ("um", "at"),
    // -- settings -------------------------------------------------------
    ("Album (optional):", "Album (optional):"),
    ("Allgemeines", "General"),
    ("Anmeldung", "Sign-in"),
    ("Automatisch (System)", "Automatic (system)"),
    ("Bandbreite", "Bandwidth"),
    ("Begrenzen auf", "Limit to"),
    (
        "Desktop-Benachrichtigungen (Upload fertig / Fehler)",
        "Desktop notifications (upload finished / error)",
    ),
    ("Fehler", "Error"),
    ("Downloads", "Downloads"),
    (
        "Downloads sind in diesem Port noch nicht implementiert.",
        "Downloads are not implemented in this port yet.",
    ),
    (
        "Gilt für neue Upload-Ziele und für Drag&Drop.",
        "Applies to new upload targets and to drag & drop.",
    ),
    ("Gleichzeitige Uploads", "Concurrent uploads"),
    (
        "Hinweis: Die Begrenzung wird pro Datei nach dem Upload angewendet.",
        "Note: the limit is applied per file after the upload.",
    ),
    ("KB/Sekunde", "KB/second"),

    ("Nach dem Anmelden starten", "Launch at login"),
    ("Hintergrund-Synchronisierung", "Background sync"),
    ("Sucht automatisch nach neuen Dateien in den aktiven Sicherungszielen. Läuft nur, solange Prime Photos geöffnet ist.",
     "Automatically looks for new files in the active backup targets. Runs only while Prime Photos is open."),
    ("Nicht begrenzen", "Do not limit"),
    ("Gilt nur bis zum nächsten App-Start; wird nicht gespeichert.", "Applies only until the next app start; not saved."),
    ("Slow-Mode (1 Datei gleichzeitig)", "Slow mode (1 file at a time)"),
    ("Standard bei Dateikonflikten", "Default for file conflicts"),
    // Global dedupe default for new upload targets and the one-time queue
    // (settings → Upload settings block, above the conflict default).
    (
        "Standard für neue Upload-Ziele",
        "Default for new upload targets",
    ),
    ("Duplikate überspringen", "Skip duplicates"),
    (
        "Dateien, die sich bereits in meinem Konto befinden, nicht sichern",
        "Do not back up files that are already in my account",
    ),
    (
        "Startet Prime Photos automatisch, wenn du dich am System anmeldest.",
        "Starts Prime Photos automatically when you sign in to your system.",
    ),
    ("Sprache", "Language"),
    ("Upload", "Upload"),
    ("Upload-Rate pro Datei", "Upload rate per file"),
    ("Uploads", "Uploads"),
    // -- login ----------------------------------------------------------
    ("Abgemeldet.", "Signed out."),
    (
        "Sitzung abgelaufen – bitte erneut anmelden.",
        "Session expired – please sign in again.",
    ),
    (
        "Anmeldeseite konnte nicht geöffnet werden.",
        "Could not open the sign-in page.",
    ),
    ("Bei Amazon anmelden", "Sign in at Amazon"),
    (
        "Fotos & Videos zu Amazon Photos hochladen",
        "Upload photos & videos to Amazon Photos",
    ),
    ("Land / Amazon-Marktplatz:", "Country / Amazon marketplace:"),
    (
        "Login erfolgreich – Refresh-Token gespeichert.",
        "Signed in – refresh token saved.",
    ),
    (
        "Die Anmeldung wurde von Amazon abgelehnt. Bitte erneut versuchen.",
        "Amazon rejected the sign-in. Please try again.",
    ),
    // -- countries / marketplaces ---------------------------------------
    ("Australien", "Australia"),
    ("Brasilien", "Brazil"),
    ("Deutschland", "Germany"),
    ("Frankreich", "France"),
    ("Indien", "India"),
    ("Italien", "Italy"),
    ("Japan", "Japan"),
    ("Kanada", "Canada"),
    ("Mexiko", "Mexico"),
    ("Schweiz", "Switzerland"),
    ("Spanien", "Spain"),
    ("USA", "USA"),
    ("Vereinigtes Königreich", "United Kingdom"),
    ("Österreich", "Austria"),
    // -- upload progress & log ------------------------------------------
    ("1 Datei", "1 file"),
    ("100% hochgeladen", "100% uploaded"),
    ("Abgeschlossen", "Completed"),
    ("Marktplatz", "Marketplace"),
    ("Als neue Datei speichern", "Save as new file"),
    ("Dateiname-Konflikt", "File name conflict"),
    ("Dateiname-Konflikt: {}", "File name conflict: {}"),
    ("Ersetzen", "Replace"),
    ("Ignorieren", "Ignore"),
    ("Keine Antwort auf den Konflikt — wird übersprungen.", "No answer to the conflict — skipping."),
    ("Nicht noch einmal fragen", "Don't ask again"),
    (
        "Nicht noch einmal nach ‚{}'-Ordner fragen",
        "Don't ask again for the '{}' folder",
    ),
    ("Vorhanden, übersprungen: {}", "Already there, skipped: {}"),
    (
        "Bereits im Konto, nur umbenannt: {}",
        "Already in your account, only renamed: {}",
    ),
    ("„{}“ existiert bereits in Amazon Photos.", "“{}” already exists in Amazon Photos."),
    ("Anmeldung abgelaufen, {} wird erneut versucht.", "Sign-in expired, retrying {}."),
    ("Auth-Token wurde erneuert.", "Auth token renewed."),
    ("Auth-Token-Erneuerung fehlgeschlagen: {}", "Auth token renewal failed: {}"),
    ("{} offen", "{} open"),
    ("Angehalten ({}%)", "Paused ({}%)"),
    ("FEHLER {}: {}", "ERROR {}: {}"),
    ("Fehlgeschlagen", "Failed"),
    ("Fertig: {} erfolgreich, {} fehlgeschlagen.", "Done: {} succeeded, {} failed."),
    (
        "Abgebrochen: {} erfolgreich, {} fehlgeschlagen, {} offen.",
        "Cancelled: {} succeeded, {} failed, {} open.",
    ),
    ("In Bearbeitung", "In progress"),
    // Per-file percentage while a row is uploading (Befund 11c). The whole
    // visible string is one key, so a translator can reorder it freely.
    ("In Bearbeitung · {}%", "In progress · {}%"),
    ("{} von {}", "{} of {}"),
    ("Schätzung läuft …", "Estimating …"),
    ("Slow-Mode aktiv: 1 Datei gleichzeitig.", "Slow mode active: 1 file at a time."),
    ("UEBERSPRUNGEN {}: bereits hochgeladen", "SKIPPED {}: already uploaded"),
    ("Übersprungen", "Skipped"),
    ("Warten", "Waiting"),
    ("Warteschlange ({})", "Queue ({})"),
    // Queue-table headers and empty state (Welle 3.1/3.5,
    // docs/SPEC_UPLOAD_PROGRESS.md §7.8).
    ("Größe", "Size"),
    ("Keine Uploads in der Warteschlange", "No uploads in the queue"),
    ("Name", "Name"),
    ("Status", "Status"),
    ("Wird vorbereitet …", "Preparing …"),
    ("ca. {}", "approx. {}"),
    ("{}% · noch {} · {}", "{}% · {} left · {}"),
    ("{n} Dateien", "{n} files"),
    // -- run outcome breakdown (footer + folder row) ---------------------
    ("{} neu hochgeladen", "{} newly uploaded"),
    ("{} bereits im Konto", "{} already in your account"),
    ("{} unverändert", "{} unchanged"),
    ("{} Fehler", "{} error(s)"),
    // -- notifications --------------------------------------------------
    ("Upload abgeschlossen: {} Datei(en).", "Upload complete: {} file(s)."),
    (
        "Upload mit Fehlern beendet: {} von {} Datei(en) fehlgeschlagen.",
        "Upload finished with errors: {} of {} file(s) failed.",
    ),
    // -- tray / menu bar ------------------------------------------------
    ("Anhalten", "Pause"),
    ("Beenden", "Quit"),
    ("Fenster anzeigen", "Show window"),
    ("Fortsetzen", "Resume"),
    // -- about / third-party licenses (Welle 4b) ------------------------
    ("Über Prime Photos", "About Prime Photos"),
    ("Version: {}", "Version: {}"),
    ("Drittanbieterlizenzen", "Third-Party Licenses"),
    ("Schließen", "Close"),
    ("© {} Prime Photos", "© {} Prime Photos"),
    ("Komponenten und Lizenztexte", "Components and license texts"),
    ("Icons und Grafiken", "Icons and graphics"),
];
