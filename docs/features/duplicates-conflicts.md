# Duplicates and name conflicts

Two orthogonal mechanisms, matching the original: **content deduplication
(MD5)** and **name-conflict resolution** (four policies). A persistent local
SQLite index is the primary defence; the server 409 is a safety net.

## How it works

- **Content deduplication (MD5).** A file's identity is its content, tracked
  in a persistent local SQLite index. Same path + same MD5 → skipped as
  unchanged; same path + different MD5 → uploaded; content already known under
  a **different** path (a renamed or moved file) → skipped locally as
  "bereits im Konto" and recorded in the index so later scans take the fast
  path. Hashing is only done for files the stat check cannot resolve.
- **Name conflicts** (same name, different content) offer all four original
  policies: ask, ignore, save as a new file (server renames), or replace.
  "Ask" prompts per file with a "don't ask again for this folder" override
  and a 60-second timeout that falls back to ignoring.
- The **safe replace rule**: the `PUT /v2/upload/<nodeId>` may overwrite the
  node named by a 409 `NameAlreadyExists` **only** when the local index binds
  exactly this path to exactly this node — then the same local file was
  changed. If the name matches a **foreign** file from a different path, the
  upload is **renamed** (`SaveAsNew`/wire `RENAME`), never deleted.

## Decisions

- **Dedup (user decision 2026-10-13):** a **persistent local MD5 index in
  SQLite** (like the original) decides skips (same path + same MD5 → skip;
  same path + different MD5 → re-upload; new file → upload). Server 409 is
  only a safety net, treated as "Ignored/Zuvor hochgeladen", not an error
  (mirrors the original's client-side `PreUploadCheck`).

- **Content identity (user decision 2026-10-14):** a file's identity is its
  **content**. A file whose **MD5 is already in the index under another path**
  (renamed/moved) is **skipped locally, before the network**, and counted as
  **"bereits im Konto"** (`conflict_skipped`) — not as "unverändert", because
  the file itself was never uploaded. Same path + same MD5 stays
  "unverändert"; same path + a different MD5 is uploaded. The local check
  demotes the server's `DuplicatesConflictError` to a pure safety net;
  `dedup = true` is the switch for both. **Persistence (user decision
  2026-10-14):** such a path is **recorded in the index with a marker**
  (`origin = 'content_skip'`, next to the real uploads) so the fast stat path
  applies on later scans and the display keeps saying "bereits im Konto" in
  **every** run — otherwise the folder (~3,6 GB for "Sport") would be read and
  hashed again on each scan. That is a deliberate one-column schema change and
  matches the original, whose `uploads` table also carries a `status` column.

- **Conflict handling (user decision 2026-10-13):** all **four original
  options** per upload target (Nachfragen / Ignorieren / Als neue Datei
  speichern / Ersetzen). Wire mapping (corrected 2025-10-15 — the earlier
  `NONE`/`RENAME`/`OVERWRITE` line was stale, see the 2.2/2.2b decisions):
  `Nachfragen`/`Ignorieren`/`Ersetzen` ⇒ `conflictResolution=NONE`;
  `SaveAsNew` ⇒ `RENAME`. **`OVERWRITE` is rejected by the cookie host**
  (`POST` ⇒ 404, empty body) and is never sent; "Ersetzen" is carried out in
  place with `PUT /v2/upload/<nodeId>` instead. Replaces the current
  hard-coded `RENAME`.

- **Conflict-handling default (user decision 2025-10-15, changed from the
  2025-10-14 `SaveAsNew` default):** a file's identity is its **content**.
  `dedup = true` keeps meaning *same content ⇒ never uploaded* (MD5:
  server-side `409` + `errorCode = "DuplicatesConflictError"` and the local
  cross-path MD5 check). For a **name** conflict (same name, **different**
  content) new targets now default to **`Replace`**: the POST sends
  `conflictResolution=NONE`, the 409 `NameAlreadyExists` names the existing
  node, and the follow-up `PUT /v2/upload/<nodeId>` (POST query set, §2.2b)
  overwrites it **in place** — same node id, same name, the old content is
  gone. Chosen deliberately with the risk known: two **different** photos
  sharing a name (documented case: `27_ÖFB-Reisinger_01.jpg` exists twice with
  different MD5) ⇒ the one already in the account is replaced. `SaveAsNew`
  (wire `RENAME`) stays selectable per target. **Two different defaults — do
  not merge them:** the *Application Default* (`Config.conflict_resolution_default`)
  is `Replace`, while the *file* fallback — the enum's `#[default]`, used by
  `#[serde(default)]` for a `folders.toml` entry **without** the key — stays
  `SaveAsNew`, so pre-existing targets are never silently switched to
  overwriting. **Beide lokalen Ablagen stehen auf `Replace` (User-Entscheidung
  2026-10-16, „alles auf replace"):** `config.toml` trägt
  `conflict_resolution_default = "replace"`, „Sport" trägt
  `conflict_resolution = "replace"` — von Hand umgestellt, **kein**
  Migrationscode. Anlass: ein zweiter JPEG-Export oder ein DNG mit geänderten
  Metadaten gehört **ersetzt**, nicht dupliziert. **Verworfen:** den
  Application Default dafür wieder auf `SaveAsNew` drehen. **Watch out:**
  `ConflictResolution` uses `#[serde(rename_all = "lowercase")]`, so the file
  value is `saveasnew`/`replace`, **not** `save_as_new` — renaming the serde
  rule would break existing config files.

- **Sichere Ersetz-Regel (User-Entscheidung 2026-10-16):** der
  `PUT /v2/upload/<nodeId>` darf den vom 409 `NameAlreadyExists` benannten
  Knoten **nur** überschreiben, wenn der **lokale Index genau diesen Pfad an
  genau diese Node** bindet — dann wurde dieselbe lokale Datei geändert
  (User-Wortlaut: „nie was oben ist löschen, außer es ist die gleiche Datei
  physisch relativ zum gesicherted Pfad, dann überschreiben"). Trifft der Name
  auf eine **fremde** Datei aus einem anderen Pfad, wird **umbenannt**
  (`SaveAsNew`/Wire `RENAME`), nie gelöscht. Träger der Bindung ist die neue
  Spalte **`node_id`** im Dedup-Index (Parität zum Original, dessen
  `uploads`-Tabelle ebenfalls `node_id` führt): `record_upload` schreibt die
  Node des Erfolgs, `COALESCE(excluded.node_id, node_id)` lässt eine bekannte
  Bindung bestehen, `node_id_for(path)` ist die Abfrage für die Regel.
  **Altdaten ohne Node-Id sind ungebunden** und werden beim ersten
  Inhaltswechsel einmalig umbenannt — vom User ausdrücklich akzeptiert
  (`../dev/DEV_NOTES.md`). Die Regel beschränkt **nur die automatische
  Ordner-Politik**: eine **ausdrückliche** Antwort „Ersetzen" im Konflikt-Dialog
  ist die Zustimmung für *diese* Datei und überschreibt sie (User-Entscheidung
  2026-10-16). Ein **Inhaltsduplikat** bleibt auch dort „bereits im Konto" —
  dieselben Bytes sind schon da, es gibt nichts zu überschreiben.
  **Verworfen:** den Application Default nur deshalb auf `SaveAsNew`
  zurückdrehen (die Sicherheit kommt aus der Bindung, nicht aus einem anderen
  Default); bei fehlender Bindung **nichts** tun und den Server entscheiden
  lassen.

- **Drei Ebenen der Konflikt-Standardwahl (User-Entscheidung 2025-10-15,
  bestätigt 2026-10-16):**
  1. **Application Default** — neues Feld in `Config` (`config.toml`),
     einstellbar in **Einstellungen → Upload** (Radiogruppe mit den vier
     vorhandenen Optionslabels, das Muster der Seite; umgesetzt in `db68cff`),
     Default **`Replace`**. Er ist die **Vorauswahl beim Anlegen eines
     Ziels** und gilt für die **Einmal-Queue (Drag&Drop)**.
  2. **Ordner-Default** — das bestehende `BackupFolder.conflict_resolution`
     (`folders.toml`). Hierher schreibt „**Nicht noch einmal fragen**" seine
     Antwort (in den Ordner der gefragten Datei); der nächste Lauf fragt dort
     nicht mehr.
  3. **Kein Per-Datei-Default** — einzelne Dateien haben keine eigene
     Einstellung. Innerhalb eines Laufs bleibt ein angekreuztes „Nicht noch
     einmal fragen" **run-weit** wirksam (unverändert); persistiert wird es
     **pro Ordner**.

- **Globaler Dedupe-Default (User-Entscheidung 2025-10-15):** neues Feld
  `Config.dedup_default` (Default **`true`**, ebenso der serde-Default, damit
  eine alte `config.toml` weiter lädt), einstellbar in **Einstellungen →
  Upload** neben der Konflikt-Standardwahl. Er ist die **Vorauswahl beim Anlegen
  eines Ziels** und gilt für die **Einmal-Queue (Drag&Drop)** — dieselbe Rolle
  wie der Application Default bei den Konflikten. Der User will **nie
  Duplikate** hochladen, `true` bleibt also die Vorgabe. (Das Original hat nach
  dem Onboarding `false`; unsere `true`-Vorgabe ist die dokumentierte,
  bewusste Abweichung.) Bestehende Ziele behalten ihren gespeicherten
  `dedup`-Wert.

- **Wortlaut-Parität beim Konflikt-Dialog (User-Entscheidung 2025-10-15):** die
  Option `SaveAsNew` heißt in den **Ziel-/Einstellungs-Radiogruppen** wie im
  Original „**Neue Datei automatisch umbenennen**", der Knopf im
  **Konflikt-Dialog** bleibt „Als neue Datei speichern" — auch das ist
  Original-Wortlaut (das Original benennt dieselbe Option an beiden Stellen
  unterschiedlich). Das Häkchen nennt den **Ordner**: „Nicht noch einmal nach
  ‚<Ordner>'-Ordner fragen" (Original-Muster `…donotaskagain_checkbox_text`);
  bei der **Einmal-Queue** (kein Ordner) bleibt der neutrale Text „Nicht noch
  einmal fragen", weil dort nichts persistiert wird. **Bewusst abweichend
  (dokumentiert in `../original/SPEC_CONFLICT_OPTIONS.md` §5):** wir fragen
  **pro Datei** (60-s-Timeout ⇒ Ignorieren), das Original zeigt **nach dem
  Lauf** ein gebündeltes Sheet („N Dateinamenkonflikte in …", eine Antwort für
  alle) plus Toast; ein nachträgliches Sheet wäre ein UI-Umbau und gehört zu
  Welle 3.

- **Duplikate & Dateikonflikte: alle Varianten wie das Original (User-Entscheidung
  2026-10-14):** Recon: `../original/SPEC_DUPLICATES.md` (Mechanismen,
  Erkennungsweg, 409, Gap-Tabelle) und `../original/SPEC_CONFLICT_OPTIONS.md` (die
  vollständige Varianten-Liste mit Abnahmekriterium). Kernbefund: das
  Original hat **zwei orthogonale** Mechanismen — **Inhalts-Dedupe** (MD5,
  **über Pfade hinweg**, `GetNodeByMd5`) und **Namenskonflikt** (exakter Name +
  Parent, **pro Ziel**, global gibt es nichts).
  **User-Entscheidung: alles umsetzen, was das Original kann.** Offene Punkte
  stehen als Checkliste im Ledger (`AGENTS.todo.md` → „Duplikate &
  Dateikonflikte"); die vier belegten Lücken sind Inhalts-Dedupe über Pfade,
  das Trennen der 409-Ursache (`MD5_DUPLICATE` vs. `NameAlreadyExists`), das
  Persistieren von „Nicht noch einmal fragen" **pro Ziel** und das Reparieren
  von „Ersetzen": `conflictResolution=OVERWRITE` wird vom Cookie-Host
  **nicht akzeptiert** (`POST` ⇒ 404, leerer Body — live belegt), der Weg des
  Originals ist `PUT /v2/upload/<nodeId>?conflictResolution=NONE`
  — **aber nur mit den Query-Parametern des POST** (`name`, `fileSize`,
  `kind`, `parentNodeId`; live belegt 2025-10-15: ohne `fileSize` ⇒ **400
  `MissingFileSize`**, mit dem POST-Satz gültig). Der `PUT` unterliegt
  außerdem der **Inhalts-Dedupe**: exakt dieselben Bytes ⇒ 409 statt 200 —
  deshalb wird ein Inhaltsduplikat **nie** per PUT „ersetzt", sondern bleibt
  „bereits im Konto". **Verworfen:** beim rein pfadbasierten Index bleiben (er
  erkennt den NFD/NFC-Zwilling nicht).

- **MVP-Lücke geschlossen (Korrektur 2025-10-14):** der interaktive
  Dateinamens-Konflikt-Dialog ist **implementiert** (`81cd277`, verifiziert in
  `a3a3625`): bei 409 fragt `Ask` pro Datei (Ignorieren / Als neue Datei
  speichern / Ersetzen) mit „Nicht noch einmal fragen" als run-weitem Override
  und 60-s-Timeout ⇒ Ignore. Die frühere Notiz „die Option ‚Nachfragen' ist
  ohne ihn wirkungslos" ist damit überholt.

## Original reference

- `../original/SPEC_DEDUP.md` — Dedup mechanics (MD5 identity, upload type, conflict policy).
- `../original/SPEC_DUPLICATES.md` — Duplicate detection and 409 behavior.
- `../original/SPEC_CONFLICT_OPTIONS.md` — Conflict options (ignore/rename/overwrite/ask).
- `../original/SPEC_FILE_IDENTITY.md` — File identity (MD5, stat check, path binding).
