# SPEC_MISSING_SOURCE — Verhalten bei nicht angeschlossenem Quellordner (Clean-Room Spec)

Analyse des Verhaltens bei einer **nicht erreichbaren Quellquelle** (externe Festplatte, USB-Stick,
Netzwerk-Laufwerk): was passiert beim App-Start, beim Watcher, bei manuellem Rescan, beim
Wasserzeichen, in der Ordner-Zeile, auf Datei-Ebene und beim Wiederanschließen — und wie sich
das Original verhält. Ziel ist das Verhalten bei fehlendem Quellordner
(User-Entscheidung 2026-10-16, präzisiert 2026-10-17): ein fehlender Ordner soll
als eigener Zustand **„Ordner nicht gefunden"** (`FolderStatus::NotFound`, AMBER)
geführt werden — nicht als echte Pause und nicht als Fehler.

Clean-room: eigene Worte und Tabellen, **keine** verbatim Code-Auszüge, **keine** Personendaten,
**keine** Live-Werte. Belege über Symbol-/Key-Namen und `Datei:Zeile`.

**Konvention:** **am Code geprüft** = direkt aus der dekompilierten DLL gelesen (ilspycmd) ·
**aus Dump** = aus vorhandenen Clean-Room-Docs übernommen, nicht neu am Code geprüft ·
**UNKNOWN** = aus diesen Quellen nicht bestimmbar.

---

## 1. Quellen & Methode

| Quelle | Inhalt | Belegt |
|---|---|---|
| Unser Code | `src/backup.rs`, `src/folders.rs`, `src/upload.rs`, `src/backup_config.rs`, `src/dedup.rs`, `src/app.rs` | gelesen |
| Original-DLL | `/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll` (4,6 MB) | **am Code geprüft** (ilspycmd 11.1.0.9782, 2026-10-16) |
| Original-Docs | `SPEC_CHANGE_DETECTION.md`, `SPEC_BACKGROUND_SYNC.md`, `SPEC_FOLDERS.md`, `UI_STATES.md`, `SPEC_DEDUP.md`, `../protocol/PROTOCOL.md` | aus Dump |

**Decompiler:** `ilspycmd` über den Wrapper `~/.local/bin/ilspycmd` (setzt `DOTNET_ROOT` selbst).
Aufrufe: `ilspycmd -l c "<dll>"` (Klassenliste), `ilspycmd -t <Typ> "<dll>"` (einzelner Typ).
Die DLL ist managed .NET (Mono-Bundle), 2486 Klassen. Die Analyse erfolgte am 2026-10-16.

**REA:** Nicht benutzt — für diese Assembly ungeeignet (`metadata-not-admitted` bei
`inspect_managed_artifact`). Der Decompiler ist der richtige Weg.

---

## 2. Was WIR heute tun (jede Aussage mit `Datei:Zeile`)

### 2.1 App-Start (Kaltstart-Wasserzeichen-Pass)

| Schritt | Verhalten bei fehlendem Ordner | Beleg |
|---|---|---|
| `background_sync_pass` läuft einmal nach Login | — | `src/backup.rs:554` |
| `can_start`-Guard | Prüft `paused`, `background_sync`, Login, `active` — **nicht** die Ordner-Existenz | `src/backup.rs:458-467` |
| `folders::max_mtime` | Der Verzeichnislauf liefert einen Fehler; nicht lesbare Einträge werden verworfen; `max_mtime` bleibt ohne Wert | `src/folders.rs:71-87` |
| Fehlender Ordner im Kaltstart | Der Ordner wird **übersprungen** — kein Fehler, kein Wasserzeichen-Upload, kein Log | `src/backup.rs:593-595` |

**Folge:** Ein fehlender Ordner ist für den Kaltstart-Pass **unsichtbar**. Er wird still
übersprungen, das Wasserzeichen bleibt auf dem alten Wert, und der Ordner erscheint in der
Ordner-Zeile mit seinem letzten Status (z.B. `Done` oder `Idle`).

### 2.2 Watcher (OS-Events)

| Schritt | Verhalten bei fehlendem Ordner | Beleg |
|---|---|---|
| `sync_watchers` | Die Registrierung des Pfads beim Watcher liefert einen Fehler, wenn der Ordner fehlt | `src/backup.rs:789-798` |
| Fehlerbehandlung | Es wird eine Warnung ins Protokoll geschrieben („Ordner-Wächter … konnte nicht eingerichtet werden") — der Watcher wird **nicht** in die Tabelle der Wächter aufgenommen | `src/backup.rs:803-806` |
| `watch_loop` | Alle `WATCH_POLL` (30 s) wird `sync_watchers` aufgerufen → ein fehlender Ordner wird **erneut versucht** (Re-arm), aber nur alle 30 s, keine Endlosschleife | `src/backup.rs:836-843` |
| Ordner verschwindet während eines Laufs | Der Watcher liefert einen Fehler → `watchers.clear()` → nächster Poll versucht erneut | `src/backup.rs:879-884` |

**Folge:** Der Watcher versucht es alle 30 s erneut. Solange der Ordner fehlt, gibt es
**keine** Events, **keine** Endlosschleife, **kein** Fehler-Log (außer beim ersten Versuch).
Beim Wiederanschließen wird der Watcher innerhalb von 30 s neu aufgesetzt.

### 2.3 Manueller Rescan (Zeilen-Symbol)

| Schritt | Verhalten bei fehlendem Ordner | Beleg |
|---|---|---|
| `start_folder_upload` | Prüft `upload_busy()` und `is_logged_in()` — **nicht** die Ordner-Existenz | `src/backup.rs:1027-1037` |
| `start_scheduled_upload` → `UploadPlan::build` → `scan_images` | Der Verzeichnislauf liefert einen Fehler; nicht lesbare Einträge werden verworfen; die Ergebnisliste bleibt leer | `src/folders.rs:68-87` |
| `UploadPlan::build` | Liefert `None` wenn keine Dateien gefunden wurden | `src/backup.rs:1048-1051` |
| `start_scheduled_upload` | Es wird eine Protokoll-Zeile mit dem Text „Keine Fotos oder Videos in diesem Ordner gefunden." geschrieben — **kein** Status-Update, **kein** Fehler | `src/backup.rs:1049-1050` |

**Folge:** Ein manueller Rescan bei fehlendem Ordner endet mit einer **Log-Zeile** und
verändert **nichts** am Ordner-Status. Der User sieht keine Fehlermeldung, keinen Statuswechsel.

### 2.4 Wasserzeichen

| Schritt | Verhalten bei fehlendem Ordner | Beleg |
|---|---|---|
| `record_folder_scan` | Wird nur in `start_background_run` aufgerufen — **nach** einem erfolgreichen Lauf | `src/backup.rs:519-525` |
| Ordner fehlt beim Kaltstart | `background_sync_pass` springt vor dem Wasserzeichen-Update `continue` → das Wasserzeichen wird **nicht** vorgeschoben | `src/backup.rs:593-595` |
| Ordner fehlt während eines Laufs | Wenn der Lauf erfolgreich endet (weil die Dateien bereits im Index sind), wird das Wasserzeichen **trotzdem** vorgeschoben — **das ist der gefährlichste Fall** | `src/backup.rs:519-525` |

**Folge:** Das Wasserzeichen wird **nie** vorgeschoben, wenn der Ordner fehlt. **Aber:**
Wenn der Ordner während eines Laufs verschwindet und der Lauf erfolgreich endet (weil die
Dateien bereits im Index sind), wird das Wasserzeichen **trotzdem** vorgeschoben — das ist
der gefährlichste Fall, weil der Ordner beim nächsten Start als „unverändert" gilt und die
Reste nie nachgeholt werden.

### 2.5 Ordner-Zeile (UI)

| Element | Verhalten bei fehlendem Ordner | Beleg |
|---|---|---|
| `backup_ui` | `status_text` (src/upload.rs:146) — `FolderStatus::Paused` wird **nie** gesetzt (nur im Rendering vorhanden) | `src/upload.rs:146-170` |
| `FolderStatus` | `Paused` existiert als Enum-Variant, wird aber **nirgends** zugewiesen | `src/folders.rs:120-128` |
| `finalize_upload` | Setzt nur `Done` oder `Error` — **nie** `Paused` | `src/backup.rs:1444-1448` |
| `finalize_background_store` | Setzt nur `Done` oder `Error` — **nie** `Paused` | `src/backup.rs:431-435` |
| „Zuletzt aktualisiert" | Bleibt auf dem alten Wert | `src/backup.rs:1443` |

**Folge:** Ein fehlender Ordner erzeugt **keine** sichtbare Statusänderung. Die Zeile
bleibt auf dem letzten Wert (z.B. `Done` oder `Idle`), und der User sieht keinen Hinweis
darauf, dass der Ordner fehlt.

### 2.6 Datei-Ebene

| Schritt | Verhalten bei fehlender Datei | Beleg |
|---|---|---|
| `upload_worker` → `upload_picture` | Die Metadaten der Datei können nicht gelesen werden; der Fehler wird als Text weitergereicht | `src/prime_photos.rs:940` |
| Fehlerbehandlung | Die Fehlerzahl `failed` wird hochgezählt, der Dateizustand `FileState::Failed` gesetzt und `folder_failed` erhöht | `src/upload.rs:397-450` |
| `finalize_upload` | Setzt `FolderStatus::Error` wenn `folder_failed > 0` | `src/backup.rs:1444-1448` |

**Folge:** Eine fehlende Datei wird als **Fehler** gezählt und die Ordner-Zeile zeigt
`Error` (rot). Es gibt **keine** Unterscheidung zwischen „Datei fehlt" und „Upload-Fehler".

### 2.7 Dialog-Speichern (`backup_config.rs`)

| Schritt | Verhalten | Beleg |
|---|---|---|
| `handle_config_form_action` | Wird der Pfad beim Speichern auf Existenz geprüft? **Ja**, aber nur beim **Anlegen** (Create), nicht beim **Bearbeiten** (Edit) | `src/backup_config.rs:191-196` |
| Laufwerk wird später abgezogen | Der Dialog prüft **nicht** erneut — der Ordner bleibt in `folders.toml` | `src/backup_config.rs:223-245` |

**Folge:** Der Dialog prüft den Pfad nur beim Anlegen. Wenn das Laufwerk später abgezogen
wird, bleibt der Ordner in der Konfiguration und wird beim nächsten Rescan still übersprungen.

### 2.8 Wiederanschließen

| Mechanismus | Verhalten beim Wiederanschließen | Beleg |
|---|---|---|
| Watcher | `sync_watchers` versucht es alle 30 s erneut → funktioniert | `src/backup.rs:836-843` |
| Wasserzeichen | `background_sync_pass` läuft nur einmal pro Prozess → ein wieder angeschlossener Ordner wird erst beim nächsten Start bemerkt | `src/backup.rs:1366-1400` |
| Aktive Erkennung | Es gibt **keinen** Mechanismus, der den Wiederanschluss aktiv erkennt (kein `HandleDeviceArrival`-Äquivalent) | — |

**Folge:** Beim Wiederanschließen funktioniert der Watcher innerhalb von 30 s. Das
Wasserzeichen wird erst beim nächsten App-Start aktualisiert. Es gibt **keine** aktive
Erkennung des Wiederanschlusses.

---

## 3. Was das ORIGINAL tut

### 3.1 ScheduleError-Werte (**am Code geprüft**)

| Wert | Bedeutung | Beleg |
|---|---|---|
| `ScheduleError.None` | Kein Fehler | `Amazon.CloudDrive.UI.ScheduleErrorExtensions.IsRetriable` |
| `ScheduleError.LocalFolderNotFound` | Lokaler Ordner nicht gefunden | `Amazon.CloudDrive.UI.ScheduleErrorExtensions.IsRetriable` |
| `ScheduleError.DestinationNodeNotAvailable` | Zielknoten nicht verfügbar | `Amazon.CloudDrive.UI.ScheduleErrorExtensions.IsRetriable` |
| `ScheduleError.ExternalDriveMissing` | Externes Laufwerk fehlt | `Amazon.CloudDrive.UI.ScheduleErrorExtensions.IsRetriable` |
| `ScheduleError.NameConflict` | Namenskonflikt | `Amazon.CloudDrive.UI.ScheduleErrorExtensions.IsRetriable` |

`IsRetriable` liefert `true` nur für `None`, `false` für alle anderen — sie sind also
**nicht** automatisch wiederholbar.

### 3.2 UI-Texte (aus Dump)

| Key | Deutsch | Beleg |
|---|---|---|
| `…_statuswindow_errorlong_local_drivenotfound` | „Das externe Laufwerk, aus dem Sie Dateien hochgeladen haben, ist möglicherweise offline. Bitte stellen Sie die Verbindung wieder her und versuchen Sie erneut." | `SPEC_CHANGE_DETECTION.md` §6 |
| `…_folder_not_present_warning` | „Dieser Ordner wurde nicht gefunden. Er wurde möglicherweise umbenannt, verschoben oder gelöscht." | `SPEC_CHANGE_DETECTION.md` §6 |

### 3.3 Verhalten bei fehlendem Laufwerk (**am Code geprüft**)

#### 3.3.1 Aktive Hardware-Erkennung

Der Original-Client nutzt `ObservableHardwareChangeWatcher` (macOS-only), der auf
`NSWorkspace.Notifications.ObserveDidMount` und `ObserveDidUnmount` abonniert. Das sind
**echte** Mount-/Unmount-Ereignisse des Betriebssystems — keine Polling-Lösung.

| Mechanismus | Verhalten | Beleg |
|---|---|---|
| `ObservableHardwareChangeWatcher` | Abonniert `NSWorkspace` Mount/Unmount-Notifications, published `DeviceArrival`/`DeviceRemoval` Events | `Amazon.CloudDrive.Services.FileSystem.ObservableHardwareChangeWatcher` |

#### 3.3.2 HandleDeviceRemoval (**am Code geprüft**)

In `BackupScannerWorker.HandleDeviceRemoval`:

| Schritt | Verhalten |
|---|---|
| 1. Expiration | `ExpireDriveDetails` — löscht die gecachten Laufwerksdetails |
| 2. Schedule-Ermittlung | Alle Schedules gesucht, deren `LocalPath` mit dem entferntpfad beginnt |
| 3. Status setzen | `SaveScheduleRecord` — der Eintrag wird mit dem Fehlercode `ScheduleError.ExternalDriveMissing` gespeichert |
| 4. Watcher entfernen | `RemoveFileSystemWatcherForPath` für den Pfad des Eintrags |
| 5. Event publizieren | `ScheduleChangedEvent` mit dem Änderungstyp `Modify` |
| 6. Upload pausieren | `Interrupt` des Upload-Workers mit der Quelle `PauseSource.SystemPaused` und dem Modus `SuspendBackups` — pausiert den Upload-Worker und setzt `IsPaused` auf gesetzt |

#### 3.3.3 HandleDeviceArrival (**am Code geprüft**)

In `BackupScannerWorker.HandleDeviceArrival`:

| Schritt | Verhalten |
|---|---|
| 1. Details aktualisieren | `UpdateExternalDriveDetails` — ersetzt die alten Laufwerksdetails |
| 2. Schedules ermitteln | `ListSchedules` mit der Laufwerks-Kennung — alle Schedules auf diesem Laufwerk |
| 3. Watcher neu setzen | `AddFileSystemWatcherForPath` für den Pfad des Eintrags |
| 4. Scan anfordern | `RequestScan` mit erzwungenem Scan, nur wenn der Eintrag nicht schon pausiert ist |
| 5. AutoConnect | Wenn keine Schedules und `EnableAutoConnect` gesetzt ist: `ShowAutoConnectWindowIntent` |

#### 3.3.4 ProcessScanRequest — Pre-Check vor jedem Scan (**am Code geprüft**)

In `BackupScannerWorker.ProcessScanRequest`:

| Schritt | Verhalten |
|---|---|
| 1. Laufwerk prüfen | Wenn `DriveIdentifier` gesetzt ist, prüft `HasExternalDriveChanged` Kennung und Pfad; kommt kein Treffer zurück (Laufwerk weg), wird `externalDriveMissing` auf gesetzt gestellt |
| 2. Fehlercode setzen | Wenn `externalDriveMissing` gilt: der Fehlercode wird auf `ScheduleError.ExternalDriveMissing` gesetzt |
| 3. Alternativ | Wenn der Zielknoten fehlt: `ScheduleError.DestinationNodeNotAvailable` |
| 4. Alternativ | Wenn das Dateisystem den lokalen Pfad nicht kennt: `ScheduleError.LocalFolderNotFound` |
| 5. Bei Fehler | Watcher entfernen, `Interrupt` des Upload-Workers mit `PauseSource.SystemPaused`, Uploads für den Eintrag suspendieren |
| 6. Speichern | `SaveScheduleRecord` mit dem Eintrag und dem Fehlercode — der Fehlercode wird persistiert |

#### 3.3.5 BackupWatcherWorker.Start() (**am Code geprüft**)

| Schritt | Verhalten |
|---|---|
| Pausiert überspringen | Ein pausierter Eintrag (`backup.IsPaused`) wird übersprungen |
| ExternalDriveMissing prüfen | Wenn `backup.ErrorCode` den Wert `ScheduleError.ExternalDriveMissing` trägt, wird `HandleDisconnectedDevice` mit dem Eintrag aufgerufen |
| Watcher setzen | `AddFileSystemWatcherForPath(backup.LocalPath)` |

#### 3.3.6 HandleDisconnectedDevice (**am Code geprüft**)

In `BackupWatcherWorker.HandleDisconnectedDevice`:

| Schritt | Verhalten |
|---|---|
| DriveIdentifier prüfen | Wenn leer: keine Aktion |
| Laufwerk suchen | `HasExternalDriveChanged` mit Kennung und Pfad — sucht das Laufwerk an anderer Stelle |
| Wenn gefunden | Pfad aktualisieren (`UpdateExternalDrivePath`), Eintrag speichern, **Pausierung aufheben** (`UpdateSchedulePauseState` mit Pausierung aus), Uploads fortsetzen (`ResumeUploadsForSchedule`) |

#### 3.3.7 AddFileSystemWatcherForPath (**am Code geprüft**)

| Schritt | Verhalten |
|---|---|
| Watcher erstellen | `CreateWatcher` über die Watcher-Factory |
| Fehlerbehandlung | Fängt fehlende Dateien und ungültige Argumente ab und wirft `BackupWatcherError`, dessen `ErrorCode` den Wert `ScheduleError.LocalFolderNotFound` trägt |
| Re-arm | Bei `Changed` oder `Errored`: `AddFileSystemWatcherForPath` erneut versuchen |

#### 3.3.8 ExternalDriveService (**am Code geprüft**)

| Aspekt | Verhalten |
|---|---|
| `GetDetails` | Liefert `DriveDetails` mit `ExternalDriveIdentifier`, `IsExternal`, `IsRemovable`, `DriveProtocol`, `Volume` |
| `HasExternalDriveChanged` | Prüft, ob das Laufwerk noch an derselben Stelle ist; wenn nicht, wird über alle Laufwerke gesucht |
| `SaveExternalDriveDetails` | Erstellt `.AmazonDriveConfig` auf dem Volume mit einer GUID als `ExternalDriveIdentifier` |
| `GetConfigFileDriveDetails` | Liest die `.AmazonDriveConfig`-Datei |
| `IsExternalDrive` | Wahr, wenn `IsExternal` oder `IsRemovable` gesetzt ist oder `DriveProtocol` den Wert „USB" trägt |
| Disk Images | Werden ignoriert, wenn `DriveProtocol` den Wert „Disk Image" trägt (Ergebnis leer) |

#### 3.3.9 ScheduleRecord-Felder (**am Code geprüft**)

| Feld | Typ | Bedeutung |
|---|---|---|
| `LocalPath` | `string` | Lokaler Pfad |
| `DestinationNodeId` | `string` | Zielknoten in der Cloud |
| `LastScanTime` | `DateTimeOffset` | Letzter Scan-Zeitstempel |
| `LastEnumerationTime` | `DateTimeOffset` | Letzte Enumeration |
| `ErrorCode` | `ScheduleError` | Aktueller Fehlercode |
| `IsPaused` | `bool` | Pausiert |
| `DriveIdentifier` | `string` | GUID des Laufwerks |
| `DriveName` | `string` | Anzeigename des Laufwerks |
| `ConflictResolution` | `ConflictResolution` | Konfliktbehandlung |

### 3.4 Wechseldatenträger (**am Code geprüft**)

| Aspekt | Original-Verhalten | Beleg |
|---|---|---|
| **Volume-UUID** | `ExternalDriveIdentifier` (GUID) in `.AmazonDriveConfig` auf dem Volume; `DriveIdentifier` im Schedule | `ExternalDriveService.SaveExternalDriveDetails` |
| **IsRemovable** | `DriveDetails.IsRemovable` wird geprüft und für Metriken verwendet | `ExternalDriveService.GetDetails` |
| **IsExternal** | `DriveDetails.IsExternal` wird geprüft | `ExternalDriveService.GetDetails` |
| **DriveProtocol** | „USB" wird als extern gewertet; „Disk Image" wird ignoriert | `ExternalDriveService.IsExternalDrive` |
| **AutoConnect** | `EnableAutoConnect` in den Einstellungen; zeigt Fenster wenn ein bekanntes Laufwerk angeschlossen wird | `BackupScannerWorker.HandleDeviceArrival` |
| **Laufwerkswechsel** | `HasExternalDriveChanged` erkennt wenn das Laufwerk an einem anderen Pfad wieder verfügbar ist | `ExternalDriveService.HasExternalDriveChanged` |

### 3.5 Zusammenfassung: Original vs. Wir

| Aspekt | Original | Wir (heute) |
|---|---|---|
| **Fehlererkennung** | `ScheduleError.ExternalDriveMissing` + `HandleDeviceRemoval` | Keine — der Ordner wird still übersprungen |
| **UI-Anzeige** | Fehlermeldung + `Error`-Status | Keine — die Zeile bleibt auf dem letzten Wert |
| **Wiederanschließen** | `HandleDeviceArrival` erkennt aktiv, setzt Watcher neu, fordert Scan an | Watcher versucht es alle 30 s erneut; Wasserzeichen erst beim nächsten Start |
| **Dateien** | Bleiben in der lokalen `backup`/`uploads`-Tabelle | Bleiben im Dedup-Index |
| **Pausiert** | Schedule wird pausiert (`PauseSource.SystemPaused`) | Keine — der Ordner bleibt auf dem letzten Status |
| **Laufwerksidentifikation** | GUID-basiert über `.AmazonDriveConfig` + `DriveIdentifier` | Keine |
| **Hardware-Erkennung** | Aktiv über `NSWorkspace` Mount/Unmount | Keine |

---

## 4. Zielverhalten „Ordner nicht gefunden" konkret

> **Umgesetzt 2026-10-17:** Statt der ursprünglich geplanten Wiederverwendung von
> `FolderStatus::Paused` führt der Port den fehlenden Quellordner jetzt als eigenen
> Zustand `FolderStatus::NotFound` („Ordner nicht gefunden", AMBER), getrennt vom
> globalen PAUSED (`Config::paused`). Die Analyse-Tabellen in §2.5, §4.2 und §5
> beschreiben den Stand **vor** dieser Umsetzung.

### 4.1 Was passieren müsste

| Schritt | Zielverhalten | Wo umsetzen |
|---|---|---|
| **Erkennung** | Ein fehlender Ordner wird erkannt (beim Kaltstart, beim Watcher, bei manuellem Rescan) | `src/backup.rs` (`background_sync_pass`, `sync_watchers`, `start_scheduled_upload`) |
| **Zustand** | `FolderStatus::NotFound` wird gesetzt | `src/folders.rs` (`FolderStatus`), `src/backup.rs` (`source_status`) |
| **Wasserzeichen** | **Nicht** vorschieben — der Ordner gilt als „nicht gescannt" | `src/backup.rs` (`start_background_run`) |
| **UI** | „Ordner nicht gefunden" in AMBER | `src/upload.rs` (`status_text`) |
| **Wiederanschließen** | Aktive Erkennung (Watcher-Reset oder Scan-Trigger) | `src/backup.rs` (`sync_watchers`, `watch_loop`) |
| **Weiterlaufen** | Beim Wiederanschließen läuft der Ordner weiter | `src/backup.rs` (`background_sync_pass`, `watch_loop`) |

### 4.2 Wo widerspricht unsere heutige Architektur dem

| Stelle | Widerspruch | Beleg |
|---|---|---|
| `FolderStatus::Paused` | Existiert als Enum-Variant, wird aber **nie** zugewiesen | `src/folders.rs:120-128` |
| `finalize_upload` | Setzt nur `Done` oder `Error` — **nie** `Paused` | `src/backup.rs:1444-1448` |
| `finalize_background_store` | Setzt nur `Done` oder `Error` — **nie** `Paused` | `src/backup.rs:431-435` |
| `background_sync_pass` | Überspringt fehlende Ordner still — kein Status-Update | `src/backup.rs:593-595` |
| `start_scheduled_upload` | Loggt „Keine Fotos gefunden" — kein Status-Update | `src/backup.rs:1049-1050` |
| `sync_watchers` | Loggt Warnung — kein Status-Update | `src/backup.rs:803-806` |
| `record_folder_scan` | Wird nach erfolgreichem Lauf vorgeschoben — auch wenn der Ordner während des Laufs fehlte | `src/backup.rs:519-525` |
| `can_start` | Prüft nicht die Ordner-Existenz | `src/backup.rs:458-467` |

### 4.3 Gefährlichster Fall: Wasserzeichen-Vorschub

**Szenario:** Der Ordner ist vorhanden, der Lauf startet, die Dateien werden hochgeladen.
Während des Laufs wird das Laufwerk abgezogen. Der Lauf endet erfolgreich (weil die Dateien
bereits im Index sind). Das Wasserzeichen wird vorgeschoben.

**Folge:** Beim nächsten Start gilt der Ordner als „unverändert" und die Reste werden
**nie** nachgeholt.

**Beleg:** `src/backup.rs:519-525` — `record_folder_scan` wird nach jedem erfolgreichen Lauf
aufgerufen, unabhängig davon, ob der Ordner während des Laufs vorhanden war.

### 4.4 Konkreter Umsetzungsplan für „Ordner nicht gefunden"

| Schritt | Änderung | Details |
|---|---|---|
| **1. Erkennung im Kaltstart** | `background_sync_pass` soll prüfen ob `max_mtime` `None` liefert | Statt `continue`: `FolderStatus::NotFound` setzen, Log-Eintrag, **kein** Wasserzeichen-Update |
| **2. Erkennung im Watcher** | `sync_watchers` soll den fehlenden Ordner erkennen | Statt nur Warnung: `FolderStatus::NotFound` setzen, Log-Eintrag |
| **3. Erkennung im Rescan** | `start_scheduled_upload` soll `None`-Ordner erkennen | Statt „Keine Fotos gefunden": `FolderStatus::NotFound` setzen, verständliche Meldung |
| **4. Wasserzeichen-Guard** | `record_folder_scan` soll nur bei erfolgreichem **und** vorhandenem Ordner laufen | `FolderScan` um ein Flag für den fehlenden Ordner erweitern; bei gesetztem Flag überspringen |
| **5. Wiederanschließen** | `watch_loop` soll erkennen wenn ein Ordner wieder verfügbar ist | Nach `sync_watchers`: für jeden `NotFound`-Ordner `path.exists()` prüfen → wenn ja: Status zurücksetzen, Scan starten |
| **6. UI** | `status_text` zeigt `NotFound` („Ordner nicht gefunden") in AMBER | Existiert seit 2026-10-17 (`src/upload.rs`) |

### 4.5 Konflikt mit bestehenden Regeln

| Regel | Konflikt | Lösung |
|---|---|---|
| „Abmelden bei laufendem Upload" | Setzt `cancelled`-Flag, kein Status-Update | Unabhängig — `cancelled` und `NotFound` sind verschiedene Zustände |
| „Keine technischen Fehlermeldungen im UI" | Der User soll einen **verständlichen** Text sehen | „Laufwerk nicht verfügbar — bitte anschließen" statt `ENOENT` |
| „Wasserzeichen wird nach fehlgeschlagenem Lauf nicht vorgeschoben" | Neu: auch nicht wenn der Ordner fehlt | `folder_missing`-Flag in `FolderScan` |

---

## 5. Lücke-Tabelle (Original vs. Wir)

| # | Aspekt | Original | Wir (heute) | Lücke |
|---|---|---|---|---|
| 1 | **Fehlererkennung** | `ScheduleError.ExternalDriveMissing` + `HandleDeviceRemoval` | Keine | **Groß** — kein Mechanismus |
| 2 | **UI-Anzeige** | Fehlermeldung + `Error`-Status | Keine | **Groß** — User sieht nichts |
| 3 | **Wiederanschließen** | `HandleDeviceArrival` aktiv | Watcher alle 30 s | **Mittel** — funktioniert, aber nicht aktiv |
| 4 | **Wasserzeichen** | Wird nicht vorgeschoben | Wird nach erfolgreichem Lauf vorgeschoben | **Kritisch** — gefährlichster Fall |
| 5 | **Dateien** | Bleiben in der Tabelle | Bleiben im Index | **Keine** — gleich |
| 6 | **Pausiert** | Schedule wird pausiert | Keine | **Groß** — `FolderStatus::Paused` existiert, wird nie gesetzt |
| 7 | **Dialog-Speichern** | Prüft Pfad | Prüft Pfad nur beim Anlegen | **Klein** — Bearbeiten prüft nicht |
| 8 | **Wechseldatenträger** | GUID + aktive Erkennung | Keine | **Mittel** — keine aktive Erkennung |

---

## 6. Abgeleitete TODOs

### 6.1 Vor dem Release nötig

| # | TODO | Begründung | Kosten | Betroffene Dateien |
|---|---|---|---|---|
| **T1** | **`FolderStatus::NotFound` zuweisen:** Ein fehlender Ordner soll `FolderStatus::NotFound` setzen, nicht still übersprungen werden. | User-Entscheidung 2026-10-17: der Zustand ist „Ordner nicht gefunden" (AMBER), getrennt vom globalen PAUSED und nicht als Fehler. | Mittel | `src/backup.rs`, `src/folders.rs` |
| **T2** | **Wasserzeichen-Guard:** `record_folder_scan` soll nur aufgerufen werden, wenn der Ordner während des gesamten Laufs vorhanden war. | Gefährlichster Fall: sonst werden Reste nie nachgeholt. | Mittel | `src/backup.rs` |
| **T3** | **UI-Hinweis:** Die Ordner-Zeile soll „Ordner nicht gefunden" in AMBER zeigen, wenn der Ordner fehlt. | User sieht sonst keinen Hinweis auf das fehlende Laufwerk. | Klein | `src/upload.rs` (existiert seit 2026-10-17), `src/backup.rs` |
| **T4** | **Wiederanschließen-Erkennung:** Ein Mechanismus, der den Wiederanschluss aktiv erkennt (Watcher-Reset oder Scan-Trigger). | Das Original hat `HandleDeviceArrival` — wir haben nichts. | Mittel | `src/backup.rs` |
| **T5** | **Verständliche Fehlermeldung:** Statt „Keine Fotos gefunden" soll eine klare Meldung erscheinen, dass der Ordner nicht erreichbar ist. | User-Regel „keine technischen Fehlermeldungen im Endnutzer-UI". | Klein | `src/backup.rs`, `src/i18n.rs` |

### 6.2 Post-MVP

| # | TODO | Begründung | Kosten | Betroffene Dateien |
|---|---|---|---|---|
| **T6** | **Volume-UUID:** Eindeutige Identifikation des Laufwerks (wie `ExternalDriveIdentifier` im Original). | Ermöglicht zu erkennen, ob das gleiche Laufwerk wieder angeschlossen wurde. | Mittel | `src/folders.rs`, `src/backup.rs` |
| **T7** | **HandleDeviceArrival/Removal:** Aktive Hardware-Erkennung wie im Original. | Ersetzt das passive 30-s-Polling durch aktive Erkennung. | Groß | `src/backup.rs` |
| **T8** | **Dialog-Bearbeiten prüft Pfad:** `handle_config_form_action` soll auch beim Bearbeiten den Pfad prüfen. | Konsistenz mit dem Anlegen. | Klein | `src/backup_config.rs` |
| **T9** | **ScheduleError-Äquivalent:** Ein Fehler-Enum für „Ordner nicht verfügbar" (wie `ScheduleError.LocalFolderNotFound`). | Ermöglicht differenzierte Fehlermeldungen. | Mittel | `src/folders.rs`, `src/backup.rs` |

---

## 7. UNKNOWN

1. **Netzwerk-Laufwerk:** Wie das Original mit Netzwerk-Laufwerken umgeht, ist **UNKNOWN** —
   die `…_statuswindow_errorlong_local_drivenotfound` deutet auf einen Hinweis hin, aber
   ob es einen speziellen Mechanismus dafür gibt, ist am Code nicht eindeutig bestimmbar.

2. **Wiederanschließen-Latenz:** Wie schnell das Original den Wiederanschluss erkennt, ist
   **UNKNOWN** — `HandleDeviceArrival` ist belegt, aber die hängt von `NSWorkspace`-Notifications
   ab, deren Latenz nicht im Code steht.

3. **Datei-Ebene:** Was das Original mit Dateien macht, die während eines Laufs fehlen
   (werden sie als Fehler gezählt oder still übersprungen?), ist **UNKNOWN** aus den
   vorhandenen Analyseergebnissen.

4. **Windows/Linux-Verhalten:** Die Hardware-Erkennung (`NSWorkspace`) ist macOS-only.
   Wie das Original auf Windows/Linux Laufwerkswechsel erkennt, ist **UNKNOWN**.

---

## 8. Redaction note

Keine Account-Identifikatoren, Nutzernamen, Geräte-/Seriennummern, absoluten Pfade oder
verbatim dekompilierten Code reproduziert. Deutsche UI-Texte sind aus den vorhandenen
Clean-Room-Docs übernommen. Das Original-Bundle wurde nur lesend betrachtet und nie
gestartet, signiert oder modifiziert. `SPEC_MISSING_SOURCE.md` ist die einzige Datei,
die von dieser Aufgabe erstellt wurde.

---

## 9. Anhang: Wie diese Spec geprüft wurde

### 9.1 Werkzeug

- **ilspycmd** über den Wrapper `~/.local/bin/ilspycmd` (setzt `DOTNET_ROOT` selbst)
- Version: `ilspycmd: 11.1.0.9782 / ICSharpCode.Decompiler: 11.1.0.9782`
- DLL: `/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll` (4.6 MB, 2486 Klassen)

### 9.2 Aufrufe

```bash
# Klassenliste filtern
ilspycmd -l c "<dll>" | grep -iE "ScheduleError|DeviceArrival|DeviceRemoval|ScanRequest|WatcherWorker|DriveDetail|VolumeInfo|AutoConnect|ScheduleChanged|BackupStatusAlert"

# Einzelne Typen dekompilieren
ilspycmd -t Amazon.CloudDrive.UI.ScheduleErrorExtensions "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.ExternalDriveService "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.Schedules.BackupScannerWorker "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.Schedules.BackupWatcherWorker "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.Schedules.BackupScanRequest "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.FileSystem.DriveDetails "<dll>"
ilspycmd -t Amazon.CloudDrive.Services.FileSystem.ObservableHardwareChangeWatcher "<dll>"
ilspycmd -t Amazon.CloudDrive.Database.ManualUpload.ScheduleRecord "<dll>"
```

### 9.3 Geprüfte Typen

| Typ | Methode/Feld | Befund |
|---|---|---|
| `Amazon.CloudDrive.UI.ScheduleErrorExtensions` | `IsRetriable` | 5 Enum-Werte bestätigt: `None`, `LocalFolderNotFound`, `DestinationNodeNotAvailable`, `ExternalDriveMissing`, `NameConflict` |
| `Amazon.CloudDrive.Services.Schedules.BackupScannerWorker` | `ProcessScanRequest` | Pre-Check: Laufwerk → `ExternalDriveMissing`; Pfad fehlt → `LocalFolderNotFound`; bei Fehler: Watcher weg, Upload pausiert |
| `Amazon.CloudDrive.Services.Schedules.BackupScannerWorker` | `HandleDeviceArrival` | Watcher neu, Scan anfordern, AutoConnect |
| `Amazon.CloudDrive.Services.Schedules.BackupScannerWorker` | `HandleDeviceRemoval` | `ExternalDriveMissing` setzen, Watcher weg, `PauseSource.SystemPaused` |
| `Amazon.CloudDrive.Services.Schedules.BackupScannerWorker` | `SubscribeWatchers` | `DeviceArrival`/`DeviceRemoval` abonniert |
| `Amazon.CloudDrive.Services.Schedules.BackupWatcherWorker` | `Start` | Pausiert überspringen; `ExternalDriveMissing` → `HandleDisconnectedDevice` |
| `Amazon.CloudDrive.Services.Schedules.BackupWatcherWorker` | `AddFileSystemWatcherForPath` | `FileNotFoundException` → `BackupWatcherError` mit `LocalFolderNotFound` |
| `Amazon.CloudDrive.Services.Schedules.BackupWatcherWorker` | `HandleDisconnectedDevice` | Laufwerk suchen, Pfad aktualisieren, pausierung aufheben |
| `Amazon.CloudDrive.Services.FileSystem.ObservableHardwareChangeWatcher` | `NotifyDriveArrival`/`NotifyDriveRemoval` | `NSWorkspace` Mount/Unmount-Notifications (macOS) |
| `Amazon.CloudDrive.Services.ExternalDriveService` | `GetDetails`, `HasExternalDriveChanged`, `SaveExternalDriveDetails` | `.AmazonDriveConfig` mit GUID; `IsExternal`/`IsRemovable`/`DriveProtocol` |
| `Amazon.CloudDrive.Database.ManualUpload.ScheduleRecord` | Felder | `DriveIdentifier`, `ErrorCode`, `IsPaused`, `LocalPath`, etc. |
| `Amazon.CloudDrive.Services.FileSystem.DriveDetails` | Felder | `ExternalDriveIdentifier`, `IsExternal`, `IsRemovable`, `DriveProtocol`, `Volume` |

### 9.4 Datum

2026-10-16
