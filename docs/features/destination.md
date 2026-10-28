# Upload destination

The destination determines where files land in the Amazon Photos account. The
current implementation still uses the fixed path `Pictures/Web`; the approved
Prime Photos path mirroring is documented here but is not implemented yet.
Existing uploads in `Pictures/Web` are not automatically migrated.

## How it works

### Observed original behavior

Configured backups in the original derive a cloud-folder hierarchy from the
local source path, rooted at `Backup/<MachineName>/`. The selected source folder
is included in the final cloud tree even though its name is not in the mapper's
suggested segment array; nested folders are created below it. Original one-time
uploads are a separate behavior: `EnumerationService.OneTimeMediaImport` uses
`Pictures/Desktop/Media Import/<yyyy-MM-dd>`, while generic one-time uploads get
an explicit `DestinationNodeId`. These are static code findings, not runtime
observations. Details: `../original/SPEC_UPLOAD_DESTINATION.md` §§3–4.

### Prime Photos target behavior (approved, not yet implemented)

- Apply the same destination mapper to **configured backup folders and
  one-time drag-and-drop uploads**. Applying it to one-time uploads is an
  intentional extension, not original parity.
- Build the mirror from **normalized absolute paths**, not paths relative to the
  home directory. Support macOS, Windows, and Linux; normalize separators and
  path components, and retain absolute path components and any system/volume
  prefixes as cloud-folder hierarchy below `Backup/<MachineName>/`.
- Reuse matching cloud folders and create missing ones. Keep the source-folder
  hierarchy, so each file uploads under its corresponding mirrored parent,
  rather than flattening every file under the mirror's base node.
- Resolve/create the target even for an empty configured backup and when every
  file is skipped by dedupe. If target resolution is missing or fails, abort the
  upload; never silently fall back to `Pictures/Web`.
- Keep cross-path content dedupe. A content skip does not move or create a cloud
  node at the new mirrored path; a previously uploaded node remains at its old
  path if the local file is moved or renamed.

## Decisions

These are **Prime Photos product decisions**, distinct from observations of the
original client:

- **Zielfeld aus dem Dialog (F9, Welle 5.7):** „Ziel für Datensicherung:" samt
  „Ändern …" ist entfernt; das Ziel wird automatisch bestimmt. Die bisherige
  Implementierung nutzt `Pictures/Web`; die beschlossene Spiegelung ersetzt
  dieses feste Ziel für neue Uploads. **Verworfen:** das Feld behalten und bei
  `Pictures/Web` bleiben.
- **Spiegelungsumfang (User, 2026-10-28):** konfigurierte Sicherungsordner und
  einmalige Drag-and-drop-Uploads verwenden denselben Mapper. Das ist für
  One-time eine bewusste Erweiterung gegenüber dem Original.
- **Pfadbasis (User, 2026-10-28):** plattformübergreifend normalisierte absolute
  Pfade auf macOS, Windows und Linux; keine Home-relative Darstellung. Absolute
  Komponenten und System-/Volume-Präfixe werden als Cloud-Ordnerhierarchie
  abgebildet.
- **Fehlerverhalten (User, 2026-10-28):** wenn das Ziel fehlt oder nicht
  aufgelöst werden kann, Upload abbrechen; kein stiller Fallback auf
  `Pictures/Web`.
- **Leere bzw. deduplizierte Läufe (User, 2026-10-28):** den gespiegelten
  Zielordner auch bei einem leeren konfigurierten Backup und dann anlegen, wenn
  alle Dateien durch Dedupe übersprungen werden.
- **Keine Migration:** bestehende Uploads unter `Pictures/Web` werden nicht
  automatisch verschoben.
- **Cross-path Dedupe bleibt bestehen:** ein Content-Skip verschiebt keinen
  Cloud-Knoten. Eine bei gleichem Inhalt verschobene/umbenannte lokale Datei
  erzeugt daher keinen Knoten am neuen gespiegelten Pfad; ihr vorhandener
  Cloud-Knoten bleibt am alten Pfad. Hintergrund und Belege:
  `duplicates-conflicts.md`.

## Live API evidence

Am 2026-10-16 konnte der Cookie-Host Ordner anlegen: `POST /drive/v1/nodes` mit
`kind: "FOLDER"` und Cookies allein ergab **201**. Der Probe-Ordner wurde
anschließend gelöscht (**202**); ein Access-Token war nicht nötig. Das belegt die
API-Fähigkeit, nicht die Implementierung der Spiegelung.

## Original reference

- `../original/SPEC_UPLOAD_DESTINATION.md` — Originalverhalten und statische Recon.
- `../original/SPEC_CONTENT_FILTER_DESTINATION.md` — Content filter and destination.
