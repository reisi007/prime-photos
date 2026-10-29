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
    tr_for(lang(), key)
}

/// [`tr`] for an explicit language. Used by the native macOS menu, whose label
/// source is also exercised in tests for both languages without mutating the
/// process-global language.
pub(crate) fn tr_for(lang: Lang, key: &'static str) -> &'static str {
    match lang {
        Lang::De => key,
        Lang::En => EN
            .iter()
            .find(|(de, _)| *de == key)
            .map(|(_, en)| *en)
            .unwrap_or(key),
    }
}

/// Context tag for the native macOS main menu (see [`tr_ctx_for`]).
///
/// Only the macOS app menu needs context-sensitive lookups; the item is
/// `cfg`-gated with [`tr_ctx_for`] so Windows/Linux builds stay warning-free.
#[cfg(target_os = "macos")]
pub(crate) const CONTEXT_NATIVE_MENU: &str = "native-menu";

/// [`tr_for`] with a surface context for German key collisions.
///
/// The German source string alone is normally the key, but a few words mean
/// different things per surface: "Wiederholen" is *Retry* in the backup list
/// and *Redo* in the native Edit menu. `context` is an internal, never-shown
/// tag; without a matching entry this falls back to [`tr_for`] (and then to
/// the German key).
#[cfg(target_os = "macos")]
pub(crate) fn tr_ctx_for(lang: Lang, context: &'static str, key: &'static str) -> &'static str {
    if lang == Lang::En {
        if let Some((_, _, en)) = EN_CONTEXT
            .iter()
            .find(|(ctx, de, _)| *ctx == context && *de == key)
        {
            return en;
        }
    }
    tr_for(lang, key)
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

/// Classify a POSIX locale / locale identifier by its language prefix: `de*`
/// (case-insensitive, surrounding whitespace trimmed) → German, everything else
/// → English. Accepts both the POSIX form (`de_AT`, `de_AT.UTF-8`) and the
/// BCP-47 form (`de-DE`). `C`/`POSIX`/empty count as English.
fn lang_from_locale_id(id: &str) -> Lang {
    if id.trim().to_ascii_lowercase().starts_with("de") {
        Lang::De
    } else {
        Lang::En
    }
}

/// A single environment value's vote: empty or whitespace-only means "not set"
/// and casts **no** vote (`None`); anything else is classified by
/// [`lang_from_locale_id`].
fn lang_from_env_value(value: &str) -> Option<Lang> {
    if value.trim().is_empty() {
        None
    } else {
        Some(lang_from_locale_id(value))
    }
}

/// Best-effort system language.
///
/// The POSIX environment variables are consulted first, in their precedence
/// order (`LC_ALL`, then `LC_MESSAGES`, then `LANG`): the **first** one that is
/// set and non-empty decides, so an explicit POSIX override keeps winning.
///
/// The environment alone is not enough, though: a GUI app launched from
/// **Finder** inherits none of these variables, which would make the app start
/// in English on a German system. On macOS the real OS locale (`NSLocale`) is
/// therefore read as a fallback when no variable is set. Windows/Linux have no
/// such fallback and stay on the environment. Anything unrecognised (including
/// `C`/`POSIX`/unset) → English.
pub(crate) fn system_lang() -> Lang {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            if let Some(lang) = lang_from_env_value(&value) {
                return lang;
            }
        }
    }
    os_lang().unwrap_or(Lang::En)
}

/// The OS UI language on macOS, via `NSLocale`. Works for a Finder-launched GUI
/// app that has no `LANG`/`LC_*` at all. `None` on other platforms (no such OS
/// API — Windows/Linux keep using the environment).
#[cfg(target_os = "macos")]
fn os_lang() -> Option<Lang> {
    let id = objc2_foundation::NSLocale::currentLocale().localeIdentifier();
    Some(lang_from_locale_id(&id.to_string()))
}

#[cfg(not(target_os = "macos"))]
fn os_lang() -> Option<Lang> {
    None
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
    (
        "Die Sicherungsziele konnten nicht geladen werden. Die beschädigte Datei wurde gesichert.",
        "The backup targets could not be loaded. The damaged file was preserved.",
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
    ("Der Ordner wird beim nächsten Lauf gesichert.", "The folder will be backed up on the next run."),
    // Missing source (user decisions 2026-10-16, 2026-10-17): a missing source
    // folder must not read as "no photos found" — the understandable cause is
    // the message.
    (
        "Der Quellordner ist gerade nicht erreichbar (z. B. externes Laufwerk nicht angeschlossen).",
        "The source folder is currently unreachable (e.g. an external drive is not connected).",
    ),
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
    ("Ordner nicht gefunden", "Folder not found"),
    (
        "Ordner wird eingelesen und in den laufenden Upload aufgenommen.",
        "The folder is being read in and added to the running upload.",
    ),
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
    (
        "{} Datei(en) zusätzlich aufgenommen.",
        "{} additional file(s) taken up.",
    ),
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
    ("Änderungen hochladen:", "Upload changes:"),
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
    ("Angehalten ({}%)", "Paused ({}%)"),
    ("FEHLER {}: {}", "ERROR {}: {}"),
    (
        "FEHLER {}: {} — wird erneut versucht.",
        "ERROR {}: {} — will be retried.",
    ),
    (
        "{} Datei(en) werden erneut versucht …",
        "{} file(s) will be retried …",
    ),
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
    ("Warteschlange ({})", "Queue ({})"),
    // Queue-table headers and empty state (Welle 3.1/3.5,
    // docs/original/SPEC_UPLOAD_PROGRESS.md §7.8).
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
    // Pause labels follow the original's status-menu wording
    // (`docs/original/SPEC_OS_INTEGRATION.md` §4); the status texts follow the
    // original's tooltip (`docs/original/SPEC_NOTIFICATIONS.md` §2.4).
    ("Alle anhalten", "Pause all"),
    ("Alle fortsetzen", "Resume all"),
    ("Angehalten", "Paused"),
    ("Beenden", "Quit"),
    ("Fenster anzeigen", "Show window"),
    ("Mit Fehlern abgeschlossen", "Finished with errors"),
    // -- native macOS main menu (Issue #8) ------------------------------
    // "Bearbeiten" and "Über Prime Photos" are shared with other surfaces and
    // live in their feature groups above. "Wiederholen" is deliberately absent:
    // in the backup list it is "Retry"; the Edit-menu "Redo" is a contextual
    // entry in `EN_CONTEXT`.
    ("Alles auswählen", "Select All"),
    ("Ausschneiden", "Cut"),
    ("Einfügen", "Paste"),
    ("Kopieren", "Copy"),
    ("Prime Photos beenden", "Quit Prime Photos"),
    ("Rückgängig", "Undo"),
    // -- about / third-party licenses (Welle 4b) ------------------------
    ("Über Prime Photos", "About Prime Photos"),
    ("Version: {}", "Version: {}"),
    ("Drittanbieterlizenzen", "Third-Party Licenses"),
    ("Schließen", "Close"),
    ("© {} Prime Photos", "© {} Prime Photos"),
    ("Komponenten und Lizenztexte", "Components and license texts"),
    ("Icons und Grafiken", "Icons and graphics"),
];

/// Context-qualified English entries for German keys that collide across
/// surfaces: `(context, German key, English)`. Consulted by [`tr_ctx_for`]
/// before the plain [`EN`] table; the German key stays the visible reference
/// text. Entries belong here only when the *same* German string must translate
/// differently per surface — otherwise the string is its own unambiguous key.
#[cfg(target_os = "macos")]
const EN_CONTEXT: &[(&str, &str, &str)] = &[
    // Backup list = "Retry", native macOS Edit menu = "Redo".
    (CONTEXT_NATIVE_MENU, "Wiederholen", "Redo"),
];

#[cfg(test)]
mod tests {
    use super::{EN, Lang, lang_from_env_value, lang_from_locale_id};
    #[cfg(target_os = "macos")]
    use super::{CONTEXT_NATIVE_MENU, EN_CONTEXT, tr_ctx_for, tr_for};
    use std::collections::HashSet;

    /// Every German key must appear exactly once. The table is a linear lookup
    /// (`tr` returns the first match), so a duplicate silently shadows the later
    /// entry — this guards against that class of drift.
    #[test]
    fn german_keys_are_unique() {
        let mut seen = HashSet::new();
        for (de, _) in EN {
            assert!(
                seen.insert(*de),
                "duplicate German key in EN table: {de:?}"
            );
        }
    }

    /// Context entries are looked up linearly too, so `(context, key)` must be
    /// unique.
    #[cfg(target_os = "macos")]
    #[test]
    fn contextual_keys_are_unique() {
        let mut seen = HashSet::new();
        for (context, de, _) in EN_CONTEXT {
            assert!(
                seen.insert((*context, *de)),
                "duplicate contextual key in EN table: {context:?}/{de:?}"
            );
        }
    }

    /// The same German key translates per surface: "Wiederholen" stays "Retry"
    /// in the backup list while the native Edit menu gets "Redo".
    #[cfg(target_os = "macos")]
    #[test]
    fn retry_and_redo_are_distinct() {
        assert_eq!(tr_for(Lang::De, "Wiederholen"), "Wiederholen");
        assert_eq!(
            tr_ctx_for(Lang::De, CONTEXT_NATIVE_MENU, "Wiederholen"),
            "Wiederholen"
        );
        assert_eq!(tr_for(Lang::En, "Wiederholen"), "Retry");
        assert_eq!(
            tr_ctx_for(Lang::En, CONTEXT_NATIVE_MENU, "Wiederholen"),
            "Redo"
        );
        // An unknown context falls back to the plain translation.
        assert_eq!(tr_ctx_for(Lang::En, "other", "Wiederholen"), "Retry");
    }

    /// Locale identifiers are classified by their `de` prefix only, in both the
    /// POSIX (`de_AT`) and the BCP-47 (`de-DE`) form, case-insensitively and
    /// ignoring surrounding whitespace.
    #[test]
    fn locale_ids_classify_by_language_prefix() {
        for id in ["de_AT", "de-DE", "de", "DE_at", "  de_AT.UTF-8  "] {
            assert_eq!(
                lang_from_locale_id(id),
                Lang::De,
                "expected German for {id:?}"
            );
        }
        for id in ["en_US", "fr_FR", "C", "POSIX", ""] {
            assert_eq!(
                lang_from_locale_id(id),
                Lang::En,
                "expected English for {id:?}"
            );
        }
    }

    /// An empty or whitespace-only environment value means "not set" and casts
    /// no vote (`None`); a real value is classified like a locale id.
    #[test]
    fn env_value_ignores_unset_like_values() {
        assert_eq!(lang_from_env_value(""), None);
        assert_eq!(lang_from_env_value("   "), None);
        assert_eq!(lang_from_env_value("de_AT.UTF-8"), Some(Lang::De));
        assert_eq!(lang_from_env_value("C"), Some(Lang::En));
    }
}
