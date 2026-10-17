# Change detection

Prime Photos detects new and changed files via **OS file system events**,
not polling. A stat check (path + size + mtime) runs before hashing; only
unknown or changed files are read and hashed. A safety net at start-up
compares the folder's newest modification time with the one stored from the
last run.

## How it works

- **OS file system events, not polling:** one watch per backup folder
  (FSEvents / ReadDirectoryChangesW / inotify). The 15/90-minute round
  (`background_sync`) is **not** MVP-ready and is dropped as the main path.
- Per event only the affected file is examined — no full folder pass, no
  re-hashing of everything.
- A stat check (path + size + mtime) runs before hashing; only unknown or
  changed files are read and MD5-hashed. The MD5 remains the identity; the
  stat hit is only the fast path.
- **Cold start (event replacement for powered-off periods):** at start or
  after sign-in, each folder is enumerated once and the stat check decides
  what is missing. No timer.
- **Safety net at start-up:** per backup folder, the **largest modification
  time** (`max mtime`) of the last run is **stored in SQLite** (new table,
  next to the MD5 index). At start, a pure metadata pass (`walkdir` without
  reading files) suffices: if the current `max mtime` is **not greater** than
  the stored one, the folder is provably unchanged and **nothing** is hashed,
  no DB query per file, no upload plan. If it is greater, only that folder
  runs the stat-check path (and only changed files are read + hashed).
  **Innate boundary (like in the original):** copies that preserve mtime
  (`rsync --times`, Finder import) do not raise `max mtime`. Whoever needs
  that starts manually — the watermark hit is the fast path, not a guarantee.
- **Drag & drop is a one-off upload:** no watch, no watermark, no permanent
  backup — only the temporary queue until completion.
- **Pause also blocks events** (global pause).
- **Event buffer: unbounded, no discarding.** Events that arrive during a
  run or during a pause are parked and fully processed at the next run.
  There is **no upper limit** (the earlier 500-item cap `WATCH_PENDING_MAX`
  is removed): the list grows dynamically with the number of **distinct**
  changed paths (~1.2 MB at 10,000, ~12 MB at 100,000 — negligible), dedup
  via `HashSet<PathBuf>` in O(1) instead of O(n²). Bursts of **10,000+**
  files (camera import, folder copy, long pause) are fully covered; nothing
  is discarded. A new burst **replaces** the parked list no longer, but is
  **merged** with it (this was a real loss bug: during multiple bursts during
  a pause only the last one survived); if `can_start` allows a run, the
  merged list goes into **one** run. Implementation: `src/backup.rs`
  (`park_paths`, `pending`/`pending_seen`), commit `2eea96c`, verify round
  1 APPROVE.
- **Folder added during a run is not dropped (Welle 5.31):** the detached
  supervisor reads the folder in **parallel** to the running upload
  (`UploadPlan::build_since` with watermark `None`, `handle_scan_requests` in
  `src/backup.rs`) and takes its files into the **live** run via
  `inject_into_live_run` (rows, worker queue, `folder_opts`, `folder_total`,
  `total`). `scans_in_flight` keeps the run open for that, and the sealing
  (`sealed`) happens under the same lock, so nothing lands in a dead queue. When
  no run is left, the folder is parked (`pending_scans`) and started as an
  ordinary run on the next allowed pass (no watermark progress, like the event
  path). **Without** a supervisor (background sync off) the old behaviour stands
  (`Es läuft bereits ein Upload.`). Automatically picked up again only **with**
  background sync — the next app start's safety net; without it, only a manual
  run does.
- **Return of a missing drive (Welle 5.33):** a target held as `NotFound`
  whose path comes back gets its **whole stock read in once** (no watermark, no
  "only changes"). The detection is edge-triggered (`returned_from_missing`:
  only `NotFound` → present; a `Paused` folder does **not** fire) and runs in
  the supervisor (`sync_watchers` → `lift_returned_sources`; `watch_loop` parks
  the path in `pending_scans` and handles it in the same pass via
  `handle_scan_requests` — parallel to a live run, or as a normal run). Exactly
  **once** per return: after the lift the folder stands as `Idle`, so no later
  30 s pass triggers it again. The **global pause** is respected (`can_start`):
  while paused the folder stays parked and runs on its own once the pause is
  lifted (within `WATCH_POLL`, 30 s). **Without** background sync there is no
  supervisor — the path does not exist then (the safety net is the next app
  start).
- **Open:** safety net for missed events (watcher limit, ejected volume,
  network mount) — as an optional switch, not yet decided.
- Original parameters (documented): FSEvents latency 3 s, throttle 100 ms,
  `Throttle(FiveSeconds)` debounce, poll `BackupWatcherPollIntervalInSeconds`
  30 s, re-arm on error, paused targets get no watcher.

## Decisions

- **Erkennung neuer/geänderter Dateien (User-Entscheidung 2026-10-14):**
  - **Primärer Mechanismus: OS-Dateisystem-Events, kein Polling.** Die
    15/90-Minuten-Runde (`background_sync`: erster Tick 15 min, dann 90 min)
    ist **nicht** MVP-fertig und entfällt als Hauptpfad. Pro überwachtem
    Backup-Ordner ein OS-Watch über eine plattformneutrale Crate (macOS
    FSEvents / Windows ReadDirectoryChangesW / Linux inotify).
    Original-Parameter (`../original/SPEC_BACKGROUND_SYNC.md` §6, belegt):
    FSEvents-Latenz 3 s, Throttle 100 ms, `Throttle(FiveSeconds)`-Debounce,
    Poll `BackupWatcherPollIntervalInSeconds` 30 s, Re-arm bei Fehler,
    pausierte Ziele bekommen keinen Watcher.
  - **Reaktion pro Event: „file add / edit per Event abfragen"**
    (User-Vorgabe) — nur die betroffene Datei prüfen, kein kompletter
    Ordner-Durchlauf und kein erneutes Hashen aller Dateien.
  - **Stat-Check vor dem Hash:** die Skip-Entscheidung fragt zuerst (path,
    size, mtime) gegen den persistierten Index; nur unbekannte/geänderte
    Dateien werden gelesen und MD5-gehasht. `size` und `mtime` stehen in der
    Tabelle bereits, blieben aber ungenutzt (`src/dedup.rs`). Das MD5 bleibt
    die Identität (`../original/SPEC_DEDUP.md` §1); der Stat-Treffer ist nur
    der schnelle Pfad.
  - **Kaltstart (Event-Ersatz fürs Ausschalten):** beim Start bzw. nach
    Anmeldung wird **einmal** pro Ordner enumeriert und per Stat-Check
    entschieden, was fehlt. Kein Timer.
  - **Sicherheitsnetz beim Start (User-Entscheidung 2026-10-14):** pro
    Backup-Ordner wird das **größte Änderungsdatum** (`max mtime`) des
    letzten Durchlaufs **in der SQLite gespeichert** (neue Tabelle, neben dem
    MD5-Index). Beim Start genügt ein **reiner Metadaten-Durchlauf**
    (`walkdir` ohne Lesen der Dateien): ist das aktuelle `max mtime`
    **nicht größer** als das gespeicherte, ist der Ordner nachweislich
    unverändert und es wird **nichts** gehasht, keine DB-Abfrage pro Datei,
    kein Upload-Plan. Wird es größer, läuft nur für diesen Ordner der
    Stat-Check-Pfad (et nur geänderte Dateien werden gelesen + gehasht).
    **Angeborene Grenze (wie im Original, `SPEC_CHANGE_DETECTION.md`):**
    Kopien, die die mtime erhalten (`rsync --times`, Finder-Import), erhöhen
    `max mtime` nicht. Wer das braucht, startet manuell — der
    Wasserzeichen-Treffer ist der schnelle Pfad, keine Garantie.
  - **Drag & Drop ist ein einmaliger Upload** (User-Klarstellung 2026-10-14):
    kein Watch, kein Wasserzeichen, keine dauerhafte Sicherung — nur die
    temporäre Queue bis zum Abschluss (siehe [one-time-upload.md](one-time-upload.md)).
  - **Pause blockt auch Events** (globale Pause, siehe [pause.md](pause.md)).
  - **Event-Puffer: unbegrenzt, kein Verwerfen (User-Entscheidung 2026-10-14).**
    Events, die während eines laufenden Uploads **oder einer Pause** eintreffen,
    werden geparkt und beim nächsten Durchlauf vollständig abgearbeitet. Es
    gibt **keine Obergrenze** (die frühere 500er-Kappe `WATCH_PENDING_MAX`
    ist entfernt): die Liste wächst dynamisch mit der Zahl der
    **verschiedenen** geänderten Pfade (~1,2 MB bei 10.000, ~12 MB bei
    100.000 — vernachlässigbar), Dedup per `HashSet<PathBuf>` in O(1) statt
    vorher O(n²). Bursts von **10.000+** Dateien (Kamera-Import, Ordner-Kopie,
    lange Pause) sind damit vollständig abgedeckt; verworfen wird nichts.
    Ein neuer Burst **ersetzt** den geparkten Bestand nicht mehr, sondern wird
    mit ihm **vereinigt** (das war ein echter Verlust-Bug: bei mehreren Bursts
    während einer Pause überlebte nur der letzte); erlaubt `can_start` einen
    Lauf, geht der vereinigte Bestand in **einen** Lauf. Umsetzung:
    `src/backup.rs` (`park_paths`, `pending`/`pending_seen`), Commit `2eea96c`,
    Verify Runde 1 APPROVE.
  - **Ordner während eines laufenden Uploads hinzugefügt (User-Entscheidung
    2026-10-17):** ein Backup-Ordner, den der Nutzer während eines Laufs
    hinzufügt (Dialog „Speichern" oder Zeilenmenü „Jetzt scannen und
    hochladen"), wird **nicht mehr still verworfen**. Befund: drei Ordner
    schnell nacheinander hinzugefügt, nur der erste wurde gesichert;
    Entscheidung: „Einlesen parallel, in den laufenden Lauf einhängen". Der
    Supervisor liest den Ordner **parallel** zum laufenden Upload ein
    (`UploadPlan::build_since` mit Wasserzeichen `None`, `handle_scan_requests`,
    `src/backup.rs`) und hängt seine Dateien über `inject_into_live_run` in den
    **laufenden** Lauf (Zeilen, Worker-Queue, `folder_opts`, `folder_total`,
    `total`). `scans_in_flight` hält den Lauf dafür offen; die Versiegelung
    (`sealed`) passiert unter derselben Sperre, damit nichts in eine tote Queue
    läuft. Läuft keiner mehr, wird der Ordner geparkt (`pending_scans`) und beim
    nächsten erlaubten Durchlauf als **normaler** Lauf gestartet (kein
    Wasserzeichen-Fortschritt, wie beim Event-Pfad). **Grenze:** ohne Supervisor
    (Hintergrund-Sync aus) bleibt es beim alten Verhalten (`Es läuft bereits ein
    Upload.`); automatisch geholt wird der Ordner nur **mit** Hintergrund-Sync
    (Sicherheitsnetz beim nächsten App-Start), ohne ihn nur durch einen manuellen
    Lauf.
  - **Rückkehr eines fehlenden Laufwerks (User-Entscheidung 2026-10-17):** ist
    ein Backup-Ordner als `NotFound` geführt und kommt sein Pfad zurück, wird
    sein **vorhandener Bestand vollständig** eingelesen — flankengetriggert
    (nur `NotFound` → vorhanden; ein `Paused`-Ordner ist **kein** Rückkehr-Fall),
    genau **einmal** pro Rückkehr. Der Supervisor erkennt den Übergang in
    `sync_watchers` (`returned_from_missing`/`lift_returned_sources`) und parkt
    den Pfad in `pending_scans`; `handle_scan_requests` arbeitet ihn im selben
    Durchlauf ab (parallel zu einem laufenden Upload oder als normaler Lauf).
    Die **globale Pause** wird über `can_start` respektiert: bei gedrückter
    Pause bleibt der Ordner geparkt und läuft beim Aufheben von selbst (innerhalb
    `WATCH_POLL`, 30 s). Original-Vorbild: `HandleDeviceArrival` /
    `HandleDisconnectedDevice` (`../original/SPEC_MISSING_SOURCE.md`
    §3.3.3/§3.3.6, `../original/SPEC_BACKGROUND_SYNC.md` §3,
    `ForceScan = !IsPaused`). **Grenze:** ohne Hintergrund-Sync gibt es den
    Supervisor nicht — der Pfad fehlt dann ganz.
  - **Verworfen:** der periodische Hintergrund-Scan als Erkennungsmechanismus
    (User 2026-10-14: „file add / edit per event abfragen"); ein
    `BackupScanIntervalInHours`-Takt.
  - **Offen:** Sicherheitsnetz für verpasste Events (Watcher-Limit,
    ausgeworfenes Laufwerk, Netzwerk-Mount) — als optionaler Schalter, noch
    nicht entschieden.
  - **Doku:** `../original/SPEC_CHANGE_DETECTION.md` (Clean-Room recon, in
    Arbeit).

## Original reference

- `../original/SPEC_CHANGE_DETECTION.md` — Change detection (events, stat check, watermark).
- `../original/SPEC_BACKGROUND_SYNC.md` — Background sync (FSEvents, cadence, pause).
- `../original/SPEC_FILE_IDENTITY.md` — File identity (MD5, stat check, path binding).
- `../original/SPEC_DEDUP.md` — Dedup mechanics.
