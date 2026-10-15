# Spec: Inhaltsfilter, Zielordner und Konflikt-Wire-Verhalten des Originals

**Status:** Clean-Room-Recon (nur lesend) des offiziellen macOS-Amazon-Photos-Clients
11.3.0 (`CFBundleShortVersionString`), `CommitId 1d7941b5`. Eigene Worte und Tabellen,
keine verbatim Code-Auszüge, keine Personendaten. Deutsche UI-Strings werden wörtlich
aus `Contents/Resources/de.lproj/Localizable.strings` zitiert, jeweils mit dem Key.

**Frage.** Vier Punkte, die für unsere Ziel-Dialoge offen waren: (F1) ob es einen
Rename-Endpunkt gibt und was „Hash geändert ⇒ ersetzen, Hash gleich ⇒ umbenennen"
im Original tatsächlich ist; (F2) was der Inhaltsfilter „Was soll gesichert werden"
 konkret tut; (F3) wie der Zielordner der Sicherung umgesetzt ist; (F4) die
vollständige Feldliste des Original-Dialogs.

**Quellen (nur lesend).** `/Applications/Amazon Photos.app` — App-Bundle unverändert;
eine **Kopie** von `Contents/MonoBundle/Amazon Photos.dll` (SHA-256
`34a616e7…bcedc0`, identisch mit dem Original) wurde nach einem Scratch-Verzeichnis
unter `$TMPDIR` kopiert und dort vollständig decompiliert (`ilspycmd`/ILSpy 11.1).
Zusätzlich `Contents/Resources/de.lproj/Localizable.strings` und die im DLL
eingebettete Ressource `…SupportedContentTypes.supported-content-types.json`.
Bereits vorhandene Specs wurden gegengeprüft; Korrekturen stehen in §8.

---

## 1. F1 — Gibt es einen Rename-Endpunkt, und was macht er?

### 1.1 Die Werte auf der Leitung

Es gibt **eine** Wire-Enum, `Amazon.CloudDrive.Services.CloudDriveConflictResolution`,
mit **drei** Werten. Die Zuordnung von Enum zu Query-Parameter steht in
`CloudDriveConflictResolutionExtensions.Values`:

| Wire-Enum | Query-Wert | Entsteht bei uns/auf der Leitung aus |
|---|---|---|
| `None` | `NONE` | Konflikt-Politik `None` („Jedes Mal nachfragen") **und** `Ignore` (dort aber gar kein Request) |
| `Rename` | `RENAME` | Konflikt-Politik `SaveAsNew` („Neue Datei automatisch umbenennen") |
| `RenameExisting` | `RENAME_EXISTING` | **nur** `UploadType.PhotosImport` bei einem One-Time-Upload (`schedule_id == "ManualUpload"`), via `UploadRecordExtensions.GetConflictResolution` |

Wichtig: **`Replace` („Alte Datei ersetzen") hat keinen eigenen Wire-Wert.** Es wird
`overwrite = true` gesetzt **und** `nodeId = destinationNode` in den Request
eingetragen; der Endpunkt wechselt dadurch von `POST v2/upload` auf
`PUT v2/upload/<nodeId>` (`CloudDriveService.UploadFile`: bei `Method == Put` wird
`v2/upload/<NodeId>` statt `v2/upload` gebaut). Ein `OVERWRITE`-Wert existiert nicht.

Für **Backup**-Uploads (Autosave) liefert `UploadRecordExtensions.GetConflictResolution`
immer `None` — `RenameExisting` ist ausschließlich der Apple-Photos-Import-Pfad.

### 1.2 UI-Option ↔ Wire-Wert

Der Entscheidungsort ist `UploadViaSimpleOrLegacyUploader`
(`CloudDriveStorageService`), gesteuert über `NameConflictStrategy.GetConflictResolution`:

| UI-Option (Radio) | Intern | Verhalten **vor** dem Request | Wire |
|---|---|---|---|
| „Jedes Mal nachfragen" | `None` | Zielknoten **nicht** vorhanden ⇒ normaler Upload; **vorhanden** ⇒ `FileUploadNameConflictException` (kein Request) | `NONE` |
| „Neue Datei ignorieren" | `Ignore` | Zielknoten vorhanden ⇒ `FileSkippedException` (kein Request) | — |
| „Neue Datei automatisch umbenennen" | `SaveAsNew` | **unabhängig** davon, ob ein Zielknoten existiert: `overwrite = false` | `RENAME` |
| „Alte Datei ersetzen" | `Replace` | Zielknoten vorhanden ⇒ `overwrite = true`, `nodeId = destinationNode` | `PUT` (kein Enum-Wert) |

**Präzisierung gegenüber `docs/SPEC_CONFLICT_OPTIONS.md`:** `SaveAsNew` ist in
`NameConflictStrategy.GetConflictResolution` die **einzige** Option **ohne** Wächter
auf `destinationNodeId != null` (Replace und Ignore haben ihn). Folge: bei
„Neue Datei automatisch umbenennen" geht **jeder** Upload mit `conflictResolution=RENAME`
raus — auch dann, wenn gar kein Namenskonflikt vorliegt. Korrektur in §8.

### 1.3 Gleicher Name **und** gleicher MD5 — Reihenfolge der Prüfungen

Der Ablauf ist streng sequenziell; `PreUploadCheck` läuft **vor jedem** Netz-Aufruf und
vor jeder Konflikt-Entscheidung:

1. **Namens-Lookup:** Knoten per Name + Parent aus dem lokalen Knoten-Cache
   (`GetNodeByNameAndParentIfExists`, exaktes `name = ?`, Typ `"FILE"`) — es sei denn,
   der Datensatz trägt bereits eine `NodeId`, dann wird per `GetNodeByNodeId` aufgelöst.
2. **MD5-Vergleich am selben Knoten:** `MD5 == md5 && status == "AVAILABLE"`.
3. **Nur wenn dedup-berechtigt** (`UploadType.Deduped` oder `PhotosImport`, sonst ist
   `GetSuppressFileUpload()` = `Deduplication`): zusätzlich `GetNodeByMd5(md5)` über
   **alle** Ordner. Ein Treffer zählt ebenfalls als Duplikat.
4. **Treffer in 2 oder 3** ⇒ `FileIgnoredPreUploadException` ⇒ Status `Ignored`
   (UI: „Zuvor hochgeladen", Key `clouddrive_desktop_upload_statuswindow_statusignored`).
   **Es geht kein Request raus**, und `UploadViaSimpleOrLegacyUploader` — und damit
   die ganze Namenskonflikt-Logik — wird **nie erreicht**.

**Also: bei identischem Namen und identischem MD5 gewinnt die Inhalts-Identität,
es entsteht kein Konflikt und keine Umbenennung.** Das deckt sich mit
`docs/SPEC_CONFLICT_OPTIONS.md` §1.3.

**Der entscheidende Nebenbefund für F1:** Schritt 2 ist **nicht** vom Dedupe-Toggle
abhängig — der Namens-Lookup samt MD5-Vergleich läuft **immer**. Der Toggle steuert
**ausschließlich** Schritt 3, die über-pfade-hinweg Suche. Konsequenz: *gleicher Pfad
+ gleicher Inhalt* ist **immer** ein Skip, gleichgültig, ob „Duplikate überspringen"
an oder aus steht.

### 1.4 Gibt es **irgendeinen** Pfad, bei dem identischer Inhalt doch umbenannt wird?

Soweit aus dem Decompil ableitbar: **nein** — mit einer einzigen, eng umrissenen
Ausnahme, die keinen echten „Rename" erzeugt:

* Der Clientseitige Skip (Schritt 2 oben) greift bereits für den **gleichen Elternordner**.
* Für einen **anderen** Zielordner mit identischem Inhalt greift der Skip nur, wenn
  dedup-berechtigt (Schritt 3). Ist Dedupe **aus**, geht der Request raus — mit
  `RENAME`, wenn `SaveAsNew` gewählt ist. Das ist jedoch **kein** Umbenennen eines
  bestehenden Knotens, sondern ein regulärer Upload in einen anderen Ordner.
  Der Server kann ihn als Inhaltsduplikat ablehnen; der Client behandelt einen
  409 mit MD5-Treffer ohnehin als Skip (`HandleNodeResponse` setzt den Status auf OK
  und wirft `FileIgnoredException`).
* Der servergetriebene Retry `UploadHelper.RetryForNameConflict` verlangt ausdrücklich
  `data.ContentProperties.MD5 != md5` **und** `ErrorType ∈ {NAME_ALREADY_EXISTS,
  NameAlreadyExists}` **und** `IsAutoSave()`. Ein Inhaltsduplikat erfüllt die erste
  Bedingung **nicht** — dieser Weg kann identischen Inhalt also **nie** umbenennen.

**Fazit in einem Satz:** Nein — das vom User beschriebene Verhalten („Hash geändert ⇒
ersetzen, Hash gleich ⇒ umbenennen") beschreibt das Original **nicht**; das Original
prüft die Inhalts-Identität **zuerst** und überspringt identischen Inhalt immer als
„Zuvor hochgeladen", während die Konflikt-Optionen (`Ignore`/`SaveAsNew`/`Replace`/
`Ask`) **ausschließlich** für Namensgleichheit bei **abweichendem** Inhalt gelten.

---

## 2. F2 — Inhaltsfilter „Was soll gesichert werden"

### 2.1 Was das Original **zeigt** — und was nicht

Die vom User vermutete Dreier-Auswahl („Alles / Photos und Videos / Photos") **existiert
in der aktuellen Oberfläche so nicht**:

| Befund | Beleg |
|---|---|
| Der aktuelle Dialog zeigt **zwei Checkboxen**, keine Radio-Gruppe | `BackupConfigurationViewController.CreateContentFilterFieldBelow`: `_backupPhotosCheckbox`, `_backupVideosCheckbox` |
| Deutsche Labels **wörtlich** | `clouddrive_desktop_upload_settings_backup_photos_checkbox` = **„Fotos sichern"**; `clouddrive_desktop_upload_settings_backup_videos_checkbox` = **„Videos sichern"** |
| Die Gruppen-Überschrift „Was soll gesichert werden:" wird von **keiner** View referenziert | `clouddrive_desktop_backupconfiguration_selectcontentfiltertitle` = „Was soll gesichert werden:" — nur in der generierten `Strings`-Klasse, nicht in `BackupConfigurationViewController` |
| „Alles" ist in diesem Dialog **nicht wählbar** | `CreateContentFilterFieldBelow` schreibt bei `Everything` sofort `ContentFilter.PhotosAndVideo` zurück |

Es gibt zwei Checkboxen, die zusammen den Wert bilden
(`ScheduledBackupItem.UpdateContentFilter`): beide ⇒ `PhotosAndVideo`, nur Fotos ⇒
`Photos`, nur Videos ⇒ `Videos`.

### 2.2 Interne Enum-Werte

`Amazon.CloudDrive.Database.ManualUpload.ContentFilter`, `[Flags]`:

| Flag | Wert | UI erreichbar? |
|---|---|---|
| `None` | 0 | nein |
| `Photos` | 2 | ja (nur „Fotos sichern") |
| `Videos` | 4 | ja (nur „Videos sichern") |
| `Other` | 8 | **nein** — wird von keiner View gesetzt |
| `PhotosAndVideo` | 6 | ja (beide Checkboxen) |
| `Everything` | 14 | **nein** — wird im Dialog sofort auf `PhotosAndVideo` zurückgeschrieben |

Gespeichert in `schedules.content_filter` (Integer-Spalte, `ScheduleRecord.ContentFilter`).

### 2.3 Was tut jede Option konkret?

`ContentFilterExtensions.Matches(path)` ist die **einzige** Klassifikationsfunktion:

| Dateityp (per Endung) | Benötigtes Flag | Konsequenz |
|---|---|---|
| `image` **oder** `image_sidecar` | `Photos` | sonst nie |
| `video` **oder** `video_sidecar` | `Videos` | sonst nie |
| **alles andere** | **keines** — es wird **immer `false`** zurückgegeben | nie |

Der `Other`-Bit (8) wird von `Matches` **nie** ausgewertet. Damit verhält sich
`Everything` (14) **exakt gleich** wie `PhotosAndVideo` (6): **nicht-Medien-Dateien
werden in keinem Filtermodus gesichert.** `Everything` ist als eigenständiger Wert
de facto wirkungslos.

**Die Extension-Taxonomie** stammt aus der eingebetteten Ressource
`supported-content-types.json` (620 Endungen, per Endung → Kategorie-Liste):

| Kategorie | Anzahl | In `Matches` abgedeckt? |
|---|---|---|
| `image` | 83 | ja, via `Photos` |
| `image_sidecar` | 1 (`xmp`) | ja, via `Photos` |
| `video` | 47 | ja, via `Videos` |
| `video_sidecar` | 1 (`xmp`) | ja, via `Videos` |
| `application` | 344 | **nein** |
| `text` | 105 | **nein** |
| `audio` | 36 | **nein** |
| `message` | 4 | **nein** |

`xmp` ist die **einzige** Mehrfach-Endung (`image_sidecar` **und** `video_sidecar`).
Da die Bild-Prüfung zuerst läuft, zählt eine `.xmp`-Datei in `Matches` als **Foto**.
(Vollständige Listen: `docs/SPEC_DEDUP.md` §3.3.)

### 2.4 Wo gefiltet wird — **beides**, Scan **und** Upload

| Ort | Funktion | Bei Nicht-Treffer |
|---|---|---|
| **Enumeration/Scan** | `EnumerationService.IsValid` → `filter.Matches(fsoPath)` | Datei wird nicht eingeplant; unbekannte Endung wird als Metrik erfasst |
| **Upload** | `BackupStorageService.UploadFile` prüft `filter.Matches(record)` **erneut** | `TransferError.FileTypeNotSupported` (= 49), kein Request |

Zusatz: in `IsValid` wird innerhalb einer Fotobibliothek `filter &= ~ContentFilter.Other`
angewendet — ohne Wirkung, da `Other` ohnehin nicht ausgewertet wird. Die
One-Time-Pfade (`EnumeratePaths`, `WalkedFileIsMedia`) verwenden konstant
`ContentFilter.Everything.ForMediaOnly()` bzw. `ContentFilter.PhotosAndVideo`, also
immer den Medien-Modus.

### 2.5 Harte Kopplung zu anderen Einstellungen

| Kopplung | Beleg |
|---|---|
| `Everything` erzwingt `AvoidDuplicates = false` | `ScheduledBackupItem.ContentFilter`-Setter |
| `Everything` erzwingt `DeduplicateOptions.None` in der DB | `ScheduledBackupItemExtensions.ToScheduleRecord` |
| **Speichern-Knopf ist gesperrt**, solange `ContentFilter == Everything` | `WireEvents`: `hasContentFilter = (filter != Everything)` fließt in `saveButton.Enabled` ein |
| Dedupe-Container wird **ausgeblendet** bei `Everything` | `_dedupePhotosVideosContainer.Hidden = (ContentFilter == Everything)` |
| `ForMediaOnly()` mappt `Everything → PhotosAndVideo` | `ContentFilterExtensions.ForMediaOnly` |

Netto ist `Everything` in der aktuellen Oberfläche **nicht erreichbar**: der Dialog
schreibt ihn beim Öffnen sofort auf `PhotosAndVideo` zurück. Die Legacy-Quittung
`clouddrive_desktop_backupconfiguration_blocknonmediaupload_text` („Ab dem 30.11.2023
werden Dateien, die keine Fotos oder Videos sind, nicht mehr hochgeladen. Deine
Einstellung wird automatisch auf „Fotos + Videos" geändert.") bestätigt, dass der
Nicht-Medien-Pfad bewusst abgeschaltet wurde.

### 2.6 Defaults und Speicherort

| Wert | Default | Speicherort |
|---|---|---|
| `DefaultOnboardingContentFilter` | **`ContentFilter.PhotosAndVideo`** | App-Settings-Store (Key `DefaultOnboardingContentFilter`) |
| `DefaultOnboardingDedupeChecked` | `true` | App-Settings (Key gleichen Namens) |
| `DefaultPostOnboardingDedupeChecked` | **`false`** | App-Settings; Quelle für `CustomerSettings.EnableDeduplicationForPhotosAndVideos` |
| Wirksamer Filter je Ziel | aus den beiden Checkboxen | SQLite `schedules.content_filter` |

Der Onboarding-Datensatz wird in `BackupScannerWorker` gebaut: Filter
`DefaultOnboardingContentFilter`, Dedupe `PhotosAndVideos` **nur wenn** der Filter
nicht `Everything` ist, sonst `None`.

---

## 3. F3 — Zielordner für die Sicherung („Ziel für Datensicherung")

### 3.1 Es ist ein **Cloud-Knoten**, kein lokaler Pfad

| Aspekt | Befund |
|---|---|
| UI-Label | `clouddrive_desktop_backupconfiguration_backupdestinationtitle` = **„Ziel für Datensicherung:"** |
| Textfeld / Button | `BackupView_SettingsDestinationPath_Text`, `BackupView_SettingsDestinationFolder_Button` mit Titel `clouddrive_desktop_backupconfiguration_selectbackupdestination_button` = **„Ändern..."** |
| Interne Property | `ScheduledBackupItem.DestinationNodeId` → `ScheduleRecord.DestinationNodeId` |
| Persistenz | SQLite-Spalte `schedules.destination_node_id` (`ManualUploadDatabase`) |
| Anzeige-Format | `CloudPath` = `Amazon Photos` + Segmente, verbunden mit `" > "` (`BackupConfigurationViewModel.RootPathSegment`) |

Der Button öffnet per `ChooseCloudPath()` einen **Cloud-Ordner-Picker**
(`FolderPickerViewController`) mit `CanCreateNewFolders = true`; Sheet-Titel
`clouddrive_desktop_clouddrivepicker_selectlocationmessage` = **„Speicherort für Ihre
Dateien auswählen"**, Primärknopf `…_selectbutton` = **„Auswählen"**.

### 3.2 Vorschlag: der Quellordner **ist** Teil des Zielpfads

`BackupPathMapper.GetSuggestedCloudPathSegmentsForLocalPath` baut:

1. Segment `"Backup"`
2. Segment `<Maschinenname>` (`IEnvironment.MachineName`)
3. **Nur** wenn der lokale Pfad **nicht** unter dem User-Home liegt: der
   `VolumeLabel` des Laufwerks
4. die Eltern-Pfade des Quellordners **unterhalb** des User-Homes, rekursiv aufgebaut
   (`BuildCloudPathSegments`), also **inklusive des Quellordner-Namens** als letztes
   Segment

Beispiel (Konstruktion, nicht ausgeführt): Quelle `~/Bilder/Urlaub2026` ⇒
`Amazon Photos > Backup > <Macname> > Bilder > Urlaub2026`.

Der Vorschlag wird **automatisch** vorbelegt (`SetLocalPath`), sobald kein
`DestinationNodeId` gesetzt ist — der User muss das Ziel also normalerweise nie
anfassen.

### 3.3 Was passiert, wenn der Zielordner nicht existiert?

**Er wird angelegt.** `BackupPathMapper.GetOrCreateBackupDestination` geht die Segmente
einzeln durch: je Segment `GetNodeByNameAndParentIfExists(segment, currentNode, "FOLDER")`;
bei `null` ⇒ `_storageService.CreateFolder(segment, currentNode.NodeId, …, forceNetwork: true)`.
Die Rekursion startet am Wurzelknoten (oder am per `fromNodeId` übergebenen Knoten).

**Normalisierungs-/Umbenennungsregel:** genau **eine** — ein Segment mit dem literalen
Namen `"Amazon Drive"` wird **nicht** als Ordner angelegt, sondern auf den Root-Knoten
abgebildet. Sonst keinerlei Umbenennung oder Zeichen-Ersatz durch den Mapper.

Die Validierung von Ordnernamen (Sonderzeichen, Höchstlänge 280, Namenskonflikt) liegt
im **Picker-Dialog** (`CreateCloudFolderViewController`), nicht im Mapper:
`…_newfolderdialog_errorinvalidcharacters` = „Ordnernamen können keine Sonderzeichen
enthalten.", `…_errornametoolong` = „Ordnername dürfen bis zu 280 Zeichen enthalten.",
`…_errornameconflict` = „Ein Ordner mit diesem Namen ist bereits vorhanden."

**Wann angelegt wird:** `ScheduleRecord.LoadDestinationNode(backupPathMapper)` läuft,
wenn `DestinationNodeId` leer ist (u. a. in `BackupScannerWorker`); sie berechnet den
Vorschlag, legt die Knoten an und speichert die ID zurück.

**Bei Fehlschlag:** `BackupScannerWorker` setzt `ScheduleError.DestinationNodeNotAvailable`,
wenn die ID leer ist **oder** der Knoten gelöscht wurde (`IsNodeDeleted`); UI-Text
`clouddrive_desktop_scheduleerror_destinationmissing_text` = **„Ziel nicht verfügbar"**.
Beim Speichern: `…_unabletosave_destinationmissing_title` = „Bitte wählen Sie einen
anderen Zielort aus", `…_extra` = „Der ausgewählte Zielort existiert nicht mehr."

### 3.4 Wählbar oder Pflicht?

**Beides** — technisch Pflicht, praktisch vorausgefüllt:

| Befund | Beleg |
|---|---|
| **Speichern ist gesperrt**, ohne Ziel kein Speichern | `WireEvents`: `saveButton.Enabled = isLocalPathSet & isCloudPathSet & hasContentFilter` |
| Ziel ist aber **immer** vorausgefüllt | `SetLocalPath` schreibt den Vorschlag (§3.2), sobald kein Knoten gesetzt ist |
| Quellordner nach dem Anlegen **nicht mehr änderbar** | `CanChangeSource` steuert `selectBackupFolderButton.Enabled` |
| **Asymmetrie:** `CanChangeDestination` existiert am VM, wird aber am Ziel-Button **nicht** verkabelt — der Ziel-Button ist in diesem Dialog immer aktiv | nur `CanChangeSource` ist in `WireEvents` mit `Enabled` verdrahtet |

---

## 4. F4 — Der Dialog als Ganzes

**Ort/Form:** `Amazon.CloudDrive.UI.DriveWindow.TabItemView.DriveBackup.ScheduledBackupConfigurationView.BackupConfigurationViewController`
(NSViewController, Sheet am Hauptfenster). Feld-Reihenfolge aus
`CreateAndAddConfigurationView` (die `…FieldBelow(viewAbove)`-Kette erzwingt die
vertikale Ordnung).

| # | Feld | UI-Label (Key ⇒ Text wörtlich) | Widget / Identifier | Zustand |
|---|---|---|---|---|
| 1 | Header | `…_upload_settings_title` ⇒ **„Upload-Einstellungen für „{0}“ bearbeiten"** | `BackupView_SettingsHeader_Text` | — |
| 2 | Gesicherter Ordner | `…_backupfolderpathtitle` ⇒ **„Gesicherter Ordner:"** | `BackupView_SettingsFolderLabel_Text`, `…_BackupPath_Text`, `…_ChangeFolderPath_Button` („Ändern...") | **Ändern gesperrt**, wenn `CanChangeSource == false` (nach dem Anlegen) |
| 3 | Ziel der Sicherung | `…_backupdestinationtitle` ⇒ **„Ziel für Datensicherung:"** | `BackupView_SettingsDestinationFolder_Text`, `…_DestinationPath_Text`, `…_DestinationFolder_Button` („Ändern...") | immer aktiv; öffnet Cloud-Picker |
| 4 | Inhaltsfilter | **kein** Gruppen-Label (der Titel „Was soll gesichert werden:" ist ungenutzt) | `BackupView_ContentFilter_Container`, `BackupView_BackupPhotos_Checkbox` („Fotos sichern"), `BackupView_BackupVideos_Checkbox` („Videos sichern") | `Everything` wird beim Öffnen auf `PhotosAndVideo` zurückgeschrieben |
| 5 | Trennlinie | — | `_contentFilterDivider` | — |
| 6 | Auto-Upload *(experiment)* | `…_auto_upload_toggle_text` ⇒ **„Neue Dateien automatisch hochladen"**, Untertitel `…_toggle_subtitle_text` ⇒ „Zukünftige Dateien oder Dateiänderungen werden automatisch als neue Dateien in die Cloud hochgeladen" | `_autoUploadContainer` | **standardmäßig ausgeblendet** — nur sichtbar bei Remote-Experiment-Behandlung `T1` (`EnableCXUploadSimplification`, Default `RemoteExperiment` = Control 100 / T1 0 / T2 0 ⇒ „C"). Umschaltet `Interval.Option` zwischen `Immediate` und `Never` |
| 7 | Duplikate | `…_skip_duplicates_option_title` ⇒ **„Duplikate überspringen"**, Untertitel `…_subtitle` ⇒ „Dateien, die sich bereits in meinem Konto befinden, nicht sichern" | `BackupView_SettingsAvoidDuplicates_Container`, `…_Checkbox` | **ausgeblendet**, wenn `ContentFilter == Everything` |
| 8 | Konflikte | `…_file_name_conflicts_title` ⇒ **„Konflikte mit Dateinamen"** | `BackupView_FileNameConflictResolution_Text` + vier Radios | Reihenfolge der Anlage: `…_SaveAsNew_RadioButton` („Neue Datei automatisch umbenennen"), `…_Ignore_RadioButton` („Neue Datei ignorieren"), `…_Replace_RadioButton` („Alte Datei ersetzen"), `…_AskMeEveryTime_RadioButton` („Jedes Mal nachfragen") |
| 9 | Fußzeile | `clouddrive_desktop_cancel` = **„Abbrechen"** (Esc), `…_savebuttontitle` = **„Speichern"** (Return) | `BackupView_SettingsCancel_Button`, `BackupView_SettingsSave_Button` | **Speichern gesperrt**, bis Quellpfad **und** Ziel gesetzt **und** Filter ≠ `Everything` |

**Nicht vorhanden in diesem Dialog:** ein Intervall-/Zeitplan-Wähler. Die einzige
Zeitsteuerung ist die hinter `EnableCXUploadSimplification` versteckte Auto-Upload-
Checkbox (Zeile 6), die binär zwischen `Immediate` und `Never` umschaltet. Die
intervallbezogenen Strings (`…_selectintervaltitle` = „Änderungen hochladen:",
`…_selectinterval_dayofweek_title` = „aktiviert", `…_selectinterval_time_title` = „um")
sowie der Fenstertitel `…_backupconfiguration_windowtitle` = „Datensicherungseinstellungen"
sind nur in der generierten `Strings`-Klasse vorhanden und werden von diesem Controller
nicht referenziert.

**Validierung beim Ablegen** (`ConfigureBackupActionIntentHandler`): `SaveScheduleRecord`
kann scheitern ⇒ Warnung `…_unabletosavetitle` = „Bitte wählen Sie einen anderen
lokalen Pfad aus" / `…_unabletosaveextra` = „Datensicherung für lokalen Pfad existiert
bereits". Weitere, in der UI belegte Prüf-Strings (Quellordner-Warnungen):
`schedules_rootfolder_warningmessage` („Drive-Stammordner ist nicht für Datensicherung
zulässig."), `schedules_overlappingfolder_warningmessage`, `schedules_syncfolder_warningmessage`,
`schedules_restrictedfolder_warningmessage` („Dieser Pfad ist eingeschränkt. …"),
`schedules_restrictedfolder_warningheader` („Bitte wählen Sie einen anderen Ordner aus.").

---

## 5. UNKNOWN

* **Serverseitige Wirkung von `conflictResolution=RENAME` bei Inhaltsidentität** —
  der Client sendet `RENAME` bei `SaveAsNew` auch ohne Konflikt (§1.2), aber ob der
  Server daraus eine umbenannte **Kopie** eines Inhaltsduplikats macht, ist aus dem
  Decompil nicht ableitbar. Kein Client-Pfad hängt daran (§1.4).
* **Serverseitige Wirkung von `suppress=DEDUPLICATION`** — der Client sendet den
  Parameter für **nicht** dedup-berechtigte Uploads; wie der Server ihn behandelt,
  ist unbelegt.
* **Ob `GetNodeByMd5` serverseitig zusätzlich gegated ist** (Weblab o. Ä.) — nur die
  clientseitige Gating (`UploadType`) wurde beobachtet.
* **Exakte Segmentliste des Zielvorschlags für Laufwerke außerhalb des User-Homes**
  (§3.2, Punkt 3) — die Bedingung und die Rekursion von `BuildCloudPathSegments` sind
  belegt, das konkrete Ergebnis für einen externen Pfad wurde nicht durchgerechnet.
* **Ob `CanChangeDestination` in einem anderen Dialog wirksam ist** — in *diesem*
  Controller ist es nicht verkabelt (§3.4); andere Öffnungspfade wurden nicht geprüft.
* **Behandlung von `Audio`-Endungen** — die Kategorie existiert in der JSON (36 Endungen)
  und wird von `Matches` nie erreicht; ob ein separater Audio-Import-Pfad sie nutzt,
  ist offen.
* **Suchweg (reproduzierbar):** vollständiger `ilspycmd -p`-Decompil von
  `Amazon Photos.dll` (1935 `.cs`-Dateien) in einen Scratch-Baum; `grep` nach
  `CloudDriveConflictResolution`, `ConflictResolution`, `ContentFilter`,
  `ForMediaOnly`, `Matches(`, `GetOrCreateBackupDestination`,
  `GetSuggestedCloudPathSegmentsForLocalPath`, `LoadDestinationNode`, `PreUploadCheck`,
  `UploadViaSimpleOrLegacyUploader`, `RetryForNameConflict`, `NameConflictStrategy`,
  `EnableCXUploadSimplification`, `BackupView_`; Extraktion der eingebetteten
  Ressource per `ilspy.sh --resource`; deutsche Labels per `grep` über
  `de.lproj/Localizable.strings`.

---

## 6. Konsequenz für Prime Photos

Lücken gegen unseren heutigen Stand (Quelle: `AGENTS.md`, `docs/SPEC_CONFLICT_OPTIONS.md`
§3/§4, `docs/SPEC_FOLDERS.md` §2):

| Thema | Original | Unser Stand | Lücke |
|---|---|---|---|
| **Inhaltsfilter** | Zwei Checkboxen „Fotos sichern"/„Videos sichern", wirksam bei Scan **und** Upload | **keiner** — 28 hartkodierte Bild-Endungen in `is_supported_image` (`src/folders.rs`), Videos gar nicht | **F2 komplett offen.** Mindestens: ein `dedup`-analoger Filterwert pro Ziel, zwei Checkboxen, Verdrahtung in `scan_images` **und** vor dem Request |
| **Nicht-Medien** | wird **nie** gesichert (`Matches` gibt `false` zurück) | wird bei uns je nach Endung teils mitgesichert | bewusst klären: unser Standard-Set ist ohnehin bilder-only, aber die Regel sollte explizit sein, falls wir Videos ergänzen |
| **Zielordner** | pro Ziel ein **Cloud-Knoten** (`destination_node_id`), vorausgefüllt als `Backup/<Macname>/<Pfad>`, bei Bedarf **angelegt** | **keiner** — Upload geht in das Cookie-Konto, ohne Node-Auswahl | **F3 komplett offen.** Das ist die größte Architektur-Lücke: wir haben weder `BackupPathMapper` noch `destination_node_id` noch den Cloud-Picker |
| **Quellordner im Zielpfad** | ja, als letztes Segment | n. a. | folgt aus dem vorherigen |
| **Ziel-Pflicht** | Save gesperrt ohne Ziel | n. a. | — |
| **„Was soll gesichert werden:"-Label** | in der aktuellen Oberfläche **ungenutzt** | — | wir sollten das Label **nicht** übernehmen, wenn wir zwei Checkboxen zeigen |
| **`Everything`-Kopplung** | `Everything` sperrt Speichern + blendet Dedupe aus + erzwingt Dedupe aus | es gibt bei uns kein `Everything` | nur relevant, falls wir „Alles" je anbieten — dann dieselbe Kopplung |
| **Intervall im Dialog** | **nicht** vorhanden (nur die experiment-gegatelete Auto-Upload-Checkbox) | bei uns deaktiviert mit „Noch nicht verfügbar" | unser deaktiviertes Feld ist **kein** Rückstand gegenüber *diesem* Dialog; das Intervall lebt woanders |
| **`SaveAsNew` ohne Konflikt** | sendet `RENAME` **jedes Mal** | unser `SaveAsNew` sendet `RENAME` (`src/folders.rs` `wire_value`) | **prüfen:** senden wir `RENAME` nur bei Konflikt oder generisch wie das Original? Abweichung wäre zu dokumentieren, nicht zu „fixen" |
| **`RENAME_EXISTING`** | nur Apple-Photos-Import (`PhotosImport`) | nicht vorhanden | **bewusst offen** — der Import-Pfad ist außerhalb des MVP |
| **Konflikt-Wire-Map** | `None`/`Ignore` → `NONE`, `SaveAsNew` → `RENAME`, `Replace` → `PUT` | identisch (§1.1) | **keine Lücke** — unsere Abbildung deckt sich |
| **Inhalt schlägt Name** | `PreUploadCheck` vor jedem Request, Skip ohne Netz | identisch (lokaler MD5-Index + 409-Sicherheitsnetz) | **keine Lücke** |
| **Dedupe-Toggle-Default** | Onboarding an / danach global `false` | `true` (bewusste Abweichung, `AGENTS.md`) | dokumentiert, kein Handlungsbedarf |

**Reihenfolge der sinnvollen Arbeit:** F3 (Zielordner) ist die Voraussetzung für
parity-nahes Backup und berührt `folders.rs`, `config` und einen Cloud-Picker;
F2 (Filter) folgt und braucht dieselbe Ziel-Struktur, weil der Filter je Ziel
gespeichert wird. F1 erfordert **keine** Änderung — unsere Konflikt-Map ist korrekt;
lediglich das Verhalten von `SaveAsNew` ohne Konflikt ist gegeneinander zu halten.

---

## 7. Belege (Symbole, Keys, Dateien — keine Codekopien)

* **Wire/Enums:** `Amazon.CloudDrive.Services.CloudDriveConflictResolution`
  (`None`/`Rename`/`RenameExisting`), `CloudDriveConflictResolutionExtensions.Values`
  (`NONE`/`RENAME`/`RENAME_EXISTING`), `UploadRequest` (`[QueryParameter]` für
  `name`/`fileSize`/`contentDate`/`conflictResolution`/`suppress`/`kind`/`subkind`/
  `assetType`/`addToFamilyArchive`/`isfavorite`/`ishidden`/`parentNodeId`),
  `CloudDriveService.UploadFile` (PUT ⇒ `v2/upload/<NodeId>`, POST ⇒ `v2/upload`).
* **Upload-Pfad:** `CloudDriveStorageService.PreUploadCheck`, `…CloudNodeCheck`,
  `…UploadViaSimpleOrLegacyUploader`, `…HandleNodeResponse` (409 + MD5 ⇒ Skip),
  `SimpleUploader.UploadFile`, `LegacyUploader`, `MultipartUploader`;
  `UploadHelper.RetryForNameConflict` (verlangt abweichenden MD5 **und**
  `NAME_ALREADY_EXISTS`/`NameAlreadyExists` **und** `IsAutoSave()`).
* **Konflikt-Politik:** `Amazon.CloudDrive.Database.ManualUpload.ConflictResolution`
  (`None`/`Ignore`/`SaveAsNew`/`Replace`), `NameConflictStrategy.GetConflictResolution`
  (Wächter auf `destinationNodeId` nur für `Replace`/`Ignore`),
  `…GetScheduleConflictResolution`, `UploadRecordExtensions.GetConflictResolution`
  (Autosave ⇒ `None`; `RenameExisting` nur bei `PhotosImport`) / `…IsAutoSave` /
  `…GetSuppressFileUpload`, `FileUploadNameConflictException`, `FileSkippedException`,
  `FileIgnoredPreUploadException`, `FileIgnoredException`.
* **Inhaltsfilter:** `…ManualUpload.ContentFilter`, `ContentFilterExtensions.Matches`
  / `…ForMediaOnly`, `SupportedMimeMapping` (`IsImageType`/`IsImageSidecar`/
  `IsVideoType`/`IsVideoSidecar`/`IsOtherType`), eingebettete Ressource
  `Amazon.CloudDrive.Helpers.SupportedContentTypes.supported-content-types.json`
  (`supported_extensions`, `categories`, `supported_content_types_prefix`),
  `EnumerationService.IsValid` / `…WalkedFileIsMedia` / `…EnumeratePaths`,
  `BackupStorageService.UploadFile`, `TransferError.FileTypeNotSupported` (= 49).
* **Zielordner:** `BackupPathMapper.GetSuggestedCloudPathSegmentsForLocalPath` /
  `…GetOrCreateBackupDestination` / `…GetCloudPathSegmentsFromNodeId` /
  `…BuildCloudPathSegments` (Segment `"Amazon Drive"` ⇒ Root),
  `IBackupPathMapper`, `ScheduleRecord.LoadDestinationNode` / `…DestinationNodeId`,
  `ManualUploadDatabase` (`schedules.destination_node_id`), `BackupScannerWorker`
  (`ScheduleError.DestinationNodeNotAvailable`), `BackupConfigurationViewModel.SetLocalPath` /
  `…SetDestination` / `…CloudPath` / `…RootPathSegment` / `…CanChangeSource` /
  `…CanChangeDestination`, `FolderPickerViewController` (`CanCreateNewFolders`),
  `CreateCloudFolderViewController`.
* **Dialog:** `BackupConfigurationViewController` (`CreateAndAddConfigurationView`,
  `CreateBackupFolderFieldBelow`, `CreateBackupDestinationFieldBelow`,
  `CreateContentFilterFieldBelow`, `UpdateContentFilter`, `CreateConflictResolutionFieldBelow`,
  `CreateAvoidDuplicatesFieldBelow`, `WireEvents`, `ChooseCloudPath`,
  `FileNameConflictResolutionRadioButtonClicked`, `ChooseLocalPath`), Identifier
  `BackupView_*`, `ScheduledBackupItem` (`ContentFilter`-Setter, `IsPhotosSelected`,
  `IsVideosSelected`, `UpdateContentFilter`/`UpdateSelection`, `AvoidDuplicates`),
  `ScheduledBackupItemExtensions.ToScheduleRecord`,
  `ConfigureBackupActionIntentHandler`, `DriveWindowViewModel.EnableCXUploadSimplification`,
  `RemoteExperiment` (Default Control 100) / `RemoteExperimentHelper.GetTreatment`.
* **Defaults:** `AppSettings.DefaultOnboardingContentFilter` (= `PhotosAndVideo`),
  `…DefaultOnboardingDedupeChecked` (`true`), `…DefaultPostOnboardingDedupeChecked`
  (`false`), `CustomerSettings.EnableDeduplicationForPhotosAndVideos`.
* **Deutsche Labels (`de.lproj/Localizable.strings`):**
  `clouddrive_desktop_upload_settings_{title,backup_photos_checkbox,backup_videos_checkbox}`,
  `clouddrive_desktop_backupconfiguration_{backupfolderpathtitle,backupdestinationtitle,selectcontentfiltertitle,selectbackupfolder_button,selectbackupdestination_button,windowtitle,blocknonmediaupload_text}`,
  `clouddrive_desktop_upload_settings_skip_duplicates_option_{title,subtitle}`,
  `clouddrive_desktop_upload_settings_file_name_conflicts_{title,ask_option,ignore_option,auto_rename_option,replace_option}`,
  `clouddrive_desktop_{cancel,auto_upload_toggle_text,auto_upload_toggle_subtitle_text}`,
  `clouddrive_desktop_statuswindow_scheduleconfiguration_{savebuttontitle,select_local_path_dialog_title,unabletosavetitle,unabletosaveextra,unabletosave_destinationmissing_title,unabletosave_destinationmissing_extra}`,
  `clouddrive_desktop_clouddrivepicker_{selectlocationmessage,selectbutton,newfolderdialog_*}`,
  `clouddrive_desktop_schedules_{rootfolder,overlappingfolder,syncfolder,restrictedfolder}_warningmessage`,
  `clouddrive_desktop_scheduleerror_destinationmissing_{header,text}`,
  `clouddrive_desktop_upload_statuswindow_statusignored` („Zuvor hochgeladen").
* **Cross-Referenz:** `docs/SPEC_CONFLICT_OPTIONS.md`, `docs/SPEC_DUPLICATES.md`,
  `docs/SPEC_DEDUP.md` (§3 Taxonomie), `docs/SPEC_FOLDERS.md` (§3 Modell),
  `docs/PROTOCOL.md`.

---

## 8. Korrekturen an den vorhandenen Docs

1. **`SPEC_CONFLICT_OPTIONS.md` §1.2, Zeile B3 (`SaveAsNew`) — „Auslösebedingung"
   war zu eng.** Dort stand sinngemäß, der Client setze `overwrite=false` **bei
   vorhandenem Zielknoten**. Belegt ist: `NameConflictStrategy.GetConflictResolution`
   prüft `destinationNodeId != null` **nur** für `Replace` und `Ignore`, **nicht** für
   `SaveAsNew`; und `UploadViaSimpleOrLegacyUploader` setzt für `SaveAsNew`
   `overwrite = false` **unconditionally**. Korrekt ist also: bei
   „Neue Datei automatisch umbenennen" geht **jeder** Upload mit `conflictResolution=RENAME`
   auf die Leitung, **auch ohne** Namenskonflikt und ohne Zielknoten. Das ändert das
   Ergebnis nicht (identischer Inhalt wird vorher als Skip abgefangen, §1.4), aber die
   bisherige Formulierung legte fälschlich eine Bedingung nahe, die es nicht gibt.
   *Begründung nachgereicht, weil sie für F1 entscheidungsrelevant ist: „Hash gleich ⇒
   umbenennen" kann der Wert `RENAME` allein nicht auslösen.*
2. **`SPEC_CONFLICT_OPTIONS.md` §7 — `RetryForNameConflict` wird um seinen dritten
   Wächter ergänzt.** Der Retry verlangt neben `409` **und** abweichendem MD5 **und**
   `ErrorType ∈ {NAME_ALREADY_EXISTS, NameAlreadyExists}` auch
   `uploadRecord.IsAutoSave()` (d. h. `schedule_id != "ManualUpload"`). Für One-Time-
   Uploads (Drag & Drop) löst ein 409-Namenskonflikt diesen servergetriebenen
   Retry also **nicht** aus. Die bisherige Zeile nannte nur die beiden ersten Bedingungen.
3. **`SPEC_FOLDERS.md` §3.4 / `SPEC_DUPLICATES.md` §1.2 — der Inhalt war korrekt,
   bleibt aber ohne Korrektur.** Die Aussage „`Everything` erzwingt
   `AvoidDuplicates=false`" und die Taxonomie stimmen; ergänzt wurde in dieser Datei
   (§2.3), dass `Matches` den `Other`-Bit **nie** auswertet und damit `Everything`
   ≡ `PhotosAndVideo` ist — das war dort zwar implizit beschrieben, wird hier aber
   explizit und mit der Konsequenz „nicht-Medien werden in keinem Modus gesichert"
   festgehalten.
4. **`SPEC_FOLDERS.md` §3.4 — „Default onboarding for a new backup" präzisiert.**
   Belegt ist `AppSettings.DefaultOnboardingContentFilter` = **`ContentFilter.PhotosAndVideo`**
   (nicht nur „PhotosAndVideo" aus dem Text der Spec, sondern der konkrete
   App-Settings-Key mit diesem Default) sowie die Aufteilung
   `DefaultOnboardingDedupeChecked` = `true` / `DefaultPostOnboardingDedupeChecked` =
   `false` in `AppSettings`. Kein Widerspruch, nur schärfere Belege.
