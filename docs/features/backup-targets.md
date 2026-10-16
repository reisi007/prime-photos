# Backup targets

A backup target pairs a local folder with an upload policy. Targets are
stored in `folders.toml` under `~/Library/Application Support/prime_photos/`.
The dialog is shared between creating and editing a target.

## How it works

- Source folder, name, and — per target — a content filter, an interval, a
  duplicate switch, and a name-conflict policy.
- Targets can be paused individually; the whole app can be paused globally.
- The dialog is shared between creating and editing a target.
- Fields whose backing service does not exist yet are shown **disabled** with
  a "Noch nicht verfügbar" hint, keeping the full original layout.
- **Content filter (F2, Welle 5.7):** two checkboxes with original labels
  "Fotos sichern" (on, selectable) / "Videos sichern" (visible, **disabled**);
  "Alles" is not selectable in the original. New type `ContentFilter` in
  `folders.rs` (serde lowercase, Default `Photos`, `from_selection` like the
  original), **per target** persisted. `is_supported_image` remains the
  effective filter — the field is parity + future.
- **Zielfeld aus dem Dialog (F9, Welle 5.7):** "Ziel für Datensicherung:" with
  "Ändern …" is **removed**; the destination is automatically determined
  (today `Pictures/Web`). The **original mirroring**
  (`Backup/<Macname>/<Quellpfad>`) is a **separate task** after
  `../original/SPEC_UPLOAD_DESTINATION.md`.
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

- **Inhaltsfilter wie das Original (F2, Welle 5.7, `0d486e2`):** zwei
  Checkboxen mit Original-Labels „Fotos sichern" (an, wählbar) / „Videos
  sichern" (sichtbar, **deaktiviert**); „Alles" ist im Original nicht wählbar.
  Neuer Typ `ContentFilter` in `folders.rs` (serde lowercase, Default
  `Photos`, `from_selection` wie das Original), **pro Ziel** persistiert.
  Wirksam bleibt `is_supported_image` — das Feld ist Parität + Zukunft.

- **Zielfeld aus dem Dialog (F9, Welle 5.7).** „Ziel für Datensicherung:" samt
  „Ändern …" ist **entfernt**; das Ziel wird automatisch bestimmt (heute
  `Pictures/Web`). Die **Original-Spiegelung** (`Backup/<Macname>/<Quellpfad>`)
  ist ein **eigener Auftrag** nach `../original/SPEC_UPLOAD_DESTINATION.md`.
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
