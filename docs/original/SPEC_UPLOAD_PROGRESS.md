# Upload-Progress / „Upload-Vorgang" — Clean-room Recon

Quellen: die realen deutschen UI-Strings der Original-App (`Localizable.strings`, hier als
Schlüssel → Wert zitiert), die OCR-Beobachtungen des Orchestrators von einem laufenden Upload,
sowie `UI_FLOWS.md` (§3.1, §4.6–§4.8, §9.1, §11), `UI_STATES.md` (§2.2, §3.1–§3.8,
§5.2, §5.7) und `SPEC_ONE_TIME_UPLOAD.md`.

Beschrieben wird **beobachtetes Verhalten in eigenen Worten** (Tabellen) — **keine verbatimten
Dekompilat-Auszüge**, **keine Personendaten**. Nicht Belegtes steht ausdrücklich als **UNKNOWN**.

## 1. Zweck & Abgrenzung

Umsetzbare Beschreibung, wie die Original-App den Fortschritt eines laufenden Upload-Vorgangs
darstellt: globale Zusammenfassung, Status pro Sicherungsziel, Status pro Datei, Wartezustände,
Warnungen/Fehler und zeitliche Angaben. Grundlage für den Ausbau unseres Fortschritts-UI
(`src/backup.rs`, `src/upload.rs`, `src/i18n.rs`). **Nicht** Teil: Download-/Sync-Fortschritt,
der Einzel-Upload-Drag-&-Drop-Ablauf (eigenes Doc) und der Speicherbalken (außerhalb des MVP,
§2.8). Unser Port hat **kein** separates Aktivitätsfenster — der Fortschritt lebt in der
Backup-Seite.

## 2. Beobachtetes Verhalten im Original

### 2.1 Globale Zusammenfassung

Dieselbe Aussage erscheint an mehreren Orten mit unterschiedlicher Wortwahl:

| Ort | Deutscher String (= Schlüssel) |
|---|---|
| Aktivitätsfenster-Titel / Feedback-Kategorie / Sicherungsbildschirm | „Upload-Vorgang läuft" (`…_activitywindow_title`, `…_feedback_category_uploading`), „Sicherungsvorgang läuft" (`…_backupscreen_backupinprogress`) |
| Fußzeilen-Zusammenfassung | „Elemente werden gesichert, ({0:n0} verbleibend)" (`…_backup_remainingcount`) |
| Aktivitätsfenster, Kopfzeile | „{0:N0} Elemente ({1:N0}) verbleibend" (`…_activitywindow_remaining_items_format`) |
| Menüleisten-Hover | „Upload-Vorgang läuft ({0} Dateien verbleibend)" (`…_contextmenu_message_uploadremaining`) |
| Status-Popup-Zusatz | „({0:n0} verbleibend)" (`…_statuspopup_stats_remaining`) |
| Status-Popup-Zusatz mit Fehlern | „({0:n0} verbleibende und {1:n0} Fehler)" (`…_statuspopup_stats_remaininganderrors`) |
| Transfer-Zeile (Singular / Plural / mit Fehlern) | „{0} Element verbleibend ({1})" / „{0} Elemente verbleibend ({1})" / „{0} Elemente verbleibend ({1}), {2} fehlgeschlagen" (`…_statuswindow_transferingmessage[_plural][_witherrors]`) |

Weitere Aggregat-Texte (`UI_STATES.md` §3.5): „{0} von {1} Elementen hochgeladen ({2} verbleibend",
„Verarbeitung abgeschlossen", „Upload abgeschlossen". Zwei Zählweisen sind belegt: „verbleibend"
und „verbleibend **und** Fehler" — Fehler werden **additiv** genannt, nicht eingerechnet. Der
zweite Zähler im Aktivitätsfenster ist **UNKNOWN** (Gesamt oder verbleibend).

**Belegt (Decompiler)** für die globale Zeile „N Artikel werden hochgeladen – Noch etwa M Minuten":
`UploadsViewModel.UploadingText` = `RemainingCountWithoutSuspended` (Singular-/Plural-Keys), und
`TimeRemainingText` = `TotalUploadSecondsLeft` über `FormatTimeRemaining` →
`days/hours/minutes/seconds_remaining_message` (`Math.Ceiling`, Deckel 3 Tage); bei unbekannter
Schätzung `calculating_time_remaining_message` („Verbleibende Zeit wird berechnet"). Die globale
Restzeit ist also **QueuedBytes ÷ gemessene aggregierte Bps** (siehe §2.7), **nicht** die Summe der
Pro-Datei-ETAs.

### 2.2 Status pro Sicherungsziel

Je Ziel eine Zeile mit **aggregiertem** Status (Precedence-Tabelle: `UI_FLOWS.md` §11.2):

| Baustein | Deutsch / Schlüssel |
|---|---|
| Laufend | „Upload-Vorgang läuft" (`…_upload_statuswindow_statusuploading`) |
| Abgeschlossen | „Datensicherung abgeschlossen" (`…_statuspopup_backupcomplete`) |
| Angehalten | „ANGEHALTEN" (`…_scheduletab_paused`) |
| Warten (Cold-Boot) | „Warten" (`…_statuswindow_statuswaiting`) |
| Vorbereitung | „Konfiguration wird durchgeführt" (`…_contextmenu_message_getchanges`) |
| Fehler-Unterzeile | „{n} Fehler" (`…_singular_errorsformat` / `…_plural_errorsformat`) |
| Fortschrittszeile | „{0}% hochgeladen" (`…_percentuploadedformat`), voll: „Vollständig" (`…_complete`), pausiert: „Angehalten bei {0} %" (`…_paused_on_text`) |

Zusätzlich „Zuletzt aktualisiert: {0}" (`…_scheduledbackupitem_lastupdatedtime`, CX-Variante
„Wurde aktualisiert: {0}"), gespeist aus `last_enumeration_time`/`last_scan_time`, relativ via
`…_datetimeago_*` („Vor 1 Min.", „Vor {0} Stunden"). Warn-/Fehlerzustände: „Einige Ordner wurden
nicht gesichert" (`…_schedules_warningmessage_folders_not_backedup`) und die Schedule-Fehler aus
`UI_FLOWS.md` §11.3. Statusfarbe aus `BackupStatusAlertLevel` ∈ {Normal, Info, Warning, Error, Success}.

**Belegt (Decompiler):** Der Primär-Status der Ziel-Zeile ist „Upload-Vorgang läuft" **ohne Zahl**.
Die Zahl in Klammern („(4)" im Screenshot) stammt aus anderen Kontexten: Menüleisten-Hover
(„Upload-Vorgang läuft ({0} Dateien verbleibend)", `…_contextmenu_message_uploadremaining`) bzw.
Aktivfenster (`…_activitywindow_remaining_items_format`). `n` = **verbleibende** Elemente
(`ITransferStats.RemainingCount = InProgress+Queued+Suspended`, bzw. `…WithoutSuspended =
InProgress+Queued`; DB `UploadStatusInfo.RemainingCount = QueuedItems + ProcessingItems`). Welcher
der beiden Zähler im Screenshot stand, ist **UNKNOWN**.

### 2.3 Status pro Datei

Detailbereich eines Ziels zeigt eine Dateitabelle. Spalten (belegt): „Name"
(`…_statuswindow_filenamecolumntitle`), „Status" (`…_statuswindow_statuscolumntitle`), „Größe"
(`…_statuswindow_sizecolumntitle`), „Letzte Aktualisierung"
(`…_statuswindow_lastmodifiedcolumntitle`, Tooltip „Letzte Änderung: {0}", `…_lastmodified`).
Die Screenshot-OCR eines laufenden Uploads zeigt je Dateizeile: Dateiname, eine **relative Zeit**
(„Vor 3 s" / „Vor 1 Min." / „Vor 30 Min.") und eine **Restzeit mit Prozent** („40 Sekunden …
verbleibend (40 %)", „17 Sekunden verbleibend (5 %)", „1 Sekunde verbleibend (100 %)").

**Belegt (Decompiler):** Der Fortschritt ist **byte-basiert**, nicht chunk- oder prozentbasiert:

- `FileUploadProgressEvent` (Basis `FileTransferProgressEvent`) trägt `PositionBytes`, `TotalBytes`,
  `BytesRead`, `Bps`, `TimeInMilliseconds`, `FilePath`; `IsCompleted => PositionBytes == TotalBytes`.
  Verteilung über `Changed` (Push bei `ChangeFinished()`) — also Byte-Stand **und** Rate.
- `ObservableStreamExtensions.TransferSpeed` zählt gelesene Bytes am Stream und **sampelt ~1 s**
  (`duration ?? TimeSpan.FromSeconds(1)`): `Bps = bytesRead / elapsed_ms * 1000`,
  `PositionBytes = stream.Position` (seekable), `TotalBytes = stream.Length`.
- Das UI-Item (`RecordItem.ProgressChanged`, `IDisplayItem.Progress`) rechnet daraus
  `Progress = PositionBytes / SizeBytes` (auf ≤ 1 begrenzt) und
  `RemainingTimeInSeconds = (SizeBytes − PositionBytes) / Bps`.
- Der angezeigte Zeilentext ist `"{Restzeit} ({Progress:P0})"` → exakt „40 Sekunden verbleibend
  (40 %)". Multipart analog: `MultipartUploadPartCompletedEvent` →
  `UpdateMultipartUploadProgressAndRemainingTime(totalParts, remainingParts, secs)`,
  `Progress = (total − remaining) / total`.

### 2.4 Statuszustände (Wortschatz)

`TransferStatus` (Persistenz-Ordinal) wird in der UI auf Labels gemappt (`UI_STATES.md` §3.2,
belegt; vollständige Liste dort): Waiting → „Warten", Processing → „In Bearbeitung", Retrying →
„Wiederholen", Completing → „Wird überprüft", Completed → „Abgeschlossen", Canceled →
„Abgebrochen", Errored → „Fehlgeschlagen", Ignored → „Zuvor hochgeladen", Skipped → „Verworfen",
Uploading → „Upload-Vorgang läuft", Warnung → „Achtung", Filter → „Alle" (Schlüssel jeweils
`…_statuswindow_status*` bzw. `…_upload_statuswindow_status*`).

### 2.5 Warten / Warteschlange

- Vor dem ersten Fortschritt (Cold-Boot): Ziel-Zeilen „**Warten**" mit Tooltip „Das Hochladen
  wird vorbereitet. Dieser Vorgang kann bis zu 10 Minuten dauern." (`…_statuswaiting_upload_tooltip`).
- Aktivitätsfenster hat zwei Modi **In Bearbeitung** / **Warten** (`ActivityDisplayMode`),
  Wartend-Titel „Warten" (`…_activitywindow_waitingitems_title`).
- Während der Zählermittlung: „Berechnung läuft …" (`…_calculating_count`), „Verbleibende Zeit
  wird berechnet" (`…_backupconfiguration_calculatingtime`).
**Belegt (Decompiler):** `TransferStatus` ist ein `[Flags]`-Enum (`None, Waiting=1, Processing=2,
Retrying=4, MultipartUploadCompleting=8, Completing=0x10, Completed=0x20, Canceled=0x100,
Errored=0x200, Ignored=0x400, Skipped=0x800, IgnoredDueToPotentialConflictWithiOSUploads=0x1000`);
final sind Completed/Canceled/Errored/Ignored/Skipped/…iOS. Der Zustand je Ziel wird aus Zählern
aggregiert (`AggregatedTransferState{Unknown,Preparing,InProgress,Completed,Failed,
InProgressWithFailures,CompletedWithFailures}` aus `TransferState.EvaluateState`). Die Zahl der
parallelen Uploads ist die Kunden-Einstellung `UploadQueues`, geklemmt **[1..8]**
(`CustomerSettingsMin/MaxSupportedUploadQueues`) — **dieselbe Spanne wie unser
„Gleichzeitige Uploads"-Slider**; Default =
`min(UploadMaxQueues, ProcessorCount * MaxNumberOfQueuesPerProcessorCore)`. Seriell/parallel
innerhalb eines Ziels ist damit **belegt**, nicht mehr UNKNOWN.

### 2.6 Fehler- und Warnzustände

| Ebene | Deutsch / Schlüssel |
|---|---|
| Kurztext allgemein | „Dateiübertragung fehlgeschlagen" (`…_errorshortdoesnotexist` / `…_errorshortfatal`) |
| Langtext unbekannt | „Diese Dateiübertragung ist aus einem unbekannten Grund fehlgeschlagen." (`…_errorlongunknown`) |
| Netzwerkumleitung/Proxy | `…_errorlong_networkredirect` |
| Lokale Datei fehlt / Ordner fehlt / keine Rechte | `…_errorlong_local_filenotfound` / `…_errorlong_local_foldernotfound` / `…_errorlong_InsufficientPermissions_ForAddChild` |
| Zeilen-Warnung | „Achtung" (`…_statuswindow_statuswarning`) |
| Wiederholen | „Erneut zur Warteschlange hinzufügen" (`…_readdtoqueue`); ferner „Alle erneut versuchen" (`…_retryall`), „Alle fehlgeschlagenen Dateien erneut versuchen" (`…_statuswindow_retryfailedfiles_button`) |
| Eigenes Fehlerfenster | „Fehler bei {0}" (`…_errorwindow_titleformat`) |

**Belegt (Decompiler):** `TransferEnumExtensions.GetDisplayString(status, error,
retryNeedsUserAttention)` liefert bei `Errored` den Kurztext (`error.GetShortString()` →
„Dateiübertragung fehlgeschlagen") und bei `retryNeedsUserAttention` zusätzlich „Achtung".
`RecordItem.RetryNeedsUserAttention` / `ErrorNeedsUserAttention` gilt nur bei Status ∈
{Retrying, Waiting} **und** einem „ernsten" Fehler; **ausgenommen** (kein „Achtung"): `None,
NetworkError, Unknown, NoResumeState, MultipartUploadCompletion*, BadRequest, TargetTypeUnknown` —
reine Netz-/Unbekannt-Fehler sind also **kein** „Achtung", während Rechte/Quota/lokale Datei fehlt
⇒ „Achtung". Retry ist ein expliziter Zustand `TransferStatus.Retrying` mit `RetryReason`
(`ManualRetry=17, NetworkError, PreviouslyPaused, RequestTimeOut, NameConflictSaveAsNew/Replace,
SizeChangedDuringUpload, …`); UI-Aktionen: „Erneut zur Warteschlange hinzufügen" (`…_readdtoqueue`)
und „Alle erneut versuchen" (`…_retryall`). **UNKNOWN bleibt:** ob das Original Fehler
**automatisch** (ohne Nutzeraktion) erneut versucht.

### 2.7 ETA-Semantik

| Situation | Deutsch / Schlüssel |
|---|---|
| Sekunden (1/n) | „1 Sekunde verbleibend" (`…_datetimeleft_onesec`) / „{0} Sekunden verbleibend" (`…_datetimeleft_manysecs`) |
| Minuten (1/n) | `…_datetimeleft_onemin` / `…_datetimeleft_manymins` |
| Stunden (1/n) | `…_datetimeleft_onehour` / `…_datetimeleft_manyhours` |
| Tage (1/n) | `…_datetimeleft_oneday` / `…_datetimeleft_manydays` |
| Restzeit+Größe (Fußzeile) | „{0} verbleibend" (`…_backup_remainingtimeandbytes`) |
| Vorberechnung | „Verbleibende Zeit wird berechnet" (`…_backupconfiguration_calculatingtime`) |

Die Grammatik ist **sprachabhängig dekliniert** (one/many je Einheit), nicht als `{h}:{m}:{s}`.
**Belegt (Decompiler):** Die ETA wird **byte-/ratenbasiert** an **zwei** Stellen gerechnet — nicht je
Ziel:

- **pro Datei**: `RecordItem.RemainingTimeInSeconds = (SizeBytes − PositionBytes) / Bps`, gerendert
  als `"{Restzeit} ({Progress:P0})"` (→ Screenshot „40 Sekunden verbleibend (40 %)").
- **global**: `IGlobalBackupTransferState.TotalUploadSecondsLeft = _multipartUploadSecondsLeft +
  _simpleUploadSecondsLeft`, je Wert = **QueuedBytes / gemessene Bps**. Die Rate ist ein EMA
  (`CalculateBps`; `*0.9` bei Stillstand, `*5`-Deckel; Cross-Schätzung `×25` bzw. `÷25`, wenn nur die
  andere Rate bekannt ist). Konsument: `UploadsViewModel.UpdateTimeRemaining`.
- **Keine** Pro-Ziel-/Pro-Ordner-Restzeit gefunden; „Verbleibende Zeit wird berechnet"
  (`…_backupconfiguration_calculatingtime`) erscheint bei unbekannter Schätzung, mit 3-Tage-Deckel.

### 2.8 Speicherbalken (Quota) — bewusst nicht im MVP

Belegt: Fußzeilen-Block „Ihr Speicherplatz" (`…_youraccount_yourstoragetitle`), Wert „{1} von {2}
belegt" (`…_youraccount_usageformat`), Upgrade-Link (`…_youraccount_upgradenow_buttontitle`); die
OCR zeigt „… MB/5,00 GB für Video verwendet". Braucht einen echten Quota-Abruf (Prime-only) →
**im MVP absichtlich weggelassen** (AGENTS.md: kein Platzhalter).

## 3. Gap-Analyse gegen unser aktuelles Programm

| Original-Element | Unser Stand | Datei |
|---|---|---|
| Globale Zusammenfassung (verbleibend + ETA) | vorhanden: `ProgressBar` 240 px mit `upload_progress_text` → „`{pct}% · noch {n} · ca. {m:ss}`" | `src/backup.rs` ~Z. 216–219 |
| Restzeit bei unzureichender Schätzung | vorhanden: `eta_secs` → `None` ⇒ „Schätzung läuft …" (`eta_suffix`) | `src/upload.rs` Z. 77–116 |
| Pro-Ziel-Statuszeile | vorhanden: `status_text(FolderStatus, pct, total)` → „{pct}% hochgeladen" / „100% hochgeladen" / „Angehalten ({pct}%)" / „Fehler" / „Wird vorbereitet …" | `src/upload.rs` Z. 47–71 |
| Pro-Ziel „Zuletzt aktualisiert" | vorhanden (absolut „Zuletzt aktualisiert: {}"), **kein** relatives „Vor X Min." | `src/backup.rs` Z. 286–293 |
| Pro-Datei-Zeile (Name/Zeit/Prozent/Status) | **fehlt** — nur `UploadShared.current: String` + Text-Log | `src/upload.rs` Z. 30, 31 |
| Status-Wortschatz (Warten/Achtung/Abgeschlossen/…) | **teilweise** — `FolderStatus` = Idle/Scanning/Uploading/Paused/Done/Error; kein „Achtung"/„Abgebrochen"/pro Datei | `src/folders.rs` Z. 46–53 |
| Wartezustand „Warten" (Cold-Boot) | **fehlt** — wir starten synchron, kein Cold-Boot-Zustand | — |
| Pro-Datei-/Pro-Ziel-ETA | **fehlt** — nur globale ETA | — |
| Relative Zeit „Vor X s/Min." | **fehlt** — nur Strings wie Datum/Uhrzeit | `src/backup.rs` Z. 30 |
| Fehler-Langtexte (Netzwerk/Datei fehlt/Rechte) | **fehlt** — generisches „FEHLER {}: {e}" aus IO-/HTTP-Fehler | `src/upload.rs` Z. 170, 229 |
| Separates Aktivitätsfenster | **fehlt** (bewusst) — Fortschritt lebt in der Backup-Seite | — |
| Speicherbalken | **bewusst nicht** (MVP) | — |

## 4. Mapping / Umsetzungsvorschlag

### 4.1 Datenmodell `UploadShared`

Für per-Datei-Zeilen und relative Zeitangaben fehlen heute die Felder. Entwurf (nicht umgesetzt):

| Bedarf (Original) | Feldvorschlag in `UploadShared` |
|---|---|
| pro-Datei-Status | `Vec<FileRow>` (`idx`, `path`, `status` inkl. Waiting/Uploading/Done/Failed/Warning) |
| „Vor X s/Min." | `started`/`last_change: Instant` je Datei, relativ formatiert beim Rendern |
| pro-Datei-Prozent | `pct: u8` je Datei — **nur falls** §4.2 es zulässt, sonst nur 0/100 |
| verbleibende/Fehler je Ziel | vorhanden: `folder_total/done/failed`, ggf. um `folder_failed`-Delta ergänzen |
| globale Restzeit | vorhanden: `started`, `done`, `total` |

`current: String` (Z. 30) kann durch die per-Datei-Liste ersetzt/ergänzt werden; der Log bleibt.

### 4.2 Byte-Level-Fortschritt?

**Im Original belegt (byte-level), bei uns offen.** Das Original hat echten Byte-Fortschritt
(§2.3): `PositionBytes/TotalBytes/Bps` aus einem ~1 s gesampelten `ObservableStream`, daraus
`Progress` und `RemainingTimeInSeconds`. **Unser** Upload läuft über den Cookie-Pfad
(`PrimePhotos::upload_picture`, `src/prime_photos.rs`) und liest die Datei als Ganzes — ob `reqwest`
dort einen Byte-/Chunk-Progress liefert, ist **nicht belegt** und vor der Umsetzung zu prüfen.
Bis dahin pro Datei nur **grob** (Wartend/Läuft/Fertig/Fehler), **kein** Live-Byte-Prozent — sonst
wäre der Wert erfunden. (Die `upload_parts`-Tabelle des Originals belegt Multipart-Nutzung *dort*,
nicht bei uns.)

### 4.3 UI-Anbindung

- `src/backup.rs`: globale Leiste bleibt; Ergänzung um die **verbleibend+Fehler**-Formulierung
  (§2.1) und eine Detailansicht, die die **per-Datei-Liste** aus §4.1 rendert.
- `src/upload.rs`: `status_text` um weitere Ziel-Zustände erweitern (mind. „Warten",
  „Abgeschlossen"/Success-Farbe, „Achtung"/amber) plus Helfer für relative Zeit („Vor {n} s/Min./Std.").
- Farblogik bleibt `theme::*` (TEAL/GREEN/AMBER/RED/MUTED), vgl. `status_text`.

### 4.4 i18n-Keys (de+en in `src/i18n.rs`)

Regel: Deutsch = Schlüssel; neue Keys direkt zweisprachig in die Gruppe „upload progress & log":

| Vorschlag Schlüssel (de) | EN (Vorschlag) |
|---|---|
| „{} Elemente verbleibend" / „{} Elemente verbleibend, {} fehlgeschlagen" | `{} items remaining` / `{} items remaining, {} failed` |
| „1 Sekunde verbleibend" / „{} Sekunden verbleibend" | `1 second remaining` / `{} seconds remaining` |
| „1 Minute verbleibend" / „{} Minuten verbleibend" | `1 minute remaining` / `{} minutes remaining` |
| „1 Stunde verbleibend" / „{} Stunden verbleibend" | `1 hour remaining` / `{} hours remaining` |
| „Vor {} s" / „Vor {} Min." / „Vor {} Std." | `{} s ago` / `{} min ago` / `{} h ago` |
| „Warten" / „Achtung" / „Abgeschlossen" | `Waiting` / `Attention` / `Completed` |
| „Dateiübertragung fehlgeschlagen" | `File transfer failed` |
| „Diese Dateiübertragung ist aus einem unbekannten Grund fehlgeschlagen." | `This file transfer failed for an unknown reason.` |

Hinweis: unser `tr`-System unterstützt **kein** `{0:n0}`-Tausendertrennzeichen und nur
`{}`-Platzhalter in fester Reihenfolge — Zahlen werden vorab formatiert (`tr1/tr2/tr3`).
Retry-/Aktivfenster-Keys erst mit deren Feature aufnehmen.

## 5. UNKNOWN (nicht belegt — nicht raten)

- Ob **unser** Cookie-Upload einen Byte-/Chunk-Fortschritt liefern kann (§4.2) — im Original belegt,
  bei uns offen.
- Welcher der beiden Zähler im Screenshot-„(4)" stand (`RemainingCount` vs. `…WithoutSuspended`);
  die Ziel-Zeile selbst zeigt belegt **keine** Zahl.
- Semantik der relativen Zeit „Vor X s/Min." je Datei: stammt belegt aus `Last-Modified`/`datetimeago_*`,
  **nicht** aus der ETA-Rechnung; im Screenshot nicht eindeutig zuordenbar.
- Ob das Original Fehler **automatisch** erneut versucht (Retry-Zustand/-Gründe sind belegt, der
  Auslöser „automatisch vs. Nutzeraktion" nicht).
- Exakte Spaltenbreiten/Sortierung der Original-Dateitabelle (nicht 1:1 klonen, nur Verhalten).
- Ob Pause **pro Datei** möglich ist (Original hat `paused` je Schedule/global; wir pausieren global
  über `watch`).

## 6. Querverweise

- `UI_FLOWS.md` §3.1 (Fußzeile), §4.6–§4.8 (Detailtabelle, Spalten, Drag & Drop), §9.1
  (Menüleisten-Hover), §11 (Status-Precedence, Schedule-Fehler, Completion/Retry).
- `UI_STATES.md` §2.2 (Fußzeile), §3.1–§3.5 (Zeilenmodell, Statuswortschatz, Aggregat- und
  Fortschrittstexte), §3.8 (DB-Felder `uploads`/`backup`/`upload_parts`), §5.2 (Aktivitätsfenster),
  §5.7 (Fehlerkatalog).
- `SPEC_ONE_TIME_UPLOAD.md` (Ad-hoc-Upload in dieselbe Fortschrittsanzeige) sowie
  `src/upload.rs`, `src/backup.rs`, `src/folders.rs`, `src/i18n.rs`; Regeln in `AGENTS.md`.

---

## 7. Darstellung: Baum/Liste, Spalten, Platzausnutzung, Anti-Abschneiden (Recon 2026-10-14)

**Nachträglich ergänzter Abschnitt**, aufgenommen, weil §1–§4 die *Texte/Byte-Semantik* beschreiben,
aber die *Layout-Mechanik* (Baum vs. Liste, Spaltenbreiten, Truncation, Fill) offen ließen — genau der
Punkt, an dem unser UI vom Original abweicht. Quellen wie §1: das gemanagte Original
(`Amazon Photos.dll`, Version 11.3.0 / Build 36), **nur lesend** mit `ilspycmd` (11.1.0.9782,
`DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`) dekompiliert und in `Localizable.strings`
(`de.lproj`) gegengelesen. Aussagen in **eigenen Worten**; Klassennamen/Feldnamen und UI-Strings als
Beleg. Ergänzt (nicht dupliziert): `SPEC_STATUS_DETAIL.md` §3.1–§3.6 (Zeilenmodell, Statusvokabular,
Fehlerflächen) und `UI_STATES.md` §3.6. **Widerspruch zum bestehenden Dokument: siehe §7.7.**

### 7.1 Grundstruktur — Baum **und** Liste, umschaltbar

| Frage | Befund | Einordnung |
|---|---|---|
| Baum oder Liste? | **Beides, umschaltbar.** `BackupItemsDisplayMode ∈ {List, DrillDown}` (persistiert als `CustomerSettings.BackupItemsDisplayMode`), zwei Icon-Buttons im Detail-Kopf („Verschachtelte Anzeige" / „Flache Listenanzeige"). | **belegt** (vgl. schon `SPEC_STATUS_DETAIL.md` §3.5) |
| Baumform | **Breadcrumb-Drill-down**, kein aufklappbarer Inline-Baum: nur Ordnerzeilen (`TargetType == Folder`, `AutoSaveNode`) sind selektierbar; Klick läuft `NavigationCommand` → `Navigate(node)` setzt `DisplayNodes = node.Children`. Kopf zeigt Großeltern-Überlaufmenü + Eltern-Ordner + gewählten Ordner (Carets). | **belegt** |
| Ebenen | beliebig tief (Pfad-Token-Baum `AutoSaveFolderNode`/`AutoSaveNodeManager`); sichtbar ist **jeweils eine Ebene** (`node.Children`). | **belegt** |
| Liste | `List`-Modus = **flache** `AutoSaveNodeList` über alle Knoten (Ordner **und** Dateien), sortiert nach `(UserStatus, TargetType, SortDate)`. | **belegt** |
| Gruppierung nach Backup-Ordner | im Detail-Baum: über die Drill-down-Navigation; in der **flachen** Backend-Liste gar nicht. Die **einzige** Status-Gruppierung im Original ist das Aktivitätsfenster („In Bearbeitung" / „Warten", nach Status-Bucket, **nicht** nach Ordner). Unsere Queue-Gruppierung nach Ordner ist **kein Port**, sondern unsere eigene Entscheidung. | **belegt** (Abwesenheit) |
| CX-Zwang | Bei gesetztem Experiment `EnableCXUploadSimplification` (Behandlung `"T1"`) schaltet das Detail **automatisch in DrillDown**. | **belegt** |
| Sammel-Knoten/Aggregat | Ja: Ordnerzeilen tragen im `Größe`-Feld **zwei Zeilen** — Gesamtgröße + „{n} Elemente" (Anzahl verschachtelter Items, `CountSizeOfNestedItems`). Fortschritt von Ordnern wird aus den Kindern aggregiert (`CalculateNestedCompleted`). | **belegt** |

Ein **Inline-Baum mit Disclosure-Dreiecken** wurde im Upload-Detail **nicht** gefunden; die Spalte
`uploads.is_expanded`/`is_visible` gehört zum Drill-down-Zustand, nicht zu einem Aufklapp-Dreieck.
→ **inferred** (kein Fund).

### 7.2 Spalten (nur `List`/Detail; Breiten in pt, inline = Mindestbreite)

Zwei Detail-Layouts, gewählt über das Experiment `EnableCXUploadSimplification`:

| Layout (Klasse) | Name | Letzte Änderung | Größe | Status | Zeilenhöhe / Extra |
|---|---|---|---|---|---|
| **CX** `DetailTableView` | 303 / min 275, Kopf mit **Sortier**-Icon | 95 / min 95 (Titel = „Letzte Aktualisierung" bis „:" gekürzt) | **87 / min 87** | min 150, Kopf mit **Filter**-Icon | **48**, 32-pt-Thumbnails, Auto-Upload-Toggle, Kopf 52 |
| **non-CX** `BackupsDetailTableView` | 275 / min 275 | 100 / min 100 | **fehlt** | min 150, Kopf mit **Down-Caret** (Filtermenü) | **44**, Kopf 40, Fuß 56 mit „Wiederholen" |
| Aktivitätsfenster | 270 / min 270 | – | 80 / min 80 | 200 / min 200 | 44 |
| Sync-Tab | 400 | „Datum hochgeladen" 120 | 90 | min 160 | 44 |
| Download-Tab | 400 | – | 90 | min 160 + Down-Caret | 44 |
| Fehlerfenster | eine Spalte 275 (nur Name) | – | – | – | **75** |

Gemeinsam (`BaseTransferItemsTableView.InitializeTableView`): `AllowsColumnResizing = true`,
`AllowsColumnReordering = false` — **breitenverstellbar, nicht umsortierbar**. Jede Spalte hat eine
`MinWidth`. **Sortieren** nur im CX-Name-Kopf (Klick = nach `FriendlyName`, Richtung schaltet um);
**Filter** über das 7-Einträge-Menü am letzten Spaltenkopf (nur non-CX) bzw. den View-Model-Filter.
→ **belegt** (Sortier-/Filterdetails: `SPEC_STATUS_DETAIL.md` §3.3).

### 7.3 Anti-Abschneiden — der Kern

Das Original **kürzt in der Mitte** und macht den vollen Pfad per Tooltip erreichbar:

1. **Mittel-Ellipse statt Rand-Abschnitt (Standard):** Jede Namenszelle (`IconWithLabelTableCell`)
   setzt `LineBreakMode = NSLineBreakMode.TruncatingMiddle`. Ebenso die Status-Zelle
   (`TextLabelCell`/`TextTwoLinesLabelCell`/`TransferItemStatusTableCell`). Lange Namen werden also
   **in der Mitte** gekürzt — Dateiende und -anfang bleiben sichtbar. → **belegt**
2. **Tooltip mit vollem Pfad** (zwei Varianten):
   - **CX (4 Spalten):** eigener **schwebender Tooltip** (`TooltipNameView`, Identifier
     `BackupItem_PathTooltip`): voller Pfad (selbst mittel-gekürzt, Breite ≤ ~484 pt), Trennlinie und
     ein anklickbarer Link **„Lokalen Ordner öffnen"** (öffnet den Finder). Anzeige über
     `CustomTooltipView` mit Verzögerung **1000 ms** / Ausblenden **5000 ms**,
     `AllowMouseOverTooltip = true` (Maus darf in den Tooltip wandern, um den Link zu klicken).
   - **non-CX (3 Spalten):** nativer AppKit-Tooltip (String) = `FullPath` + Zeilenumbruch +
     „Größe: {size}" (nur Dateien) + Zeilenumbruch + „Letzte Änderung: {age}". → **belegt**
3. **Die Namensspalte ist die flexible Spalte:** das CX-Layout setzt
   `ColumnAutoresizingStyle = FirstColumnOnly` — beim Fensterbreiter-Machen **wächst die Namensspalte**
   (mehr Name sichtbar, weniger Kürzung), statt toten Raum zu lassen. `MinWidth 275` verhindert zu
   starkes Schrumpfen. → **belegt**
4. **Horizontaler Scrollbalken als letzte Reserve:** jede Detail-Tabelle liegt in einer `NSScrollView`
   mit `HasHorizontalScroller = true`, `AutohidesScrollers = true` — wird der Inhalt trotzdem breiter,
   ist er per Scrollen erreichbar (nichts wird unerreichbar). → **belegt**
5. **Kein Umbruch/keine variable Zeilenhöhe für den Namen:** Der Name ist **einzeilig** mit Ellipse;
   die einzige zweizeilige Zelle (`TextTwoLinesLabelCell`) ist die **Größe**-Spalte (Größe + „{n}
   Elemente"). `MaximumNumberOfLines` kommt nur an drei anderen Stellen vor, nicht an der Namenszelle.
   → **belegt**
6. **Nebenbei:** die **Backup-Listenzeile links** (Ordnerliste, `BackupItemView`) kürzt non-CX in der
   Mitte (Literal `5` = `TruncatingMiddle`), **CX aber am Kopf** (Literal `4` = `TruncatingHead`) und
   hat ebenfalls einen (Zusammenfassungs-)Tooltip. → **belegt** (Literalwert) / *inferred* (Enum-Mapping)

### 7.4 Platzausnutzung

- **Die Tabelle füllt die Fläche vollständig.** Das Details-View ist vertikal
  `Kopf (40 / 52)` + `NSScrollView` + `Fuß (56)` [+ optional 36-pt-Fehlerbanner zwischen Kopf und
  Tabelle]; die Scrollview ist an **alle vier Kanten** des Content-Bereichs gepinnt
  (`Leading/Trailing = this`, `Top = Kopfende`, `Bottom = Fußanfang`). Zwischen Liste und Fuß bleibt
  **kein** toter Raum. → **belegt**
- **Virtuelles, zeilen-recycelndes NSTableView** (`MakeView`/`GetViewForItem`): nur sichtbare Zeilen
  werden materialisiert. Die einzige Obergrenze im Original ist die **View-Model-Paginierung
  (1000 Items/Seite)** — **keine** Rendering-Kappung wie unsere 300-Zeilen-Grenze. → **belegt**
- **Letzte Spalte streckt sich:** `ReloadData()` ruft nach jedem Neuaufbau `SizeLastColumnToFit()`; die
  letzte Spalte füllt die Restbreite. Kombiniert mit der flexiblen ersten Spalte (7.3.3) gibt es
  keinen horizontalen Leerraum. → **belegt**
- **Leerzustand füllt ebenfalls:** die Datenquelle meldet **immer ≥ 1 Zeile**
  (`GetRowCount = max(count, 1)`); diese eine Zeile ist eine zentrierte `TextLabelRowView` mit dem
  filterabhängigen Leer-Text („Keine Dateien hochgeladen …", „Keine fehlgeschlagenen Uploads. (Puh!)"
  …). → **belegt**
- **Der ganze Backup-Screen** ist vertikal gefüllt: optionaler Kopf 65 + `PhotosSplitView` an
  leading/trailing/bottom (CX ohne Kopf) → linke Liste + rechtes Detail nutzen die volle Fensterhöhe.
  → **belegt**

### 7.5 Aggregat / Fußzeile

- **Global** (Hauptfenster-Fußzeile, 60 pt): „Elemente werden gesichert, ({0:n0} verbleibend)" +
  Restzeit/Größe — siehe §2.1/§2.7; **keine** Fortschritts-**Leiste** in der Fußzeile, sondern Text.
- **Pro Ziel** (Backup-Listenzeile): aggregierter Status + „Zuletzt aktualisiert: {0}" (§2.2).
- **Pro Ordner** (nur CX-4-Spalten): Status spaltenweise aggregiert; `Größe` zweizeilig (Gesamtgröße +
  „{n} Elemente"); Fortschritt „{n}% hochgeladen…" aus `CalculateNestedCompleted`. **Keine**
  Pro-Ordner-Restzeit (→ §2.7).
- **Pro Datei**: byte-basiert „{Restzeit} ({Progress:P0})" (§2.3).
- **Aktivitätsfenster-Kopfzeile**: „{0:N0} Elemente ({1:N0}) verbleibend" — die einzige rein
  aggregierte Fortschrittszeile über alle Quellen.
- **Detail-Fußzeile** (non-CX): Nur der Button „Wiederholen", aktiv nur bei `FailedCount > 0`; im
  CX-Layout entfällt die Fußzeile.

### 7.6 Zustände, Zahlenformat, Zeilenhöhe/Hover

- **Statusdarstellung je Zeile:** Icon + Text. Icon-Map (beide Status-Zellen, `UserStatus`):
  `Completed` → success-Icon (16 pt), `Failed` → roter Fehlerkreis (Text in Fehlerfarbe),
  `Processing` → rotierend animiertes Laden-Icon (CALayer-Rotation), `Waiting` → Warten-Icon,
  `NeedsUserAttention` → „Achtung"-Icon (amber), `Canceled`/`Skipped` → Abbruch-Icon. Hover-Tooltip =
  Status-Text. Vollständiges Vokabular/Fehlerfälle: `SPEC_STATUS_DETAIL.md` §3.5 + §2.4/§2.6.
- **Fehlerzeile** bleibt in der Tabelle (Filter „Fehlgeschlagen"); Wiederholen über die Fußzeile
  (non-CX), das Fehlerfenster oder die In-Zell-Links. → siehe `SPEC_STATUS_DETAIL.md` §3.6.
- **Selektion/Hover:** Standard **nicht selektierbar**; nur Ordnerzeilen im Drill-down sind
  selektierbar und bekommen Hover-Hintergrund + Zeiger-Cursor. Zeilentrennlinie 0,5 pt. **Kein**
  Kontextmenü in der Datei-Tabelle (Aktionen sind In-Zell-Links + Fußzeile). → **belegt**
- **Zeilenhöhe fix:** 44 (3-Spalten/Aktivität), 48 (CX-4-Spalten, 32-pt-Thumbnails), 75
  (Fehlerfenster) — `GetRowHeight` liefert 48 nur für CX. **Nicht** variabel. → **belegt**
- **Zahlen:** `HumanizedSize` = **binär** (÷1024) mit **einer** Nachkommastelle (abgerundet), Einheiten
  kurz und unübersetzt: `B/KB/MB/GB/TB`. Zähler mit Tausendertrennung (`{0:N0}`). Item-Zeile
  „{0} Elemente". Restzeit `datetimeleft_*` one/many, Prozent `{0:P0}`; relative Zeit 12-Zweig
  `datetimeago_*`, ab einer Woche absolute `MMM d yyyy` / `MMM d` (englische Monatskürzel, nicht
  lokalisiert). → **belegt** (Details: `SPEC_STATUS_DETAIL.md` §3.4) / **belegt** (`HumanizedSize`)

### 7.7 Widerspruch zum bestehenden Dokument (benannt, nicht überschrieben)

1. **`SPEC_STATUS_DETAIL.md` §3.2** nennt `BackupsDetailTableView` „(CX layout …)" und
   `DetailTableView` „(legacy layout, thumbnails)" und folgert „CX details view has **three** …
   legacy … **four**". Die Verdrahtung im Code ist **umgekehrt**: die **CX**-Variante
   (`ScheduledBackupDetailsTableView`, gewählt bei `EnableCXUploadSimplification == "T1"`) nutzt
   **`DetailTableView` = vier Spalten** (mit Größe/Thumbnails); die **non-CX**
   `ScheduledBackupDetailsView` nutzt **`BackupsDetailTableView` = drei Spalten**. Die Spaltenmengen
   *je Klasse* stimmen, die Labels **CX/legacy sind vertauscht**. → **belegt**
   (`ScheduledBackupDetailsViewController`: CX ⇒ `new ScheduledBackupDetailsTableView()`, sonst
   `new ScheduledBackupDetailsView()`; deren `AddTableView` wählen `DetailTableView` bzw.
   `BackupsDetailTableView`.) Der Abschnitt bleibt stehen; diese Zeile korrigiert ihn.
2. **§6 UNKNOWN „Which layout a real user sees"** ist damit **auflösbar**: der Schalter ist das
   Remote-Experiment `EnableCXUploadSimplification` (Behandlung `"T1"` ⇒ CX = 4 Spalten, sonst
   non-CX = 3 Spalten). *Welche* Behandlung ein konkreter Nutzer bekommt, bleibt UNKNOWN.
3. **§5 UNKNOWN „Exakte Spaltenbreiten/Sortierung"** ist für die Upload-Tabellen **belegt** (§7.2).
4. **Quirk (belegt):** `TooltipNameView` liefert für Datei **und** Ordner denselben Link-Text
   („Lokalen Ordner öffnen") — die Datei-Variante ist im Code unerreichbar. Für den Port irrelevant,
   aber nicht als Vorlage „Datei-Variante existiert" übernehmen.

### 7.8 Gap-Tabelle: Original (belegt) | unser Stand heute | Empfehlung

Unser Stand aus `src/backup.rs` (Queue-Panel `Warteschlange (n)`, Z. ~1327–1367), `src/upload.rs`,
`src/folders.rs` gelesen. → **belegt**.

| Thema | Original (belegt) | unser Stand heute | Empfehlung |
|---|---|---|---|
| Struktur | Baum (Drill-down + Breadcrumb) **und** flache Liste, umschaltbar | nur **flache** Liste je Lauf, nach Ordner gruppiert (unsere Idee), in einer Aufklappung | Liste behalten; **Ordnerzeilen** mit Zähler/Größe zu einem **Drill-down** ausbauen — in egui ohne echte Breadcrumb zunächst als „Ordner → Kinder" mit Zurück-Zeile |
| Spalten | Name · Letzte Änderung · Größe · Status (Kopfzeile) | **keine** Kopfzeile; pro Zeile nur „Status + Dateiname" | Mindestens **Größe** und **Status als Spalte** (feste Spaltenköpfe, Name flexibel), statt Status als Präfix |
| Abschneiden | **Mittel-Ellipse** + **Tooltip mit vollem Pfad**; Name-Spalte wächst mit | einzeiliges Label, am Rand gekürzt, **kein** Tooltip | egui-Label `truncate(true)` + **`on_hover_text(full path)`**; Pfad zusätzlich als zweite, gedämpfte Zeile ODER Tooltip |
| Platzausnutzung | Scrollview füllt Content; virtuelle Zeilen; letzte Spalte `SizeLastColumnToFit`; Leerzustand zentriert | `ScrollArea::vertical().max_height(220.0)`, **Kappung auf 300 Zeilen**, Leerraum unter der Liste | `max_height` an die verfügbare Höhe binden (**füllen**), Kappung entfernen (egui virtualisiert bei `ScrollArea::show_rows`); Leerzustand als zentrierter Hinweis |
| Aggregat | Fußzeile „(n verbleibend)" + Restzeit; Ordnerzeile „{n} Elemente + Größe"; Aktivitätskopf „{0:N0} Elemente ({1:N0}) verbleibend" | globale Leiste vorhanden; **kein** Ordner-Aggregat (Anzahl/Größe) | pro Ordner-Zeile „{offen}/{gesamt} · {Größe}" ergänzen (Daten in `UploadShared` teils vorhanden) |
| Zahlenformat | binär (1024), 1 Dezimalstelle, `B/KB/MB/GB/TB`; `{0:N0}` | teils vorhanden (`tr1/tr2`), keine Tausender-Trennung | Byte-Formatierung zentral (`format_size`) + Tausender-Trennung für Zähler |
| Zeilenhöhe/Hover | fix 44/48; Hover nur bei selektierbaren (Ordner-)Zeilen | fixe Zeilenhöhe; kein Hover-Stil | Hover-Highlight für Ordner-/ aufklappbare Zeilen; Datei-Zeilen flach |
| Leerzustand | gefüllte Tabelle mit zentriertem Filtertext | Panel ist einfach leer/ausgeblendet | zentrierter Leertext („Keine Uploads in der Warteschlange") im gefüllten Bereich |

### 7.9 UNKNOWN (dieser Abschnitt)

- **Welche Experiment-Behandlung** (`T1` vs. Kontrolle) ein realer Nutzer bekommt → **UNKNOWN**
  (nur der Mechanismus + die Folgen je Zweig sind belegt).
- Ob die **Name-Spaltenbreite** beim Nutzer-Resize exakt so wächst, wie `FirstColumnOnly` es
  vorsieht, wenn zuvor manuell resized wurde → **UNKNOWN** (Autoresizing-Kette nicht weiterverfolgt).
- Ob es einen **„Alles aufklappen"**-Schalter im Drill-down gibt → **UNKNOWN** (kein Fund; die
  Navigation ist klick-per-Ordner).
- Exakter **Icon-Satz** je `UserStatus` (Dateinamen belegt, Pixel-Design nicht gelesen) → **UNKNOWN**.
