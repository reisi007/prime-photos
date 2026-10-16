# Spec: Upload-Zielordner (Zielknoten) des Originals

**Status:** Clean-Room-Recon (nur lesend) des offiziellen macOS-Amazon-Photos-Clients
11.3.0 (`CFBundleShortVersionString`), `CommitId 1d7941b5`. Eigene Worte und Tabellen,
keine verbatim Code-Auszüge, keine Personendaten. Deutsche UI-Strings werden wörtlich
aus `Contents/Resources/de.lproj/Localizable.strings` zitiert, jeweils mit dem Key.

**Frage.** User-Wunsch 2026-10-16: „Upload Ort soll gleich sein wie bei der
Originalen App." Diese Spec beantwortet: Was ist der Upload-Ort im Original, wie
funktioniert er, und was fehlt bei uns?

**Quellen (nur lesend).** `/Applications/Amazon Photos.app` — App-Bundle unverändert.
**Decompiler:** `ilspycmd` 11.1.0.9782 (ICSharpCode.Decompiler 11.1.0.9782) über
Wrapper `~/.local/bin/ilspycmd` (setzt `DOTNET_ROOT` selbst). REA ist für diese
Assembly unbrauchbar: `inspect_managed_artifact` liefert `metadata-not-admitted`
(Budget 67 MB), `search_android_classes` ist der falsche Weg (jadx/APK).
Die Spec stützt sich auf den **Decompiler** (Typ/Methode) und auf die
vorhandenen Specs (`docs/SPEC_CONTENT_FILTER_DESTINATION.md` §3,
`docs/SPEC_FOLDERS.md`, `docs/SPEC_UPLOAD.md`, `docs/SPEC_DEDUP.md`,
`docs/PROTOCOL.md`, `docs/openapi.yaml`, `docs/UI_FLOWS.md`,
`docs/UI_STATES.md`) sowie auf **unseren Code** (`src/prime_photos.rs`,
`src/folders.rs`, `src/backup_config.rs`, `src/backup.rs`).

**Markierung:** Jede Aussage ist als „**am Code geprüft**" (Typ/Methode) oder
„**weiterhin aus alten Dumps**" (vorhandene Spec, nicht neu geprüft) markiert.
Ungeprüftes steht unter „UNKNOWN".

---

## 1. Was der Zielordner im Original ist

### 1.1 Ein Cloud-Knoten, kein lokaler Pfad

| Aspekt | Befund | Beleg |
|---|---|---|
| UI-Label | `clouddrive_desktop_backupconfiguration_backupdestinationtitle` = **„Ziel für Datensicherung:"** | **am Code geprüft** (`BackupConfigurationViewController._selectDestinationFolderLabel`) |
| Textfeld / Button | `BackupView_SettingsDestinationPath_Text`, `BackupView_SettingsDestinationFolder_Button` mit Titel `clouddrive_desktop_backupconfiguration_selectbackupdestination_button` = **„Ändern..."** | **am Code geprüft** (`BackupConfigurationViewController._selectDestinationFolderButton`) |
| Interne Property | `ScheduledBackupItem.DestinationNodeId` → `ScheduleRecord.DestinationNodeId` | **am Code geprüft** (`ScheduledBackupItem.DestinationNodeId`, `ScheduleRecord.DestinationNodeId`) |
| Persistenz | SQLite-Spalte `schedules.destination_node_id` (`ManualUploadDatabase`) | **am Code geprüft** (`ScheduleRecord`, Spalten-Attribut auf `destination_node_id`) |
| Anzeige-Format | `CloudPath` baut aus dem Anfang „Amazon Photos" und den Segmenten, verbunden mit „ > " | **am Code geprüft** (`BackupConfigurationViewModel.CloudPath`, `RootPathSegment` beginnt mit „Amazon Photos") |

Der Button öffnet per `ChooseCloudPath` einen **Cloud-Ordner-Picker**
(`FolderPickerViewController`) mit der Erlaubnis, neue Ordner anzulegen; Sheet-Titel
`clouddrive_desktop_clouddrivepicker_selectlocationmessage` = **„Speicherort für Ihre
Dateien auswählen"**, Primärknopf `…_selectbutton` = **„Auswählen"**.

### 1.2 Kette: UI → DB → Leitung

```
UI (BackupConfigurationViewController)
  → BackupConfigurationViewModel.SetDestination(nodeId)
    → ScheduledBackupItem.DestinationNodeId
      → ScheduleRecord.DestinationNodeId
        → SQLite schedules.destination_node_id
          → BackupScannerWorker: ScheduleRecord.LoadDestinationNode(backupPathMapper)
            → BackupPathMapper.GetOrCreateBackupDestination(...)
              → parentNodeId im Upload-Request
```

**am Code geprüft** (`BackupConfigurationViewModel.SetDestination`,
`ScheduledBackupItem.DestinationNodeId`, `ScheduleRecord.LoadDestinationNode`,
`BackupPathMapper.GetOrCreateBackupDestination`).

---

## 2. UI: Label, Button, Aktivität, Picker

### 2.1 Label und Feld

| Element | Identifier | Zustand |
|---|---|---|
| Label | `clouddrive_desktop_backupconfiguration_backupdestinationtitle` = „Ziel für Datensicherung:" | immer sichtbar |
| Textfeld | `BackupView_SettingsDestinationPath_Text` | zeigt `CloudPath` (`Amazon Photos > Backup > <Macname > …`) |
| Button | `BackupView_SettingsDestinationFolder_Button` („Ändern...") | **immer aktiv** (siehe §2.2) |

### 2.2 Asymmetrie: `CanChangeDestination` ist nicht verkabelt

| Befund | Beleg |
|---|---|
| `CanChangeDestination` existiert am ViewModel | **am Code geprüft** (`BackupConfigurationViewModel.CanChangeDestination`, Zeile 157) |
| `CanChangeDestination` wird am Ziel-Button **nicht** verdrahtet — der Button ist in diesem Dialog immer aktiv | **am Code geprüft** (`BackupConfigurationViewController.WireEvents`: `_selectDestinationFolderButton` hat nur `Subscribe` auf Click, **kein** `Enabled`-Binding) |
| `CanChangeSource` steuert den Quellordner-Button („Ändern...") | **am Code geprüft** (`WireEvents`: die Aktivierung von `_selectBackupFolderButton` folgt `canChangeSource` über `ObservableForPropertyValue`) |

**Asymmetrie bestätigt:** Der Quellordner ist nach dem Anlegen **nicht mehr änderbar**
(`CanChangeSource` aus), aber der Zielordner-Button bleibt aktiv. Das liegt
daran, dass `CanChangeDestination` existiert, aber nicht in `WireEvents` mit
`Enabled` verdrahtet ist — der Button ist also de-facto immer schaltbar.
**Zusatzbefund:** `CanChangeDestination` wird in der gesamten Codebase **niemals**
abgeschaltet (Voll-Dump-Grep: alle Konstruktoren von `BackupConfigurationParams`
schalten sie ein) — die Property ist de-facto vestigial.

### 2.3 Was der Picker zeigt

| Aspekt | Befund | Beleg |
|---|---|---|
| Picker-Typ | `FolderPickerViewController` (Cloud-Ordner-Liste) | **am Code geprüft** |
| `CanCreateNewFolders` | `true` | **am Code geprüft** (`FolderPickerViewController.CanCreateNewFolders`) |
| Sheet-Titel | `clouddrive_desktop_clouddrivepicker_selectlocationmessage` = „Speicherort für Ihre Dateien auswählen" | **am Code geprüft** (`BackupConfigurationViewController.ChooseCloudPath`) |
| Primärknopf | `…_selectbutton` = „Auswählen" | **am Code geprüft** (`ChooseCloudPath`: `PrimaryActionButtonText`) |
| Neuen-Ordner-Dialog | `CreateCloudFolderViewController` mit Validierung | **am Code geprüft** (`FolderPickerViewController.CreateNewFolder`) |

**Baumstruktur des Pickers (am Code geprüft):**

Der Picker zeigt eine **flache Liste** pro Ebene, keinen verschachtelten Baum:

| Element | Beschreibung | Beleg |
|---|---|---|
| `NodeBranch` | Datenmodell: `Parents` (Breadcrumb), `Current` (aktueller Knoten), `Children` (Unterordner) | **am Code geprüft** (`NodeBranch`) |
| `TotalItems` | Elternanzahl plus Kinderanzahl plus eins (Breadcrumb + aktueller Ordner + Kinder) | **am Code geprüft** (`NodeBranch.TotalItems`) |
| `Indexer` | Index null = Current, eins bis Elternanzahl = Parents, danach Children | **am Code geprüft** (`NodeBranch`-Indexer) |
| `Name` | Name des aktuellen Knotens, sonst der Text-Key `clouddrive_desktop_clouddrivepicker_nonamefolder` | **am Code geprüft** (`NodeBranch.Name`) |
| Navigation | `GetRootBranch` (Root) bzw. `GetNodeBranch` (Unterordner) | **am Code geprüft** (`CloudDrivePickerViewModel`) |

**Der Picker zeigt also:** Eine Breadcrumb-Leiste (Eltern-Pfade), den aktuellen
Ordner und seine Unterordner als flache Liste. Die Navigation ist **ebenenweise**
(hinunter/hinüber), nicht als verschachtelter Baum.

**Neuen-Ordner-Dialog (am Code geprüft):**

| Validierung | Key | Text |
|---|---|---|
| Sonderzeichen | `…_newfolderdialog_errorinvalidcharacters` | „Ordnernamen können keine Sonderzeichen enthalten." |
| Länge | `…_errornametoolong` | „Ordnername dürfen bis zu 280 Zeichen enthalten." |
| Konflikt | `…_errornameconflict` | „Ein Ordner mit diesem Namen ist bereits vorhanden." |

---

## 3. Default und Ableitung

### 3.1 Vorschlag: `GetSuggestedCloudPathSegmentsForLocalPath`

**am Code geprüft** (`BackupPathMapper.GetSuggestedCloudPathSegmentsForLocalPath`):

1. Der Elternpfad wird gebildet (`PathEx.GetParentPathLong`)
2. Eine Liste wird angelegt, die mit `"Backup"` und dem Maschinennamen
   (`MachineName`) beginnt
3. Wenn der Quellpfad dem Benutzerpfad entspricht oder **kein** Unterpfad des
   Benutzerpfads ist (`PathEx.IsPathChildOf`):
   - Die Laufwerksdetails des Quellpfads werden geholt (`GetDriveDetails`)
   - Wenn das Volume-Label nicht leer ist, wird es der Liste angehängt
4. `BuildCloudPathSegments` — baut die Eltern-Pfade rekursiv auf

**`BuildCloudPathSegments` (am Code geprüft):**

- Rekursiv: wenn `currentPath` nicht leer ist und weder Root noch Benutzerpfad:
  - `BuildCloudPathSegments` wird zuerst mit dem Elternpfad aufgerufen
  - dann wird der Dateiname des aktuellen Pfads der Liste angehängt

**Reihenfolge bestätigt:** `Backup` → `<Macname>` → [`VolumeLabel`] → Eltern-Pfade
des Quellordners unterhalb des User-Homes (inklusive Quellordner-Name als letztes
Segment).

Beispiel (Konstruktion, nicht ausgeführt): Quelle `~/Bilder/Urlaub2026` ⇒
`Amazon Photos > Backup > <Macname> > Bilder > Urlaub2026`.

Der Vorschlag wird **automatisch** vorbelegt (`SetLocalPath`), sobald kein
`DestinationNodeId` gesetzt ist — der User muss das Ziel also normalerweise nie
anfassen.

### 3.2 Anlegen: `GetOrCreateBackupDestination`

**am Code geprüft** (`BackupPathMapper.GetOrCreateBackupDestination`):

| Schritt | Beschreibung | Beleg |
|---|---|---|
| Start | Wenn `fromNodeId` leer ist, wird der Wurzelknoten geholt (`GetRootNode`); sonst der Knoten zu der Id (`GetNode`) | **am Code geprüft** |
| Segment-Lookup | `GetNodeByNameAndParentIfExists` mit Segment, aktuellem Knoten und dem Kind-Typ `FOLDER` | **am Code geprüft** |
| Wiederverwendung | Wenn ein Knoten gefunden wird: er wird wiederverwendet | **am Code geprüft** |
| Anlegen | Wenn kein Knoten gefunden wurde: `CreateFolder` mit Segment und Knoten-Id des aktuellen Knotens, mit Netzwerk-Zwang und ohne Abbruch bei ignorierten Konflikten | **am Code geprüft** |
| Sonderfall | Segment `"Amazon Drive"` ⇒ Wurzelknoten (`GetRootNode`) | **am Code geprüft** |

**Kernfrage beantwortet:** Ja, das Original legt Zielordner **an**, und zwar über
`CreateFolder` (intern `POST /drive/v1/nodes` mit dem Kind `FOLDER`).
Vorhandene Knoten werden **wiederverwendet** (`GetNodeByNameAndParentIfExists`).

### 3.3 Wann das läuft: `ScheduleRecord.LoadDestinationNode`

**am Code geprüft** (`ScheduleRecord.LoadDestinationNode`):

- Wenn `DestinationNodeId` leer ist: `GetSuggestedCloudPathSegmentsForLocalPath` mit dem
  lokalen Pfad liefert Segmente, `GetOrCreateBackupDestination` legt sie an, und die
  Knoten-Id wird in `DestinationNodeId` gespeichert
- Das bestätigt: Der Vorschlag wird berechnet, die Knoten werden angelegt, die ID wird
  gespeichert.

---

## 4. Leitung: `parentNodeId`

### 4.1 Der Parameter

| Aspekt | Befund |
|---|---|
| Wire-Parameter | `parentNodeId` (Query-Parameter bei `POST` und `PUT`) |
| Quelle | `ScheduleRecord.DestinationNodeId` → `UploadRecord.DestinationNodeId` |
| Bedeutung | Der Zielordner als Cloud-Knoten; alle Dateien eines Backups landen direkt in diesem Ordner (keine Verschachtelung pro Datei) |

### 4.2 POST und PUT

| Methode | Pfad | Query-Parameter |
|---|---|---|
| `POST` | `/v2/upload` | `conflictResolution`, `fileSize`, `name`, `parentNodeId` |
| `PUT` | `/v2/upload/<nodeId>` | identisch (`conflictResolution`, `fileSize`, `name`, `parentNodeId`) — vgl. `docs/SPEC_UPLOAD.md` §2.4 |

**Ohne `fileSize` ⇒ 400 `MissingFileSize`** (live belegt, `docs/SPEC_UPLOAD.md` §2.4).

### 4.3 Was das Original sendet

| Parameter | Original | Beleg |
|---|---|---|
| `parentNodeId` | `DestinationNodeId` (Cloud-Knoten) | **am Code geprüft** (`UploadRecord.DestinationNodeId`, `GetUploadRecord` übernimmt `destinationNodeId`) |
| `conflictResolution` | `NONE` (Backup) / `RENAME` (`SaveAsNew`) / `PUT` (`Replace`) | **am Code geprüft** (`CloudDriveConflictResolutionExtensions`: None→NONE, Rename→RENAME) |
| `fileSize` | Byte-Länge | **weiterhin aus alten Dumps** (`UploadRequest`) |
| `name` | Dateiname | **weiterhin aus alten Dumps** (`UploadRequest`) |
| `contentDate` | **nicht gesendet** (server-seitige Ableitung) | **weiterhin aus alten Dumps** (`docs/SPEC_CAPTURE_DATE.md`) |

### 4.4 Was WIR heute senden

| Parameter | Wir | Beleg |
|---|---|---|
| `parentNodeId` | `web_dir_node.id` (Root → Pictures → Web) | **am Code geprüft** (`src/prime_photos.rs:953`) |
| `conflictResolution` | `RENAME` (hartkodiert) | **am Code geprüft** (`src/prime_photos.rs:965`, `src/urls.rs:314`) |
| `fileSize` | Byte-Länge | **am Code geprüft** (`src/prime_photos.rs:934`) |
| `name` | Dateiname (NFC-normalisiert) | **am Code geprüft** (`src/prime_photos.rs:959`) |
| `contentDate` | **nicht gesendet** | **am Code geprüft** (`src/prime_photos.rs:921`) |

---

## 5. Fehlerfälle

| Fehler | Original | UI-Text | Was passiert mit den Dateien |
|---|---|---|---|
| Ziel fehlt / weg | `ScheduleError.DestinationNodeNotAvailable` | `clouddrive_desktop_scheduleerror_destinationmissing_header` = **„Stammfehler"** + `…_destinationmissing_text` = **„Ziel nicht verfügbar"** — **am Code geprüft** (`ScheduleErrorExtensions.Update`) | Der Lauf wird nicht gestartet; das Schedule trägt den Fehlercode |
| Ziel wurde gelöscht | `ScheduleError.DestinationNodeNotAvailable` (via `IsNodeDeleted`) | wieoben | wieoben |
| Speichern fehlschlägt | `…_unabletosave_destinationmissing_title` = „Bitte wählen Sie einen anderen Zielort aus", `…_extra` = „Der ausgewählte Zielort existiert nicht mehr." | Dialog bleibt offen | Keine Persistenz, kein Upload |

**am Code geprüft** (`BackupScannerWorker.ProcessScanRequest`):
- Wenn `DestinationNodeId` leer ist → `ScheduleError.DestinationNodeNotAvailable`
- Wenn `IsNodeDeleted` für `DestinationNodeId` zutrifft → `ScheduleError.DestinationNodeNotAvailable`

**Was das Original dann mit den Dateien macht:** Es werden **keine** Dateien
hochgeladen. Der Lauf bricht ab, das Schedule trägt den Fehlercode. Die Dateien
bleiben lokal und werden beim nächsten Lauf erneut versucht.

---

## 6. Zusammenspiel mit Dedupe/Konflikt

Der Zielknoten ist der **Eltern-Knoten** für die Dedup-Prüfung und den
Konflikt-Weg:

| Mechanismus | Rolle des Zielknotens |
|---|---|
| `PreUploadCheck` | Namens-Lookup: `GetNodeByNameAndParentIfExists` mit Name, Zielknoten und dem Kind-Typ `FILE` |
| `GetNodeByMd5` | über alle Ordner; der Treffer wird **cross-path** übernommen, **ohne** ihn mit dem Zielknoten zu vergleichen |
| Konflikt (`Replace`) | `PUT /v2/upload/<nodeId>` — der `nodeId` kommt aus dem 409-Antwort, nicht aus dem Zielknoten |
| Konflikt (`SaveAsNew`) | `RENAME` im POST — Zielknoten bleibt der Elter |

**Korrektur (2026-10-16):** Die frühere Fassung behauptete, der `GetNodeByMd5`-Treffer
werde mit dem Zielknoten verglichen. Das ist **falsch** — der Treffer wird
**cross-path** übernommen, ohne Vergleich mit dem Zielknoten. Die maßgebliche
Beschreibung steht in `docs/SPEC_FILE_IDENTITY.md` §2; hier wird keine zweite
Fassung geführt.

**Knapp mit Verweis:** `docs/SPEC_FILE_IDENTITY.md` §2 (Dedup-Identität,
cross-path), `docs/SPEC_DEDUP.md` §1–§2 (Dedup-Identität),
`docs/SPEC_CONTENT_FILTER_DESTINATION.md` §1 (Konflikt-Wire-Verhalten).

---

## 7. Was WIR heute tun

| Aspekt | Wir | Beleg |
|---|---|---|
| Upload-Ziel | Root → Pictures → Web ( Cloud-Ordner „Web" unter „Pictures") | **am Code geprüft** (`src/prime_photos.rs:818–846`) |
| Woher kommt der Eltern-Knoten | `update_web_pictures_dir_node`: `find_root_node` holt den Wurzelknoten, dann wird per `find_children_nodes` mit einem Filter nach dem Ordner „Pictures" und danach nach dem Ordner „Web" gesucht | **am Code geprüft** (`src/prime_photos.rs:818–846`) |
| Gibt es eine Ziel-Auswahl? | **Nein** — die Ziel-Auswahl ist deaktiviert | **am Code geprüft** (`src/backup_config.rs:71–75`) |
| Wie wird der Eltern-Knoten transportiert? | aus dem gespeicherten Web-Ordnerknoten (`web_dir_node`) | **am Code geprüft** (`src/prime_photos.rs:952–955`) |
| Wo wird er gesetzt? | `urls::upload_url` mit Zone, Konflikt-Auflösung, Dateigröße, Dateiname und Eltern-Knoten | **am Code geprüft** (`src/prime_photos.rs:963–970`) |
| BackupFolder hat `destination_node_id`? | **Nein** — `BackupFolder` hat nur `path`, `name`, `enabled`, `dedup`, `conflict_resolution`, `total_files`, `uploaded_files`, `last_updated`, `status` | **am Code geprüft** (`src/folders.rs:193–214`) |

**Die Lücke:** Wir haben weder `destination_node_id` noch `BackupPathMapper` noch
den Cloud-Picker. Der Upload geht immer in denselben Cloud-Ordner („Web"), ohne
Möglichkeit der Auswahl. Der Quellordner ist Teil des lokalen Pfads, aber nicht
Teil des Cloud-Zielpfads.

---

## 8. Lücke-Tabelle (Original vs. wir)

| Thema | Original | Unser Stand | Lücke |
|---|---|---|---|
| **Zielordner-Typ** | Cloud-Knoten (`destination_node_id`) | Keiner (hartkodiert Root/Pictures/Web) | **Komplett offen** |
| **Ziel-Auswahl** | Cloud-Picker (`FolderPickerViewController`) | Deaktiviert mit „Noch nicht verfügbar" | **Komplett offen** |
| **Vorschlag** | `Backup/<Macname>/<Pfad>` (automatisch) | Keiner | **Komplett offen** |
| **Anlegen** | `GetOrCreateBackupDestination` (Segment für Segment) | Keiner | **Komplett offen** |
| **Persistenz** | SQLite `schedules.destination_node_id` | Keine | **Komplett offen** |
| **Fehlerbehandlung** | `DestinationNodeNotAvailable`, „Ziel nicht verfügbar" | Keine | **Komplett offen** |
| **parentNodeId** | `DestinationNodeId` | `web_dir_node.id` (fester Cloud-Pfad) | **Verschoben, nicht gleich** |
| **Der Ordner im Zielpfad** | Ja, als letztes Segment | Nein | folgt aus `BackupPathMapper` |
| **Ziel-Pflicht** | Speichern gesperrt ohne Ziel | Keine (Ziel-Auswahl deaktiviert) | folgt aus `BackupPathMapper` |

---

## 9. Umsetzungs-Skizze

### 9.1 Was ein `destination_node_id` je Ziel braucht

| Schritt | Beschreibung | Original-Referenz |
|---|---|---|
| 1. Feld | ein optionales `destination_node_id` in `BackupFolder` | `ScheduleRecord.DestinationNodeId` |
| 2. Default | `GetSuggestedCloudPathSegmentsForLocalPath` baut `Backup/<Macname>/<Pfad>` | `BackupPathMapper` |
| 3. Anlegen | `GetOrCreateBackupDestination` (Segment für Segment) | `BackupPathMapper` |
| 4. Persistenz | `destination_node_id` in `BackupFolder` | `schedules.destination_node_id` |
| 5. Upload | `parent_node_id` aus dem Zielknoten des Ordners | `UploadRecord.DestinationNodeId` |
| 6. Fehler | `DestinationNodeNotAvailable`, „Ziel nicht verfügbar" | `ScheduleError` |

### 9.2 Was ein Cloud-Picker bräuchte

| Schritt | Beschreibung | API |
|---|---|---|
| 1. Baum-Navigation | `GET /drive/v1/nodes` mit Filtern nach Ordner-Kind, Status und Name, plus Eltern-Id | `docs/PROTOCOL.md` §2.10 |
| 2. Root | `GET /drive/v1/nodes` mit dem Filter `isRoot:true` | `docs/PROTOCOL.md` §2.10 |
| 3. Ordner anlegen | `POST /drive/v1/nodes` mit dem Kind `FOLDER`, Name, Elternliste und Ressourcenversion `V2` | `docs/PROTOCOL.md` §2.12 |
| 4. Neuer-Ordner-Dialog | Validierung: Sonderzeichen, 280 Zeichen, Namenskonflikt | `CreateCloudFolderViewController` |

### 9.3 Was der Pfad-Mapper leisten muss

| Schritt | Beschreibung | Original-Referenz |
|---|---|---|
| 1. Segment-Bau | `Backup/<Macname>/<VolumeLabel>/<Pfad>` | `GetSuggestedCloudPathSegmentsForLocalPath` |
| 2. Segment-Lookup | `GetNodeByNameAndParentIfExists` mit Segment und aktuellem Knoten | `BackupPathMapper` |
| 3. Segment-Anlegen | `CreateFolder` mit Segment und Knoten-Id, mit Netzwerk-Zwang | `CreateFolder` |
| 4. Sonderfall | `"Amazon Drive"` ⇒ Root-Knoten | `BuildCloudPathSegments` |
| 5. Rekursion | Start am Wurzelknoten | `GetOrCreateBackupDestination` |

### 9.4 Was davon Post-MVP wäre

| Thema | Begründung |
|---|---|
| Voller Cloud-Picker mit Baum-Navigation | Erfordert `/drive/v1/nodes`-Integration und UI-Aufwand |
| Voller `BackupPathMapper` mit Segment-Anlage | Erfordert `/drive/v1/nodes`-POST und Fehlerbehandlung |
| `CanChangeDestination`-Äquivalent | Erfordert UI-Änderung im Dialog |
| `DestinationNodeNotAvailable`-Fehlerbehandlung | Erfordert Worker-Änderung |
| **Nicht Post-MVP:** `destination_node_id` Feld in `BackupFolder` | **MVP** — Grundlage für alles andere |
| **Nicht Post-MVP:** `parent_node_id` aus dem Zielknoten des Ordners | **MVP** — Upload-Ziel |
| **Nicht Post-MVP:** Vorschlag `Backup/<Macname>/<Pfad>` | **MVP** — Default-Ziel |

### 9.5 Was ohne Live-Probe am Konto nicht belegbar ist

| Thema | Warum unbelegt |
|---|---|
| Exakte Segmentliste für Laufwerke außerhalb des User-Homes | Konstruktion, nicht ausgeführt |
| Ob der Cookie-Host `POST /drive/v1/nodes` unterstützt | Nicht live getestet — offene Frage für den Spiegelungs-Auftrag |
| Ob der Cloud-Picker „Pictures"/„Web" vorschlägt | Kontozustand, nicht Code |
| Wire-Pfad `UploadRecord.DestinationNodeId` → HTTP `parentNodeId`-Query | Code-Teil bestätigt, HTTP-Mapping nicht verfolgt |
| Nutzort der Speicher-Meldung | Strings vorhanden, zeigender Code-Pfad nicht verfolgt |

---

## 10. Konsequenz für uns

**User-Entscheidung 2026-10-16:** Das Zielfeld fällt aus dem Dialog (die
Ziel-Auswahl bleibt deaktiviert). Die **Spiegelung** (Anlegen der Cloud-Ordner
nach dem Muster `Backup/<Macname>/<Pfad>`) wird ein **eigener Auftrag nach
dieser Spec**.

**Kernfrage beantwortet:** Ja, das Original legt Zielordner **an**, und zwar über
`CreateFolder` (intern `POST /drive/v1/nodes` mit dem Kind `FOLDER`).
Vorhandene Knoten werden **wiederverwendet** (`GetNodeByNameAndParentIfExists`).
**Beleg:** `BackupPathMapper.GetOrCreateBackupDestination` — **am Code geprüft**.

**Was das für unseren Cookie-Host bedeutet:** Der Cookie-Host
(`content-<zone>.drive.amazonaws.com`) unterstützt `POST /v2/upload` und
`PUT /v2/upload/<nodeId>` (live belegt, `docs/SPEC_UPLOAD.md` §2.4). Die
Ordner-Anlage über `POST /drive/v1/nodes` ist **nicht live getestet** — das ist
eine offene Frage, die im Spiegelungs-Auftrag geklärt werden muss.

**Zusatzbefunde (2026-10-16, am Code geprüft):**
- `CanChangeDestination` ist in **jedem** Aufruf `true` (Voll-Dump-Grep) —
  die Property ist vestigial, die Asymmetrie ist ein Refactor-Artefakt.
- Der Fehler-UI-Text hat **zwei** Keys: `_header` = „Stammfehler" und
  `_text` = „Ziel nicht verfügbar" (`ScheduleErrorExtensions.Update`).
- Der Picker ist eine **Breadcrumb-Tabelle** (`NodeBranch`: Parents + Current +
  Children), kein hierarchischer Baum; er startet am Root-Knoten.

---

## 11. Nachweis

### 11.1 Wie diese Spec geprüft wurde

**Datum:** 2026-10-16

**Decompiler:**
```bash
ilspycmd --version
# ilspycmd: 11.1.0.9782
# ICSharpCode.Decompiler: 11.1.0.9782
```

**DLL:** `/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll`

**Aufrufe:**
```bash
# Klassen auflisten
ilspycmd -l c "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"

# Einzelne Typen dekompilieren
ilspycmd -t Amazon.CloudDrive.Services.Schedules.BackupPathMapper "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Database.ManualUpload.ScheduleRecord "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.UI.DriveWindow.DesktopBackup.ScheduledBackupItem "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.UI.DriveWindow.DesktopBackupLegacyViews.BackupConfigurationViewModel "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.UI.DriveWindow.TabItemView.DriveBackup.ScheduledBackupConfigurationView.BackupConfigurationViewController "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.UI.DriveWindow.PickerViews.FolderPickerViewController "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.UI.DriveWindow.PickerViews.CreateCloudFolderViewController "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Services.Schedules.BackupScannerWorker "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Services.CloudDrive.Model.CloudDrivePickerViewModel "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Services.CloudDrive.Model.NodeBranch "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.UI.DriveWindow.DesktopBackup.ScheduledBackupItemExtensions "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.UI.DriveWindow.DesktopBackupLegacyViews.BackupConfigurationParams "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Database.ManualUpload.UploadRecord "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Database.ScheduleError "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.UI.ScheduleErrorExtensions "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Services.CloudDriveConflictResolutionExtensions "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Services.CloudDrive.NodeService "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -t Amazon.CloudDrive.Services.CloudDrive.CloudDriveStorageService "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -o /tmp/pp-decomp "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
grep -n "CanChangeDestination" /tmp/pp-decomp/"Amazon Photos.decompiled.cs"
```

**REA:** Nicht benutzt. `inspect_managed_artifact` liefert `metadata-not-admitted`
(Budget 67 MB), `search_android_classes` ist der falsche Weg (jadx/APK).

### 11.2 Tabelle: Aussage → vorher (Dump) → jetzt (Code) → geändert? → Beleg

| Aussage | vorher (Dump) | jetzt (Code) | geändert? | Beleg (Typ/Methode) |
|---|---|---|---|---|
| Zielordner ist Cloud-Knoten | `ScheduledBackupItem.DestinationNodeId` | identisch | nein | `ScheduledBackupItem.DestinationNodeId` |
| Persistenz in SQLite | `schedules.destination_node_id` | identisch | nein | `docs/SPEC_FOLDERS.md` §3.1 |
| Vorschlag: `Backup/<Macname>/<Pfad>` | `GetSuggestedCloudPathSegmentsForLocalPath` | identisch | nein | `BackupPathMapper.GetSuggestedCloudPathSegmentsForLocalPath` |
| Reihenfolge: Backup → Macname → VolumeLabel → Pfad | `BuildCloudPathSegments` | identisch | nein | `BackupPathMapper.BuildCloudPathSegments` |
| Anlegen: `GetOrCreateBackupDestination` | `CreateFolder` | identisch | nein | `BackupPathMapper.GetOrCreateBackupDestination` |
| Wiederverwendung vorhandener Knoten | `GetNodeByNameAndParentIfExists` | identisch | nein | `BackupPathMapper.GetOrCreateBackupDestination` |
| Sonderfall: `"Amazon Drive"` ⇒ Root | `BuildCloudPathSegments` | identisch | nein | `BackupPathMapper.GetOrCreateBackupDestination` |
| `LoadDestinationNode` setzt `DestinationNodeId` | `ScheduleRecord.LoadDestinationNode` | identisch | nein | `ScheduleRecord.LoadDestinationNode` |
| Asymmetrie: `CanChangeDestination` nicht verkabelt | `WireEvents` | identisch | nein | `BackupConfigurationViewController.WireEvents` |
| Picker: `FolderPickerViewController` | `CanCreateNewFolders` | identisch | nein | `FolderPickerViewController.CanCreateNewFolders` |
| Picker-Baumstruktur | **UNKNOWN** | `NodeBranch` (Parents + Current + Children) | **ja** | `NodeBranch` |
| Picker-Navigation | **UNKNOWN** | `GetRootBranch` / `GetNodeBranch` | **ja** | `CloudDrivePickerViewModel` |
| Fehler: `DestinationNodeNotAvailable` | `ScheduleError` | identisch | nein | `BackupScannerWorker.ProcessScanRequest` |
| Fehler: `IsNodeDeleted` | `IsNodeDeleted` | identisch | nein | `BackupScannerWorker.ProcessScanRequest` |
| Speicher-Meldung | `…_unabletosave_destinationmissing_title` | Strings in `Strings`-Klasse vorhanden; Nutzort nicht vollständig verfolgt | **ja** | **teilgeprüft** (`Strings`-Klasse; Save-Failure-Pfad nicht verfolgt) |
| `parentNodeId` im Upload | `DestinationNodeId` | Code-Teil bestätigt; Wire-Pfad aus alten Dumps | **ja** | **teilgeprüft** (`GetUploadRecord` mit `destinationNodeId` → `UploadRecord.DestinationNodeId`; HTTP-Query-Mapping nicht verfolgt) |
| Unser Upload-Ziel | Root → Pictures → Web | identisch | nein | `src/prime_photos.rs:818–846` |
| Unser `parentNodeId` | `web_dir_node.id` | identisch | nein | `src/prime_photos.rs:952–955` |
| Unser BackupFolder ohne `destination_node_id` | kein Feld | identisch | nein | `src/folders.rs:193–214` |
| Unser Ziel-Feld deaktiviert | deaktiviert | identisch | nein | `src/backup_config.rs:71–75` |
| Neuen-Ordner-Dialog Validierung | **UNKNOWN** | `ValidateInput` (280 Zeichen, Sonderzeichen, Namenskonflikt) | **ja** | `CreateCloudFolderViewController.ValidateInput` |

### 11.3 Was NEU nachgeprüft wurde

| Thema | Quelle | Ergebnis |
|---|---|---|
| `BackupPathMapper` | `ilspycmd -t` | `GetOrCreateBackupDestination`, `GetSuggestedCloudPathSegmentsForLocalPath`, `BuildCloudPathSegments` |
| `ScheduleRecord` | `ilspycmd -t` | `LoadDestinationNode` |
| `ScheduledBackupItem` | `ilspycmd -t` | `DestinationNodeId` |
| `BackupConfigurationViewModel` | `ilspycmd -t` | `CanChangeDestination`, `SetDestination`, `SetLocalPath`, `CloudPath` |
| `BackupConfigurationViewController` | `ilspycmd -t` | `WireEvents`, `_selectDestinationFolderButton` |
| `FolderPickerViewController` | `ilspycmd -t` | `CanCreateNewFolders`, `CreateNewFolder`, `LoadAsync` |
| `CloudDrivePickerViewModel` | `ilspycmd -t` | `GetRootBranch`, `GetNodeBranch` |
| `NodeBranch` | `ilspycmd -t` | `Parents`, `Current`, `Children`, `TotalItems` |
| `BackupScannerWorker` | `ilspycmd -t` | `ProcessScanRequest`, `DestinationNodeNotAvailable` |
| `src/prime_photos.rs` | `codegraph_explore` | `upload_picture`, `update_web_pictures_dir_node` |
| `src/folders.rs` | `codegraph_explore` | `BackupFolder` |
| `src/backup_config.rs` | `codegraph_explore` | `render_backup_config` |
| `src/backup.rs` | `codegraph_explore` | `start_folder_upload`, `finalize_upload` |

### 11.4 Was übernommen wurde (mit Grund)

| Thema | Quelle | Grund |
|---|---|---|
| F3 (Zielordner) | `docs/SPEC_CONTENT_FILTER_DESTINATION.md` §3 | Bereits geprüft, konsistent mit meiner Analyse |
| schedules-Spalten | `docs/SPEC_FOLDERS.md` §3.1 | Bereits geprüft |
| Upload-Endpunkte | `docs/PROTOCOL.md` §2.17, `docs/openapi.yaml` | Bereits geprüft |
| Dedup/Konflikt | `docs/SPEC_DEDUP.md` | Bereits geprüft |
| UI-Flows | `docs/UI_FLOWS.md` §3 | Bereits geprüft |
| UI-States | `docs/UI_STATES.md` §3 | Bereits geprüft |

### 11.5 Was UNKNOWN geblieben ist

| Thema | Warum unbelegt |
|---|---|
| Live-Probe am Konto | Nicht durchgeführt (User-Auftrag: nur lesend) |
| Ob `CanChangeDestination` in einem anderen Dialog wirksam ist | Property ist überall an (Voll-Dump-Grep); ein abgeschalteter Kontext existiert nicht |
| Ob der Cookie-Host `POST /drive/v1/nodes` (Ordner-Anlage) unterstützt | **Nicht live getestet** — offene Frage für den Spiegelungs-Auftrag |
| Ob der Cloud-Picker „Pictures"/„Web" vorschlägt | Kontozustand, nicht Code — der Picker zeigt Root + Kinder |
| Wire-Pfad `UploadRecord.DestinationNodeId` → HTTP `parentNodeId`-Query | `GetUploadRecord` bestätigt den Code-Teil; das HTTP-Query-Mapping liegt tiefer in der Pipeline und wurde nicht verfolgt |
| Nutzort der Speicher-Meldung (`unabletosave_destinationmissing_*`) | Strings in `Strings`-Klasse vorhanden; der zeigende Code-Pfad wurde nicht verfolgt |

---

## 12. Korrekturen an den vorhandenen Docs

1. **`docs/SPEC_CONTENT_FILTER_DESTINATION.md` §3.1 — Asymmetrie bestätigt.**
   Die Spec sagt: „`CanChangeDestination` existiert am VM, wird aber am Ziel-Button
   **nicht** verkabelt — der Ziel-Button ist in diesem Dialog immer aktiv." Diese
   Spec bestätigt das und ergänzt: „`CanChangeSource` steuert den Quellordner-Button
   („Ändern...") — die Aktivierung von `_selectBackupFolderButton` folgt `canChangeSource`
   (`WireEvents`)."
   Kein Widerspruch, nur schärfere Belege.

2. **`docs/SPEC_CONTENT_FILTER_DESTINATION.md` §3.2 — Vorschlag bestätigt.**
   Die Spec sagt: „`BackupPathMapper.GetSuggestedCloudPathSegmentsForLocalPath` baut:
   Segment `"Backup"`, Segment `<Maschinenname>`, VolumeLabel, Eltern-Pfade." Diese
   Spec bestätigt das und ergänzt: „`BuildCloudPathSegments` (Segment `"Amazon Drive"`
   ⇒ Root)." Kein Widerspruch, nur schärfere Belege.

3. **`docs/SPEC_UPLOAD.md` §2.1 — `parentNodeId` bestätigt.**
   Die Spec sagt: „`parentNodeId` = cached Web folder id (oder `NONE` in dry run)."
   Diese Spec bestätigt das und ergänzt: „der Eltern-Knoten stammt aus dem
   gespeicherten Web-Ordnerknoten (`web_dir_node`)" (`src/prime_photos.rs:953`). Kein
   Widerspruch, nur schärfere Belege.

---

## 13. Belege (Symbole, Keys, Dateien — keine Codekopien)

* **Zielordner (Original, am Code geprüft):**
  `BackupPathMapper.GetSuggestedCloudPathSegmentsForLocalPath`,
  `…GetOrCreateBackupDestination`, `…GetCloudPathSegmentsFromNodeId`,
  `…BuildCloudPathSegments` (Segment `"Amazon Drive"` ⇒ Root),
  `ScheduleRecord.LoadDestinationNode` / `…DestinationNodeId`,
  `ScheduledBackupItem.DestinationNodeId`,
  `ScheduledBackupItemExtensions.ToScheduleRecord`,
  `BackupConfigurationViewModel.SetLocalPath` / `…SetDestination` / `…CloudPath` /
  `…RootPathSegment` / `…CanChangeSource` / `…CanChangeDestination`,
  `BackupConfigurationParams` (`CanChangeDestination` überall `true`),
  `BackupConfigurationViewController.WireEvents` / `…_selectDestinationFolderButton` /
  `…ChooseCloudPath`,
  `FolderPickerViewController` (`CanCreateNewFolders`, `CreateNewFolder`, `LoadAsync`),
  `CreateCloudFolderViewController.ValidateInput`,
  `CloudDrivePickerViewModel` (`GetRootBranch`, `GetNodeBranch`),
  `NodeBranch` (`Parents`, `Current`, `Children`, `TotalItems`),
  `NodeService.GetRootBranch` / `…GetNodeBranch` / `…GetNodeByNameAndParentIfExists` /
  `…IsNodeDeleted`,
  `CloudDriveStorageService.CreateFolder`,
  `CloudDriveConflictResolutionExtensions` (None→NONE, Rename→RENAME),
  `ScheduleError` (Enum), `ScheduleErrorExtensions.Update` / `…IsRetriable`,
  `BackupScannerWorker.ProcessScanRequest` / `…HandleMissingDestinationNode`
  (`ScheduleError.DestinationNodeNotAvailable`),
  `UploadRecord.DestinationNodeId`.
* **Deutsche Labels (`de.lproj/Localizable.strings`):**
  `clouddrive_desktop_backupconfiguration_backupdestinationtitle` („Ziel für Datensicherung:"),
  `clouddrive_desktop_backupconfiguration_selectbackupdestination_button` („Ändern..."),
  `clouddrive_desktop_clouddrivepicker_selectlocationmessage` („Speicherort für Ihre Dateien auswählen"),
  `clouddrive_desktop_clouddrivepicker_selectbutton` („Auswählen"),
  `clouddrive_desktop_clouddrivepicker_newfolderdialog_errorinvalidcharacters`,
  `clouddrive_desktop_clouddrivepicker_newfolderdialog_errornametoolong`,
  `clouddrive_desktop_clouddrivepicker_newfolderdialog_errornameconflict`,
  `clouddrive_desktop_scheduleerror_destinationmissing_text` („Ziel nicht verfügbar"),
  `clouddrive_desktop_statuswindow_scheduleconfiguration_unabletosave_destinationmissing_title`
  („Bitte wählen Sie einen anderen Zielort aus"),
  `clouddrive_desktop_statuswindow_scheduleconfiguration_unabletosave_destinationmissing_extra`
  („Der ausgewählte Zielort existiert nicht mehr.").
* **Unser Code (am Code geprüft):**
  `src/prime_photos.rs:818–846` (`update_web_pictures_dir_node`),
  `src/prime_photos.rs:921–970` (`upload_picture`, `parent_node`),
  `src/urls.rs:314–325` (`upload_url`),
  `src/folders.rs:193–214` (`BackupFolder`),
  `src/backup_config.rs:29–36` (`BackupConfigForm`),
  `src/backup_config.rs:44–101` (`render_backup_config`),
  `src/backup.rs:1027–1038` (`start_folder_upload`),
  `src/backup.rs:1402–1477` (`finalize_upload`).
* **Cross-Referenz:** `docs/SPEC_CONTENT_FILTER_DESTINATION.md` §3,
  `docs/SPEC_FOLDERS.md` §3, `docs/SPEC_UPLOAD.md` §2,
  `docs/SPEC_DEDUP.md` §1–§2, `docs/PROTOCOL.md` §2,
  `docs/openapi.yaml`, `docs/UI_FLOWS.md` §3, `docs/UI_STATES.md` §3.

---

## 14. REDA

* Account identifiers, user names, device serial numbers and absolute file paths
  are shown as placeholders (`<account-id>`, `<user-name>`, `<path>`).
* German UI labels are quoted verbatim from `de.lproj/Localizable.strings`.
* SQLite-derived structural facts are described; no personal row data is included.
* No verbatim decompiled code is reproduced.
