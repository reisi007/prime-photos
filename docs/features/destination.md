# Upload destination

The upload destination determines where files land in the Amazon Photos
account. The current implementation uses a fixed path (`Pictures/Web`); the
original's mirroring scheme is documented but not yet implemented.

## How it works

- The destination is automatically determined (today `Pictures/Web`). There is
  no user-facing field for it in the backup configuration dialog.
- The **original mirroring** (`Backup/<Macname>/<Quellpfad>`) is a **separate
  task** after `../original/SPEC_UPLOAD_DESTINATION.md`.

## Decisions

- **Zielfeld aus dem Dialog (F9, Welle 5.7):** „Ziel für Datensicherung:" samt
  „Ändern …" ist **entfernt**; das Ziel wird automatisch bestimmt (heute
  `Pictures/Web`). Die **Original-Spiegelung** (`Backup/<Macname>/<Quellpfad>`)
  ist ein **eigener Auftrag** nach `../original/SPEC_UPLOAD_DESTINATION.md`.
  **Verworfen:** das Feld behalten und bei `Pictures/Web` bleiben.

## Original reference

- `../original/SPEC_UPLOAD_DESTINATION.md` — Upload destination and resolution.
- `../original/SPEC_CONTENT_FILTER_DESTINATION.md` — Content filter and destination.
