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

- **Live-Beleg Upload-Ziel-Spiegelung (2026-10-16):** der Cookie-Host **kann**
  Ordner anlegen — `POST /drive/v1/nodes` mit `kind: "FOLDER"` und **Cookies
  nur** ⇒ **201** (Probe-Ordner angelegt, danach gelöscht ⇒ 202); ein
  Access-Token ist **nicht** nötig. Damit ist die Umsetzung der
  Original-Spiegelung `Backup/<Macname>/<Quellpfad>` (Segmente bei Bedarf
  anlegen) **frei**. Bestehende Uploads in `Pictures/Web` bleiben. Die
  **Umsetzung** selbst bleibt offene Arbeit im Ledger.

- **Bauhinweis Spiegelung: der Content-Skip ist kein Move im Konto (User-Frage
  2026-10-17):** sobald `Backup/<Macname>/<Quellpfad>` gebaut wird, heißt der
  Content-Skip konkret: eine **verschobene/umbenannte** Datei (gleicher MD5,
  anderer Quellpfad) bekommt **keinen** Knoten am **neuen** gespiegelten Pfad,
  und ihr alter Knoten bleibt unter dem **alten** Pfad liegen — der gespiegelte
  Baum folgt Quell-Verschiebungen also **nicht** (Parität mit dem Original, das
  ebenfalls nur überspringt). Entscheidung und Belege:
  `duplicates-conflicts.md` (Cross-path-Entscheidung); offener Punkt:
  `AGENTS.todo.md` → „Offen aus dem geschlossenen Test 2026-10-16", Punkt 1
  (Hinweis seit 2026-10-17).

## Original reference

- `../original/SPEC_UPLOAD_DESTINATION.md` — Upload destination and resolution.
- `../original/SPEC_CONTENT_FILTER_DESTINATION.md` — Content filter and destination.
