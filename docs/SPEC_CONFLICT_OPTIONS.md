# Spec: alle Dateikonflikt- und Duplikat-Varianten des Originals

**Status:** Clean-Room-Recon (nur lesend) des offiziellen macOS-Amazon-Photos-Clients
11.3.0/36, ergänzt um den Ist-Stand unseres Rust-Codes. Eigene Worte/Tabellen, keine
verbatim Code-Auszüge, keine Personendaten. Deutsche UI-Labels werden wörtlich aus
`Contents/Resources/de.lproj/Localizable.strings` zitiert.

**Frage.** „Bitte alle Varianten wie Dateikonflikte wie das Original unterstützen" — diese
Datei ist die **vollständige, prüfbare Liste** aller Varianten aus **beiden** Mechanismen
(Inhalts-Dedupe und Namenskonflikt), je Variante mit internem Namen, deutschem Label, Default,
Auslösebedingung, Leitungsverhalten, 409-Verhalten, unserem Stand (mit `Datei:Zeile`), Aufwand
und Abnahmekriterium — plus der Konflikt-Dialog und die Lücken.

**Vorhandene Grundlage.** `docs/SPEC_DUPLICATES.md` (Mechanismen, Erkennungsweg, lokaler
Zustand, 409, Gap-Tabelle) und `docs/SPEC_FILENAME_ENCODING.md` (Namens-Kodierung). Jede
übernommene Aussage wurde gegen das Assembly bzw. unseren Code geprüft; die Abweichungen
stehen in §8 „Korrekturen an den vorhandenen Docs".

**Quellen (nur lesend).** `/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll`
(ILSpy 11.1, vollständiger Decompile), `Contents/Resources/de.lproj/Localizable.strings`,
lokale SQLite (`~/Library/Application Support/Amazon Cloud Drive/`). Unser Code:
`src/folders.rs`, `src/dedup.rs`, `src/upload.rs`, `src/prime_photos.rs`, `src/backup_config.rs`,
`src/backup.rs`, `src/i18n.rs`. Symbol-/Key-Namen stehen in §7.

---

## 0. Die drei Mechanismen

Das Original kennt **drei** voneinander unabhängige Duplikat-Mechanismen:

| # | Mechanismus | Entscheidungsort | Identität | Ebene |
|---|---|---|---|---|
| A | **Inhalts-Dedupe** („Duplikate überspringen") | Client (`PreUploadCheck` gegen `cloud.db`), Server als Netz | MD5 des ganzen Files, **über Pfade hinweg** | pro Sicherungsziel (Toggle) + globaler Default |
| B | **Namenskonflikt** („Konflikte mit Dateinamen") | Client (Name+Parent gegen `cloud.db`), Server als Netz | exakter Name + Parent-Ordner | strikt pro Sicherungsziel |
| C | **Apple-Photos-`.heic`-Heuristik** | Client (erste 2 Bytes = JPEG-BOM) | Byte-Muster, nur `.heic` in einer Photos-Bibliothek | global (Kunden-Einstellung) |

A und B sind **orthogonal**: eine Datei kann gleichzeitig Inhalts-Duplikat und Namenskonflikt
sein. Das Original entscheidet **erst A, dann B** (§1.3). C liegt quer dazu und ist bei uns
bewusst außen vor (§5).

---

## 1. Die Liste

### 1.1 Mechanismus A — Inhalts-Dedupe

| Feld | Beleg / Wert |
|---|---|
| **Interner Name** | `Amazon.CloudDrive.Database.ManualUpload.DeduplicateOptions` — `[Flags]`: `None=0`, `Photos=2`, `Videos=4`, `PhotosAndVideos=6`. **UI nutzt nur ein Bool** (`ScheduledBackupItem.AvoidDuplicates`); die Flag-Werte `Photos`/`Videos` werden von keiner View gesetzt. |
| **Deutsches Label + Ort** | Aktueller Dialog („Upload-Einstellungen für „<Ordner>" bearbeiten", Checkbox `BackupView_SettingsAvoidDuplicates_Checkbox`): `clouddrive_desktop_upload_settings_skip_duplicates_option_title` = **„Duplikate überspringen"**, Untertitel `..._subtitle` = „Dateien, die sich bereits in meinem Konto befinden, nicht sichern". Legacy-Wortlaut: `clouddrive_desktop_statuswindow_scheduleconfiguration_dedupeoption` = „Duplikate vermeiden: Keine Fotos und Videos sichern, die sich bereits in meinem Konto befinden". Global (Voreinstellungen): `EnableDeduplicationForPhotosAndVideos`. |
| **Default + Herkunft** | **Onboarding-Ziel:** an, **wenn** der Inhaltsfilter ≠ „Everything" (`AppSettings.DefaultOnboardingDedupeChecked` = `true`). **Ziel nach dem Onboarding:** `CustomerSettings.EnableDeduplicationForPhotosAndVideos`, dessen Default `DefaultPostOnboardingDedupeChecked` = **`false`** ist. Harte Kopplung: Inhaltsfilter `Everything` erzwingt `AvoidDuplicates=false` (`ScheduledBackupItem.ContentFilter`-Setter). |
| **Auslösebedingung** | `CloudDriveStorageService.PreUploadCheck` **vor** jedem Netz-Aufruf: (1) Knoten per Name+Parent (`GetNodeByNameAndParentIfExists`, exaktes `name = ?`) und `MD5 == md5 && status == AVAILABLE` → Duplikat; (2) **nur wenn dedup-berechtigt** (`UploadType` = `Deduped` oder `PhotosImport`, sonst `suppress = Deduplication`) zusätzlich `GetNodeByMd5(md5)` über **alle** Ordner → Duplikat. Treffer ⇒ `FileIgnoredException`/`FileIgnoredPreUploadException` ⇒ Status `Ignored`. |
| **Auf der Leitung** | Kein Request bei Treffer (rein lokal). Sonst wird `suppress=DEDUPLICATION` als Query-Parameter mitgesendet, **wenn nicht** dedup-berechtigt (`UploadRecordExtensions.GetSuppressFileUpload`), d. h. der Server soll für dieses File nicht selbst deduplizieren. Bei dedup-berechtigten Uploads entfällt `suppress`. |
| **Bei Server-409** | 409 mit `data.contentProperties.md5 == md5` **oder** `ErrorType ∈ {MD5_DUPLICATE, DuplicatesConflictError}` ⇒ als Duplikat normalisiert ⇒ intern 200, Knoten gecacht, `FileIgnoredException` ⇒ Status `Ignored` („Zuvor hochgeladen"). Kein Fehler. |
| **Unser Stand** | **Teilweise.** Toggle pro Ziel vorhanden: `src/folders.rs:166` (`dedup`, Default `true` via `default_true` `src/folders.rs:181`). Skip-Regel ist **pfadbasiert**: `src/dedup.rs:101` (`should_skip`, Pfad+MD5) und `src/dedup.rs:143` (`stat_matches`, Pfad+size+mtime); Index `ix_uploads_md5` existiert (`src/dedup.rs:63`), wird aber **nie** abgefragt. **Cross-Path-MD5 fehlt.** Aufruf: `src/upload.rs:416` (stat) und `src/upload.rs:463` (md5). |
| **Aufwand** | **S** (Index liegt bereits vor; nur eine zweite Query + Verdrahtung). |
| **Abnahmekriterium** | „Gleicher Inhalt unter **anderem Pfad** ⇒ kein zweiter Upload; ein Test mit zwei Pfaden und gleichem MD5 beweist den Skip ohne Netz." |

### 1.2 Mechanismus B — Namenskonflikt (die vier Optionen)

`Amazon.CloudDrive.Database.ManualUpload.ConflictResolution`: `None=0`, `Ignore=1`,
`SaveAsNew=2`, `Replace=3`. Spalte `schedules.conflict_resolution integer default 0`
(Migration 3, `SchedulesDatabase`); ein neuer `ScheduleRecord` setzt das Feld nie, also gilt
`None`. **Kein** globaler Namenskonflikt-Default.

| # | Interner Wert | Deutsches Label (aktueller Dialog) | Auslösebedingung (Client, vor dem Request) | Leitung | 409 | Unser Stand | Aufwand | Abnahmekriterium |
|---|---|---|---|---|---|---|---|---|
| B1 | `None` | `..._file_name_conflicts_ask_option` = **„Jedes Mal nachfragen"** | Zielknoten per Name+Parent **vorhanden** ⇒ `FileUploadNameConflictException` (kein Request); **kein** Zielknoten ⇒ normaler Upload | — (wirft vorher) | 409 mit `NAME_ALREADY_EXISTS`/`NameAlreadyExists` **und** abweichendem MD5 ⇒ `RetryForNameConflict` ⇒ Schedule erneut befragen | **Umgesetzt** als `ConflictResolution::Ask` (`src/folders.rs:134`), Wire `NONE` (`src/folders.rs:150`); Dialog `src/backup.rs:1109`, Auslöser erst bei 409 (`src/upload.rs:520`) | — (da) | „Ask ohne `Nicht noch einmal fragen` fragt pro Datei; mit Häkchen nur einmal pro Lauf." |
| B2 | `Ignore` | `..._ignore_option` = **„Neue Datei ignorieren"** | Zielknoten vorhanden ⇒ `FileSkippedException` **vor** dem Request ⇒ Status `Skipped` („Verworfen") | — (wirft vorher) | wie B1, aber Schedule `Ignore` ⇒ `FileSkippedException` | **Umgesetzt** (`src/folders.rs:136`, Default `#[default]`), Wire `NONE`; 409 ⇒ `FileOutcome::ConflictSkipped` (`src/upload.rs:597`, `src/upload.rs:685`) | — (da) | „Gleicher Name, anderer Inhalt, `Ignore` ⇒ kein Byte geht raus bzw. 409 wird als Skip gezählt." |
| B3 | `SaveAsNew` | `..._auto_rename_option` = **„Neue Datei automatisch umbenennen"** | Zielknoten vorhanden ⇒ Server soll umbenennen; Client setzt `overwrite=false` | `POST v2/upload?…&conflictResolution=RENAME` | 409-Name-Konflikt ⇒ `RetryForNameConflict` ⇒ erneut mit `RENAME` | **Umgesetzt** (`src/folders.rs:139`), Wire `RENAME` (`src/folders.rs:151`) | — (da) | „Namensgleichheit ⇒ Server legt umbenannte Kopie an; kein 409-Fehler." |
| B4 | `Replace` | `..._replace_option` = **„Alte Datei ersetzen"** | Zielknoten vorhanden ⇒ Client setzt `overwrite=true` **und** `nodeId=destinationNode` | `PUT v2/upload/<nodeId>?…&conflictResolution=NONE` (overwrite, **kein** `OVERWRITE`-Wert) | erzeugt normalerweise keinen 409 (PUT ersetzt) | **Umgesetzt** (2026-10-15, User-Auftrag) — `POST` mit `conflictResolution=NONE`, danach `PUT /v2/upload/<nodeId>`, aber **nur bei Namenskonflikt** (`NameAlreadyExists`/unbekannt/leer): `src/folders.rs:156`, Wire `:170`, `src/urls.rs:180`, Zweig `src/prime_photos.rs:1123`, `replace_picture` `:1202`, Dispatch `should_replace_after_conflict` `:358`. Ein 409-Inhalts-Duplikat wird **nicht** ersetzt (gleiche Bytes ⇒ verschwendete Bandbreite und „bereits im Konto"), sondern bleibt Conflict-Marker. **Abweichung:** wir kennen den Zielknoten erst aus der 409 (`errorDetails.conflictNodeIds[0]`), senden also zwei Requests statt einem direkten `PUT` — kein Knoten-Cache (bewusst, §5); `replace_upload_url` normalisiert die Node-ID **nicht** (Server-Wert) | — (da) | „Ersetzen überschreibt den vorhandenen Knoten statt eine Kopie anzulegen; ein Test belegt, dass danach genau ein Knoten mit diesem Namen existiert." |

**Legacy-Labelsatz** (`clouddrive_desktop_statuswindow_scheduleconfiguration_filenameconflict_*`:
„Ignorieren", „Ersetzen", „Als neue Dateien speichern", „Jedes Mal nachfragen") existiert nur
in der generierten `Strings`-Klasse, wird von **keiner** decompilierten View referenziert ⇒
unbenutzt/alte Fassung. Titel des aktuellen Blocks: `..._upload_settings_file_name_conflicts_title`
= **„Konflikte mit Dateinamen"**; ein weiterer, nur in `Strings` vorhandener Titel
`clouddrive_desktop_backupconfiguration_selectfilenameconflictresolutiontitle` = „Umgang mit
Dateinamenkonflikten:".

### 1.3 Der häufigste Fall: gleicher Name **und** gleicher MD5

Belegt aus `PreUploadCheck`: der Name+Parent-Lookup läuft **zuerst** und prüft direkt
`MD5 == md5`. Bei Treffer ⇒ `FileIgnoredException`/`FileIgnoredPreUploadException` ⇒ Status
`Ignored` — **Mechanismus A gewinnt**, Mechanismus B wird **nie** befragt. Kein Konflikt-Dialog,
kein Fehler; in der UI steht „Zuvor hochgeladen"
(`clouddrive_desktop_upload_statuswindow_statusignored`). Nur wenn der Name-Lookup **daneben**
greift (stale Cache) und die Datei dedup-berechtigt ist, fängt `GetNodeByMd5` denselben Fall
über den Inhalt. Kurz: **Inhalt schlägt Name; das Ergebnis ist ein Skip („Zuvor hochgeladen"),
kein Konflikt und kein Fehler.**

### 1.4 Mechanismus C — Apple-Photos-`.heic`-Heuristik (nur zur Vollständigkeit)

`FilterOutPotentialDuplicatesFromApplePhotosLibrary` (Kunden-Einstellung) bzw. der
Erst-Nutzungs-Prompt `HasCustomerSeenApplePhotosDuplicatesPrompt`: für `.heic` aus einer
Photos-Bibliothek prüft `PreUploadCheck` die ersten zwei Bytes auf einen JPEG-BOM und wirft
dann `FileIgnoredForPotientialDuplicationException` ⇒ Status
`IgnoredDueToPotentialConflictWithiOSUploads`, `TransferError.PreUploadCheckFailed`. Labels:
`clouddrive_desktop_preferences_filter_potentialduplicates_{title,description}`,
`clouddrive_desktop_iosdedupeprompt_{header,information}`. **Bei uns bewusst nicht** (§5).

---

## 2. Der Konflikt-Dialog

**Ort/Form.** Ein **modales Sheet** am Hauptfenster (`FileNameConflictWindowService`,
Tag `NameConflictModal`, `BeginSheet`), gefüllt aus `FileNameConflictViewModel`. Es erscheint
**nach einem abgeschlossenen Backup-Lauf**, nicht pro Datei-Download: `GlobalBackupTransferState`
zählt beim Übergang nach `Completed` die Konflikte (`CountFileNameConflicts`) und publiziert
`ResolveScheduleNameConflictsEvent`; der Handler zeigt das Modal **nur, wenn** die
Schedule-Auflösung `None` ist — sonst löst er die Konflikte still in der DB auf
(`ResolveNameConflictsForSchedule`).

**Inhalt (wörtlich):**

| Element | Key | Text |
|---|---|---|
| Titel (Sg./Pl.) | `..._filenameconflict_overlayview_title[_plural]` | „{0} Dateinamenkonflikt in {1}" / „{0} Dateinamenkonflikte in {1}" |
| Text | `..._overlayview_body[_plural]` | „Das Ziel hat {0} Datei mit demselben Namen. Möchtest du sie als neue Datei speichern oder die Datei ersetzen?" |
| Knopf 1 | `..._saveasnew_button_text[_plural]` | „Als neue Datei speichern" / „Als neue Dateien speichern" |
| Knopf 2 | `..._ignore_button_text` | „Ignorieren" |
| Knopf 3 | `..._replace_button_text` | „Ersetzen" |
| Checkbox | `..._donotaskagain_checkbox_text` | „Nicht noch einmal nach {0}-Ordner fragen" (`{0}` = Ordnername) |

**Knopf → Enum:** „Als neue Datei speichern" → `SaveAsNew`, „Ignorieren" → `Ignore`,
„Ersetzen" → `Replace` (kein „Jedes Mal nachfragen"-Knopf im Dialog — `None` ist der Zustand,
der den Dialog überhaupt erst auslöst).

**„Nicht noch einmal fragen" — wohin geschrieben:** Ist die Checkbox gesetzt, schreibt
`ApplyResolution` die gewählte `ConflictResolution` **in den Schedule-Record** dieser Sicherung
(`IMutableScheduleRecord.ConflictResolution` + `SaveScheduleRecord`) — also **pro Ziel** und
**dauerhaft** (in `schedules.conflict_resolution`). Unabhängig vom Häkchen werden **immer** die
offenen Konflikte dieses Schedules in der DB aufgelöst: `ResolveNameConflictsForSchedule`
setzt die betroffenen `backup`-/`uploads`-Zeilen von `Errored`/`FileConflict`/`RetryReason.Error`
auf `Skipped` (`Ignore`) bzw. `Waiting` mit `RetryReason.NameConflictSaveAsNew`/`…Replace`
(SaveAsNew/Replace). „Jedes Mal nachfragen" heißt also real: **einmal pro Ziel, bis der User
„nicht noch einmal" sagt** — danach ist die Wahl fest.

**Fenster zu:** Statt des Sheets erscheint eine Toast-Benachrichtigung
`clouddrive_desktop_filenameconflict_toast_title` = „Dateinamenkonflikte gefunden",
`..._toast` = „Öffne die App, um Konflikte zu lösen.", `..._toast_button` = „Öffnen"; gedrosselt
über `FileConflictNotificationThrottleHours`, abschaltbar über `FileConflictNotificationEnabled`,
mit `ForceShowNameConflictModal` als Override, der das Hauptfenster erzwingt.

---

## 3. Was wir heute haben (Beleg je Variante)

| Variante | Ist-Stand | Beleg |
|---|---|---|
| Dedupe-Toggle pro Ziel | vorhanden, Default **an** | `src/folders.rs:166` (`dedup`), `:181` (`default_true`); Dialog `src/backup_config.rs:119` |
| Dedupe-Skip gleicher Pfad+MD5 | vorhanden | `src/dedup.rs:101`; Aufruf `src/upload.rs:463` |
| Dedupe-Skip gleicher Pfad+size+mtime | vorhanden (schneller Pfad) | `src/dedup.rs:143`; Aufruf `src/upload.rs:416` |
| Dedupe **über Pfade hinweg** (MD5) | **umgesetzt** (`f3c773b`) — Treffer ⇒ „bereits im Konto", nicht „unverändert" | `src/dedup.rs:206` (`content_seen`), Aufruf `src/upload.rs:537`; Marker `Origin::ContentSkip` |
| 409 ⇒ Skip | vorhanden (jede 409 wird Skip; die Ursache entscheidet nur über den Index-Eintrag) | `src/prime_photos.rs:1123`, `src/upload.rs:597`/`:685` |
| 409-Ursache trennen (`DuplicatesConflictError` vs `NameAlreadyExists`) | **umgesetzt** (2026-10-15) — Marker trägt den `errorCode`; Inhalts-Duplikat ⇒ `record_content_skip` + „bereits im Konto", Namenskonflikt/unbekannt ⇒ unverändert | Marker `src/prime_photos.rs:294`, `conflict_cause` `:319`, `is_content_duplicate_code` `:333`, 409-Zweig `:1123`; Zweig `src/upload.rs:685` |
| B1 `Ask` | vorhanden (Dialog pro Datei) | `src/folders.rs:134`, `src/upload.rs:292` (`resolve_conflict`), `src/backup.rs:1109` |
| B2 `Ignore` (Default) | vorhanden | `src/folders.rs:136` (`#[default]`), `src/upload.rs:511` |
| B3 `SaveAsNew` | vorhanden (Wire `RENAME`) | `src/folders.rs:139`, `:151` |
| B4 `Replace` | **umgesetzt** (2026-10-15) — Wire `NONE`, danach `PUT /v2/upload/<nodeId>` bei Namenskonflikt | `src/folders.rs:156`, Wire `:170`; `src/urls.rs:180`; `src/prime_photos.rs:1123`, `:1202` |
| „Nicht noch einmal fragen" | vorhanden, aber **nur pro Lauf** (in-memory) | `src/upload.rs:98` (`conflict_remember`), `:327`; **nicht** persistiert |
| Konflikt-Dialog | vorhanden (egui-Fenster, pro Datei, 60-s-Timeout → Ignore) | `src/backup.rs:1109`–`:1154`; Timeout `src/upload.rs:297`/`:333` |
| Globaler Dedupe-Default (Voreinstellungen) | **fehlt** (jeder neue Ordner startet `dedup=true`) | `src/folders.rs:195` |
| Per-Media-Dedupe (Photos/Videos) | fehlt — wie im Original nicht in der UI | — |
| Apple-`.heic`-Heuristik | fehlt (bewusst) | — |

Ergebnis-Aufschlüsselung bei uns (bereits vorhanden, 2026-10-14): `uploaded_new` /
`conflict_skipped` / `unchanged_skipped` (`src/upload.rs:61`–`:63`) — der 409-Skip und der
lokale Skip sind schon getrennt.

---

## 4. Was fehlt (nach Aufwand sortiert)

1. **Cross-Path-Inhalts-Dedupe** — **S.**
   Abnahme: gleicher MD5 unter anderem Pfad ⇒ kein zweiter Upload; Test mit zwei Pfaden,
   gleichem MD5, ohne Netz. (Query auf dem vorhandenen `ix_uploads_md5`.)
2. **409-Ursache trennen** — **S. — erledigt 2026-10-15.**
   `errorCode` wird aus der 409-Antwort gelesen und im Konflikt-Marker mitgeführt
   (`src/prime_photos.rs:1123`, Marker `:294`–`:365`, 409 liest `ResponseUploadFail.errorCode`
   `:1128`). `DuplicatesConflictError`/`MD5_DUPLICATE` (case-insensiver Vergleich,
   `is_content_duplicate_code` `:333`) ⇒ weiterhin Skip, aber im MD5-Index als
   `Origin::ContentSkip` (`record_content_skip`) statt als echter Upload — andernfalls hätte
   der **nächste** Lauf „unverändert" gemeldet, obwohl von diesem Pfad nie etwas hochgeladen
   wurde (Verifier-Fund). `NameAlreadyExists`/`NAME_ALREADY_EXISTS` und jeder unbekannte/leere
   Wert ⇒ heutiges Verhalten unverändert (`record_upload`, „Vorhanden, übersprungen"): bei
   einem Namenskonflikt ist der Inhalt **nicht** im Konto, ein Content-Skip wäre falsch. Der
   `Ask`-Dialog bleibt unberührt.
   Umsetzung: Zweig `src/upload.rs:685`–`:748`, Ursache aus dem Marker `:586`/`:597`. Tests
   ohne Netz: `prime_photos::tests::conflict_marker_*`,
   `content_duplicate_codes_are_recognised_case_insensitively` und
   `upload::tests::the_conflict_cause_decides_the_index_origin`.
3. **„Nicht noch einmal fragen" pro Ziel persistieren** — **S.**
   Bei gesetztem Häkchen die Antwort in `BackupFolder.conflict_resolution` schreiben
   (`folders.toml`), wie das Original in `schedules.conflict_resolution`.
   Abnahme: nach „Ersetzen" + Häkchen steht in `folders.toml` `conflict_resolution = "replace"`
   und der nächste Lauf fragt nicht mehr.
4. **`Replace` auf der Leitung verifizieren** — **M** (braucht Live-Probe) — **erledigt 2026-10-15.**
   Live-Beleg (2026-10-15, Hauptagent): `conflictResolution=OVERWRITE` im POST wird von
   unserem Cookie-Endpunkt **nicht** akzeptiert ⇒ **404, leerer Body**. Der Arbeitsweg ist der
   des Originals: **`PUT /v2/upload/<nodeId>?conflictResolution=NONE`** ⇒ **200**, **gleiche
   Node-ID**, Inhalt **wirklich getauscht**.
   Umsetzung: `ConflictResolution::wire_value` liefert für `Replace` jetzt `NONE`
   (`src/folders.rs:170`, Test `every_conflict_resolution_maps_to_its_wire_value`); die
   PUT-URL entsteht ausschließlich in `src/urls.rs:180` (`replace_upload_url`, Test
   `replace_upload_url_is_zone_and_node_exact` mit Zone `eu`/`na`). Kommt vom POST ein 409
   **Namenskonflikt** (`NameAlreadyExists`, unbekannt oder leer — **nicht**
   `DuplicatesConflictError`/`MD5_DUPLICATE`) **mit** `errorDetails.conflictNodeIds[0]`, folgt
   in `upload_picture` der PUT mit **demselben Body und Header-Satz**
   (`src/prime_photos.rs:1123`, `replace_picture` `:1202`); 200 ⇒ `Ok(id)` aus der Antwort,
   sonst die Node-ID aus der URL (`replace_upload_id` `:376`, Tests `replace_upload_id_*`) —
   der Worker sieht für `Replace` **kein** 409 und ist unverändert. Die Dispatch-Entscheidung
   ist rein und steht als `should_replace_after_conflict` (`src/prime_photos.rs:358`, Test
   `replace_follows_a_name_conflict_but_never_a_content_duplicate`) mit der Matrix
   Namenskonflikt+ID ⇒ PUT / `DuplicatesConflictError`+ID ⇒ **kein** PUT / `MD5_DUPLICATE`+ID
   ⇒ **kein** PUT / leere ID ⇒ **kein** PUT / andere Policy ⇒ **kein** PUT.
   Ein **Inhalts-Duplikat** geht unter `Replace` **nicht** in den PUT: die Bytes liegen schon
   im Konto, der PUT würde sie identisch erneut senden (verschwendete Bandbreite) und als
   „neu hochgeladen" zählen — es bleibt der Conflict-Marker, den der Worker als „bereits im
   Konto" mit `Origin::ContentSkip` merkt (User-Entscheidung 2026-10-15). 409 **ohne** Node-ID
   (und jeder Nicht-409-Fehler) ⇒ unverändertes Fehlverhalten. Abnahme offen: der Live-Lauf
   mit `Replace` (Hauptagent) bestätigt genau **einen** Knoten mit diesem Namen.
5. **Globaler Dedupe-Default in den Einstellungen** — **S** (optional).
   Ein Config-Feld als Default für neue Ziele (Original: `EnableDeduplicationForPhotosAndVideos`).
   Abnahme: neues Ziel übernimmt den globalen Default statt hart `true`.
6. **Namenskonflikt clientseitig vor dem Request** — **L.**
   Das Original entscheidet aus einem lokalen Knoten-Cache (Name+Parent) und wirft für
   `Ignore`/`None` **vor** dem Request. Wir haben bewusst keinen Knoten-Cache (AGENTS.md); unser
   409-Weg ist das pragmatische Äquivalent. Nur sinnvoll, wenn Punkt 2 steht.
   Abnahme: entfällt bzw. „ein `Ignore`-Konflikt erzeugt null Requests" (nur mit Cache erreichbar).

---

## 5. Bewusst nicht

| Variante | Grund |
|---|---|
| **Per-Media-Dedupe** (`Photos`/`Videos`) | Das Original exponiert es selbst nie in der UI (nur toter Flag-Wert); unser Bool deckt die reale Oberfläche ab. |
| **Apple-Photos-`.heic`-JPEG-BOM-Heuristik** | Nur für den Import einer Apple-Photos-Bibliothek sinnvoll — laut AGENTS.md „Scope-Grenzen" außerhalb des MVP. |
| **Lokaler `cloud.db`-Knoten-Cache** | Bewusste Architekturgrenze; der Server-409 ist unser Ersatz für die clientseitige Namensprüfung. |
| **Onboarding-Dedupe-Default (an) vs. Post-Onboarding (aus)** | Unser Default ist **an** (freundlicher); als bewusste Abweichung dokumentiert (AGENTS.md). |
| **Konflikt-Default `None` („Jedes Mal nachfragen")** | Unser Default ist **`Ignore`** (bewusst, AGENTS.md) — wer ihn auf „Ask" „korrigiert", ändert eine getroffene Entscheidung. |

---

## 6. UNKNOWN

* **~~Unsere 409-`errorCode`-Vokabeln~~ — gelöst (2026-10-15).** Live-Beleg gegen unseren
  Cookie-Flow (`content-<zone>.drive.amazonaws.com/v2/upload`): das Inhalts-Duplikat
  antwortet mit `DuplicatesConflictError`, der Namenskonflikt mit `NameAlreadyExists` —
  **beide** mit `errorDetails.conflictNodeIds` (der vorhandene Knoten). `MD5_DUPLICATE`
  (die zweite Schreibweise des Originals) tritt bei uns nicht auf, wird aber aus
  Vorsicht gleich behandelt. Umsetzung: `src/prime_photos.rs:333`, `src/upload.rs:685`.
* **~~Akzeptanz von `conflictResolution=OVERWRITE`~~ — gelöst (2026-10-15).** Live-Beleg gegen
  unseren Cookie-Flow (`content-<zone>.drive.amazonaws.com/v2/upload`): der POST mit
  `conflictResolution=OVERWRITE` wird **abgewiesen** — **404, leerer Body**. Der Wert geht von
  uns deshalb **nicht mehr** auf die Leitung (`ConflictResolution::wire_value` sendet für
  `Replace` `NONE`, `src/folders.rs:170`); der Arbeitsweg ist der des Originals —
  `PUT /v2/upload/<nodeId>?conflictResolution=NONE` (200, gleiche Node-ID, Inhalt getauscht),
  umgesetzt in `src/prime_photos.rs:1123`/`:1202` (nur Namenskonflikt).
* **Ob `GetNodeByMd5`-Dedupe serverseitig zusätzlich gegated ist** (Weblab o. Ä.) — nur die
  clientseitige Gating (`UploadType`) wurde beobachtet.
* **Ob der Server einen dedup-berechtigten Upload am `suppress`-Weg anders behandelt** —
  der Client setzt `suppress=DEDUPLICATION` nur für **nicht** dedup-berechtigte Uploads; die
  Serverwirkung ist nicht belegt.
* **Genaue Knopf→Ergebnis-Zuordnung des iOS-Dedupe-Prompts** — aus `docs/SPEC_DEDUP.md` §9
  übernommen, hier unverändert UNKNOWN.
* **Suchweg (reproduzierbar):** vollständiger `ilspycmd`-Decompile von `Amazon Photos.dll`;
  Greps nach `ConflictResolution`, `DeduplicateOptions`, `CloudDriveConflictResolution`,
  `NameConflictStrategy`, `GetConflictResolution`, `GetServiceValue`, `PreUploadCheck`,
  `HandleNodeResponse`, `RetryForNameConflict`, `FileUploadNameConflictException`,
  `ResolveNameConflictsForSchedule`, `CountFileNameConflicts`, `AvoidDuplicates`,
  `EnableDeduplicationForPhotosAndVideos`, `DefaultOnboardingDedupeChecked`; deutsche Labels
  per `grep -iE` über `de.lproj/Localizable.strings` für
  `conflict|duplikat|überspringen|nachfragen|ersetzen|neue datei|bereits|filenameconflict`.

---

## 7. Belege (Symbole, Keys, Dateien — keine Codekopien)

* **Enums:** `Amazon.CloudDrive.Database.ManualUpload.ConflictResolution` (`None`, `Ignore`,
  `SaveAsNew`, `Replace`), `…DeduplicateOptions` (`None=0`, `Photos=2`, `Videos=4`,
  `PhotosAndVideos=6`), `…UploadType` (`Normal`/`PhotosImport`/`Deduped`/`Reupload`),
  `…RetryReason` (`NameConflictSaveAsNew`, `NameConflictReplace`, `Reupdate`);
  `Amazon.CloudDrive.Services.CloudDriveConflictResolution` (`None`/`Rename`/`RenameExisting`);
  `Amazon.CloudDrive.Services.CloudDrive.SuppressFileUpload` (`Deduplication`/`Processing`/
  `Transcoding`/`All`); `Amazon.CloudDrive.Database.TransferError` (`FileConflict=24`,
  `PreUploadCheckFailed=51`, `CloudDriveNodeConflict=194`), `…TransferStatus`
  (`Skipped`/`Ignored`/`Errored`/`IgnoredDueToPotentialConflictWithiOSUploads`).
* **Wire:** `CloudDriveConflictResolutionExtensions.Values` (`NONE`/`RENAME`/`RENAME_EXISTING`);
  `UploadRequest` (`[QueryParameter]` für `name`/`fileSize`/`conflictResolution`/`suppress`/…,
  `NodeId` **ohne** Query-Attribut); `CloudDriveService.UploadFile` (`PUT` ⇒
  `v2/upload/<nodeId>`, `POST` ⇒ `v2/upload`); `SimpleUploader`/`LegacyUploader`/
  `MultipartUploader.InitiateMultipartUpload`.
* **Entscheidung/Erkennung:** `CloudDriveStorageService.PreUploadCheck`, `…CloudNodeCheck`,
  `…UploadViaSimpleOrLegacyUploader` (der Options-Switch), `…HandleNodeResponse` (nur
  `MD5 == md5`), `…HandleNodeResponseSync` (zusätzlich `MD5_DUPLICATE`/`DuplicatesConflictError`);
  `UploadHelper.RetryForNameConflict` (`NAME_ALREADY_EXISTS`/`NameAlreadyExists`, MD5 muss
  **abweichen**); `NameConflictStrategy.GetConflictResolution` / `…GetScheduleConflictResolution`;
  `UploadRecordExtensions.GetConflictResolution` (Autosave ⇒ `None`) / `…GetSuppressFileUpload`;
  `CloudDriveDatabase.GetNodeByNameAndParentIfExists` / `…GetNodeByMd5`;
  `System.ExceptionExtensions.TryParseErrorType` / `…ToTransferError`.
* **Zustand:** `ScheduleRecord` (`[Column("conflict_resolution")]`, `[Column("dedupe_photos_videos")]`),
  `SchedulesDatabase` (Migration 3: `add column conflict_resolution integer default 0`),
  `ManualUploadDatabase.CountFileNameConflicts` / `…ResolveNameConflictsForSchedule` /
  `…TransferStatusForConflictResolution` / `…RetryReasonForConflictResolution`.
* **Defaults/Settings:** `AppSettings.DefaultOnboardingDedupeChecked` (`true`) /
  `…DefaultPostOnboardingDedupeChecked` (`false`) / `…DefaultOnboardingContentFilter`;
  `CustomerSettings.EnableDeduplicationForPhotosAndVideos` (Default =
  `DefaultPostOnboardingDedupeChecked`) / `FilterOutPotentialDuplicatesFromApplePhotosLibrary` /
  `HasCustomerSeenApplePhotosDuplicatesPrompt` / `ForceShowNameConflictModal`;
  `AppSettings.FileConflictNotificationEnabled` / `…FileConflictNotificationThrottleHours`;
  `ScheduledBackupItem.AvoidDuplicates` / `ConflictResolution` / `ContentFilter`-Setter
  (`Everything` ⇒ `AvoidDuplicates=false`); `ScheduledBackupItemExtensions.ToScheduleRecord`.
* **UI:** `BackupConfigurationViewController` (`BackupView_SettingsAvoidDuplicates_Checkbox`,
  `BackupView_FileNameConflictResolution_{SaveAsNew,Ignore,Replace,AskMeEveryTime}_RadioButton`,
  `BackupView_FileNameConflictResolution_Text`), `FileNameConflictViewModel`,
  `FileNameConflictWindowService` (Tag `NameConflictModal`), `FileNameConflictViewController`,
  `ResolveScheduleNameConflictsEventHandler`, `GlobalBackupTransferState.PublishScheduleWithConflictsCompletedEvent`,
  `PreferencesViewModel.EnableDeduplicationForPhotosAndVideos`.
* **Deutsche Labels (`de.lproj/Localizable.strings`):**
  `clouddrive_desktop_upload_settings_skip_duplicates_option_{title,subtitle}`,
  `clouddrive_desktop_statuswindow_scheduleconfiguration_dedupeoption` (Legacy),
  `clouddrive_desktop_upload_settings_file_name_conflicts_title`,
  `…_file_name_conflicts_{auto_rename,ignore,replace,ask}_option`,
  `clouddrive_desktop_statuswindow_scheduleconfiguration_filenameconflict_{askmeeverytime,ignore,replace,saveasnewfile}` (Legacy, ungenutzt),
  `clouddrive_desktop_backupconfiguration_selectfilenameconflictresolutiontitle`,
  `clouddrive_desktop_filenameconflict_overlayview_{title[_plural],body[_plural],saveasnew_button_text[_plural],ignore_button_text,replace_button_text,donotaskagain_checkbox_text}`,
  `clouddrive_desktop_filenameconflict_toast{,_title,_button}`,
  `clouddrive_desktop_upload_statuswindow_statusignored` („Zuvor hochgeladen") / `…statusskipped` („Verworfen"),
  `clouddrive_desktop_preferences_filter_potentialduplicates_{title,description}`,
  `clouddrive_desktop_iosdedupeprompt_{header,information}`.
* **Unser Code:** `src/folders.rs` (`ConflictResolution` `:136`, `wire_value` `:166`, `dedup`
  `:185`, `default_true` `:199`), `src/dedup.rs` (`should_skip` `:101`, `stat_matches` `:143`,
  `record_upload` `:175`, `ix_uploads_md5` `:63`), `src/upload.rs` (`FileOutcome` `:279`,
  `resolve_conflict` `:292`, Stat-Skip `:416`, MD5-Skip `:463`, Konflikt-Schleife `:496`,
  `ConflictSkipped` `:685`, Zähler `:61`), `src/prime_photos.rs` (`CONFLICT_PREFIX` `:294`,
  `conflict_error` `:303`, `conflict_node_id` `:308`, `conflict_cause` `:319`,
  `is_content_duplicate_code` `:333`, `should_replace_after_conflict` `:358`,
  `replace_upload_id` `:376`, 409-Zweig `:1123`, `wire_value`-Aufruf `:996`,
  `replace_picture` `:1202`), `src/urls.rs` (`upload_url`,
  `replace_upload_url` `:180`), `src/backup_config.rs` (`:119`, `:126`), `src/backup.rs`
  (`conflict_dialog` `:1109`), `src/i18n.rs` (Konflikt-/Dedupe-Strings).
* **Cross-Referenz:** `docs/SPEC_DUPLICATES.md`, `docs/SPEC_DEDUP.md`,
  `docs/SPEC_ONE_TIME_UPLOAD.md`, `docs/PROTOCOL.md`, `docs/SPEC_FILENAME_ENCODING.md`.

---

## 8. Korrekturen an den vorhandenen Docs

1. **`SPEC_DUPLICATES.md` §4 ordnet die `ErrorType`-Prüfung dem falschen Handler zu.** Die
   ErrorType-Werte `MD5_DUPLICATE`/`DuplicatesConflictError` werden **nicht** in
   `HandleNodeResponse` geprüft, sondern nur in **`HandleNodeResponseSync`**; der asynchrone
   `HandleNodeResponse` prüft ausschließlich `data.contentProperties.md5 == md5`. Der
   Name-Konflikt-`ErrorType` heißt in beiden Schreibweisen **`NAME_ALREADY_EXISTS`** und
   **`NameAlreadyExists`** (der Retry-Helfer `RetryForNameConflict` akzeptiert beide und
   verlangt zusätzlich einen **abweichenden** MD5).
2. **`SPEC_DUPLICATES.md` §1.3/§5 unterschlägt, dass die Schedule-Wahl bei Backup-Uploads
   nie als `conflictResolution` auf die Leitung geht.** `UploadRecordExtensions.GetConflictResolution`
   liefert für Autosave/Backup **immer `None`**; nur `SaveAsNew` überschreibt auf `Rename`
   (`RENAME`). `Replace` bleibt `NONE` und wird über **`overwrite=true` + `nodeId`** als
   **`PUT v2/upload/<nodeId>`** ausgeführt — es gibt keinen `OVERWRITE`-Wert.
3. **`SPEC_DUPLICATES.md` §2.2/§5 lässt den „vor dem Request"-Charakter offen.** Für `Ignore`
   und für `None` mit vorhandenem Zielknoten wirft der Client **vor** jedem HTTP-Aufruf
   (`FileSkippedException` bzw. `FileUploadNameConflictException`) — diese beiden Optionen
   erzeugen also nie einen 409. Ein 409 entsteht nur bei stale Cache (Client sah den Knoten
   nicht) bzw. auf dem zweiten, servergetriebenen Pfad (`RetryForNameConflict`), der den
   Schedule erneut befragt (Ignore ⇒ Skip, SaveAsNew ⇒ erneut mit `RENAME`).
