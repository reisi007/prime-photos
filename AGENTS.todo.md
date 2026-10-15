# TODO / decisions ledger

## Offene Punkte (Bugs & Wünsche aus 2026-10-13)

Oberziel des Users: **nur neue Dateien hochladen, keine Bandbreite verschwenden.**

- [ ] **Icons nicht sauber ausgerichtet.** Nav-/Sidebar-Icons an die Doku-Werte ausrichten
  (Header 64 pt bzw. 104 pt mit Banner, 4 pt Akzentleiste, Nav-Spalte — `docs/UI_FLOWS.md` §3.1);
  braucht die Maßwerte/Design-Abmachung, kein Rate-Wert.

- [ ] **Logout-Residuen.** `logout()` löscht die 5 Auth-Felder, stoppt aber weder eine laufende
  Login-WebView noch einen laufenden Upload (Worker hält eigenen `cfg`-Klon und arbeitet mit dem alten
  Cookie weiter); `config_error` nach fehlgeschlagenem `save()` wird auf dem Login-Screen nicht angezeigt.
  Soll: Login-/Upload-Status zurücksetzen, Fehler sichtbar machen.

- [ ] **Nav-Accessibility.** Selbstgezeichnete `nav_item`-Zeilen (`allocate_exact_size`) haben keine
  Keyboard-Fokus-/Accessibility-Semantik; optional `Sense`-Widget/`widget_info` ergänzen.

## Assets: Tray-Icons + Logo — Quellen geliefert (2026-10-14), Einbindung **offen**

Eigene Assets liegen in `assets/` — **als SVG-Quellen** (maßgeblich: `AGENTS.md` §„Assets"), daraus
rastet der Build die PNGs. Auftrag: einplanen, damit „MVP polished" inkl. Autostart und effizientem
Upload erreicht wird.

**Verbindliche Einbindung (Vorgabe, damit es auf jeder Plattform/im Bundle funktioniert):**
- Ablageort: **`assets/` im Repo-Root**, **SVG-Quellen** — keine committeten PNGs mehr (sie entstehen im Build).
- `build.rs` rastet die SVGs mit `resvg` (`[build-dependencies]`, `default-features = false`, reine
  Rust-Bibliothek, keine Laufzeit-Abhängigkeit) nach `OUT_DIR`; Einbindung per
  **`include_bytes!(concat!(env!("OUT_DIR"), …))`** (nicht zur Laufzeit vom Pfad) + Dekodierung über die
  bereits vorhandene `image`-Crate → `tray_icon::Icon::from_rgba` bzw. eframe-Viewport-Icon.
  Keine Pfadauflösung im `.app`-Bündel, Binary bleibt self-contained.
**Kein Fallback (User-Vorgabe 2026-10-14):** fehlt ein SVG, bricht `build.rs` den Build (Compile-Fehler).
Kein `Option`, keine `path.exists()`-Prüfung, kein Rückfall auf die gezeichnete Variante (`src/icons.rs`
bleibt nur für die Vektor-Glyphen im UI selbst: Navigation, Status-Punkt, Flagge). Ein Dekodier-Fehler
zur Laufzeit ist ebenfalls hart (fail fast, klare Meldung) — eine stille degradierte UI ist
ausdrücklich **nicht** gewollt.

**Geliefert (Stand 2026-10-14) — Quellen sind SVGs in `assets/`:**

| Datei | viewBox | Zweck |
|---|---|---|
| `tray.svg` | 84×84 | Tray-Basis (Leerlauf: Upload-Pfeil) |
| `tray-active.svg` | 84×84 | „Upload läuft" (Pfeil mit Bewegungslinien) |
| `tray-paused.svg` | 84×84 | „Angehalten" (zwei Balken) |
| `tray-done.svg` | 84×84 | „Fertig" (Haken) |
| `tray-error.svg` | 84×84 | „Fehler" (Ausrufezeichen) |
| `logo.svg` | 256×256 | App-Logo (orange, Upload-Pfeil + Fotostapel) |
| `logo-small.svg` | 256×256 | kleine farbige Variante |
| `logo-small-black.svg` | 256×256 | schwarze Variante (dunkler Hintergrund) |

Die Tray-Zustände sind **monochrom über `stroke="currentColor"`** — die Farbe entscheidet der Renderer,
deshalb ergibt sich schwarz/weiß ohne zweite Datei. **Entschieden (User 2026-10-14): die PNGs entstehen
im Build** — `build.rs` + `resvg` als `[build-dependencies]` (`default-features = false`, 53 Crates,
keine Laufzeit-Abhängigkeit), Ausgabe nach `OUT_DIR`, Einbettung per `include_bytes!`. Verworfen wurden
(mit Beleg): `sips`/`qlmanage` im Build (gute Qualität, aber macOS-only ⇒ CI/Release-Matrix bricht),
ImageMagicks interner Renderer (malt die Striche nicht, `rsvg-convert` fehlt), und „einmalig rendern und
committen" (zwei Quellen, die auseinanderlaufen). **Theme:** `egui::Context::system_theme()` — dunkel ⇒
weiß, hell ⇒ schwarz, `None` ⇒ farbiges Logo (immer sichtbar). **Zu erzeugende PNGs:** fünf Zustände ×
{schwarz, weiß} × {32, 64} + `logo` 1024 + `logo-small` 256 + `logo-small-black` 256.

Der Tray-Satz ist durchgängig **monochrom (`currentColor`) auf transparent** — genau die
macOS-Template-Semantik. Es gibt **keine** separaten schwarzen/weißen Dateien und **keine**
PNG-Zwillinge im Repo: Weiß/Schwarz ist ein Raster-Override beim Build (`*{color:#fff}` bzw. `#000`).
Es gibt auch **keine QA-Artefakte** mehr im Repo — die früheren Kontrollbilder (`chk-logo-*`,
`contact-sheet`, `zoom-corners`, `debug-mask-*`, `preview-tray`) und der komplette PNG-Satz wurden
entfernt; committet werden ausschließlich die acht SVG-Quellen der Tabelle oben.

**Einbindungspunkte, die bereits existieren (verifiziert im Code):**
- `src/tray.rs`: `ICON_SIZE = 32`, `tray_icon::Icon::from_rgba(…)`, und **auf macOS ist das
  Template-Flag schon gesetzt** (`with_icon_templated`, nicht-deprecatierter Nachfolger von
  `with_icon_as_template(true)`), außermacOS `with_icon(icon)`. Der heutige Glyph wird gezeichnet
  und ist **teal** — die neuen schwarz/Alpha-Raster passen dort 1:1 hinein.
- macOS skaliert (`tray-icon` 0.26.1, `to_nsimage(max_height)`) auf die Menüleisten-Höhe **herunter**;
  ein Icon, das schon passt, wird nicht hochskaliert. ⇒ Als Quelle das 64-px-Raster (`@2x`) nehmen
  (mehr Bitmap pro Punkt = retina-scharf), das 32-px-Raster für die Nicht-macOS-Pfade/1×-Fälle.
- Dock-/Fenster-Icon: eframe-Viewport-Icon (RGBA) aus dem Build-Raster von `logo.svg` (1024 px); die
  gezeichnete Variante `src/icons.rs::upload_logo` bleibt nur, wo sie heute als Vektor genutzt wird.

**Windows/Linux-Tray (User-Entscheidung 2026-10-14): weiße Varianten + Theme-Erkennung.** Außerhalb
macOS wird das Icon **ohne** Template-Flag gesetzt (`with_icon`), ein schwarzes Monochrom-Icon wäre
auf dunkler Taskleiste unsichtbar. Weiß entsteht **beim Rastern aus derselben SVG-Quelle** per
`currentColor`-Override (`*{color:#fff}`) — es gibt **keine** separaten `tray-*-white`-Dateien. Die App
wählt per **Theme-Erkennung** — `egui::Context::system_theme()` (in 0.31 vorhanden): dunkel ⇒ weiß,
hell ⇒ schwarz; `None` (unbekannt) ⇒ das **farbige `logo`-Raster**, das auf jedem Hintergrund sichtbar
ist (dann ohne Zustandswechsel). macOS bleibt beim schwarzen Template-Satz (das System invertiert
selbst). **Verworfen:** farbiges Logo als generelle Non-macOS-Lösung (verliert die Zustände);
„bewusst akzeptieren" (nicht polished). **Die Quellen liegen vor ⇒ Umsetzung kann starten;** die
Assets sind per Vorgabe **compile-pflichtig** (`build.rs` bricht bei fehlendem SVG).

Recherche-Grundlage: `docs/SPEC_NOTIFICATIONS.md` §2.4 (Icon-/Status-Map des Originals:
idle/processing/paused/done/error) und §2.3 (Menüleisten-Item); das Original liefert monochrome
Template-Bilder plus eine farbige Ungelesen-Variante.

**Einplanung:** Symbolwechsel zur Laufzeit (aktiv/pausiert/Fehler) gehört in die Asset-Welle;
das Dock-/Fenster-Icon ist davon unabhängig und kann allein kommen. Nicht Teil: das `.app`-Icon
für das macOS-Bundle (`packaging/make-app.sh`) — eigener, bereits zurückgestellter Punkt.

## Weitere offene Punkte (dokumentiert, nicht eingeplant)

Auswahl des Users als „Sonst noch offene Punkte"; bewusst **ohne** Starttermin:
- Queue-Paginierung (heute bricht das Panel bei 300 Zeilen ab, Original lädt seitenweise 1000).
- Fehlerfenster mit `readdtoqueue`/`retryall`; Semantik laut `SPEC_STATUS_DETAIL.md` (§3).
- Pause **pro Ordner** (Zeilen-Symbol) — im Original vorhanden, bei uns Post-MVP.
- Sortierung/Filter der Tabelle; Spalte „Größe" + relative Zeit (`datetimeago_*`).
- Tray-Status-Popup mit Statistiken (`statuspopup_stats_*`).
- Kaltstart-Verhalten nach Login; Pro-Ziel-Limit; netzwerkgesteuerte
  Drosselung; Logout während laufendem Upload; Nav-Accessibility; `.app`-Icon.
- **Nicht** eingeplant (bewusst Out of Scope, siehe Scope-Inventar): Sync, Downloads/Restore,
  Datei-Management, Apple-Photos-Import, Videos, Autoplay, Upsell-/Quota-Banner, Auto-Update,
  Proxy, ausgeblendete Dateien, iOS-Dedupe-Prompt, Crash-Relaunch.

## Referenz: was **nicht** neu erfunden werden darf

Alle Rechte-/Verhaltensfragen sind entweder aus den acht Clean-Room-Specs belegt oder als
UNKNOWN markiert. Wo eine Spec einen Original-Wert nennt, gilt er als Vorgabe, nicht als Ratewert.

## Wellenplan (Neufassung 2026-10-14, nach der Duplikat-/Konflikt-Kette) — **NUR PLAN**

Der User hat die Restliste ausdrücklich **nur gruppiert** haben wollen („dann rest in wellen
gruppieren, ausgeben und stoppen"). **Keine** Welle startet ohne ausdrücklichen Auftrag. Die Wellen
sind nach **gemeinsamem Umfeld/Dateien** geschnitten, damit jede in einem Durchgang ohne
Merge-Konflikte läuft; die Reihenfolge ist eine Empfehlung.

### Welle 2 — Konflikt-/Duplikat-Kette abschließen (ein Umfeld, dieselben Dateien)
- **2.1 409-Ursache trennen** — ✅ **erledigt 2026-10-15** (`42149bb`, Verifier APPROVE, 79 Tests):
  `DuplicatesConflictError` ⇒ „bereits im Konto" (und als `content_skip` im Index, damit der nächste
  Lauf **nicht** „unverändert" meldet), `NameAlreadyExists` ⇒ Konflikt (Prompt/Fehler je Option).
  Beide Codes sind live belegt.
- **2.2 „Ersetzen" reparieren** — ✅ **erledigt 2026-10-15** (`4ec98e7`, Verifier APPROVE, 79 Tests):
  `wire_value(Replace) = "NONE"` (vorher der unbekannte Wert `OVERWRITE` ⇒ live **404**, die Option
  war **kaputt**), PUT **nur** bei Namenskonflikt, ein Inhaltsduplikat bleibt „bereits im Konto"
  (identische Bytes erneut zu senden spart die Dedup-Semantik ein), PUT-Fehler zählt als Fehler.
  **Die Live-Abnahme schlug fehl** — sie fand genau den Fehler, der jetzt 2.2b ist.
- **2.2b „Ersetzen" korrigieren — umgesetzt, Verifikation und Live-Abnahme offen (2026-10-15, vom
  Nutzer beauftragt).** Unser `PUT` trug nur `conflictResolution=NONE`; der Server verlangt die
  **Parameter des POST** (live: ohne `fileSize` ⇒ **400 `MissingFileSize`**, mit dem POST-Satz kommt
  er durch die Validierung). **Geltende Regel (auch in `AGENTS.md`): der `PUT` trägt denselben
  Query-Satz wie der POST** — über einen **gemeinsamen** Query-Bauer, kein geratenes Subset; welche
  Parameter im Einzelnen Pflicht sind, bleibt bewusst offen. **Zweiter Live-Befund:** der `PUT`
  unterliegt der Inhalts-Dedupe (identische Bytes ⇒ 409 statt 200) — deshalb wird ein
  Inhaltsduplikat **nie** per PUT „ersetzt". **Abnahme (offen):** derselbe Live-Lauf noch einmal
  (Knoten behält den Namen, Inhalt danach nachweislich der neue — Größe/MD5/Version), danach Knoten
  + Testziel wieder entfernen.
- **Eigenes Paket (beauftragt, läuft nach 2.3b und vor Welle 3): EXIF-loser Upload darf kein Fehler
  sein.** Ein **erfolgreicher** Upload von `c2.png` liefert gültiges JSON mit `id`/`name`, unser
  `ResponseUpload` verlangt aber `resolutionUnit` ⇒ serde bricht ab („missing field
  'resolutionUnit'") ⇒ der Lauf meldet **Fehler** statt Erfolg und schreibt **keine** Indexzeile (die
  Datei würde bei jedem Lauf erneut gelesen und gehasht). Betrifft alle EXIF-losen Typen, die
  `is_supported_image` zulässt (png, bmp, gif, webp); im Sport-Ordner (nur JPEG/CR3) unsichtbar, in
  einer echten Sammlung nicht. Fix: Antwortfelder **optional**, Test mit einer Antwort ohne
  `resolutionUnit`.
- **Zwei echte Befunde aus der Hänger-Diagnose (angelegt, nicht beauftragt):** (a) ein Ziel, dessen
  Dateien **alle** fehlschlagen, behält `status = "uploading"` dauerhaft:
  `finalize_background_store` (`src/backup.rs:353-363`) schreibt Done/Error nur für Indizes aus
  `folder_done` — bei 0 Erfolgen fehlt der Index; (b) `FolderStore::load()`
  (`src/folders.rs:238-243`) macht bei **jedem** TOML-Parse-Fehler `unwrap_or_default()`: **ein**
  fehlerhafter Eintrag in `folders.toml` leert **still** den ganzen Store — danach läuft **kein**
  Hintergrund-Lauf mehr, ohne eine einzige Meldung.
- **Regel für Live-Läufe + Ops-Wissen:** Testdateien vorher per
  `SetFile -d/-m "01/01/2020 12:00:00"` altern — sonst greift die **10-Minuten-Regel** aus
  `upload_picture` (`src/prime_photos.rs:950-975`): fehlt EXIF **und** ist die Datei jünger als
  10 Minuten, kommt `Err("No EXIF data found and file is too recent …")` zurück — **bevor** die
  Upload-URL gebaut wird, sichtbar nur im UI-Log (nicht im tracing), `status` bleibt `uploading`.
  Genau das war der vermeintliche Hänger vom 2026-10-15 — er war **keiner** (Beweis: dieselbe Datei
  gealtert lädt sofort, Index-Zeile geschrieben). Knoten entfernt man per
  `DELETE https://www.amazon.<cc>/drive/v1/nodes/<id>` ⇒ **202 `{}`**; `PUT …/drive/v1/trash/<id>`
  gibt es auf dem Cookie-Host **nicht** (⇒ 404). **Offen/fraglich (nicht beauftragt):** ob diese
  Regel für EXIF-lose Dateien richtig ist — frische Screenshots/PNGs werden damit gar nicht
  hochgeladen.
- **2.3 Konflikt-Standard in drei Ebenen** (User-Entscheidung 2026-10-15):
  **(a) Application Default** — ✅ **erledigt 2026-10-15** (`db68cff`, Verifier APPROVE, 81 Tests):
  Feld in `Config` (`config.toml`), Radiogruppe in **Einstellungen → Upload**, Default `SaveAsNew`,
  **Vorauswahl beim Anlegen eines Ziels** **und** für die **Einmal-Queue (Drag&Drop)**.
  **(b) „Nicht noch einmal fragen" pro Ziel — offen (Paket 2.3b):** die Antwort wird in das
  **bestehende** `BackupFolder.conflict_resolution` des **Ordners der gefragten Datei** geschrieben
  (`folders.toml` — kein Schemawechsel, das Feld existiert); **kein Per-Datei-Default**; innerhalb
  des Laufs bleibt der Override **run-weit** (heutiges Verhalten unverändert).
  Wörtlich: „es soll der ordner default sein, einzelne dateien haben keinen Default. Zusätzlich soll
  es auch einen Application Default geben, der die Standardauswahl bei einem neuen Folder und die
  Drag & Drop Sachen haben das ApplicationDefault."
  Abnahme (b): nach „Ersetzen" + Häkchen steht `conflict_resolution = "replace"` im Ordner und der
  **nächste** Lauf fragt dort nicht mehr, während ein zweiter Ordner weiterhin fragt.
- **2.4 Rest laut `docs/SPEC_CONFLICT_OPTIONS.md`** — jede Variante mit Abnahmekriterium; bewusste
  Abweichungen dokumentieren: die Client-Vorprüfung des Originals ersetzen wir durch die
  Server-Antwort (kein Node-Cache), die `.heic`-iOS-Heuristik bleibt draußen.
  **Mitnehmen (nicht blockierende Verifier-Funde aus 2.1/2.2/2.2b, dort bewusst liegen gelassen):**
  (a) ein Test für **gepaddete** Inhaltsduplikat-Codes („ DuplicatesConflictError " ⇒ **kein** PUT —
  `is_content_duplicate_code` trimmt, aber kein Test pinnt das); (b) ein Test, der den
  **Byte-Durchgriff** einer Nicht-ASCII-Node-ID durch `replace_upload_url` festhält (die
  „kein NFC für die Node-ID"-Begründung steht bisher nur im Doc-Kommentar); (c)
  `docs/SPEC_CONFLICT_OPTIONS.md` §4, Satz „der Worker sieht für `Replace` **kein** 409" ist zu groß
  formuliert (beim Inhaltsduplikat sieht er sehr wohl einen); (d) `docs/SPEC_SYNC.md:734` und
  `docs/SPEC_LIBRARY.md:649` führen `OVERWRITE` noch als unseren Vokabular-Wert; (e) die
  vorbestehenden stale Zeilenreferenzen in `docs/SPEC_CONFLICT_OPTIONS.md` (§1.2 B1/B2/B3, §7-Refs
  auf `upload.rs`/`dedup.rs`) und der Widerspruch in `docs/SPEC_DUPLICATES.md` §3 („Dedupe über
  Pfade hinweg = fehlt") einsammeln, wenn die Datei ohnehin angefasst wird.

### Welle 3 — Progress-/UI-Umbau (entschieden; Recon liegt vor: `docs/SPEC_UPLOAD_PROGRESS.md` §7)
- **3.1** Detail in den **Hauptbereich**, Klapp-Panel „Warteschlange" (`CollapsingHeader` in
  `src/backup.rs`) entfernen — Baum/Liste mit Spalten wie im Recon.
- **3.2** `CollapsingHeader`-Zustand hält nicht: feste ID (`id_salt`); dieselbe Fehlerklasse bei
  **jedem** `CollapsingHeader` mit dynamischen Zahlen prüfen.
- **3.3** Zeile zeigt „Fehler" statt „Upload läuft": Status **live** ableiten (läuft `folder_total`
  für dieses Ziel ⇒ „Upload läuft"; erst nach `finalize` gilt der persistierte Ausgang).
- **3.4** Die zwei noch **gezeichneten** Glyphen (Nav-Icons, Flagge) auf Material Symbols umstellen
  (User-Regel: nichts selbst zeichnen außer dem Logo).

### Welle 4 — Release-Pflicht: About + Third-Party-Lizenzen (die alte „Welle 4")
Recon liegt vor (`docs/SPEC_ABOUT.md`), Attributionen sind gepflegt (`docs/THIRD_PARTY.md`, u. a.
Material Symbols). Das Original hat **keine** Lizenzliste — wir erzeugen sie selbst.

### Welle 5 — Robustheit & Politur (klein, unabhängig)
- **5.1** `rfd::FileDialog::pick_folder()` (`src/backup.rs`, `src/backup_config.rs`) auf
  `AsyncFileDialog`: modales `NSOpenPanel` mit eigener RunLoop = **dieselbe Klasse** wie der behobene
  Absturz (`notify-rust`). **Ungeprüft**, ob es real abstürzt.
- **5.2** P2-Reste: Icon-Ausrichtung (`docs/UI_FLOWS.md` §3.1), Logout-Residuen (laufender
  Login-WebView/Upload, unsichtbarer `config_error`), Nav-Accessibility.
- **5.3** Optional/Branding: die Zeile „Amazon Photos" im Account-Block durch die Domain ersetzen
  (Projektregel: kein Amazon-Branding im eigenen Fenster).
- **5.4** (Verifier-Fund 2026-10-14, nicht blockierend) Bei fehlgeschlagener
  `ensure_origin_column`-Migration `conn: None` zurückgeben: heute bleibt eine **halb
  funktionierende** Verbindung — `record`/`should_skip`/`stat_matches` scheitern dann still, der
  **ganze** Dedup-Index fällt aus (jede Datei würde wieder gelesen und neu gesendet), nicht nur die
  Herkunfts-Unterscheidung. Dazu den überzeichnenden Kommentar in `src/dedup.rs` korrigieren.

### Welle 6 — Post-MVP / bewusst später (dokumentiert, **nicht** eingeplant)
Pro-Ziel-Limit, netzwerkgesteuerte Drosselung, Cooldown nach Event-Läufen, Watcher-Sicherheitsnetz,
Queue-Paginierung, Fehlerfenster (`readdtoqueue`/`retryall`), Pause **pro Ordner**, Sortierung/Filter +
Größen-Spalte, Tray-Status-Popup, „Details anzeigen"-Panel, Videos, echte Speichernutzung (Quota),
`.app`-Bundle + Icon, „Startseite". **Out of Scope bleibt Out of Scope:** Sync, Downloads/Restore,
Datei-Management, Apple-Photos-Import, Autoplay, Upsell-/Quota-Banner, Auto-Update, Proxy,
ausgeblendete Dateien, iOS-Dedupe-Prompt, Crash-Relaunch.

## Recon-Notiz am Original (nur Doku, bewusst **keine** TODO daraus)

**„Löschen / Patchen" — direkt am DLL belegt (2026-10-14).** Antwort: das Original kann **alles
drei**. `CloudDriveService` baut neben dem Upload (`POST /v2/upload`, `PUT` bei `overwrite`) auch:
`DELETE <metadata>/nodes/{id}` (optional `?recurse=true`, „DeleteNode"),
`DELETE /v2/upload/multipart-upload/{id}` (Abbruch), `PUT <content>/nodes/{id}/content`
(Inhalt ersetzen), **`PATCH <metadata>/nodes/{id}`** mit `nodeChanges`-JSON
(`NullValueHandling.Ignore`, `HealthMetrics.Request.PatchNode`, für Umbenennen/Verschieben/
Favorit/Album) sowie `trash/{id}` und `trash/{id}/restore` (Verben dafür noch offen).
Nebenbefund: **unser `docs/PROTOCOL.md` enthält nur `POST`/`GET`**, weil ausschließlich
Upload-Flows mitgeschnitten wurden — Grenze der Aufzeichnung, **nicht** das Verhalten der App.
**Für unseren Port folgt daraus nichts:** Uploader-Scope, Löschen/Download/Papierkorb ist
bewusst Out. User-Vorgabe 2026-10-14: dokumentieren, **keine** konkreten TODOs ableiten.

## Scope-Inventar: vergessene Features & bewusste In/Out-Grenzen (User-Wunsch 2026-10-13)

Anlass: Beim Reverse-Engineering sind Features aufgetaucht, die in keiner TODO standen. Ziel ist eine
**absichtliche** Scope-Entscheidung — **nicht alles umsetzen**. „Scope" = Vorschlag/Empfehlung;
**nicht umsetzen** heißt: dokumentiert und bewusst draußen. Nichts hiervon ist implementiert.

Belege: `Localizable.strings` (Keys) + `Amazon Photos.dll` (Symbole), `docs/*`.

| # | Feature | Beleg | Status bei uns | Scope (Vorschlag) |
|---|---|---|---|---|
| 1 | **Bidirektionale Synchronisation** (Sync-Ordner) | `enablesync`/`disablesync`, `navigationMenu_sync`, `failedsyncitemswindow_*`, `SyncEngineQueueTimeoutInSeconds` | fehlt komplett | **Out / Post-MVP** — anderer Produktkern (Sync ≠ Backup) |
| 2 | **Interaktiver Dateinamens-Konflikt-Dialog** | `filenameconflict_overlayview_{title,body,ignore/replace/saveasnew}_*`, `_donotaskagain_`, `filenameconflict_toast` | „Nachfragen" existiert nur als Option, **kein** Dialog | **In (MVP-Lücke)** — sonst ist die Option wirkungslos |
| 3 | **Finder-/Kontextmenü-Service** „Bei Amazon Photos hochladen" | `context_menu_upload`, `menu_services`, `ServicesMenu.strings` | fehlt | **Out / Post-MVP** — OS-Integration |
| 4 | **Apple-Photos-/iPhoto-/Aperture-Import** | `mediaimport_*`, `onboard_noautosave_*` | fehlt | **Out** — nicht unser Datenmodell |
| 5 | **Ausgeblendete Dateien** (Setting) | `ExcludeHiddenFilesAndFolders` (Default true), `preferences_(in|ex)cludehiddenfiles` | fehlt | **Out / optional** — kleine, spätere Einstellung |
| 6 | **Proxy-Einstellungen** | `preferences_proxy_*` | fehlt | **Out** |
| 7 | **Speicher-/Quota-Balken** | `youraccount_*` | bewusst nicht (MVP) | **Out (bereits entschieden)** |
| 8 | **Videos hochladen** | ContentFilter-Bits, `no_photos_or_videos_found` | nur Bilder | **Out / Post-MVP (bereits getrackt)** |
| 9 | **Downloads / Restore** | `navigationMenu_download`, `displayitememptymessage_download_*` | fehlt | **Out (bereits entschieden: nicht MVP)** |
| 10 | **Datei-Management** (Albums, Favoriten, Papierkorb, Teilen) | `album`, `favorite`, `trash`, `restore` | fehlt | **Out** — Uploader-Scope |
| 11 | **Autoplay / Media-Player** | `mediaimport_autoplayaction` | fehlt | **Out** |
| 12 | **Upsell-/Trial-/Overquota-Banner** | `notification_{overquota,trialexpired,campaignwindow}_*`, `outage_banner_text` | fehlt | **Out** |
| 13 | **Auto-Update** | `DefaultAutoUpdateAfterTimeInSeconds` | fehlt | **Out** |
| 14 | **About / Privacy / ToS / Third-Party-Lizenzen** | `aboutwindow_*` | fehlt | **Out für MVP**, **In für ein öffentliches Release** (Lizenzhinweise) |
| 15 | **Menüleisten-Status-Popup** (Statistiken, Benachrichtigungen) | `statuspopup_stats_*`, `contextmenu_*` | Tray existiert (PORT-3), Popup-Detail fehlt | **Post-MVP** |
| 16 | **Tabellen-Sortierung/-Filter** (Name/Letzte/Status/Größe) | `statuswindow_{status,size,lastmodified}columntitle`, `statusall` | fehlt (keine Sortierung/Filter) | **Post-MVP** |
| 17 | **Größe-Spalte + relatives „Letzte Aktualisierung"** | `datetimeago_*`, `sizecolumntitle` | fehlt | **In** (in `SPEC_UPLOAD_PROGRESS.md` enthalten) |
| 18 | **iOS-Dedupe-Prompt** | `iosdedupeprompt_*` | fehlt | **Out** (Apple-Photos-Kontext) |
| 19 | **Mehrere Konten** | `folderconflict_*` „anderes Benutzerkonto" | fehlt | **Out / UNKNOWN** |
| 20 | **Crash-Relaunch (LaunchAgent)** | `WorkerMaxRestarts*`, macOS-LaunchAgent | fehlt | **Out** (macOS-spezifisch) |
| 21 | **Retry-All / Re-add-to-queue / Failed-Fenster** | `readdtoqueue`, `retryall`, `failedsyncitemswindow_retry` | fehlt | **Teilweise In** — im Upload-Progress-TODO (Retry) |
| 22 | **Sync-Mass-Delete-Benachrichtigungen** | `notification_syncmassdeletes_*` | fehlt | **Out** (Sync) |
| 23 | **„Alle Aktivitäten anhalten" global (Upload+Download)** | `activitywindow_*` | nur Backup-Pause | **In bleibt** (Download Teil davon Out) |
| 24 | **„Gleichzeitige Uploads"-Default = min(4, Cores)** | `UploadMaxQueues=4`, `MaxNumberOfQueuesPerProcessorCore=1.0` | Slider 1..8, Default **3** | **Entscheidung offen** — Original: `clamp(1, min(4, Cores×1), 8)` |

### Concurrency-Cap „8" — Research-Ergebnis (belegt, `Infrastructure.AppSettings`/`CustomerSettings`)

- `CustomerSettingsMaxSupportedUploadQueues = 8`, `…Min… = 1`; Downloads symmetrisch (8/1).
  Die „8" ist **kein technisches Limit**, sondern die Obergrenze einer **kunden-/server-gelieferten**
  Einstellung (`SettingsRefreshTimeInSeconds = 3600`), die der `CustomerSettings`-Getter per
  `CheckMinMax` klemmt (`IgnoreCustomerSettingsUploadQueuesSetting` = false ⇒ gilt).
- Der **Geräte-Default** ist kleiner: `UploadMaxQueues = 4` und `MaxNumberOfQueuesPerProcessorCore = 1.0`,
  also `UploadQueues = max(1, min(4, ProcessorCount × 1))` — typisch **4** (bei ≤4 Kernen entsprechend
  weniger). Download identisch (`DownloadMaxQueues = 4`).
- **UNKNOWN:** eine inhaltliche Begründung der 8 gibt es im Code nicht — nur den gesetzten Wert.
  Unser Slider `1..=8` trifft die Obergrenze; unser Default 3 weicht vom Original-Default ab.

## Bekannte Schwachstelle: `glib` (Dependabot, medium) — derzeit **nicht** fixbar (2026-10-14)

- **Befund:** GitHub/Dependabot meldet auf `main` **1 moderate** Schwachstelle: `glib`
  („Unsoundness in `Iterator`/`DoubleEndedIterator` impls für `glib::VariantStr`"), betroffen
  `>= 0.15.0, < 0.20.0`, Fix wäre **0.20.0**. Im Lock steht `glib 0.18.5`.
- **Herkunft (ausschließlich Linux-Backend, auf macOS nicht kompiliert):** `tray-icon` →
  `libappindicator 0.9.0` → `glib/gtk ^0.18`; `muda 0.21.2` → `gtk ^0.18`; `wry 0.57.0` →
  `webkit2gtk =2.0.2` → `glib/gtk ^0.18`. Damit hängt die komplette gtk-rs-0.18-Familie daran
  (glib, gio, gdk, gtk, gtk-sys, pango, atk, gdk-pixbuf).
- **Kein Upgrade-Pfad (belegt über die crates.io-API am 2026-10-14):** die jeweils **neuesten**
  Versionen verlangen weiterhin gtk-rs 0.18 — `tray-icon 0.26.1` (`libappindicator ^0.9`),
  `libappindicator 0.9.0` (`glib ^0.18`, `gtk ^0.18`), `muda 0.21.2` (`gtk ^0.18`), `wry 0.57.0`
  (`gtk ^0.18`), `webkit2gtk 2.0.2` (`glib/gtk ^0.18`). Ein isolierter `glib`-Bump ist
  ausgeschlossen, weil `gtk 0.18` `glib ^0.18` pinnt.
- **Folge:** Die Dependabot-Auto-Updates **scheitern deshalb** (Job „cargo in /. for glib - Update",
  Lauf `38542163059`, failure). Erwartbar — kein Config-Fehler, nicht „reparieren".
- **Einordnung (bewusst akzeptiert):** die Unsoundness betrifft glib-Iterator-APIs, die wir nicht
  aufrufen; sie liegt nur im Linux-Tray/WebView-Stack. Auflösbar erst, wenn `tray-icon`/`wry`/`muda`
  auf gtk-rs 0.20 wechseln. **Nicht eigenmächtig patchen** (Vendoring oder `[patch]` wäre ein neuer,
  ungeprüfter Eingriff und auf diesem Rechner nicht testbar). Beobachten: `cargo update` nach dem
  nächsten Tray-Icon-/wry-Release.
- **Fürs öffentliche Release:** als bekannte Einschränkung nennen — gehört zusätzlich in
  `docs/RELEASE.md` (noch zu ergänzen).

## UI-Befunde aus dem ersten echten Upload (User-Meldung 2026-10-14) — Fix-Paket offen

Beobachtet während des echten 3,8-GB-Laufs (`~/Pictures/Sport`, 480 Dateien) am **ungebündelten
Debug-Binary**. Vier Befunde, zwei davon mit User-Entscheidung:

1. **Umlaute als Kasten (`O□FB`)** — **erledigt** (Commit `212f8e1`): NFC-Normalisierung für den
   Anzeigetext **und** den Upload-Namen über `nfc()` in `src/text.rs`, Pfade und MD5-Index unverändert.
   Begründung (Server normalisiert nicht; macOS NFD vs. Windows NFC), die Korrektur am
   `27_ÖFB-Reisinger_01.jpg`-Fall und die verworfenen Ansätze stehen in `AGENTS.md` →
   „Dateinamen-Kodierung: NFC". Die zwei damals offenen kosmetischen Lücken sind inzwischen
   **erledigt** — `crate::text::nfc()` sitzt an beiden Stellen (`src/backup.rs:835`,
   `src/upload.rs:448`/`:452`/`:473`).
2. **Fortschritt doppelt angezeigt:** die Seiten-Fußzeile (P3, mit Spinner + „Gesamt: x von y · z % ·
   noch …") **und** die untere Statusleiste (`TopBottomPanel::bottom("log")`, `src/app.rs:808`,
   „Upload läuft: n/total · <Datei> · ETA").
   **Entscheidung (User 2026-10-14): genau EINE Fußzeile ganz unten, app-weit** — Spinner +
   Fortschrittsbalken + „Gesamt: x von y · z % · noch …" + aktuelle Datei/ETA in einer (etwas
   höheren) Zeile; die Seiten-Fußzeile aus P3 entfällt damit. Verworfen: „Seiten-Fußzeile behalten,
   unten nur Log" (dann fehlt Datei/ETA) und „zwei Zeilen unten" (mehr Höhe).
3. **Zeile zeigt „Fehler" statt „Upload läuft …"** (`src/backup.rs:1399` für die Ordner-Zeilen, `:1425`
   für die Einmal-Zeile). Die Zeile rendert den **persistierten** Status (`Row.status ← f.status`)
   aus dem letzten fehlgeschlagenen Lauf; der laufende Upload setzt ihn nicht auf „läuft". Solange
   das so ist, steht bei jedem neuen Lauf erst einmal wieder **„Fehler"** — für den Nutzer wertlos,
   weil es den Zustand *vor* dem Lauf beschreibt. Fix: Status **live** ableiten — enthält
   `folder_total` das Ziel, läuft dieser Ordner ⇒ „Upload läuft …"; erst nach `finalize` gilt der
   persistierte Ausgang.
4. **Aufklappen hält nicht (User: „bleibt nicht auf Dauer", `src/backup.rs:1327`).** Die
   Klapp-Überschrift heißt `tr1("Warteschlange ({})", remaining)`; ihr Text ändert sich mit **jedem
   Zähler-Update** (jede hochgeladene Datei). egui leitet die Widget-ID aus dem Label ab ⇒ neue ID
   pro Update ⇒ der Auf-/Zu-Zustand wird jedes Mal zurückgesetzt, die Liste klappt von selbst wieder
   zu. **Fix:** feste ID vergeben (`CollapsingHeader::id_salt("queue")` in egui 0.31) — dann bleibt
   der Zustand über Updates und Frames erhalten. **Dieselbe Fehlerklasse** gilt für jedes
   `CollapsingHeader`, dessen Label dynamische Zahlen enthält (beim Umbau mitprüfen).

**Beide Punkte (3+4)** sind der Grund, warum sich die Anzeige im ersten echten Lauf als
„dauerhaft Fehler + Liste klappt zu" zeigte. Sie gehen **zusammen mit dem Fortschritts-Umbau** in
**ein** UI-Paket (Welle 3; Recon liegt vor: `docs/SPEC_UPLOAD_PROGRESS.md` §7).

**Kein** App-Fehler (aufgeklärt): der **pixelige** Font in den Screenshots kam vom **ungebündelten**
Binary (per Terminal gestartet, ohne `Info.plist`). `packaging/Info.plist` enthält
`NSHighResolutionCapable = true`, das gebaute `.app` ist also retina-scharf. **Konsequenz:** für
Sichtprüfungen/Screenshots künftig das Bundle bauen (`packaging/make-app.sh`), nicht die nackte
Binary starten.

## Duplikate & Dateikonflikte — User-Entscheidung 2026-10-14: alle Varianten wie das Original

Recon-Grundlage: `docs/SPEC_DUPLICATES.md` (Mechanismen, Erkennungsweg, lokaler Zustand, 409,
Gap-Tabelle) und `docs/SPEC_CONFLICT_OPTIONS.md` (vollständige Varianten-Liste mit Abnahmekriterium,
kommt von einem Subagenten). Kernbefund: **zwei orthogonale Mechanismen** — Inhalts-Dedupe (MD5,
**über Pfade hinweg**, `GetNodeByMd5`) und Namenskonflikt (exakter Name + Parent, **pro Ziel**;
global existiert nichts).

**Stand:** die vier Konflikt-Optionen pro Ziel sind da (`ConflictResolution::{Ask,Ignore,SaveAsNew,
Replace}`, `src/folders.rs`; `wire_value()`), dazu der Application Default (2.3a). Der Inhalts-Skip
greift **über Pfade hinweg** und ist **persistiert** (`origin = 'content_skip'`, Welle 1/2), die
409-Ursache wird getrennt (2.1), „Ersetzen" läuft über `PUT /v2/upload/<nodeId>` mit dem POST-Query-Satz
(2.2/2.2b).

**Offen (Welle 2.4):**
- [ ] **Alle Varianten laut Liste** (`docs/SPEC_CONFLICT_OPTIONS.md`) umsetzen, jede mit
      Abnahmekriterium.

**Verworfen (2026-10-15):** „Nicht noch einmal fragen" **pro Datei** als neues persistiertes Feld.
Geltende Regel (2.3a/2.3b): die Antwort wird in das **bestehende**
`BackupFolder.conflict_resolution` des Ordners der gefragten Datei geschrieben, plus **Application
Default** für neue Ziele und Drag&Drop — **kein** Schemawechsel, **kein** Per-Datei-Feld.

## Offene Kandidaten derselben Absturzklasse (2026-10-14) — **ungeprüft**

Regel aus dem Absturz am Laufende (2026-10-14, behoben — `notify-rust` läuft jetzt über die
Main-Queue): **keine synchrone AppKit-/RunLoop-API direkt aus `update`.** Die Kandidaten standen
bisher nur im Fix-Bericht, nicht im Ledger — hiermit nachgetragen:
- [ ] **`rfd::FileDialog::pick_folder()`** (`src/backup.rs:808`, `src/backup_config.rs:59`) — öffnet
      auf macOS ein **modales** `NSOpenPanel` und blockiert mit eigener RunLoop. Dieselbe Klasse wie
      der Absturz (`notify-rust`). **Ungeprüft:** ob es real abstürzt, ist **nicht** belegt. Prüfen
      und ggf. auf den asynchronen Weg (`AsyncFileDialog` + `self.rt`) umstellen — der Tokio-Runtime
      existiert bereits (`src/backup.rs:925`).

## Zurückgestellt — braucht eine bewusste User-Entscheidung (nicht autonom lösbar)

Nichts da ist umgesetzt; alle Punkte sind bewusst **nicht** angefasst, weil sie eine Vorgabe brauchen.

1. **Hintergrund-Sync-Rest:** (a) ob nach einem Event-Lauf ein Cooldown gelten soll (mehrere Drops
   kurz hintereinander) und (b) ob ein Lauf nur bei bestehendem WLAN/nicht-kostenpflichtigem Netz
   starten soll — beides Plattform-APIs, noch nicht festgelegt.
2. **Pro-Ziel-Limit / netzwerkgesteuerte Drosselung** (bereits als Post-MVP markiert).
3. **macOS-Signatur:** selbstsignierte Identität (Keychain-Zugriff), ob Benachrichtigungen im
   signierten Bundle erscheinen, ob `--options runtime` für ad-hoc nötig ist.
4. **Logout während laufendem Upload:** `logout()` löscht die Auth-Cookies, stoppt aber den laufenden
   Lauf nicht. Soll der Upload stoppen/pausieren oder weiterlaufen und mit 401 scheitern? (Eine
   saubere Antwort bräuchte einen Abbruch-Kanal in der Upload-Pipeline.)
5. **Farbe/Sprache des Multi-Fenster-Verhaltens** („Einmalige Sicherung"-Zeilendarstellung).

## Offen (Backlog)

- [ ] **Optional/Branding:** Account-Block zeigt unter „Angemeldet als:" noch die Zeile „Amazon Photos"
  (Label des angemeldeten Amazon-Kontos, kein Eigen-Branding). Falls keine Amazon-Wortmarke im eigenen
  Fenster gewünscht ist: durch die Domain (`amazon.<cc>`) ersetzen. Bewusst nicht im Rename-Lauf geändert.
- [ ] „Details anzeigen"-Panel (verschachtelt/flach wie das Original, `docs/UI_FLOWS.md` §4.7).
- [ ] Menüleisten-Status-Item (Status-Popup: Status, Benachrichtigungen, Speicher, §9).
- [ ] Videos hochladen (bisher nur Fotos — `.mp4`/`.mov`/… werden wie alle Nicht-Bilder ignoriert).
  Original-Befund (`docs/`, nur gelesen): gleicher Upload-Endpoint (`kind=FILE`, nur `FILE` beobachtet),
  Modus nur über `content_filter`-Bitmaske (`None=0, Photos=2, Videos=4, Other=8, PhotosAndVideo=6,
  Everything=14`; dazu `DeduplicateOptions`, `docs/UI_STATES.md`-Tabellen). Unknown: welche
  Video-Extensions das Original akzeptiert; ob sich Upload-Parameter für Video unterscheiden;
  Quota-Frage (Prime: Fotos unbegrenzt, Videos begrenzt? — `…_notification_overquota_unlimitedphotos_*`,
  Videospeicher-Upsell-String).
- [ ] „Startseite"/Übersichtsseite? (Strings vorhanden, im Build ungenutzt.)
- [ ] Als `.app`-Bundle paketieren (Info.plist, Icon) — saubere Menüleiste/Doppelklick (.app und Menü hängen zusammen, siehe ⌘V).
- [ ] Rest aus `Amazon Photos.dll`: Quota-Form, exakter Extension-Filter, Watcher-Intervall, Concurrency-Defaults, Konfliktbehandlung, Video-Support.
- [ ] Echte „Speichernutzung"-Anzeige (nur Prime; bis dahin bewusst nichts anzeigen).
- [ ] **`pct` in der Ordner-Zeile kappen** (`src/backup.rs:1495` rechnet `done/total` ungekappt): ein
  inkonsistenter Alt-Zustand (`total_files` < `uploaded_files` — genau so wurden einmal „320 %"
  angezeigt) würde wieder über 100 % anzeigen.
- [ ] **Win/Linux: In-App-„Beenden".** Ohne sichtbares Tray-Icon gibt es dort keinen Weg zurück
  (macOS ist safe, ⌘Q) — Residuum aus PORT-3.
- [ ] **Gezeichnete UI-Glyphen ersetzen:** `nav_icon`, Flagge und Logo auf die Material-Symbols-
  Pipeline umstellen (Icons werden nicht handgezeichnet, siehe `AGENTS.md`); nicht beauftragt.
- [ ] Download-Vorgang (Herunterladen-Tab des Originals) — relevant, aber explizit **nicht** Teil des
  MVP (siehe `AGENTS.md` → „Scope-Grenzen (In/Out)").

## Verworfen

- Separates Login-Fenster (`ppu-login` Helper-Prozess mit eigener `tao`-Eventloop): wirkte ungewohnt und passte nicht zum Original, das im Hauptfenster anmeldet. Ersetzt durch die eingebettete Child-WebView im Hauptfenster.
- Synchrone Cookie-Abfrage in `update` (wry `WebView::cookies`): Panic durch winit-Reentrancy (dreht die NSRunLoop). Ersetzt durch `dispatch_async` auf der Main-Queue.
- mitmproxy live capture: kept only as a debugging tool; app uses the cookie upload path.
- Device registration / OAuth from scratch: replaced by browser-cookie auth.
- Standalone paste-only login: superseded by CDP browser login (paste stays as fallback).
- **Avatar-Thumbnail per Raten/Probieren (2026-10-14, verworfen):** `/drive/v1/nodes/{id}/content` (mit und ohne `tempLink=true`), `/drive/v1/nodes/{id}/thumbnail` (mit `width`/`height`/`transform`), `/drive/v1/nodes/{id}/transforms/IMAGE_THUMBNAIL` (mit und ohne `?tempLink=true`) sowie `/drive/v1/nodes/{id}/content?transform=IMAGE_THUMBNAIL&tempLink=true` — **alle HTTP 404**. Der Parameter `?transform=IMAGE_THUMBNAIL` wird **ignoriert** (der `tempLink` liefert weiterhin die Rohdatei). Das `transforms: ["IMAGE_THUMBNAIL"]` eines Suchtreffers ist eine **Deklaration**, keine URL. Stattdessen belegbar über das Assembly (`GetSmartCroppedThumbnail`/`GetThumbnailUrl`) — Recon läuft, siehe Welle 3.
- **Avatar lokal dekodieren/schneiden (2026-10-14, verworfen):** der Avatar-Quellknoten kann ein RAW sein (bei diesem Konto ein Canon CR3, 13,5 MB) — die `image`-Crate kann kein CR3. Der 60×60-Crop muss daher **serverseitig** passieren (wie im Original), nicht bei uns.
- **`deepseek-v4.1-flash` als Umsetzer-/Verifier-Modell (2026-10-14 gesetzt, 2026-10-15 zurückgenommen):** lief genau einen Tag, ohne dass ein Fehler oder Gate daran schuld war — der User hat schlicht wieder zurückgestellt („ab jetzt wieder step 5 preview für subagenten nutzen"). Gültig sind damit wieder `opencode-go/step-5-preview-free` für **beide** Rollen, dieselbe Vorgabe wie global. Nicht „reparieren": ein erneuter Wechsel auf ein Gratis-/Billig-Modell für Subagenten braucht eine **neue** User-Entscheidung, nicht den alten Eintrag.
