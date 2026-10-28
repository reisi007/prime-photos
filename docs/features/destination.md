# Upload destination

The destination determines where files land in the Amazon Photos account.
Prime Photos implements local-path mirroring for configured backups and one-time
drag-and-drop uploads. Existing uploads in `Pictures/Web` are not automatically
migrated.

**Implementation and verification status (2026-10-28):** Implemented. Targeted
tests were run locally on macOS Apple Silicon; no Windows or Linux verification
is claimed.

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

### Prime Photos behavior

- Apply the same mapper to **configured backup folders and one-time
  drag-and-drop uploads**. Applying it to one-time uploads is an intentional
  extension, not original parity.
- Build the mirror from normalized **absolute** paths, without shortening the
  home directory, below `Backup/<MachineName>/`. POSIX root `/` remains implicit,
  with its absolute path components following it. Windows drive prefixes are one
  segment (`C`); UNC paths keep `UNC/<server>/<share>` as components. All output
  segments are checked for emptiness, dot traversal, separators, and control
  characters; invalid paths fail closed.
- Reuse matching cloud folders by parent and create missing ones. Include the
  selected backup folder itself (even when empty). Each file uploads under the
  mirror of its **actual local parent**, not flat under the selected root; for a
  one-time file, only its parent hierarchy is mirrored and its basename remains
  the uploaded file name.
- Resolve/create targets before file uploads, including empty configured
  backups, fully deduplicated runs, and files injected into an active run. On a
  create-folder 409, load the node named by `info.nodeId`; retain child relisting
  as a fallback if that lookup fails.
- If target resolution is missing or fails, abort affected uploads; never
  silently fall back to `Pictures/Web`. A file's failure is counted once across
  destination-root handling and its retry.
- Keep cross-path content dedupe. A content skip does not move or create a cloud
  node at the new mirrored path; a previously uploaded node remains at its old
  path if the local file is moved or renamed.

## Decisions

These are **Prime Photos product decisions**, distinct from observations of the
original client:

- **Zielfeld aus dem Dialog (F9, Welle 5.7):** „Ziel für Datensicherung:" samt
  „Ändern …" ist entfernt; das Ziel wird automatisch bestimmt. Die
  implementierte Spiegelung ersetzt das frühere feste Ziel `Pictures/Web` für
  neue Uploads. **Verworfen:** das Feld behalten und bei `Pictures/Web` bleiben.
- **Spiegelungsumfang (User, 2026-10-28):** konfigurierte Sicherungsordner und
  einmalige Drag-and-drop-Uploads verwenden denselben Mapper. Das ist für
  One-time eine bewusste Erweiterung gegenüber dem Original.
- **Pfadbasis (User, 2026-10-28):** plattformübergreifend normalisierte absolute
  Pfade auf macOS, Windows und Linux; keine Home-relative Darstellung. Absolute
  Komponenten und System-/Volume-Präfixe werden als Cloud-Ordnerhierarchie
  abgebildet.
- **Windows-Laufwerk (User, 2026-10-28):** ein Laufwerk wie `C:\` wird als
  genau ein einzelnes Cloud-Segment `C` dargestellt; es gibt keinen zusätzlichen
  Präfix.
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
API-Fähigkeit. Die aktuelle Implementierung nutzt diesen bestätigten Create-Flow
und behandelt 409 über `info.nodeId` mit Relisting-Fallback.

## Original reference

- `../original/SPEC_UPLOAD_DESTINATION.md` — Originalverhalten und statische Recon.
- `../original/SPEC_CONTENT_FILTER_DESTINATION.md` — Content filter and destination.
