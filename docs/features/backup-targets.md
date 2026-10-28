# Backup targets

A backup target pairs a local folder with an upload policy. Targets are
stored in `folders.toml` under `~/Library/Application Support/prime_photos/`.
The dialog is shared between creating and editing a target.

## How it works

- Source folder, name, and — per target — a content filter, an interval, a
  duplicate switch, and a name-conflict policy.
- Targets can be paused individually; the whole app can be paused globally.
- The dialog is shared between creating and editing a target.
- Fields that remain visible despite their backing service not being implemented
  yet (such as Videos or the interval, where shown) are **disabled** with a
  "Noch nicht verfügbar" hint. The destination field is **removed**; the
  destination is mirrored automatically (`destination.md`).
- **Content filter (F2, Welle 5.7):** two checkboxes with original labels
  "Fotos sichern" (on, selectable) / "Videos sichern" (visible, **disabled**);
  "Alles" is not selectable in the original. New type `ContentFilter` in
  `folders.rs` (serde lowercase, Default `Photos`, `from_selection` like the
  original), **per target** persisted. `is_supported_image` remains the
  effective filter — the field is parity + future.
- **Zielfeld aus dem Dialog (F9, Welle 5.7):** "Ziel für Datensicherung:" with
  "Ändern …" is **removed**. Configured backups and one-time drag-and-drop
  uploads automatically mirror their local paths under
  `Backup/<MachineName>/`; existing nodes in `Pictures/Web` are not migrated.
  Full path and target-resolution semantics: `destination.md`.
- Folder scans enforce `is_supported_image` (single source in `folders.rs`).
  There is **no file picker** in HEAD (only `FileDialog::pick_folder()`) and
  `supported_extensions()` is currently unwired.
- **Folder reconciliation (observed in the client's local DBs):** local
  SQLite state, not an HTTP call. `backup` table (schedule_path,
  relative_path, node_id, size, modified_date, status, last_scan_time) +
  `uploads` table (path, status, precomputed_md5, node_id) + `cloud.db` node
  cache, synced via `/drive/v1/changes` and `/drive/v1/nodes`.
  `BackupScanIntervalInHours = 336`.

## Decisions

- **Backup configuration window (user decision 2026-10-13):** build the full
  "Datensicherungseinstellungen" window now (destination, content filter,
  interval, dedup, conflict handling, save/cancel), and **share the form
  logic** with the create-backup flow (same dialog for create and edit)
  instead of a separate row-menu. Fields whose backend does not exist yet
  (destination, content filter, interval) are shown **disabled** with a
  "Noch nicht verfügbar" hover (user decision 2026-10-13: "volle Anordnung,
  Rest ausgegraut"); functional now: source folder (read-only when editing),
  dedup, name-conflict policy.

- **Missing source ⇒ `NotFound`, not PAUSED (user decision 2026-10-17):** a
  target whose source folder is gone (external drive unplugged, folder renamed
  or deleted) is flagged `FolderStatus::NotFound` and shown as
  **"Ordner nicht gefunden"** in **AMBER** — a state of its own, deliberately
  distinct from the global PAUSED (`Config::paused`) and its per-row
  "ANGEHALTEN" wording. The rule (`source_status` in `backup.rs`) is total: a
  missing path wins from any state; once the path is back, both `NotFound` and
  a `Paused` written by an older `folders.toml` lift to `Idle` (the
  never-advanced watermark makes the next cold start re-scan the missed
  files), while every other state stays untouched. `NotFound` claims **no
  progress** (`folder_progress_text` is empty); the upload gate and watermark
  are unchanged — a missing path simply yields no files. Original reference:
  `ScheduleError.LocalFolderNotFound`
  (`../original/SPEC_MISSING_SOURCE.md` §3.1/§3.2).

  **Live open (2026-10-17):** a live run of the installed bundle (0.1.0) showed a
  target whose drive was removed correctly as "Ordner nicht gefunden" in AMBER
  (not "ANGEHALTEN"), **but** the run did **not** resume on its own when the
  drive returned, and the row state was inconsistent — one folder sat on
  "Warten", one showed no status at all.

  **Resume on return (Welle 5.33, 2026-10-17):** the missing half was the scan
  trigger, not the status lifting — `source_status` was already applied live in
  `sync_watchers` (every `WATCH_POLL` pass, `src/backup.rs`). The return is now
  edge-triggered: `returned_from_missing` fires only for `NotFound` → present,
  `lift_returned_sources` lifts the state to `Idle` and hands the path back, and
  `watch_loop` parks it (`pending_scans`) so its **full stock** is read in once —
  no watermark, no "only changes" — in the same pass, parallel to a live run or
  as a normal run. A `Paused` folder is **not** a return (only `NotFound` →
  present). The **global pause** is respected: with the pause set the folder
  stays parked (`can_start` in `handle_scan_requests`) and starts on its own as
  soon as the pause is lifted (within `WATCH_POLL`, 30 s). Original reference:
  the device-return path in `../original/SPEC_MISSING_SOURCE.md`
  §3.3.3/§3.3.6 (`HandleDeviceArrival` / `HandleDisconnectedDevice`: refresh the
  path, lift the pause, `ResumeUploadsForSchedule`) and
  `../original/SPEC_BACKGROUND_SYNC.md` §3 ("external-drive arrival",
  `ForceScan = !IsPaused`).

  **Still open:** the resume on a **real** drive return is not live-confirmed
  (no live test on a removable drive yet); the row-state inconsistency ("Warten"
  / no status) is a separate UI observation. Open item: `AGENTS.todo.md` (live
  acceptance 2026-10-17 — "Ordner nicht gefunden": resume on drive return).

- **Inhaltsfilter wie das Original (F2, Welle 5.7, `0d486e2`):** zwei
  Checkboxen mit Original-Labels „Fotos sichern" (an, wählbar) / „Videos
  sichern" (sichtbar, **deaktiviert**); „Alles" ist im Original nicht wählbar.
  Neuer Typ `ContentFilter` in `folders.rs` (serde lowercase, Default
  `Photos`, `from_selection` wie das Original), **pro Ziel** persistiert.
  Wirksam bleibt `is_supported_image` — das Feld ist Parität + Zukunft.

- **Zielfeld aus dem Dialog (F9, Welle 5.7).** „Ziel für Datensicherung:" samt
  „Ändern …" ist **entfernt**; konfigurierte Sicherungen und einmalige
  Drag-and-drop-Uploads erhalten automatisch ein gespiegeltes Ziel unter
  `Backup/<MachineName>/`. Bestandsknoten in `Pictures/Web` werden nicht
  migriert. Vollständige Pfad- und Auflösungssemantik: `destination.md`.
  **Verworfen:** das Feld behalten und bei `Pictures/Web` bleiben.

- **Supported photos:** jpg, jpeg, png, heic, heif, tif, tiff, gif, webp, bmp
  plus common RAW (cr2, cr3, arw, nef, dng, raf, rw2, orf, srw, pef, and
  others). `.xmp` sidecars are explicitly rejected.

- **Folder scans enforce `is_supported_image`** (single source in `folders.rs`).
  There is **no file picker** in HEAD (only `FileDialog::pick_folder()`) and
  `supported_extensions()` is currently unwired.

## Original reference

- `../original/SPEC_FOLDERS.md` — Folder scan and filter logic.
- `../original/SPEC_CONTENT_FILTER_DESTINATION.md` — Content filter and destination.
- `../original/SPEC_UPLOAD_DESTINATION.md` — Upload destination resolution.
