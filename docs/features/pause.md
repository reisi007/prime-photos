# Pause

A global pause toggle ("Alle Sicherungsvorgänge anhalten" ↔ "fortsetzen") in
the backup header and in the tray menu. The state is persisted in the config
and restored at start-up.

## How it works

- A global pause toggle ("Alle Sicherungsvorgänge anhalten" ↔ "fortsetzen")
  in the backup header and in the tray menu. No second button.
- The state is stored in `config.toml` (`paused`, serde-default `false`,
  so an old config loads) and initialised from it at start.
- The button is **always** switchable once at least one backup target exists
  **or** a run is live — previously only during a running upload, which (a)
  prevented pause in idle and (b) blocked a pure drag-and-drop run without
  configured folders.
- A set pause also blocks the background sync (guard already existed).
  **Pause per folder** remains Post-MVP (original has a row icon).
- A **missing source folder** is **not** this pause: it is its own state
  (`FolderStatus::NotFound`, "Ordner nicht gefunden", AMBER), deliberately
  separate from the global PAUSED — see [backup-targets.md](backup-targets.md).
- **Slow mode** (see [queue-progress.md](queue-progress.md)) is a separate
  run-time setting that forces `concurrency = 1`.

## Decisions

- **Pause, One-Time-Queue & Gesamt-Fortschritt (User-Entscheidung 2026-10-14):**
  - **Stoppen/Fortsetzen ist ein Umschalter** (wie im Original,
    `UI_STATES.md` §70): „Alle Sicherungsvorgänge anhalten" ↔ „… fortsetzen"
    in der Backup-Kopfzeile plus derselbe Eintrag im Tray. Kein zweiter
    Button.
  - **Pause wie Original (Entscheidung 2026-10-14):** der Zustand wird in
    `config.toml` gespeichert (`paused`, serde-default `false`, lädt also
    eine alte Config) und beim Start daraus initialisiert. Der Button ist
    **immer** schaltbar, sobald mindestens ein Sicherungsziel existiert
    **oder** ein Lauf läuft — früher nur während eines laufenden Laufs, was
    (a) Pause im Leerlauf verhinderte und (b) einen reinen Drag&Drop-Lauf ohne
    angelegte Ordner sperrte. Eine gesetzte Pause blockt auch den
    Hintergrund-Sync (Guard existierte bereits). **Pause pro Ordner** bleibt
    Post-MVP (Original hat ein Zeilen-Symbol).

- **Tray: Statuszeile + Pause-Labels (User-Entscheidung 2026-10-16).** Das
  Tray-Menü bekommt ganz oben eine **nicht klickbare Statuszeile** („Bereit" /
  „Upload läuft: x/y" / „Angehalten" / „Letzter Upload: x/y erfolgreich");
  der Pause-Eintrag heißt im Tray „**Alle Sicherungen anhalten**" ↔
  „**Sicherung starten**". Das **App-Label** bleibt unverändert („Alle
  Sicherungsvorgänge anhalten" ↔ „… fortsetzen") — der User wollte nur den
  Tray angleichen. Anlass: „Es ist alles abgeschlossen, da macht ‚Anhalten'
  keinen Sinn."

## Original reference

- `../original/UI_STATES.md` §70 — Row model, status vocabulary, pause toggle.
- `../original/SPEC_BACKGROUND_SYNC.md` — Background sync (pause behavior).
