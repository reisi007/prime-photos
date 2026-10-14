# Prime Photos

Cross-platform Rust desktop app (egui) for uploading photos to Amazon Photos.
Primärtarget ist **macOS (Apple Silicon)**; **Windows und Linux sind vollwertige Ziele**
(User-Entscheidung 2026-10-13: „Jetzt voll cross-platform portieren" — Kehrtwende vom
früheren reinen macOS-Ziel, siehe „Plattform / Portierung").

## Source of truth

- GUI: `src/main.rs` (egui/eframe), theme in `src/theme.rs`
- Upload engine: `src/prime_photos.rs` (`PrimePhotos`, cookie-based upload API)
- Persistent backup folders: `src/folders.rs` -> `~/Library/Application Support/prime_photos/folders.toml`
- Login: `src/login.rs` (embedded wry WKWebView as a child view of the main window; cookies captured with an asynchronous main-queue probe)
- Config: `src/config.rs`, stored at `~/Library/Application Support/prime_photos/config.toml`
- Protocol notes: `docs/PROTOCOL.md`

## Projektregeln

- **Name & Branding (Projektregel, 2026-10-13):** Projekt und git-Repo heißen **„Prime Photos"**.
  Nichts im Projekt darf den Eindruck erwecken, es sei eine offizielle App von Amazon: kein
  „Amazon Photos" als *unsere* App-/Produkt-/Bundle-/Wortmarken-Identität, keine Amazon-Logos oder
  Markenauftritte in der Chrome/UI des eigenen Fensters. **Zulässig bleibt alles funktional
  Notwendige:** echte Hosts/URLs (`amazon.de`, `www.amazon.de`, `amazonaws.com`, `upload-photos…`),
  Header-/Cookie-Namen (`x-amzn-*`), und Hinweise wie „Bei Amazon anmelden"/„Mit Amazon anmelden"
  (dort wird tatsächlich angemeldet). In Docs, die die Original-App beschreiben, bleibt die
  faktische Nennung des Referenz-Produkts erlaubt (das ist Recon eines echten Produkts); nur
  **unser** Projekt/Code/UI darf sich nirgends als offizielle Amazon-App darstellen.
- **Subagenten führen nichts Neues ein (Projektregel, 2026-10-13):** Jeder Auftrag ist strikt auf die
  beauftragte Änderung begrenzt — keine zusätzlichen Features, Dateien, Abhängigkeiten, Refactorings
  oder „Verbesserungen" auf Verdacht. Unklarheiten werden zurückgemeldet, nicht eigenmächtig gelöst.

## Decisions

- Name/Rename (user decision 2026-10-13): project, git repo and app are **"Prime Photos"**. Cargo package/binary `prime-photos`; upload module `src/prime_photos.rs` with struct `PrimePhotos`; config dir `~/Library/Application Support/prime_photos/` (was `amzn_photos_uploader`); the project directory is renamed to `prime-photos`. The old config folder is migrated **once, locally, by hand** (no migration code in the app).
- GUI framework: egui/eframe (native ARM, single binary).
- Auth (GUI): embedded WKWebView login, **in the same window as the app** (like the original). egui's window hosts a wry `WKWebView` child view; no extra window, no external browser, no CDP. Amazon's sign-in page is loaded directly (`/ap/signin?…` with `assoc_handle=amzn_photos_web_<cc>`). Cookie capture must **not** run inside egui's `update`: wry's `WebView::cookies` is synchronous and spins the macOS run loop, which re-enters winit's event handler and panics — so probes are dispatched to the main queue and the result is picked up on the next frame. A country/marketplace selector picks the TLD (default from system locale on first run).
- Auth (login chrome): deliberate deviation from the measured original (`docs/UI_FLOWS.md` §2.1). While Amazon's page is open, a slim branded header (our upload logo + "Prime Photos" + "Abbrechen") sits above the webview and the window title reads "Login – Prime Photos". Reason: the app is a third-party tool, not Amazon's own, so its identity and a way to cancel must stay reachable. The measured window sizes stay the benchmark for the main app window.
- Auth (upload engine): browser cookies (session-id + x-acb/at-acb/ubid-acb) -> `content-<zone>.drive.amazonaws.com/v2/upload`.
- Flow note: this is the **web** flow (cookie-based). The desktop client's **device** flow (`amzn_photos_desktop_*` signin -> `/auth/register` -> refresh token -> `/auth/token` -> `upload-photos.amazon.de/v2/upload`) is documented in `docs/openapi.yaml` but not used.
- UI reference: `docs/UI_FLOWS.md` + `docs/UI_STATES.md` (clean-room spec from the decompiled app + resources). Use them when changing the UI.
- Folder reconciliation (observed in the client's local DBs under `~/Library/Application Support/Amazon Cloud Drive`): local SQLite state, not an HTTP call. `backup` table (schedule_path, relative_path, node_id, size, modified_date, status, last_scan_time) + `uploads` table (path, status, precomputed_md5, node_id) + `cloud.db` node cache, synced via `/drive/v1/changes` and `/drive/v1/nodes`. `BackupScanIntervalInHours = 336`.
- Desktop-client upload protocol (observed): `POST https://upload-photos.amazon.de/v2/upload?name=<urlenc>&fileSize=<n>&conflictResolution=NONE&kind=FILE&parentNodeId=<id>` with raw body, `x-amz-access-token`, `x-amzn-file-md5` (hex), `Content-MD5` (base64), `x-amz-clouddrive-appid`, `x-amzn-clouddrive-source`. Not used by the app yet (we use the cookie path); documented for parity.
- Desktop auth chain (observed): `POST api.amazon.com/auth/token` (`source_token_type=refresh_token` -> `access_token`) -> `GET drive.amazonaws.com/drive/v2/account/endpoint` -> `POST cdws.../drive/v1/account/source`.
- Supported photos: jpg, jpeg, png, heic, heif, tif, tiff, gif, webp, bmp plus common RAW (cr2, cr3, arw, nef, dng, raf, rw2, orf, srw, pef, and others). `.xmp` sidecars are explicitly rejected.
- Folder scans enforce `is_supported_image` (single source in `folders.rs`). There is **no file picker** in HEAD (only `FileDialog::pick_folder()`) and `supported_extensions()` is currently unwired.
- Uses reference implementation from entourloop/amazon-photo-uploader (MIT) for API details.
- GUI language: German (matches the original Amazon Photos UI).
- "Speichernutzung"/storage bar is intentionally omitted for now: it only applies to Prime members and needs a real quota fetch; no placeholder is shown.
- Account/avatar block in the sidebar: when signed in, show the Amazon profile picture (no API identified yet — placeholder until then) plus "Angemeldet als:" and "Abmelden"; when **not** signed in, an icon replaces the picture. The green/red status color coding is kept deliberately (user likes it).
- MVP scope (user decision 2026-10-13): login, upload-target creation ("Upload-Ziel erstellen": backup configuration with destination, content filter, interval, dedup, conflict handling), upload flow, settings dialog. The download flow is relevant but explicitly NOT part of the MVP.
- Dedup (user decision 2026-10-13): a **persistent local MD5 index in SQLite** (like the original) decides skips (same path + same MD5 → skip; same path + different MD5 → re-upload; new file → upload). Server 409 is only a safety net, treated as "Ignored/Zuvor hochgeladen", not an error (mirrors the original's client-side `PreUploadCheck`).
- Conflict handling (user decision 2026-10-13): all **four original options** per upload target (Nachfragen / Ignorieren / Als neue Datei speichern / Ersetzen), mapped to `conflictResolution` (`NONE`/`RENAME`/`OVERWRITE`). Replaces the current hard-coded `RENAME`.
- Backup configuration window (user decision 2026-10-13): build the full "Datensicherungseinstellungen" window now (destination, content filter, interval, dedup, conflict handling, save/cancel), and **share the form logic** with the create-backup flow (same dialog for create and edit) instead of a separate row-menu. Fields whose backend does not exist yet (destination, content filter, interval) are shown **disabled** with a "Noch nicht verfügbar" hover (user decision 2026-10-13: "volle Anordnung, Rest ausgegraut"); functional now: source folder (read-only when editing), dedup, name-conflict policy.

## Upload-Queue & Concurrency (User-Entscheidung 2026-10-13)

- **Globaler Pool bleibt** (status quo): alle in einem Lauf ausgewählten Sicherungsziele werden zu
  **einer** gemeinsamen Warteschlange, die sich maximal `concurrency` (1..8, Einstellung) Arbeiter
  teilt; mehr als 8 **gesamt** macht keinen Sinn.
- **Pro-Ziel-Limit ist nur „nicht mehr als global"** und erst **Post-MVP**: ein optionaler,
  persistent gespeicherter Wert (0/None = globaler Default; nicht größer als global) wie bereits
  bei Dedup/Conflict vorbereitet. Im MVP bewusst **nicht** umgesetzt.
- **Netzwerkabhängigkeit (MVP):** „Slow-Mode" als temporäre, **nicht** persistierte
  Run-Zeit-Einstellung (`PrimePhotoApp.slow_mode`, Checkbox in Einstellungen → Upload), die
  für den aktuellen Lauf `concurrency = 1` erzwingt; bei App-Neustart zurückgesetzt. Automatische
  Drosselung anhand des Netzwerks/SSID/Namen ist **nachgeordnet** (Plattform-APIs, Post-MVP).

## Assets: Tray-Icons + Logo (User 2026-10-14) — geplant, **nicht** implementiert

Eigene Assets des Users liegen künftig in **`assets/`** (PNG mit Alpha) und werden per
`include_bytes!` + die vorhandene `image`-Crate eingebunden (keine neue Dependency, kein
Laufzeit-Pfad, self-contained Binary). Konkrete Dateiliste + Formatvorgaben steht im Ledger
(`AGENTS.todo.md` → „Assets"). **Geliefert (2026-10-14):** vollständiger Tray-Satz
(`tray`/`tray-active`/`tray-paused`/`tray-done`/`tray-error`, je 32 px + `@2x`, monochrom + Alpha
= macOS-Template) und Logo (`logo.png` 1024², `logo-small*.png`). Nur die Laufzeit-Dateien werden
eingebettet; die QA-Artefakte in `assets/` (`chk-logo-*`, `contact-sheet`, `zoom-corners`,
`debug-mask-*`, `preview-tray`) gehören nicht in den Git-Bestand. Einbindungspunkte existieren
bereits (`src/tray.rs`: `with_icon_templated` auf macOS, `with_icon` sonst, `ICON_SIZE = 32`).
**Windows/Linux-Tray (User-Entscheidung 2026-10-14): weiße Varianten + Theme-Erkennung.** Der User
liefert die fünf Zustände zusätzlich in weiß (`tray-*-white.png` + `@2x`, gleiche Maße); die App
wählt per Theme-Erkennung (Windows `AppsUseLightTheme`, Linux GTK `color-scheme`) weiß bei dunklem,
schwarz bei hellem Theme — **Fallback bei fehlgeschlagener Erkennung: das farbige `logo.png`**, das
auf jedem Hintergrund sichtbar ist. macOS bleibt Template (System invertiert selbst). Verworfen:
farbiges Logo generell (verliert die Zustände) und „bewusst akzeptieren". Umsetzung erst, wenn die
weißen Dateien vorliegen — Assets sind per Vorgabe compile-pflichtig.
**Kein Fallback (User 2026-10-14):** fehlt ein Asset, ist das ein **Compile-Fehler** — ohne
`Option`, ohne `exists()`-Prüfung, ohne Rückfall auf die gezeichnete Variante (`src/icons.rs`
bleibt nur für die Vektor-Glyphen im UI selbst: Navigation, Status-Punkt, Flagge). Dekodier-Fehler
zur Laufzeit = fail fast, keine stille degradierte UI.

MVP-Zielbild des Users: **polished** — Autostart (Default **an**, siehe §Autostart), effizienter
funktionierender Upload (Event-Watcher + Stat-Check + Wasserzeichen) und echte Tray-/Logo-Assets.

## Autostart (User-Entscheidung 2026-10-14) — Default **an**

Der Login-Item-Schalter „Nach dem Anmelden starten" ist bei erster Nutzung **ein** (PORT-4 baute
nur den Schalter, Default aus). Begründung: ohne Login-Item läuft nach Neustart/Ausloggen **kein
Watcher** — die Event-Erkennung würde schlafen (Wasserscan ist nur Sicherheitsnetz). Das Original
schaltet Autostart beim Onboarding ebenfalls selbst ein (`docs/SPEC_OS_INTEGRATION.md` §3.6).
Umsetzung (**erledigt**, Commit `7b8f3d1`, zwei Verify-Runden): `Config.autostart_enabled:
Option<bool>` (`None` = nie entschieden ⇒ einmal registrieren + `Some(true)` speichern; `Some(false)`
= Veto, nie einschalten, ein von Hand angelegter Eintrag wird nicht bekämpft; `Some(true)` ⇒
fehlendes Login-Item wird repariert). Der OS-Zustand bleibt die operative Wahrheit, `Config`
speichert nur die Absicht. Details: `AGENTS.todo.md` → „Zurückgestellt" #2.

## Geplante Implementierungs-Wellen (User-Auswahl 2026-10-14) — dokumentiert, **nicht gestartet**

Der User hat die nächsten Wellen ausgewählt, aber ausdrücklich **keinen Startauftrag** erteilt
(„nur dokumentieren, aber nicht starten"). Details und Begründungen je Punkt stehen im Ledger
(`AGENTS.todo.md` → „Implementierungs-Wellen"). Reihenfolge bei Start:

1. **P2 + P3** — Drag&Drop-Dateien temporär in der SQLite (Überleben Neustart, Löschen nach
   vollständigem Upload; kein Watch, kein Wasserzeichen) + **eine** aggregierte Fortschrittsanzeige
   über **alle** Backup-Ziele inkl. One-Time-Zeile (Fußzeile, auch im Leerlauf).
2. **Echter Name + Profilbild** im Account-Block — Endpunkt belegt
   (`GET <metadataUrl>/drive/v1/account/preferences` → `name` + `avatarNodeId`, 60×60-Crop,
   Cache `ThumbnailsPath/<nodeId>_60_60.jpeg`, Refresh Start + 24 h + on demand). Harte
   Voraussetzung: ein Test muss zeigen, dass unser Cookie-Pfad dort akzeptiert wird; sonst bleibt
   der Block wie heute.
3. **About / Third-Party-Lizenzen** — Voraussetzung für ein öffentliches Release.

Weitere offene Punkte (Queue-Paginierung, Fehlerfenster, Pause pro Ordner, Sortierung/Filter,
Tray-Status-Popup, Pro-Ziel-Limit, Drosselung, Logout im Upload) sind im Ledger
dokumentiert, aber **nicht** eingeplant. Regel bleibt: „Anlegen bleibt Pflicht — Abarbeiten nicht";
ein Listeneintrag allein löst keinen Umsetzungs- oder Verify-Lauf aus.

## Erkennung neuer/geänderter Dateien (User-Entscheidung 2026-10-14)

- **Primärer Mechanismus: OS-Dateisystem-Events, kein Polling.** Die 15/90-Minuten-Runde
  (`background_sync`: erster Tick 15 min, dann 90 min) ist **nicht** MVP-fertig und entfällt als
  Hauptpfad. Pro überwachtem Backup-Ordner ein OS-Watch über eine plattformneutrale Crate
  (macOS FSEvents / Windows ReadDirectoryChangesW / Linux inotify).
  Original-Parameter (`docs/SPEC_BACKGROUND_SYNC.md` §6, belegt): FSEvents-Latenz 3 s, Throttle
  100 ms, `Throttle(FiveSeconds)`-Debounce, Poll `BackupWatcherPollIntervalInSeconds` 30 s,
  Re-arm bei Fehler, pausierte Ziele bekommen keinen Watcher.
- **Reaktion pro Event: „file add / edit per Event abfragen"** (User-Vorgabe) — nur die betroffene
  Datei prüfen, kein kompletter Ordner-Durchlauf und kein erneutes Hashen aller Dateien.
- **Stat-Check vor dem Hash:** die Skip-Entscheidung fragt zuerst (path, size, mtime) gegen den
  persistierten Index; nur unbekannte/geänderte Dateien werden gelesen und MD5-gehasht. `size` und
  `mtime` stehen in der Tabelle bereits, blieben aber ungenutzt (`src/dedup.rs`). Das MD5 bleibt die
  Identität (`docs/SPEC_DEDUP.md` §1); der Stat-Treffer ist nur der schnelle Pfad.
- **Kaltstart (Event-Ersatz fürs Ausschalten):** beim Start bzw. nach Anmeldung wird **einmal** pro
  Ordner enumeriert und per Stat-Check entschieden, was fehlt. Kein Timer.
- **Sicherheitsnetz beim Start (User-Entscheidung 2026-10-14):** pro Backup-Ordner wird das
  **größte Änderungsdatum** (`max mtime`) des letzten Durchlaufs **in der SQLite gespeichert**
  (neue Tabelle, neben dem MD5-Index). Beim Start genügt ein **reiner Metadaten-Durchlauf**
  (`walkdir` ohne Lesen der Dateien): ist das aktuelle `max mtime` **nicht größer** als das
  gespeicherte, ist der Ordner nachweislich unverändert und es wird **nichts** gehasht, keine
  DB-Abfrage pro Datei, kein Upload-Plan. Wird es größer, läuft nur für diesen Ordner der
  Stat-Check-Pfad (und nur geänderte Dateien werden gelesen + gehasht).
  **Angeborene Grenze (wie im Original, `SPEC_CHANGE_DETECTION.md`):** Kopien, die die mtime
  erhalten (`rsync --times`, Finder-Import), erhöhen `max mtime` nicht. Wer das braucht, startet
  manuell — der Wasserzeichen-Treffer ist der schnelle Pfad, keine Garantie.
- **Drag & Drop ist ein einmaliger Upload** (User-Klarstellung 2026-10-14): kein Watch, kein
  Wasserzeichen, keine dauerhafte Sicherung — nur die temporäre Queue bis zum Abschluss
  (siehe unten).
- **Pause blockt auch Events** (globale Pause, siehe oben).
- **Event-Puffer: unbegrenzt, kein Verwerfen (User-Entscheidung 2026-10-14).** Events, die während
  eines laufenden Uploads **oder einer Pause** eintreffen, werden geparkt und beim nächsten
  Durchlauf vollständig abgearbeitet. Es gibt **keine Obergrenze** (die frühere 500er-Kappe
  `WATCH_PENDING_MAX` ist entfernt): die Liste wächst dynamisch mit der Zahl der **verschiedenen**
  geänderten Pfade (~1,2 MB bei 10.000, ~12 MB bei 100.000 — vernachlässigbar), Dedup per
  `HashSet<PathBuf>` in O(1) statt vorher O(n²). Bursts von **10.000+** Dateien (Kamera-Import,
  Ordner-Kopie, lange Pause) sind damit vollständig abgedeckt; verworfen wird nichts.
  Ein neuer Burst **ersetzt** den geparkten Bestand nicht mehr, sondern wird mit ihm **vereinigt**
  (das war ein echter Verlust-Bug: bei mehreren Bursts während einer Pause überlebte nur der
  letzte); erlaubt `can_start` einen Lauf, geht der vereinigte Bestand in **einen** Lauf.
  Umsetzung: `src/backup.rs` (`park_paths`, `pending`/`pending_seen`), Commit `2eea96c`, Verify
  Runde 1 APPROVE.
- **Verworfen:** der periodische Hintergrund-Scan als Erkennungsmechanismus (User 2026-10-14:
  „file add / edit per event abfragen"); ein `BackupScanIntervalInHours`-Takt.
- **Offen:** Sicherheitsnetz für verpasste Events (Watcher-Limit, ausgeworfenes Laufwerk,
  Netzwerk-Mount) — als optionaler Schalter, noch nicht entschieden.
- **Doku:** `docs/SPEC_CHANGE_DETECTION.md` (Clean-Room recon, in Arbeit).

## Pause, One-Time-Queue & Gesamt-Fortschritt (User-Entscheidung 2026-10-14)

- **Stoppen/Fortsetzenist ein Umschalter** (wie im Original, `UI_STATES.md` §70): „Alle
  Sicherungsvorgänge anhalten" ↔ „… fortsetzen" in der Backup-Kopfzeile plus derselbe Eintrag im
  Tray. Kein zweiter Button.
- **Pause wie Original (Entscheidung 2026-10-14):** der Zustand wird in `config.toml` gespeichert
  (`paused`, serde-default `false`, lädt also eine alte Config) und beim Start daraus
  initialisiert. Der Button ist **immer** schaltbar, sobald mindestens ein Sicherungsziel existiert
  **oder** ein Lauf läuft — früher nur während eines laufenden Laufs, was (a) Pause im Leerlauf
  verhinderte und (b) einen reinen Drag&Drop-Lauf ohne angelegte Ordner sperrte. Eine gesetzte
  Pause blockt auch den Hintergrund-Sync (Guard existierte bereits). **Pause pro Ordner** bleibt
  Post-MVP (Original hat ein Zeilen-Symbol).
- **One-Time-Queue (Drag&Drop) wird temporär in SQLite gespeichert** (Entscheidung 2026-10-14): die
  abgelegten Dateien landen in derselben SQLite, in der der MD5-Dedup-Index liegt, und werden
  **nach vollständig abgeschlossenem Upload wieder gelöscht**. Damit überlebt die Zeile „Einmalige
  Sicherung" einen Neustart und der Lauf wird fortgesetzt (Guards wie Hintergrund-Sync). Das
  entspricht dem Original, das die Drop-Queue persistiert
  (`uploads`/`backup`, `schedule_id="ManualUpload"`, `docs/SPEC_ONE_TIME_UPLOAD.md` §6).
  **Verworfen:** die alte Regel „Drop-Queue ist absichtlich transient/nie persistiert"
  (`SPEC_ONE_TIME_UPLOAD.md` §10.3) — sie galt nur, bis der User die Parität verlangte.
- **Gesamt-Fortschritt über alle Uploads** (Wunsch 2026-10-14): genau **eine** aggregierte Anzeige
  über alle Uploads (Summe aller Backup-Ziele inkl. One-Time-Zeile), nicht nur über den gerade
  laufenden Lauf. Manuelle Läufe starten weiterhin pro Ordner; die Anzeige summiert trotzdem über
  alle.

## Scope-Grenzen (In/Out) — User-Wunsch 2026-10-13

Bewusste, absichtliche Abgrenzung. Die vollständige Feature-für-Feature-Liste mit Beleg-Keys/-Symbolen
steht im Ledger (`AGENTS.todo.md` → „Scope-Inventar: vergessene Features & bewusste In/Out-Grenzen").

- **Out (bewusst, nicht MVP):** bidirektionale Synchronisation, Downloads/Restore, Datei-Management
  (Albums/Favoriten/Papierkorb/Teilen), Apple-Photos-/iPhoto-/Aperture-Import, Autoplay/Media-Player,
  Upsell-/Trial-/Overquota-/Outage-Banner, Auto-Update, Proxy- und „ausgeblendete Dateien"-Einstellungen,
  Crash-Relaunch-LaunchAgent, iOS-Dedupe-Prompt.
- **MVP-Lücke geschlossen (Korrektur 2026-10-14):** der interaktive Dateinamens-Konflikt-Dialog ist
  **implementiert** (`81cd277`, verifiziert in `a3a3625`): bei 409 fragt `Ask` pro Datei
  (Ignorieren / Als neue Datei speichern / Ersetzen) mit „Nicht noch einmal fragen" als run-weitem
  Override und 60-s-Timeout ⇒ Ignore. Die frühere Notiz „die Option ‚Nachfragen' ist ohne ihn
  wirkungslos" ist damit überholt.
- **Für ein öffentliches Release relevant:** About / Third-Party-Lizenzen (Hinweispflicht).
- **Concurrency-Cap „8":** kein technisches Limit, sondern die Obergrenze der kunden-/server-gelieferten
  Einstellung (`CustomerSettingsMaxSupportedUploadQueues = 8`); der **Geräte-Default** ist kleiner
  (`UploadMaxQueues = 4`, `MaxNumberOfQueuesPerProcessorCore = 1.0` ⇒ praktisch `min(4, Cores)`).
  Eine sachliche Begründung der 8 ist im Original **nicht** belegt.
- „Nicht umsetzen" heißt hier: dokumentiert und bewusst draußen — nicht „später automatisch".

## Plattform / Portierung (User-Entscheidung 2026-10-13)

- **Ziel: voll cross-platform.** macOS (Apple Silicon), Windows und Linux sind gleichwertige
  Ziele. Kehrtwende vom früheren „Native macOS"-Ziel.
- **Neue Features über cross-platform Crates** (nicht über macOS-APIs): Menüleiste/Tray →
  `tray-icon`, Benachrichtigungen → `notify-rust`, Autostart → `auto-launch`. Alle drei
  funktionieren auf macOS/Windows/Linux.
- **Bewusst macOS-spezifisch bleiben** (mit `#[cfg(target_os = "macos")]` bzw. No-op-Fallback auf
  anderen OS): das native App-Menü (`src/app_menu.rs`, damit ⌘X/⌘C/⌘V im WebView gehen — auf
  Windows/Linux erledigt das die WebView selbst) und die Main-Queue-Scheduling der Cookie-Probe in
  `src/login.rs` (der Run-Loop-Reentry ist ein macOS-Problem; andere OS proben direkt).
- Das eingebettete Login-WebView bleibt ein **`wry`-Kind-View** — `wry` ist cross-platform
  (macOS WKWebView / Windows WebView2 / Linux WebKitGTK) und wird über `raw-window-handle` an das
  eframe-Fenster gehängt; keine Änderung an der Grundarchitektur nötig.
- **Signierung ist nur auf macOS relevant.** Dort ist eine (selbstsignierte) Codesignatur nötig,
  damit Benachrichtigungen/Bundle-Identifier sauber funktionieren; Windows/Linux brauchen keine.
  Das macOS-`.app`-Bundling + Signatur wird als eigenes Paket geliefert.
- **Verifikationsgrenze:** Auf diesem Rechner ist nur der **macOS-Build lauffähig und testbar**.
  Windows/Linux werden durch `cfg`-Guards und cross-platform Crates abgedeckt und — soweit
  toolchainseitig möglich — per Cross-Compile-Check (`cargo check --target …`) belegt, **nicht**
  durch echte Läufe. Das ist eine bekannte, akzeptierte Einschränkung.
- **Verworfen:** „selbstsigniertes `.app` nur für macOS, sonst nichts" als Antwort auf den
  Benachrichtigungs-/Menüleisten-Wunsch (war die erste Teilantwort des Users, ersetzt durch
  „voll cross-platform").

## Modul-Struktur (User-Entscheidung 2026-10-13)

`src/main.rs` ist ein Monolith (~1780 Zeilen: State + Rendering + Upload-Worker + Formular +
Icons). Das ist der Engpass des Ports: fast jedes Paket ändert dieselbe Datei, also läuft alles
seriell. Deshalb wird **reiner Refactor ohne Verhaltensänderung** in Module gesplittet, danach
trifft jedes Paket möglichst eine Datei und Subagenten können parallel arbeiten.

**Schnittregel (User-Vorgabe 2026-10-13): nach Feature/„Screen", nicht nach technischer Schicht.**
Ein Dialog/eine Seite = eine Datei; zusammengehörige Logik bleibt zusammen. **Nicht** pro Funktion
ein Modul (das macht Felder überall `pub(crate)`, verschiebt den Merge-Punkt nur in den `mod`-Block
und bringt keinen Parallelitätsgewinn).

Zielstruktur (feature-orientiert):

| Datei | Inhalt (Feature) |
|---|---|
| `src/main.rs` | nur `mod`-Deklarationen, `HEADER_H`, `fn main()` |
| `src/app.rs` | `PrimePhotoApp`: Felder, Lebenszyklus, `impl eframe::App` (`update`), Navigation/Sidebar-Chrome |
| `src/login.rs` | **Anmelden als Ganzes**: bestehender Webview/Cookie-Code **+** Login-Screens (`login_ui`, `login_screen_ui`, `begin_login`, `poll_login`, `apply_cookie_string`, `logout`) |
| `src/backup.rs` | Backup-Seite: Liste, `Row`, Zeilen-Aktionen (`apply_action`, `add_backup_folder`, `start_folder_upload`, `finalize_upload`) |
| `src/backup_config.rs` | Dialog „Datensicherungseinstellungen" (`BackupConfigForm`, `ConfigFormMode`, `ConfigFormAction`, `render_backup_config`, `handle_config_form_action`) |
| `src/settings.rs` | Einstellungs-Seite (`settings_ui`, `general_settings`, `bandwidth_settings`, Konto-Block) |
| `src/upload.rs` | `UploadShared`, `upload_worker`, `run_upload`, ETA-/Fortschritts-Helfer |
| `src/countries.rs` | `COUNTRIES`, `country_label`, `system_country`, `parse_cookie_string` |
| `src/icons.rs` | gezeichnete Icons (`NavIcon`, `nav_icon`, `status_dot`, `draw_avatar`, `upload_logo`) |

Unverändert bleiben: `prime_photos.rs`, `config.rs`, `folders.rs`, `dedup.rs`, `theme.rs`,
`notify.rs`, `tray.rs`, `app_menu.rs`.

**`config.rs` bleibt eigenständig** (nicht in `settings.rs` aufgehen): `Config` ist das Datenmodell
**und** die Persistenz, das auch der Upload-Worker, `login` und `tray` benutzen — es in die
Einstellungs-UI zu ziehen wäre eine Schichtumkehr. `settings.rs` ist nur die Oberfläche darüber.
(Falls doch gewünscht, wäre die saubere Variante `src/settings/{mod.rs (Config+load/save), ui.rs}` —
nicht `Config` in die UI-Datei.)

Regel: **kein Verhaltens- oder UI-Änderung** in den Refactor-Paketen — verschobener Code bleibt
byteweise gleich, nur Sichtbarkeit (`pub(crate)`) und Modulzugehörigkeit ändern sich.

## Internationalisierung (i18n) — User-Entscheidung 2026-10-13

- **Jetzt einführen, minimal:** direkt **nach REF-2** (dann sind die UI-Strings frisch pro Feature-Modul
  sortiert) eine zentrale String-Schicht `src/i18n.rs` mit `Lang`-Enum und einer Tabelle. Zuerst
  **de + en** vollständig; jede weitere Sprache ist danach **additiv** (kein Umbau).
- **Sprachwahl:** Default = OS-Locale, **Override in den Einstellungen**; im `Config` persistiert
  (`language`, serde-Default aus der OS-Locale, damit alte `config.toml` weiter lädt).
- **Umfang mittelfristig: lateinische Sprachen** (de, en, fr, it, es, pt). **Kein CJK** (egui's
  Default-Font ist Latin-only → bräuchte einen Noto-CJK-Font, gebündelt ~16 MB oder pro OS geladen)
  und **kein RTL** (egui hat kein Bidi-Layout; Arabisch/Hebräisch nur in der Webview, nicht in
  unserem Chrome).
- **Webview-Sprache:** **Marktplatz-Default belassen** — Amazon entscheidet die Seitensprache aus
  TLD + Cookies/`Accept-Language`. Wir erzwingen **nichts** (`lc-`-Cookie/`language`-Parameter wären
  unbelegt und könnten die Login-Seite verändern). Folge: deutsches UI + `amazon.com` ⇒ Loginseite
  englisch — bewusst akzeptiert.
- **Regel:** Kein hartkodierter UI-Text außerhalb von `src/i18n.rs`. Neue Strings kommen direkt
  zweisprachig dazu. Die Marktplatz-Ländernamen in `countries.rs` sind ebenfalls UI-Text (→ i18n).

## Arbeitsablauf (Build-Verify, seit 2026-10-13)

- Der Hauptagent ist **Orchestrator**: er selbst schreibt nur `AGENTS.md`, `AGENTS.todo.md` und `AGENTS.skills.md`. Alle anderen Dateien bearbeiten Subagenten.
- **Umsetzer:** `general`-Subagent mit Modell `opencode-go/deepseek-v4.1-flash` (User-Entscheidung
  2026-10-14; löst `opencode-go/step-5-preview-free` ab, das wiederum `opencode-go/deepseek-v4.1-flash#high`
  ersetzt hatte) — bekommt Ziel-Dateien und die vollständige Spezifikation, frische Session pro Task.
  **Kein `#variant`-Suffix** (deepseek-v4.1-flash wirbt low/high/max; bewusst ungenutzt, siehe globale
  Modellvorgaben).
- **Verifier:** ein **anderer** `general`-Subagent mit Modell `opencode-go/deepseek-v4.1-flash`, frische Session pro Runde; prüft die Substanz (Diff gegen Auftrag, Invarianten, Blast Radius), nicht die Gates.
- Reihenfolge je Task: `cargo build` (grün, Ausgabe zitieren) → Verify → **immer committen**; bei „CHANGES REQUIRED": fixen → erneut verifizieren → `git commit --amend`.
- **CI/Release (User-Entscheidung 2026-10-13, hebt „Kein Push, kein CI" auf):** Ziel ist ein
  öffentliches GitHub-Repo (bei öffentlichen Repos sind die macOS/Windows/Linux-Runner kostenlos).
  - **Push/PR gegen `main`:** nur **Linux** — `cargo build` + `cargo test` als Pflicht-Gate; die
    anderen OS werden dort **nicht** gebaut.
  - **Release:** **Git-Tag `v*`** → CI-Matrix-Build **Windows + Linux**; alle Beine müssen grün sein
    (`needs:`), sonst kein Release → GitHub Release mit den Artefakten. **macOS wird NICHT im CI
    gebaut** (User-Entscheidung 2026-10-13: „nur lokal macOS bauen ist ok") — lokal über
    `packaging/make-app.sh` (arm64) und bei Bedarf manuell ans Release hängen.
  - **macOS-Signierung:** lokal `packaging/make-app.sh` (ad-hoc per Default, `CODESIGN_IDENTITY`
    für eine eigene Identität). Für ein Release an Dritte bräuchte es eine **„Developer ID
    Application"**-Identität aus dem **Apple Developer Program** (99 $/Jahr) + **Notarisierung**
    (`notarytool`) + Stapling — nur das akzeptieren fremde Macs. **Selbstsigniert/ad-hoc sind nur
    lokal vertraut**; für heruntergeladene Builds gilt der dokumentierte Gatekeeper-Workaround
    (Rechtsklick→Öffnen bzw. `xattr -dr com.apple.quarantine`, siehe `docs/RELEASE.md`).
  - Kein `codesign --deep` (von Apple veraltet/abgeraten). `packaging/make-app.sh` nimmt die Identität
    per `CODESIGN_IDENTITY`.
  - Secrets nur in `main`/Tag-Läufen (fork-PRs erhalten keine Secrets). Windows: Authenticode optional
    (sonst SmartScreen-Hinweis); Linux: keine Signatur. Details: `docs/RELEASE.md`.
- **Spec-first pro Feature (User-Entscheidung 2026-10-13):** Vor jeder Implementierung wird das
  Original-Binary (`/Applications/Amazon Photos.app`: .NET-DLL + Ressourcen, nur lesend) zum
  jeweiligen Feature untersucht; das Ergebnis wird als **Clean-room-Spec** in `docs/` festgehalten
  (beobachtetes Verhalten in eigenen Worten/Tabellen, keine verbatimen Code-Auszüge, keine
  Personendaten) — analog zu `docs/UI_FLOWS.md` / `docs/UI_STATES.md`. Erst danach Umsetzer-Briefing.
- **Build-Verify ist nur grobe Basis:** Bauen → prüfen → committen gilt; Förmlichkeit
  (Runden-Protokoll, Footer-Schreibweise) nach Augenmaß, kein Selbstzweck.

## OpenCode-MCP: rea-Server (2026-10-13, User-Entscheidung)

- **Befund „MCP rea failed / Request timed out":** kein Config-Fehler. Der Start läuft über
  `npx -y rea-agents@6.4.0 mcp` und braucht bei jedem Connect die npm-Registry. Die Maschine hatte
  heute 13:00–18:00 DNS-Flackern (`getaddrinfo ENOTFOUND`, 9–18/h — auch Model-Fetch/Update-Check
  betroffen). Gegen eine unresolvable Registry retryt npm ~70 s; OpenCode's MCP-Connect-Timeout
  bricht vorher ab ⇒ „Request timed out". `codegraph` (lokales Binary, kein Netz) lief durch —
  daran wurde die Ursache eingegrenzt. Älteres Symptom (vor 12:04, alter Server-Prozess ohne `npx`
  im PATH): `NotFound: ChildProcess.spawn` — mit Service-Neustart erledigt.
- **Fix (User gewählt: minimal):** `command: ["npx","-y","--prefer-offline","rea-agents@6.4.0","mcp"]`
  in `~/.config/opencode/opencode.jsonc` — warmer npx-Cache ⇒ ~0,6 s Start, kein Registry-Kontakt.
  Verifiziert: `mcp connected server=rea tools=170`.
- **`HOPPER_LAUNCHER_PATH` entfernt** (User: „remove hopper"): zeigte auf
  `~/Applications/Hopper Disassembler.app/…` — die App lag im `~/.Trash`. Getestet: fehlendes
  Hopper bricht das MCP-Handshake nicht, nur native-Analyse-Tools wären davon betroffen. Bei
  Bedarf: Hopper neu installieren, Variable ergänzen.
- **Reserve bei Wiederauftreten:** `npm i -g rea-agents@6.4.0` + MCP-Command auf den absoluten
  Binary-Pfad (kein Netz im Startpfad); Maschinen-DNS prüfen (Public Resolver flackerten hinter
  dem 10.x-LAN).
- Protokoll/Begründung: `AGENTS.todo.md` „OpenCode-MCP: rea-Server".

## Repo

- Project / git repo name: **Prime Photos** (directory `prime-photos`).
- Git repository initialized on main.
- Build: `cargo build` / `cargo build --release`.
