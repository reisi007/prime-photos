# Spec: Wie erkennt das Original, dass eine Datei dieselbe ist?

**Status:** clean-room Verdichtung (2026-10-16, **neu gegen das Binary verifiziert**).
Beobachtetes Verhalten des offiziellen Amazon-Photos-Clients (macOS 11.3.0,
`Contents/MonoBundle/Amazon Photos.dll`) in eigenen Worten; keine verbatim
Code-Auszüge, keine Personendaten, keine Live-Werte.
**Quelle des Originals:** Decompil der DLL via `ilspycmd` (siehe Anhang) —
jede Aussage über das Original ist **am Code geprüft** (Typ/Methode benannt).
**Quelle von uns:** `src/dedup.rs`, `src/upload.rs`, `src/prime_photos.rs`,
`src/urls.rs` — direkt gelesen, mit `Datei:Zeile`.

**Scope.** Diese Datei beantwortet **eine** Frage: *Wie entscheidet das Original,
dass eine Datei dieselbe ist?* — und stellt das gegenüber, wie **wir** entscheiden.
Sie verdichtet die zerstreuten Belege aus den genannten Specs, findet Widersprüche
zwischen ihnen und dokumentiert die Lücke zum Original.

**Vermerk zu REA:** Der `rea`-Server kann managed .NET **nicht** analysieren
(`inspect_managed_artifact` liefert `metadata-not-admitted`). Der Werkzeug-Absatz
über „nicht neu verifiziert" ist **überholt** — siehe Anhang.

---

## 1. Die zwei orthogonalen Mechanismen

Das Original kennt **zwei unabhängige** Identitätsmechanismen:

| # | Mechanismus | Frage | Identität | Ort | Ebene |
|---|---|---|---|---|---|
| **A** | Inhalts-Dedupe („Duplikate überspringen") | „Ist dieser **Inhalt** schon im Konto?" | MD5 der ganzen Datei-Bytes, **über Pfade hinweg** | lokal (`cloud.db`) + Server als Netz | pro Ziel (Toggle) + globaler Default |
| **B** | Namenskonflikt („Konflikte mit Dateinamen") | „Ist dieser **Name** schon in diesem Ordner vergeben?" | exakter Name + Elternknoten | lokal (`cloud.db`), Server als Netz | strikt pro Ziel |

**am Code geprüft:** `CloudDriveStorageService.PreUploadCheck` führt beide
Mechanismen aus; `UploadRecordExtensions.GetSuppressFileUpload` steuert die
Dedup-Berechtigung über `UploadType`. A und B sind **orthogonal**: eine Datei kann
beides gleichzeitig sein. Das Original entscheidet **erst A, dann B** (§6).

---

## 2. Inhalts-Identität: MD5

| Punkt | Beleg |
|---|---|
| **Algorithmus** | MD5 über den **gesamten Datei-Body** — **am Code geprüft** (`Amazon.CloudDrive.Helpers.HashStream`, Berechnung über den ganzen Stream) |
| **Wer berechnet** | Der Client (`HashStream`, seekable) — **am Code geprüft** (`Amazon.CloudDrive.Services.CloudDrive.BackupStorageService.UploadFile`, Aufruf von `ComputeHash` vor dem Upload) |
| **Wann** | **Einmal pro Upload-Versuch** vor dem Request; der berechnete Wert wird in `UploadRecord.PrecomputedMd5` gecacht — **am Code geprüft** (Zuweisung in `BackupStorageService.UploadFile`) |
| **Woraus** | Datei-Bytes (der Body), **nicht** Name, Größe oder Datum |
| **Form** | Lower-case Hexadezimal, 32 Zeichen — **am Code geprüft** (Hex-Konvertierung per `ToHexString` auf dem Hash-Byte-Array) |
| **Wo verglichen** | (1) lokal am aufgelösten Knoten: das MD5-Feld der Inhaltseigenschaften wird mit dem berechneten Wert verglichen; (2) lokal über Pfade hinweg: `GetNodeByMd5`; (3) serverseitig: 409-Antwort (§4) — **am Code geprüft** (`CloudDriveStorageService.PreUploadCheck`) |
| **Request-Header** | `x-amzn-file-md5: <hex>` (Wire/Response: `contentProperties.md5`) — weiterhin aus `../protocol/PROTOCOL.md` |
| **Vorab-Prüfung** | `CloudDriveStorageService.PreUploadCheck` läuft **vor jedem Netz-Aufruf** gegen den lokalen Knoten-Cache (`cloud.db`). Treffer ⇒ `FileIgnoredPreUploadException` ⇒ **kein Request**, Status `Ignored` — **am Code geprüft** |
| **PreUploadCheck-Kosten** | Lokale SQLite-Abfragen (`GetNodeByNameAndParentIfExists`, ggf. `GetNodeByMd5`), **kein** Netz. Ein Retry mit dem Grund `RetryReason.Reupdate` kann den MD5-Check unterdrücken (`ShouldSuppressMd5Check`) — **am Code geprüft** (`UploadRecordExtensions.ShouldSuppressMd5Check`) |

**Wer dedup-berechtigt ist** (`UploadType`): **am Code geprüft** —
`UploadRecordExtensions.GetSuppressFileUpload` gibt `null` (nicht unterdrückt)
für `PhotosImport` und `Deduped`, `SuppressFileUpload.Deduplication` für alle
anderen. In `PreUploadCheck` wird `GetNodeByMd5` nur aufgerufen, wenn
`GetSuppressFileUpload` nicht `Deduplication` ergibt.

**Folge der MD5-Identität:** Rename, geänderter Timestamp oder Re-Encoding sind
**kein** Duplikat (neue Bytes ⇒ neuer MD5 ⇒ Upload). Zwei Datei mit identischen
Bytes aber verschiedenen Namen **sind** Duplikat (cross-path dedup).
Datum/EXIF-Felder sind nicht Teil der Identität.

---

## 3. Lokaler Index und Schnellpfad

### 3.1 Original: `uploads`-Tabelle (ManualUpload-DB)

| Spalte | Trägt | Rolle |
|---|---|---|
| `precomputed_md5` | berechneter MD5 | Dedup-Identitäts-Cache — **am Code geprüft** (`UploadRecord.PrecomputedMd5`, Spalte `precomputed_md5`) |
| `upload_type` | `Normal`/`PhotosImport`/`Deduped`/`Reupload` | Gate für Dedup-Berechtigung — **am Code geprüft** (`UploadType`-Enum: `Normal=0`, `PhotosImport=1`, `Deduped=2`, `Reupload=99`) |
| `node_id`, `destination_node_id` | Knoten-Ids | Ziel-/Existenzknoten |
| `status`, `retry_reason`, `error_code` | `Ignored`/`Skipped`/`Errored` + `PreUploadCheckFailed`/`FileConflict` | Ausgang + Wiederholungsgrund — **am Code geprüft** (`TransferStatus.Ignored=0x400`, `Skipped=0x800`; `TransferError.PreUploadCheckFailed=51`, `FileConflict=24`; `RetryReason.NameConflictSaveAsNew=13`, `NameConflictReplace=14`) |

Die `backup`-Tabelle (Größe, `modified_date`, `node_id`, `status`) trägt den
Scan-Zustand pro Datei; Namenskonflikte stehen dort als `Errored`/`FileConflict`
— **am Code geprüft** (`backup`-Tabellendefinition in `ManualUploadDatabase`).

### 3.2 Original: `cloud.db`-Knoten-Cache

`cloud.db` hält einen **Server-Knoten-Cache** (Tabelle `nodes`, Spalte
`content_md5` mit dem Index `IX_md5`) — **am Code geprüft** (`CloudDriveDatabase`,
der Index auf `content_md5` in `nodes`). `GetNodeByMd5` liefert den ersten Knoten
mit passendem `content_md5` und dem Status `AVAILABLE`. Er **ersetzt keine
Prüfung**, er ist die **lokale Beschleunigung** — die Server-409 bleibt das Netz-Sicherheitsnetz.

### 3.3 Original: Schnellpfad vor dem Hashen

**Am Code geprüft:** Das Original berechnet den MD5 **einmal pro Versuch** in
`BackupStorageService.UploadFile` (es berechnet den Hash über den Stream und
speichert die Hex-Form in `PrecomputedMd5`) und **hat keinen**
dokumentierten Stat-Schnellpfad (size+mtime vor dem Lesen) für den
Upload-Algorithmus — der MD5 wird immer gelesen, gehasht, verglichen. Die
`backup`-Tabelle mit `size`/`modified_date` dient der **Scan-Reconciliation**
(was wurde wann gesehen), nicht als Hash-Ersatz.

### 3.4 Unser Code: Stat-Schnellpfad vor dem Hash (bewusste Erweiterung)

| Schritt | Code | Bedingung |
|---|---|---|
| **Stat** (size + mtime) | `DedupIndex::stat_matches` (`src/dedup.rs:355`) | exakter Pfad + gleiche Größe + gleiche mtime |
| Skip ohne Lesen | `src/upload.rs:541` | `dedup_on` und `stat_matches` ⇒ `Some(origin)` |
| **MD5** (ganze Datei) | `md5::compute` über den gelesenen Inhalt (`src/upload.rs:603`) | Datei gelesen, Hash berechnet |
| Skip nach Hash | `DedupIndex::should_skip` (`src/dedup.rs:284`) | exakter Pfad + gleicher MD5 |
| Cross-path Skip | `DedupIndex::content_seen` (`src/dedup.rs:323`) | MD5 irgendwo im Index (Pfad egal) |

**Sicherheitsregel:** Ein unbekannter mtime (`0`) gilt **nie** als Treffer
(`src/dedup.rs:359`). Ein Stat-Treffer ist nur der schnelle Pfad — die Identität
bleibt der MD5.

**Gleiche Größe/mtime, aber geänderter Inhalt?** Unser Stat-Pfad kann das nicht
erkennen (Größe + mtime gleich ⇒ Skip). Das Original hat diesen Weg gar nicht erst
gebaut — es liest und hasht immer. Beide verlassen sich auf die **Unverannahme**,
dass gleiche Größe+mtime bei gleichem Pfad auf gleichen Inhalt schließen lassen.
Praxis: Editieren ohne Größen- oder Änderungszeit-Änderung (z. B. durch
Setzen des Änderungsdatums) erzeugt einen false-negative Skip. Dokumentierte
Grenze, kein Bug.

---

## 4. Server-Beweis: HTTP 409

| Punkt | Original | Unser Code |
|---|---|---|
| **Request** | `POST /v2/upload` (Host aus `uploadServiceUrl`) | `POST content-<zone>.drive.amazonaws.com/v2/upload` |
| **Antwort bei Duplikat** | **409 Conflict** | **409 Conflict** |
| **Ursache — Inhalt** | `ErrorType ∈ {MD5_DUPLICATE, DuplicatesConflictError}` — **am Code geprüft** (`HandleNodeResponse`: bei Antwort-Status `Conflict` und übereinstimmendem MD5 in den Inhaltseigenschaften der Antwort) | `DuplicatesConflictError` / `MD5_DUPLICATE` (case-insensitive, `src/prime_photos.rs:449`) |
| **Ursache — Name** | Fehler-Typ `NameAlreadyExists` — **am Code geprüft** (`UploadHelper.RetryForNameConflict`: bei abweichendem MD5 und einer der beiden Schreibweisen des Namenskonflikt-Typs) | `NameAlreadyExists` |
| **Felder** | `errorDetails.conflictNodeIds` (Node-Ids der Konflikt-Knoten) | `ResponseUploadFailDetails.conflict_node_ids` (`src/prime_photos.rs:323`) |
| **Normalisierung** | **Am Code geprüft** (`HandleNodeResponse`): 409 mit MD5-Match ⇒ Status `OK`, Knoten via `SaveNode` gecacht, dann `FileIgnoredException` ⇒ `Ignored` („Zuvor hochgeladen") | `FileOutcome::ConflictSkipped` (`src/upload.rs:358`), `Origin::ContentSkip` bei Content-Duplicate (`src/upload.rs:933`) |
| **Name-Konflikt** | **Am Code geprüft** (`UploadViaSimpleOrLegacyUploader`): `NameConflictStrategy.GetConflictResolution` → `FileUploadNameConflictException` → Error-Pfad, gelöst durch `ConflictResolution` | `resolve_conflict` (`src/upload.rs:375`) oder automatisch `Replace`→PUT |

**Unterschied „Inhalt schon da" vs. „Name schon da":**

| 409-Ursache | Original | Unser Code |
|---|---|---|
| `DuplicatesConflictError` / `MD5_DUPLICATE` (oder MD5-Match im Body) | Skip, kein Fehler, „Zuvor hochgeladen" — **am Code geprüft** | `FileOutcome::ConflictSkipped`, `Origin::ContentSkip`, „bereits im Konto" |
| `NameAlreadyExists` | Error → Konflikt-Dialog/Politik — **am Code geprüft** | `resolve_conflict` / `Replace`→PUT / `SaveAsNew`→Rename |
| unbekannt / leer | (nicht belegt) | behandelt wie Name-Konflikt (konservativ: kein Replace) |

---

## 5. Namens-Identität

| Punkt | Original | Unser Code |
|---|---|---|
| **Vergleich** | exakt (ordinal) — **am Code geprüft** (`GetNodeByNameAndParentIfExists`: vier Gleichheitsbedingungen — Name, Kind, Elternknoten und Status — mit dem Statuswert `AVAILABLE`) | exakt (Server-Antwort) |
| **Ebene** | pro Zielordner (`parentNodeId`) | pro Zielordner (Server) |
| **Lookup** | `GetNodeByNameAndParentIfExists` mit Name, Elternknoten und dem Kind-Typ `FILE` — **lokal** gegen `cloud.db` — **am Code geprüft** | kein lokaler Knoten-Cache; Name-Konflikt erst aus 409 |
| **Encoding** | **nicht normalisiert** — `fso.Name` raw (NFD auf macOS) — weiterhin aus `SPEC_FILENAME_ENCODING.md` (konsistent mit fehlender Normalisierung in `PreUploadCheck`) | **NFC** normalisiert (`crate::text::nfc`, `src/urls.rs:344`) |

**Encoding-Folge für das Original:** Zwei Namen, die sich nur in der Unicode-Form
unterscheiden (NFD vs. NFC), sind für **jeden** Namensvergleich im Client
**verschiedene** Strings. Nur der Inhalts-MD5 vereinheitlicht sie. Auf macOS sendet
das Original roh NFD — ein von Windows (NFC) angelegter Knoten wird per Name
**nicht** gefunden (`SPEC_FILENAME_ENCODING.md` §3).

**Encoding-Folge für uns:** Wir normalisieren auf NFC vor dem Upload und beim
Abfragen. Der Server vergleicht exakt; NFC ist die gemeinsame Form (Windows-Default,
Web-Seite). **Konsequenz:** Bereits unter NFD hochgeladene Dateien (vom Original)
werden per Name **nicht** mehr gefunden — der MD5-Dedup fängt identischen Inhalt
aber weiter ab (`SPEC_FILENAME_ENCODING.md` §5).

---

## 6. Reihenfolge der Prüfungen

**Am Code geprüft** (`CloudDriveStorageService.PreUploadCheck` +
`UploadViaSimpleOrLegacyUploader`):

1. **Namens-Lookup** (lokal, `cloud.db`): Knoten per Name + Parent + `FILE`
   auflösen — **immer**, unabhängig vom Dedupe-Toggle. Optional kann ein
   lokaler ID-Lookup (`GetNodeIdForLocalId`) den Namens-Lookup ersetzen.
2. **MD5-Vergleich am aufgelösten Knoten:** übereinstimmendes MD5 und Status `AVAILABLE`.
3. **Nur wenn dedup-berechtigt** (`GetSuppressFileUpload` ergibt nicht
   `Deduplication`): zusätzlich `GetNodeByMd5` über alle Ordner.
4. **Treffer in 2 oder 3** ⇒ `FileIgnoredPreUploadException` ⇒ `Ignored`,
   **kein Request**, Mechanismus B wird **nie** erreicht.

**Also: bei gleicher Name und gleicher MD5 gewinnt die Inhalts-Identität.** Es
entsteht kein Konflikt, keine Umbenennung, kein Dialog. Nur wenn der Name-Lookup
**daneben** greift (stale Cache) und die Datei dedup-berechtigt ist, fängt
`GetNodeByMd5` denselben Fall über den Inhalt.

**Der Fall „gleicher Name, anderer Inhalt":** Kein Inhalts-Duplikat. Mechanismus B
greift — **am Code geprüft** (`UploadViaSimpleOrLegacyUploader` +
`NameConflictStrategy`): lokaler Name+Parent-Lookup ⇒ Zielknoten vorhanden ⇒
`ConflictResolution` Politiken:
- `Ignore` ⇒ `FileSkippedException` (skip vor Request)
- `SaveAsNew` ⇒ kein Überschreiben, Konflikt-Auflösung `Rename` (Wire: `RENAME`)
- `Replace` ⇒ Überschreiben, Knoten-Id des Zielknotens, Konflikt-Auflösung
  bleibt `None` (Wire: `NONE`)
- `None` ⇒ wenn kein Zielknoten: kein Überschreiben, Konflikt-Auflösung `None`;
  sonst `FileUploadNameConflictException` → Dialog nach dem Lauf

`SaveAsNew` schickt `RENAME` **unbedingt** — auch ohne Namenskonflikt und ohne
Zielknoten (Korrektur in `SPEC_CONTENT_FILTER_DESTINATION.md` §8) —
**am Code geprüft** (`NameConflictStrategy.GetConflictResolution`).

**Wire-Werte** — **am Code geprüft** (`CloudDriveConflictResolutionExtensions`):
`None` ⇒ `"NONE"`, `Rename` ⇒ `"RENAME"`, `RenameExisting` ⇒ `"RENAME_EXISTING"`.

**Unser Code (Reihenfolge):**
1. Stat-Skip (`src/upload.rs:541`)
2. MD5-Skip (Pfad+MD5, `src/upload.rs:617`)
3. Cross-path Content-Skip (`src/upload.rs:661`)
4. Upload POST
5. Bei 409: Content-Duplicate ⇒ Skip; Name-Konflikt ⇒ `resolve_conflict` oder
   automatisch `Replace`→PUT / `SaveAsNew`→Rename (`src/upload.rs:747-972`)

---

## 7. Anzeige: Welche Zustände sieht der User?

| Zustand | Original | Unser Code | Quelle |
|---|---|---|---|
| „Zuvor hochgeladen" / `Ignored` | PreUploadCheck-Treffer (lokal, kein Request) **oder** 409-Inhalts-Duplikat — **am Code geprüft** (`TransferStatus.Ignored = 0x400`) | `Origin::ContentSkip`, `conflict_skipped` | `src/upload.rs:551,627,671` |
| „Verworfen" / `Skipped` | `Ignore`-Politik bei Namenskonflikt — **am Code geprüft** (`TransferStatus.Skipped = 0x800`) | — (wir haben keinen separaten `Skipped`-Zustand; `Ignore` ⇒ `ConflictSkipped`) | — |
| „Erfolgreich hochgeladen" | 200/201 | `FileOutcome::Uploaded`, `uploaded_new` | `src/upload.rs:853` |
| Unverändert (Stat-Skip) | **nicht vorgesehen** (kein Stat-Pfad) | `Origin::Uploaded`, `unchanged_skipped` | `src/upload.rs:555` |
| Fehler | 409-Name-Konflikt, andere HTTP-Fehler | `FileOutcome::Failed`, `failed` | `src/upload.rs:893` |

**Bewusste Abweichung:** Unser Stat-Pfad erzeugt den Zustand „unverändert" (diese
Datei wurde schon hochgeladen) — das Original kennt den nicht, weil es nicht
hasht, wenn die Stat passt. Unser `Origin`-System trennt „unverändert" (`'uploaded'`)
von „bereits im Konto" (`'content_skip'`) — beide sind für den User sichtbar als
getrennte Zähler.

---

## 8. Rolle des Cloud-Caches (`cloud.db`)

| Frage | Antwort |
|---|---|
| Ersetzt er eine Prüfung? | **Nein** — er ist die **lokale Beschleunigung** für `GetNodeByMd5` und `GetNodeByNameAndParentIfExists` — **am Code geprüft** |
| Was passiert ohne Cache? | Jeder Upload geht raus; der Server-409 fängt den Duplikat-Fall als Sicherheitsnetz |
| Cache-Miss bei bekanntem Inhalt? | 409 ⇒ Skip (wie bei Cache-Treffer) — **am Code geprüft** (`HandleNodeResponse`) |
| Unser Stand | **Kein** `cloud.db`-Cache (bewusst, `AGENTS.md` „Scope-Grenzen"); der lokale `uploads`-Index ersetzt ihn funktional für den dedup-Pfad |

---

## 9. Lücke-Tabelle: Original vs. wir

| Thema | Original | Unser Stand | Beleg (unser Code) | Bewusst? |
|---|---|---|---|---|
| **MD5-Identität** | ja, hex lower-case | ja | `md5::compute` über die Bytes, als Hex ausgegeben (`src/upload.rs:603`) | Parität |
| **Cross-path Dedup** | `GetNodeByMd5` über `cloud.db` | `DedupIndex::content_seen` über `ix_uploads_md5` | `src/dedup.rs:323`, `src/upload.rs:661` | **Umgesetzt** (2026-10-14) |
| **Stat-Schnellpfad** | **nein** | ja (size+mtime) | `src/dedup.rs:355`, `src/upload.rs:541` | **Bewusste Erweiterung** |
| **`origin`-Trennung** | **nein** | ja (`uploaded` / `content_skip`) | `src/dedup.rs:41`, `src/upload.rs:549` | **Bewusste Erweiterung** |
| **`node_id`-Bindung** | `cloud.db` Knoten | `uploads.node_id` | `src/dedup.rs:395` (`record_upload`), `src/dedup.rs:432-444` (persistierter Upsert) | **Umgesetzt** (2026-10-16) |
| **NFC-Normalisierung** | **nein** (roh NFD auf macOS) | **ja** | `crate::text::nfc` (`src/urls.rs:344`) | **Bewusste Abweichung**, dokumentiert |
| **PreUploadCheck** | ja, vor jedem Request | äquivalent: Stat→MD5→content_seen vor POST | `src/upload.rs:540-686` | Parität (implementiert als Worker-Schleife) |
| **Knoten-Cache (`cloud.db`)** | ja | **nein** | — | **Bewusste Grenze** (AGENTS.md) |
| **Name-Konflikt-Erkennung** | lokal vor Request | nur aus 409 | `src/upload.rs:766` | **Lücke** — pragmatisch akzeptiert |
| **409-Ursache gesplitzt** | ja | ja (`conflict_cause`, `is_content_duplicate_code`) | `src/prime_photos.rs:449` | **Umgesetzt** (2026-10-15) |
| **`Replace` via PUT** | ja (Überschreiben gesetzt plus Knoten-Id) | ja (`PUT /v2/upload/<nodeId>` mit POST-Query-Satz) | `src/urls.rs:374`, `src/prime_photos.rs:1274-1280` | **Umgesetzt** (2026-10-15) |
| **Replace-Ziel nach 409** | Name+Parent-Lookup liefert das Ziel | `conflictNodeIds[0]` wird per PUT ersetzt, unabhängig von lokaler `node_id`-Bindung | `src/prime_photos.rs` (`should_replace_after_conflict`) | **Parität** (REA-statische Analyse Amazon Photos 11.3.0,36) |
| **Apple-Photos-Heuristik** | ja | **nein** | — | Bewusst außen vor |
| **Dedup-Default** | Onboarding an, Post-Off | `true` | `src/folders.rs` (`default_true`) | Parität (dokumentiert) |
| **Konflikt-Default** | `None` („Jedes Mal nachfragen") | Application `Replace` | `src/folders.rs` | **Bewusste Abweichung** |
| **„Nicht noch einmal fragen"** | pro Schedule, dauerhaft | pro Ordner, dauerhaft | `src/backup.rs` (`remember_conflict_in_folder`) | Parität |
| **Dedup pro Ziel** | ja (`dedupe_photos_videos`) | ja (`BackupFolder.dedup`) | `src/folders.rs` | Parität |
| **Server 409 bei `RENAME`** | (nicht belegt) | ja (Content-Duplikat auch mit `RENAME`) | `src/upload.rs:700` | Live belegt (2026-10-15) |

---

## 10. Korrekturen an vorhandenen Docs (Widersprüche)

1. **`SPEC_DEDUP.md` §2.3 — „the original effectively uses
   `conflictResolution=NONE`" vs. `SPEC_DUPLICATES.md` §1.2/§4.**
   **`SPEC_DUPLICATES.md` hat recht.** Der Original-Client nutzt `NONE` **oder**
   `RENAME` je nach `ConflictResolution`-Politik; bei `SaveAsNew` geht **immer**
   `RENAME` raus (Korrektur in `SPEC_CONTENT_FILTER_DESTINATION.md` §8) — **am
   Code geprüft** (`UploadViaSimpleOrLegacyUploader`). **`SPEC_DEDUP.md` zu
   präzisieren.**

2. **`SPEC_CONTENT_FILTER_DESTINATION.md` §1.3 vs.
   `SPEC_CONFLICT_OPTIONS.md` §1.3** — beide beschreiben die Reihenfolge.
   Ersteres ist präziser (4 Schritten, inkl. „Schritt 2 nicht Toggle-abhängiger
   MD5-Vergleich"). Letzteres fasst zusammen. **Kein echter Widerspruch**, aber
   `SPEC_CONTENT_FILTER_DESTINATION.md` sollte als Referenz gelten.

3. **`SPEC_DEDUP.md` §1 — „The original never compares names or sizes for
   dedup, only MD5"** — **beide Docs ungenau.** Korrekt für die
   **Dedup-Entscheidung** (Schritt 2 prüft am aufgelösten Knoten den MD5). Aber:
   der **Name-Lookup** (Schritt 1) ist die *Voraussetzung*, um den Knoten zu
   finden, an dem der MD5 verglichen wird. Der Satz ist also missverständlich
   formuliert — sollte heißen: „Der Original vergleicht nie **Grönde** für
   Dedup; der Name dient nur der Knoten-Auflösung."

4. **`SPEC_FILENAME_ENCODING.md`** — vollständig korrekt. Die Behauptung
   „Original normalisiert nicht" ist konsistent mit `SPEC_DEDUP.md` (kein
   Normalisierung in `PreUploadCheck`).

5. **`SPEC_FOLDERS.md` §3.4** — sagt nichts über Stat-Schnellpfad; korrekt,
   denn den gibt es nicht. Unsere Erweiterung ist in `SPEC_FILE_IDENTITY.md` §3.4
   dokumentiert.

---

## 11. UNKNOWN

- **NFC-Unterstützung des Servers** — ob der Server NFC-normalisiert oder roh
  speichert; nicht am Code belegt (Live-Belege zeigen exakten Vergleich).
  Weiterhin UNKNOWN.
- **Original-`Replace`-Wire-Verhalten bei gesetztem Überschreiben** — der
  exakte Wire-Wert (vermutlich `conflictResolution=NONE`) ist aus dem Decompil
  ableitbar (`UploadViaSimpleOrLegacyUploader` setzt das Überschreiben und lässt
  `conflictResolution` auf `None`), aber der Wire-Wert wurde nicht gegen einen
  echten Request verifiziert. Teilweise UNKNOWN.
- **Weblab-Flag für Dedup** — ob ein zusätzlicher Server-seitiger Flag (Weblab)
  die Dedup-Berechtigung steuert; am Code nicht belegt (die lokale Steuerung
  über `UploadType` ist dokumentiert). Weiterhin UNKNOWN.

---

## 12. Belege

**Original-Symbole (am Code geprüft, 2026-10-16):**
- `Amazon.CloudDrive.Services.CloudDrive.CloudDriveStorageService` —
  `PreUploadCheck` (zwei Overloads), `UploadViaSimpleOrLegacyUploader`,
  `HandleNodeResponse`, `CloudNodeCheck`
- `Amazon.CloudDrive.Database.CloudDrive.CloudDriveDatabase` —
  `GetNodeByNameAndParentIfExists`, `GetNodeByMd5`, `NodeExistsWithMd5`,
  `IX_md5`
- `Amazon.CloudDrive.Database.ManualUpload.ManualUploadDatabase` —
  `InternalSaveUploadRecord`, `precomputed_md5` (Spalte)
- `Amazon.CloudDrive.Database.ManualUpload.UploadRecord` —
  `PrecomputedMd5`-Eigenschaft
- `Amazon.CloudDrive.Database.ManualUpload.UploadRecordExtensions` —
  `GetSuppressFileUpload`, `ShouldSuppressMd5Check`, `GetConflictResolution`
- `Amazon.CloudDrive.Services.ManualUpload.UploadStrategy.NameConflictStrategy`
  — `GetConflictResolution`, `GetScheduleConflictResolution`
- `Amazon.CloudDrive.Services.CloudDrive.BackupStorageService` —
  `UploadFile` (MD5-Berechnung + `PrecomputedMd5`-Zuweisung)
- `Amazon.CloudDrive.Helpers.HashStream` — `ComputeHash`, `Md5Hash`,
  `hashOnlyOnce`
- `Amazon.CloudDrive.Helpers.UploadHelper` — `RetryForNameConflict`
- `Amazon.CloudDrive.Services.CloudDriveConflictResolutionExtensions` —
  Wire-Werte: `None`→`NONE`, `Rename`→`RENAME`, `RenameExisting`→`RENAME_EXISTING`
- `Amazon.CloudDrive.Database.ManualUpload.UploadType` — `Normal`, `PhotosImport`,
  `Deduped`, `Reupload`
- `Amazon.CloudDrive.Database.TransferStatus` — `Ignored = 0x400`,
  `Skipped = 0x800`, `Errored = 0x200`
- `Amazon.CloudDrive.Database.TransferError` — `FileConflict = 24`,
  `PreUploadCheckFailed = 51`, `CloudDriveNodeConflict = 194`
- `Amazon.CloudDrive.Database.RetryReason` — `NameConflictSaveAsNew = 13`,
  `NameConflictReplace = 14`, `Reupdate = 999`, `RequestTimeOut = 6`,
  `PendingNodeCompleting = 7`
- `Amazon.CloudDrive.Database.ManualUpload.SuppressFileUpload` — `Deduplication`

**Unser Code (neu geprüft, 2026-10-16; Zeilenverweise nachgezogen 2026-10-17):**
- `src/dedup.rs:355` — `stat_matches`
- `src/dedup.rs:284` — `should_skip`
- `src/dedup.rs:323` — `content_seen`
- `src/dedup.rs` — `record_upload` persists the successful-upload `node_id`
- `src/dedup.rs:41` — `Origin` enum
- `src/upload.rs:541` — Stat-Skip im Worker
- `src/upload.rs:603` — MD5-Berechnung
- `src/upload.rs:617` — `should_skip` Aufruf
- `src/upload.rs:661` — `content_seen` Aufruf
- `src/upload.rs:747-972` — 409-Behandlung und Konflikt-Logik
- `src/prime_photos.rs:449` — `is_content_duplicate_code`
- `src/prime_photos.rs:463` — `should_replace_after_conflict`
- `src/urls.rs:344` — NFC-Normalisierung in `upload_query`
- `src/urls.rs:374` — `replace_upload_url`

**Specs (verdichtet):**
- `SPEC_DEDUP.md` §1, §2.2, §4
- `SPEC_DUPLICATES.md` §2, §3, §4, §5
- `SPEC_FILENAME_ENCODING.md` §1, §3, §5
- `SPEC_CONTENT_FILTER_DESTINATION.md` §1.3, §1.4, §8
- `SPEC_CONFLICT_OPTIONS.md` §0, §1.1, §1.2, §1.3
- `SPEC_FOLDERS.md` §3.1, §3.4
- `../protocol/PROTOCOL.md` §409-Verhalten

---

## Anhang: Wie diese Spec geprüft wurde

**Datum:** 2026-10-16

**Werkzeug:** `ilspycmd` (ICSharpCode.Decompiler) 11.1.0.9782, installiert unter
`~/.dotnet/tools`; Wrapper `ilspycmd` in `~/.local/bin` (setzt `DOTNET_ROOT`
selbst, läuft in jeder Shell). .NET-Runtime 10.0.12 (Homebrew).

**Ziel-DLL:** `/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon
Photos.dll` (4,6 MB)

**Exakte Aufrufe:**

```bash
# Klassenliste filtern
ilspycmd -l c "<dll>" | grep -iE "CloudDriveStorageService|CloudDriveDatabase|NameConflictStrategy|UploadRecordExtensions|ManualUploadDatabase|HashStream|PreUploadCheck|ConflictResolution|DeduplicateOptions|TransferStatus|TransferError|CloudDriveNodeConflict|FileIgnored"

# Einzelne Typen dekompilieren
ilspycmd -t Amazon.CloudDrive.Services.CloudDrive.CloudDriveStorageService "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.ManualUpload.UploadStrategy.NameConflictStrategy "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.ManualUpload.UploadRecordExtensions "<dll>"
ilspycmd -t Amazon.CloudDrive.Helpers.HashStream "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.CloudDrive.CloudDriveDatabase "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.ManualUpload.ManualUploadDatabase "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.ManualUpload.UploadRecord "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.TransferStatus "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.TransferError "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.RetryReason "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.ManualUpload.UploadType "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.CloudDrive.BackupStorageService "<dll>"
ilspycmd -t Amazon.CloudDrive.Helpers.UploadHelper "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.CloudDriveConflictResolutionExtensions "<dll>"
```

**Bewertungs-Skala:** „am Code geprüft" (Typ/Methode im Decompil sichtbar) vs.
„weiterhin aus alten Dumps" (aus vorheriger Recon, nicht neu verifiziert) vs.
„UNKNOWN" (nicht belegt).
