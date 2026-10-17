# Scope-Inventar: vergessene Features & bewusste In/Out-Grenzen

Anlass (User-Wunsch 2026-10-13): beim Reverse-Engineering sind Features
aufgetaucht, die in keiner Aufgabenliste standen. Ziel ist eine **absichtliche**
Scope-Entscheidung — **nicht alles umsetzen**. „Scope" ist ein Vorschlag;
**nicht umsetzen** heißt hier: dokumentiert und bewusst draußen, nicht
„später automatisch". Nichts davon ist implementiert.

Belege: `Localizable.strings` (Strings/Keys) + `Amazon Photos.dll` (Symbole),
jeweils gegen `SPEC_*.md` gehalten.

| # | Feature | Beleg | Status bei uns | Scope |
|---|---|---|---|---|
| 1 | **Bidirektionale Synchronisation** (Sync-Ordner) | `enablesync`/`disablesync`, `navigationMenu_sync`, `failedsyncitemswindow_*`, `SyncEngineQueueTimeoutInSeconds` | fehlt komplett | **Out / Post-MVP** — anderer Produktkern (Sync ≠ Backup) |
| 2 | **Interaktiver Dateinamens-Konflikt-Dialog** | `filenameconflict_overlayview_{title,body,ignore/replace/saveasnew}_*`, `_donotaskagain_`, `filenameconflict_toast` | umgesetzt (vier Optionen + Dialog) | **In** — sonst ist „Nachfragen" wirkungslos |
| 3 | **Finder-/Kontextmenü-Service** „Bei Amazon Photos hochladen" | `context_menu_upload`, `menu_services`, `ServicesMenu.strings` | fehlt | **Out / Post-MVP** — OS-Integration |
| 4 | **Apple-Photos-/iPhoto-/Aperture-Import** | `mediaimport_*`, `onboard_noautosave_*` | fehlt | **Out** — nicht unser Datenmodell |
| 5 | **Ausgeblendete Dateien** (Setting) | `ExcludeHiddenFilesAndFolders` (Default true), `preferences_(in|ex)cludehiddenfiles` | fehlt | **Out / optional** — kleine, spätere Einstellung |
| 6 | **Proxy-Einstellungen** | `preferences_proxy_*` | fehlt | **Out** |
| 7 | **Speicher-/Quota-Balken** | `youraccount_*` | bewusst nicht (MVP) | **Out** (entschieden) |
| 8 | **Videos hochladen** | ContentFilter-Bits, `no_photos_or_videos_found` | nur Bilder | **Out / Post-MVP** |
| 9 | **Downloads / Restore** | `navigationMenu_download`, `displayitememptymessage_download_*` | fehlt | **Out** (nicht MVP) |
| 10 | **Datei-Management** (Albums, Favoriten, Papierkorb, Teilen) | `album`, `favorite`, `trash`, `restore` | fehlt | **Out** — Uploader-Scope |
| 11 | **Autoplay / Media-Player** | `mediaimport_autoplayaction` | fehlt | **Out** |
| 12 | **Upsell-/Trial-/Overquota-Banner** | `notification_{overquota,trialexpired,campaignwindow}_*`, `outage_banner_text` | fehlt | **Out** |
| 13 | **Auto-Update** | `DefaultAutoUpdateAfterTimeInSeconds` | fehlt | **Out** |
| 14 | **About / Privacy / ToS / Third-Party-Lizenzen** | `aboutwindow_*` | fehlt | **Out für MVP**, **In für ein öffentliches Release** (Hinweispflicht) |
| 15 | **Menüleisten-Status-Popup** (Statistiken, Benachrichtigungen) | `statuspopup_stats_*`, `contextmenu_*` | Tray da (PORT-3), Popup fehlt | **Post-MVP** |
| 16 | **Tabellen-Sortierung/-Filter** (Name/Letzte/Status/Größe) | `statuswindow_{status,size,lastmodified}columntitle`, `statusall` | fehlt | **Post-MVP** |
| 17 | **Größe-Spalte + relatives „Letzte Aktualisierung"** | `datetimeago_*`, `sizecolumntitle` | fehlt | **In** (in `SPEC_UPLOAD_PROGRESS.md` enthalten) |
| 18 | **iOS-Dedupe-Prompt** | `iosdedupeprompt_*` | fehlt | **Out** (Apple-Photos-Kontext) |
| 19 | **Mehrere Konten** | `folderconflict_*` „anderes Benutzerkonto" | fehlt | **Out / UNKNOWN** |
| 20 | **Crash-Relaunch (LaunchAgent)** | `WorkerMaxRestarts*`, macOS-LaunchAgent | fehlt | **Out** (macOS-spezifisch) |
| 21 | **Retry-All / Re-add-to-queue / Failed-Fenster** | `readdtoqueue`, `retryall`, `failedsyncitemswindow_retry` | fehlt | **Teilweise In** — Retry steckt im Upload-Progress-TODO |
| 22 | **Sync-Mass-Delete-Benachrichtigungen** | `notification_syncmassdeletes_*` | fehlt | **Out** (Sync) |
| 23 | **„Alle Aktivitäten anhalten" global (Upload+Download)** | `activitywindow_*` | nur Backup-Pause | **In bleibt** (Download-Pause gehört zum Out-Teil) |
| 24 | **„Gleichzeitige Uploads"-Default** | `UploadMaxQueues=4`, `MaxNumberOfQueuesPerProcessorCore=1.0` | Slider 1..8 | **Entscheidung offen** — siehe unten |

## Concurrency-Cap „8" — Research-Ergebnis

Belegt über `Infrastructure.AppSettings` / `CustomerSettings` der App-Assembly.

| Punkt | Wert |
|---|---|
| `CustomerSettingsMaxSupportedUploadQueues` | **8** (`…Min…` = 1) — Downloads symmetrisch (8/1) |
| Art der „8" | **kein** technisches Limit, sondern die Obergrenze einer kunden-/server-gelieferten Einstellung (`SettingsRefreshTimeInSeconds = 3600`), die der `CustomerSettings`-Getter per `CheckMinMax` klemmt (`IgnoreCustomerSettingsUploadQueuesSetting` = false ⇒ gilt) |
| Geräte-Default | `UploadMaxQueues = 4`, `MaxNumberOfQueuesPerProcessorCore = 1.0` ⇒ `UploadQueues = max(1, min(4, ProcessorCount × 1))` — typisch **4** (bei ≤4 Kernen entsprechend weniger); Download identisch (`DownloadMaxQueues = 4`) |
| Begründung der 8 | **UNKNOWN** — im Code steht nur der gesetzte Wert, keine inhaltliche Herleitung |

**Folge für uns:** der Slider `1..=8` trifft die Obergrenze; unser Default weicht
vom Geräte-Default ab. Die geltende Entscheidung steht in
`docs/features/queue-progress.md` → „Upload-Queue & Concurrency
(User-Entscheidung 2026-10-13)".

## Video-Support — offene Fragen (falls er je drankommt)

Gleicher Upload-Endpoint (`kind=FILE`, nur `FILE` beobachtet); der Modus läuft
allein über die `content_filter`-Bitmaske (`None=0, Photos=2, Videos=4,
Other=8, PhotosAndVideo=6, Everything=14`, dazu `DeduplicateOptions`;
Tabellen in `UI_STATES.md`). **UNKNOWN:** welche Video-Extensions das
Original akzeptiert, ob sich Upload-Parameter für Video unterscheiden, und die
Quota-Frage (Prime: Fotos unbegrenzt, Videos begrenzt? —
`…_notification_overquota_unlimitedphotos_*`, Videospeicher-Upsell-String).
