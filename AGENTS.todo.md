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
- **2.2b „Ersetzen" korrigieren — ✅ umgesetzt und verifiziert 2026-10-15 (Verifier APPROVE,
  82 Tests); Live-Abnahme offen.** Unser `PUT` trug nur `conflictResolution=NONE`; der Server
  verlangt die **Parameter des POST** (live: ohne `fileSize` ⇒ **400 `MissingFileSize`**, mit dem
  POST-Satz kommt er durch die Validierung). **Geltende Regel (auch in `AGENTS.md`): der `PUT` trägt
  denselben Query-Satz wie der POST** — gebaut **einmal** in `urls::upload_query`, benutzt von POST
  **und** PUT, kein geratenes Subset; welche Parameter im Einzelnen Pflicht sind, bleibt bewusst
  offen. Der Invarianten-Test `replace_upload_url_carries_the_same_query_as_the_post` pinnt genau
  das. **Zweiter Live-Befund:** der `PUT` unterliegt der Inhalts-Dedupe (identische Bytes ⇒ 409 statt
  200) — deshalb wird ein Inhaltsduplikat **nie** per PUT „ersetzt". **Abnahme (offen):** derselbe
  Live-Lauf noch einmal (Knoten behält den Namen, Inhalt danach nachweislich der neue —
  Größe/MD5/Version), danach Knoten + Testziel wieder entfernen. **Zwei Low-Funde des Verifiers sind
  bewusst offen gelassen** (reine Doku, kein Logikfehler) und stehen in der 2.4-Liste.
- **2.3 Konflikt-Standard in drei Ebenen — ✅ erledigt 2026-10-15:** (a) Application Default in
  `Config` + Einstellungen → Upload *(`db68cff`)*, (b) „Nicht noch einmal fragen" pro Ordner
  persistiert *(`a71191b`)*, (c) **kein** Per-Datei-Default. Details unter „2.3 Konflikt-Standard in
  drei Ebenen" weiter unten. **Achtung:** der Default-Wert ändert sich am 2026-10-15 von `SaveAsNew`
  auf `Replace` (2.4a).
- **EXIF-loser Upload darf kein Fehler sein — ✅ erledigt 2026-10-15 (`261501e`, Verifier APPROVE,
  90 Tests).** Ein **erfolgreicher** Upload von `c2.png` lieferte gültiges JSON mit `id`/`name`, unser
  `ResponseUpload` verlangte aber `resolutionUnit` ⇒ serde brach ab („missing field
  'resolutionUnit'") ⇒ der Lauf meldete **Fehler** statt Erfolg und schrieb **keine** Indexzeile (die
  Datei wurde bei jedem Lauf erneut gelesen und gehasht). Betroffen sind alle EXIF-losen Typen, die
  `is_supported_image` zulässt (im Sport-Ordner mit nur JPEG/CR3 unsichtbar). **Fix:** gelesen wird aus
  der Antwort ohnehin nur `id` — jetzt ist jedes Feld außer `id` optional (`#[serde(default)]`,
  `modifiedDate`/`createdDate` als `Option<DateTime<Utc>>`), `id` bleibt **Pflicht** (kein
  Container-`default`, kein `Default`-Derive — der Verifier hat das per Mutation belegt: mit
  Container-Default wird der `id`-Test rot). Vier Tests: Live-Fall (`image` ohne `resolutionUnit`),
  Body nur mit `id`, das vollständige `docs/PROTOCOL.md`-§2.17-Beispiel als Regressionsschutz, `id`
  bleibt Pflicht. **Bekannte, gewollte Folge:** fehlt künftig ein Nicht-`id`-Feld, greift ein Default
  statt eines Parse-Fehlers — heute liest diese Felder niemand.
  **Drei Low-Nits des Verifiers → in die 2.4b-Liste (h)/(i) aufgenommen:** die Aufzählung „png, bmp,
  gif, webp" (`docs/SPEC_UPLOAD.md:146`) ist nicht abschließend (tif/tiff fehlen, webp *kann* EXIF
  tragen) und „differs only in how `id` is treated" (`src/prime_photos.rs:286`) ist leicht überzogen.
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
  gibt es auf dem Cookie-Host **nicht** (⇒ 404). **Geklärt 2026-10-15:** die 10-Minuten-Regel ist
  **Original-Parität** (`docs/SPEC_UPLOAD.md:106-113`), keine eigene Erfindung — sie bleibt. Folge
  bewusst akzeptiert: eine EXIF-lose Datei, die jünger als 10 Minuten ist, wird in **diesem** Lauf
  nicht geladen (Fehlerzeile nur im UI-Log, `status` bleibt `uploading`), geht aber **nicht**
  verloren — ein späterer Event-/Wasserzeichen-Lauf nimmt sie mit, sobald sie alt genug ist.
- **2.3 Konflikt-Standard in drei Ebenen** (User-Entscheidung 2026-10-15):
  **(a) Application Default** — ✅ **erledigt 2026-10-15** (`db68cff`, Verifier APPROVE, 81 Tests):
  Feld in `Config` (`config.toml`), Radiogruppe in **Einstellungen → Upload**, Default `SaveAsNew`,
  **Vorauswahl beim Anlegen eines Ziels** **und** für die **Einmal-Queue (Drag&Drop)**.
  **(b) „Nicht noch einmal fragen" pro Ziel — ✅ erledigt 2026-10-15 (`a71191b`, Verifier APPROVE,
  86 Tests):** die Antwort wird in das
  **bestehende** `BackupFolder.conflict_resolution` des **Ordners der gefragten Datei** geschrieben
  (`folders.toml` — kein Schemawechsel, das Feld existiert); **kein Per-Datei-Default**; innerhalb
  des Laufs bleibt der Override **run-weit** (heutiges Verhalten unverändert).
  **Einmal-Queue (Drag&Drop) ausgenommen** (User-Entscheidung 2026-10-15): dort gibt es keinen Ordner
  in `folders.toml` (`ONE_TIME_IDX`, `src/app.rs:26`) — das Häkchen wirkt nur **run-weit**; beim
  nächsten Drop entscheidet wieder der Application Default. **Die Drop-Liste selbst bleibt davon
  unberührt persistiert** (SQLite `one_time_queue`, Entscheidung 2026-10-14: beim Start
  wiederhergestellt, gelöscht erst nach vollständig abgeschlossenem Lauf) — nicht persistiert wird
  hier ausschließlich die **Konflikt-Antwort**. **Umsetzung:** genau **eine** Schreibstelle
  `remember_conflict_in_folder` (`src/backup.rs:825`) mit drei Guards — One-Time-Index, Index
  außerhalb des Stores, Datei **nicht** unter dem Ordner (deckt einen mitten im Lauf entfernten
  Ordner: `FolderStore::remove` **verschiebt** die Indizes); `UploadShared.pending_conflict_idx`
  und `-path` werden zusammen mit `pending_conflict` unter **einem** Lock gesetzt und auf **jedem**
  Ausgang geleert. Der Verifier hat unabhängig am Code belegt, dass der Worker-`idx` wirklich der
  `store.folders`-Index ist, und das Timeout-Race als **theoretisch** eingeordnet.
  Wörtlich: „es soll der ordner default sein, einzelne dateien haben keinen Default. Zusätzlich soll
  es auch einen Application Default geben, der die Standardauswahl bei einem neuen Folder und die
  Drag & Drop Sachen haben das ApplicationDefault."
  Abnahme (b): nach „Ersetzen" + Häkchen steht `conflict_resolution = "replace"` im Ordner und der
  **nächste** Lauf fragt dort nicht mehr, während ein zweiter Ordner weiterhin fragt.
- **2.4 — der Rest laut `docs/SPEC_CONFLICT_OPTIONS.md` (User-Entscheidung 2026-10-15: Umfang
  „Wortlaut angleichen"), in zwei Paketen.** Bewusste Abweichungen dokumentieren: die
  Client-Vorprüfung des Originals ersetzen wir durch die Server-Antwort (kein Node-Cache), die
  `.heic`-iOS-Heuristik bleibt draußen, und wir fragen **pro Datei** statt im gebündelten Sheet.
  **2.4a — Code/UI — ✅ erledigt 2026-10-15 (`36aee39`, Verifier APPROVE, 95 Tests).** Umsetzung wie
  unten beschrieben, alle drei Abnahmekriterien erfüllt; der Verifier hat sie per **Mutation in einer
  TMPDIR-Kopie** belegt (ohne `default_replace` fallen beide Default-Tests, ohne `#[serde(default)]`
  auf `BackupFolder.conflict_resolution` fällt der `SaveAsNew`-Fallback-Test, die beiden Label-Tests
  sind in beide Richtungen scharf). **Drei nicht blockierende Funde** → „Offene Befunde" unten.
  Beauftragt war:
  - **Globaler Dedupe-Default:** neues `Config.dedup_default` (Default **`true`**, serde-Default
    `true`, damit alte `config.toml` lädt), Einstellung in **Einstellungen → Upload**; er ist die
    Vorauswahl beim Anlegen eines Ziels **und** gilt für die Einmal-Queue (Drag&Drop). Bestehende
    Ziele behalten ihren Wert, keine Migration. Abnahme: ein neues Ziel übernimmt den Default statt
    hart `true`; Tests pinnen `Config::default().dedup_default` und den serde-Default ohne Key.
  - **Konflikt-Default → `Replace`** (User-Entscheidung 2026-10-15): `Config.conflict_resolution_default`
    defaultet auf `Replace` (auch im serde-Default). Der **Enum-`#[default]` bleibt `SaveAsNew`** —
    das ist der **Datei**-Fallback für `folders.toml`-Einträge ohne Schlüssel; sonst sprängen
    bestehende Ziele still aufs Überschreiben. Abnahme: der Legacy-Key-Test erwartet jetzt `Replace`,
    der Enum-Fallback-Test weiterhin `SaveAsNew`.
  - **Wortlaut-Parität:** `SaveAsNew` heißt in beiden Radiogruppen „Neue Datei automatisch
    umbenennen" (de+en); der Dialog-Knopf bleibt „Als neue Datei speichern". Das Häkchen im
    Konflikt-Dialog nennt den Ordner („Nicht noch einmal nach ‚<Ordner>'-Ordner fragen"), bei der
    Einmal-Queue bleibt „Nicht noch einmal fragen".
  **2.4b — Doku + zwei Tests (nach 2.4a):**
  (a) Test für **gepaddete** Inhaltsduplikat-Codes („ DuplicatesConflictError " ⇒ **kein** PUT —
  `is_content_duplicate_code` trimmt, aber kein Test pinnt das); (b) Test für den **Byte-Durchgriff**
  einer Nicht-ASCII-Node-ID durch `replace_upload_url`; (c) `docs/SPEC_CONFLICT_OPTIONS.md` §4, Satz
  „der Worker sieht für `Replace` **kein** 409" ist zu groß formuliert (beim Inhaltsduplikat sieht er
  sehr wohl einen); (d) `docs/SPEC_SYNC.md:734` und `docs/SPEC_LIBRARY.md:649` führen `OVERWRITE`
  noch als unseren Vokabular-Wert; (e) die vorbestehenden stale Zeilenreferenzen in
  `docs/SPEC_CONFLICT_OPTIONS.md` (§1.2 B1/B2/B3, §7-Refs auf `upload.rs`/`dedup.rs`) und der
  Widerspruch in `docs/SPEC_DUPLICATES.md` §3 („Dedupe über Pfade hinweg = fehlt"); (f)
  `src/prime_photos.rs:1139` („live-proven: 200"-Kommentar), `src/folders.rs:153`,
  `docs/SPEC_DUPLICATES.md:286-287`. **(g) NEU (durch 2.3b):** `docs/SPEC_CONFLICT_OPTIONS.md:161`
  behauptet „nicht persistiert" — überholt, die Antwort wird **pro Ordner** persistiert; dort auch die
  §3-Zeile „Konflikt-Dialog" und die §5-Zeile „Default `None`" auf den neuen Stand bringen (Default
  `Replace`, per-Datei-Dialog als bewusste Abweichung). **(h) NEU (EXIF-Verifier):**
  `docs/SPEC_UPLOAD.md:146` — die Aufzählung „png, bmp, gif, webp" ist nicht abschließend (tif/tiff
  fehlen, webp *kann* EXIF tragen). **(i) NEU (EXIF-Verifier):** `src/prime_photos.rs:286` „differs
  only in how `id` is treated" ist leicht überzogen. **(j) NEU:** den gebündelten Original-Sheet
  („N Dateinamenkonflikte in …", eine Antwort für alle, Toast) als bewusste Abweichung in §5
  dokumentieren. **(k) NEU (Hygiene-Verifier):** in `docs/SPEC_ABOUT.md` ist „der Papierkorb ist leer"
  per **TCC nicht messbar** — nur die belegte Aussage stehen lassen (die App liegt nachweislich nicht
  dort, weil sie lesbar unter `/Applications` steht). **(l) NEU (Aufräumkandidat):** den toten
  i18n-Key „Als neue Dateien speichern" entfernen und den Doku-Kommentar an `SaveAsNew` in
  `src/folders.rs` auf den gültigen Wortlaut bringen (Radiogruppen: „Neue Datei automatisch
  umbenennen"; Konflikt-Dialog: „Als neue Datei speichern").

  **2.4b — ✅ erledigt 2026-10-15 (`40cf024`; Verifier Runde 1 = CHANGES REQUIRED, danach
  Ein-Zeilen-Fix in `docs/SPEC_DUPLICATES.md`, Abnahme ohne neue Runde zugesagt).** Umgesetzt sind
  (a)–(l) plus die zwei Tests **mit Mutationsbeweis**: `padded_content_duplicate_codes_stay_out_of_the_replace_put`
  pinnt den einzigen Produktivaufruf (`should_replace_after_conflict` im 409-Zweig) mit einem echten,
  gepolsterten 409-Body — ein Inhaltsduplikat darf nie in den PUT, ein Namenskonflikt schon; ohne
  `trim()` in `is_content_duplicate_code` fällt der Test. `replace_upload_url_passes_a_non_ascii_node_id_through_byte_exactly`
  pinnt, dass die Server-Node-Id byte-identisch durchläuft, während `name` im selben Query NFC-gefaltet
  wird; eine NFC-Faltung der Id fällt auf. Der Verifier hat die vier Korrekturen des Umsetzers als
  **nötig und richtig** bestätigt (Cross-Path-MD5 existiert seit `f3c773b`, globaler Dedupe-Default seit
  `36aee39`, `Ignore` ist nirgends Default) und die Gates nachgemessen: **97 Tests, 0 Warnungen**, keine
  neuen fmt-Diffs. Offen bleibt allein die **Live-Abnahme des PUT** (Leitung — nicht durch Tests
  abdeckbar).

  **Zweiter Verify-Lauf (nachgeholt, frische Session): APPROVE.** Beide Tests mit Mutationsbeweis
  scharf, alle Punkte (c)–(l) und die vier Eigenkorrekturen bestätigt, Umfang = genau die 10 Dateien,
  `cargo clean -p` + Build 0 Warnungen, 97 Tests, keine neuen fmt-Diffs. Sein Fund 1 (die stale
  Gap-Zeile) war zum Zeitpunkt des Laufs **schon behoben** — er hatte gegen den `a5a9e2d`-Snapshot
  gelesen; die Funde 2 und 3 sind echt und stehen unten in der Befundliste.

  **Zwischenpakete (2026-10-15, nicht Teil der Welle-2-Planung; je eigener Commit):**
  - **P3-Nits — ✅ `506df58`** (Verifier APPROVE): die aggregierte Fußzeile wird nur noch unter
    `running || sum_total > 0` gezeichnet — bei laufendem Lauf sichtbar auch ohne Ziele, und ein nie
    gelaufenes Ziel (0 von 0) zeichnet gar nichts mehr statt „Gesamt: 0 von 0 Dateien"; dazu die
    Kommentar-Korrektur in `src/app.rs` (`Idle` zeigt denselben „{}% hochgeladen"-Text wie
    `Uploading`, nur gedämpft). Lock-Disziplin unverändert. Stammt aus einer früheren Session, die
    erst nach einem Server-Neustart zurückmeldete.
  - **About-Belegbasis — ✅ `2803aa9`** (Verifier Runde 1 = **CHANGES REQUIRED**, siehe unten):
    `docs/SPEC_ABOUT.md` behauptete, das `.app`-Bundle sei nicht mehr lesbar und liege nicht mehr
    unter `/Applications`. **Falsch:** es steht seit **2026-10-14 18:14** wieder dort und ist
    vollständig lesbar (`11.3.0` / `CommitId 1d7941b5`, 812 Einträge in `Contents/Resources`,
    Assembly lesbar). Wahr bleibt: `~/.Trash` ist per **TCC** nicht lesbar (und leer — die App liegt
    nachweislich nicht dort), und ein **neues Decompile** geht derzeit nicht (`ilspycmd` vorhanden,
    .NET-Runtime fehlt). Die Notiz hätte den nächsten Agenten dazu verleitet, das in `AGENTS.md`
    vorgeschriebene Zurücklegen des `.app` vor dem Recon zu überspringen — genau der Schaden, den
    die Regel verhindern will. Korrigiert nach dem Wortlautvorschlag des Verifiers, Zahlen vom
    Korrektor selbst nachgemessen; Re-Check der Doku im nächsten Verify-Lauf.
  - **Aufräumpaket — ✅ `b0fa4c4`** (User-Entscheidung 2026-10-15; Verifier APPROVE, Runde 1):
    **Baseline 7 → 0 Warnungen** plus die `app.rs:346`-Konsistenz, gelaufen **vor 2.4b**, damit alle
    weiteren Wellen gegen null messen. Der Verifier hat nach `cargo clean -p` **voll neu kompiliert**
    (0 Warnungen, nicht aus dem Cache), die HEAD-Baseline mit exakt den sieben erwarteten Warnungen
    nachgestellt und per Mutation belegt, dass ohne `1.6_f32` genau diese eine Warnung zurückkehrt.
    Alle drei entfernten toten Funktionen sind repo-weit referenzfrei (lebender Ersatzpfad je Fall
    belegt), das **einzige** `#[allow]` im Diff ist das bewusst begründete auf
    `supported_extensions`; Tests 95 grün, keine neuen fmt-Diffs.

### Neue Welle: Sitzung wie das Original (Login einmal + Refresh-Token) — **User-Entscheidung 2026-10-15**

**Auftrag (wörtlich, zwei Nachrichten):** „Ziel wäre so gut es geht so wie im Original mit der Anmeldung
/ Update der Tokens zu verfahren" und „Ziel: nur einmal Anmeldung nötig, refresh token speichern.
Nachbau Original hier mit Rea clean room spec für Login / auth. Gerne aber unsere aktuelle Login Ui so
gut wie es geht beibehalten." Die verbindliche Regel steht in `AGENTS.md` („Sitzung wie das Original");
Clean-Room-Grundlage ist `docs/SPEC_LOGIN.md` § 3.8 + `docs/PROTOCOL.md` § 2.1 (Recon 2026-10-15 am
DLL, siehe „Sitzung am Leben halten — Recon am Original").

**Entschieden (2026-10-15, Frage + Antwort):** Reihenfolge **Sitzung zuerst (S1–S7), dann Welle 3**;
die Live-Abnahme von „Ersetzen" (2.2b) fährt **zusammen mit S7** — kein doppeltes Anmelden. Der Auftrag
lautet „alles in Wellen implementieren bis fertig": Sitzung → Welle 3 → Welle 4 → Welle 5 (Welle 6
bleibt dokumentiert, nicht eingeplant).

**Fachliste (bleibt gültig):**
- [ ] **S1 — Anmeldeweg auf das Portal umstellen:** `login_url` auf den Parametersatz des Originals
      (`amzn_photos_desktop_<region>`, marketplaceId, language, PKCE-Paar, `client_id=device:<hex>`,
      `return_to=/ap/maplanding`, `forceMobileLayout`). **Unsere Login-UI bleibt** (Header, Abbrechen,
      Marktplatz-Auswahl, Fenstertitel) — nur URL und Erfolgserkennung ändern sich.
- [ ] **S2 — Erfolgserkennung umstellen:** statt Cookie-Prädikat (`session-id` + `at-acb*` + `x-acb*`)
      den `openid.oa2.authorization_code` aus einer `/ap/…`-Navigation abfangen (Navigation verwerfen).
      Die bisherige Cookie-Übernahme bleibt als **Rückfall** erhalten.
- [ ] **S3 — Code einlösen + Tokens ablegen:** `POST https://api.amazon.com/auth/register` (Felder wie
      das Original), Refresh-Token in den **OS-Schlüsselbund** (eigener Service, cross-platform Crate),
      Access-Token nur im Speicher; leerer Access-Token = Fehlschlag, abgelehnte Client-Identität = klare
      Meldung + Rückfall.
- [ ] **S4 — Auffrischen:** 80 %-Regel (Ablauf = `expires_in` − 60 s), eigene dokumentierte
      Versuchs-Defaults, fataler Ausgang = Sitzung ungültig.
- [ ] **S5 — Cookies bei Bedarf erzeugen:** `/auth/token?requested_token_type=auth_cookies`, in die
      bestehende Cookie-Kette einspeisen; Upload-Engine unverändert.
- [ ] **S6 — Ungültig/Abmelden:** Tokens löschen → unser Login-Screen (ohne App-Neustart); „Abmelden"
      über `/auth/deregister` + Token-Löschung.
- [ ] **S7 — Live-Abnahme:** einmal anmelden, App neu starten (Sitzung läuft **ohne** Anmeldung weiter),
      Auffrischung im Log, Cookies frisch geholt, Upload läuft. Damit kann auch die noch offene
      Live-Abnahme des `PUT` (2.2b) in derselben Sitzung gefahren werden.

**Verworfen (2026-10-15):** der reine Web-/Cookie-Weg als **Ziel** (bleibt nur Rückfall). Der
**CDP-Browser-Login** (externer Browser) bleibt ebenso verworfen wie die **Paste-Variante** als
Hauptweg. Ebenfalls verworfen: die Original-Login-**UI** (zwei Spalten, Willkommens-Panel, Fenster
880 × 530) zu übernehmen — unsere UI bleibt.

**Umsetzungs-Schnitt (Orchestrator, aus dem Recon abgeleitet — je Paket Build → unabhängige
Verifikation → Commit):**
- **S-A — `src/auth.rs` (neu) + `src/urls.rs`:** reine, testbare Bausteine **ohne** UI-Verdrahtung:
  PKCE-Paar, Portal-URL, Marktplatz-IDs, Client-Identität, JSON-Körper/Antworttypen für
  `/auth/register`, `/auth/token` (Refresh) und `/auth/token?…auth_cookies`, Abbildung der
  Cookie-Antwort auf unsere `Config`-Felder, 80-%-Terminberechnung, Schlüsselbund-Ablage
  (neue Abhängigkeiten `sha2`, `base64`, `rand`, `keyring`). Am Verhalten der App ändert sich nichts.
- **S-B — Verdrahtung `login.rs`/`app.rs`:** Portal-URL statt `amzn_photos_web_<cc>`, Code-Fang im
  Navigations-Handler (Navigation verwerfen), Einlösung, Refresh-Token in den Schlüsselbund, Cookies
  erzeugen und in die bestehende Kette einspeisen; der bisherige Cookie-Weg bleibt **Rückfall**.
- **S-C — Auffrischung, ungültige Sitzung, Abmelden:** 80-%-Tick, `/auth/deregister` beim Abmelden,
  ungültige Sitzung ⇒ Tokens löschen + Login-Screen.
- **S-D — Live-Abnahme (Hauptagent):** einmal anmelden, Neustart **ohne** Anmeldung, Auffrischung im
  Log, Upload läuft; darin die offene `PUT`-Abnahme (2.2b).

**Modell-Ausnahme (2026-10-15, Rate-Limit):** Der Gratis-Pool
(`opencode-go/step-5-preview-free`) antwortete mehrfach mit „Rate limit exceeded" — mitten in
laufenden Aufträgen, nachdem schon Teile geschrieben waren. Auf User-Anweisung gilt für diese Welle
deshalb: **`opencode-go/longcat-2.5-preview-free` zuerst**, **`opencode-go/deepseek-v4.1-flash`** nur
als **Fallback**, wenn Longcat denselben Fehler liefert (User-Korrektur 2026-10-15: „Umgekehrt.
Longcat Preview priorisieren, DS Flash als fallback"). Bestätigt 2026-10-15: „Longcat 2.5 Preview für
subagenten nutzen" — Longcat ist damit das **Standard-Modell für alle Subagenten-Läufe** dieser
Welle (Umsetzer, Verifier, Doku), DeepSeek Flash nur noch Fallback. Longcat lief in dieser Welle ohne
Rate-Limit-Fehler. Die Regel „ein Modell für alle Rollen"
(`step-5-preview-free`) bleibt gültig — das ist eine **Lauf-Ausnahme**, kein Config-Modellwechsel;
sobald der Gratis-Pool wieder antwortet, laufen die Wellen wieder dort.

**Recon-Fakten, auf denen der Nachbau steht (2026-10-15 am DLL; Verhalten in `docs/SPEC_LOGIN.md`
§ 3.8 / `docs/PROTOCOL.md` § 2.1, die Zahlen hier nachgetragen):**
- Portal-Parameter: `openid.ns`, `openid.claimed_id`/`openid.identity` = `identifier_select`,
  `openid.mode=checkid_setup`, `openid.oa2.scope=device_auth_access`, `openid.ns.oa2`,
  `openid.oa2.response_type=code`, `openid.oa2.code_challenge_method=S256`,
  `openid.oa2.client_id`, `language`, `marketPlaceId`,
  `openid.return_to=https://www.<domain>/ap/maplanding`, `openid.pape.max_auth_age=0`,
  `forceMobileLayout=true`, `openid.assoc_handle` **und** `pageId` = `amzn_photos_desktop_<region>`
  (gb→uk).
- `code_verifier` = UrlSafeBase64 (ohne `=`-Padding) von 32 Zufallsbytes; `code_challenge` = dasselbe
  von `SHA-256(verifier)`.
- `client_id` = `"device:" + hex(utf8("<device_serial>#<device_type>"))`, **lowercased** — das ist
  **kein** Hash, sondern der Hex-Dump der Bytes. Gerätetyp des Originals: `A2UXDCSFQXA1M7`;
  `device_serial` = 32-Zeichen-Hash, lokal **einmal** erzeugt und gespeichert.
- Marktplatz-IDs: DE `A1PA6795UKMFR9`, GB `A1F83G8C2ARO7P`, US `ATVPDKIKX0DER`, FR `A13V1IB3VIYZZH`,
  IT `APJ6JRA9NG5V4`, ES `A1RKKUPIHCS9HS`, JP `A1VC38T7YXB528`, CA `A2EUQ1WTGCTBG2`,
  AU `A39IBJ37TRP1C6`, BR `A2Q3Y263D00KWC`, CN `AAHKV2X7AFYLW`.
- `/auth/register`-Körper: `registration_data` (app_name, app_version, device_type, device_serial,
  device_model, os_version, `domain="Device"`) + `auth_data` (`use_global_authentication=false`,
  `authorization_code`, `code_verifier`, `code_algorithm="SHA-256"`, `client_id` **ohne**
  `device:`-Präfix, `client_domain="DeviceLegacy"`), dazu die Defaults
  `requested_extensions=["customer_info","device_info"]`, `requested_token_type=["bearer","mac_dms"]`,
  `user_context_map={}`. Antwort: `response.success.tokens.bearer{access_token, expires_in,
  refresh_token}` — **dort** liegt der Refresh-Token.
- Panda-Basis ist `https://api.amazon.com`; der Host wird auf den Marktplatz umgeschrieben
  (`api.amazon.de`). Retry-Defaults des Dienstes: `PandaServiceRetries=5`, BackOff-Faktor `1.5`,
  MaxBackOff `60 s`, Netz-Timeout `1 s`.
- `auth_cookies`-Antwort: `response.tokens.cookies` ist eine **Map Domain → Array** von
  `{Name, Value, Path, Expires, HttpOnly, Secure}` — der **Domain-Schlüssel** ist die Cookie-Domain
  (nicht im Cookie-Objekt enthalten).
- Zugriffs-Token-Ablauf = `expires_in − 60 s`; nächster Auffrischungstermin = **80 % der
  Restlaufzeit**. Die Auffrischungswerte sind **belegt** (Verifier 2026-10-15, `AppSettings` der
  App-Assembly): `AccessTokenMaxRetries = 5`, `AccessTokenTimeBetweenRetriesInSeconds = 15`,
  Ticktakt des `TickTockTimer` = **3 s**, dazu `PandaServiceRetries = 5`,
  `PandaServiceBackOffFactor = 2.0` (der Map-Default ist 1.5 — die App-Settings binden 2.0),
  `PandaServiceMaxBackOffInSeconds = 60` (alle per Settings-Datei überschreibbar; das Bundle liefert
  keine). **Unser Nachbau nimmt diese Werte** — kein eigener Default mehr; der Einlöse-Pfad behält
  ohnehin die belegten **3** Versuche.
- Abmelden = `POST /auth/deregister` (Bearer = Access-Token, Körper
  `request_metadata{app_name, app_version}`), danach Tokens löschen.

**Nachzuziehen:** Der Doku-Verifier hat 6 Befunde + 3 Präzisierungen gemeldet (u. a. § 7 #12/#13/#14
sind **belegt** statt *unknown*, `amzp-rt` ist ein Keychain-Account-Name und kein geschriebener Pfad,
`auth/deregister` löscht Tokens nur bei Rückkehr, unserer HEAD **erneuert** sehr wohl
`cookie_x_amz_access_token`); das Korrekturpaket läuft als eigenes Doku-Paket (S-Doku) und wird vor dem
Sitzungs-Umbau abgenommen. **Geräte-Identität entschieden** (User 2026-10-15, nach der Suche): die
Werte des Originals (`app_name = "Amazon Photos"`, `app_version = "11.3.0.1d7941b5"`,
`device_type = "A2UXDCSFQXA1M7"`); im Bundle existiert **genau ein** Gerätetyp. Nur der
`device_serial` ist unserer (32 Zeichen, einmalig lokal erzeugt, in `config.toml`). Details in
`AGENTS.md` („Sitzung wie das Original").

### Welle 3 — Progress-/UI-Umbau (entschieden; Recon liegt vor: `docs/SPEC_UPLOAD_PROGRESS.md` §7)
- **3.1** Detail in den **Hauptbereich**, Klapp-Panel „Warteschlange" (`CollapsingHeader` in
  `src/backup.rs`) entfernen — Baum/Liste mit Spalten wie im Recon.
- **3.2** `CollapsingHeader`-Zustand hält nicht: feste ID (`id_salt`); dieselbe Fehlerklasse bei
  **jedem** `CollapsingHeader` mit dynamischen Zahlen prüfen.
- **3.3** Zeile zeigt „Fehler" statt „Upload läuft": Status **live** ableiten (läuft `folder_total`
  für dieses Ziel ⇒ „Upload läuft"; erst nach `finalize` gilt der persistierte Ausgang).
- **3.4** Die zwei noch **gezeichneten** Glyphen (Nav-Icons, Flagge) auf Material Symbols umstellen
  (User-Regel: nichts selbst zeichnen außer dem Logo).
- **3.5** Die Layout-Punkte aus `docs/SPEC_UPLOAD_PROGRESS.md` § 7.8, die der User ausdrücklich mit
  in diese Welle genommen hat (User-Entscheidung 2026-10-15): **feste Spaltenköpfe** (einmal oben,
  nicht pro Zeile), **Kürzen + Tooltip** bei zu langen Namen, **Liste füllt die Höhe** (kein
  Leerraum unter der Liste), **300-Zeilen-Kappe entfällt**, **leerer Zustand zentriert**.
- **Ein Paket:** 3.1–3.5 gehen zusammen in **einen** UI-Umbau — die Zustandsfehler (3.2/3.3) und der
  Fortschritt (3.1) sitzen an denselben Stellen, ein getrennter Schnitt erzeugt nur Merge-Konflikt.
  Beauftragt wird das Paket erst **nach** der Sitzungs-Welle (Reihenfolge-Entscheidung 2026-10-15).

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

### Sitzung am Leben halten — Recon am Original (2026-10-15)

**Anlass (User-Auftrag):** „schau mit Rea nach, wie es die Original-App macht — ich hab mich da nicht
öfter angemeldet; das könnte immer refreshed werden. Browser Login würde schon lange verworfen,
Dokumentation updaten." Gemeint ist der **CDP-Browser-Login** aus dem „Verworfen"-Block weiter unten,
nicht unser eingebettetes Webview.

**Befund (am DLL belegt: `Amazon Photos.dll` + `Amazon.Auth.Map.dll`, gelesen mit `ilspycmd` 11.1.0.9782
und gesetztem `DOTNET_ROOT`; REAs `inspect-managed-members` scheitert an diesem Assembly an einem
Repräsentations-Budget — „Managed metadata exceeds the 67108864 byte representation budget" —, daher der
`ilspycmd`-Weg):** das Original meldet sich **einmal** an und hält die Sitzung danach selbst am Leben.

1. **Anmelde-Code → Refresh-Token:** `POST <Panda-Base>/auth/register` mit dem `authorization_code` des
   Portal-Fensters, dem PKCE-`code_verifier`, `code_algorithm = "SHA-256"`, `client_domain =
   "DeviceLegacy"` und der Client-Id **ohne** `device:`-Präfix; der Body trägt zusätzlich die
   Geräte-Registrierung (app_name/version, device_type/serial/model, os_version). Antwort:
   `tokens.bearer.access_token` **und** `refresh_token`.
2. **Ablage:** Refresh-Token als **Generic-Password-Item der macOS-Keychain**, Service
   `com.amazon.clouddrive.mac`, Account `amzp-rt` (UTF-8); Access-Token **nur im Speicher**. Die alte
   Datei `<LocalStorage>/refresh-token` (AES-CBC/PKCS7, Geräteseriennummer als Schlüssel, festes IV)
   wird beim Lesen in die Keychain migriert und gelöscht. **Empirisch:** das Item existiert auf dieser
   Maschine, `cdat`/`mdat` = **2026-10-13 09:35** — genau eine Anmeldung, seither nie wieder.
3. **Takt:** Access-Token-Auffrischung bei **80 % der Restlaufzeit** (Ablauf = `expires_in − 60 s`),
   Timer-getrieben, bis zu `AccessTokenMaxRetries` Versuche im Abstand
   `AccessTokenTimeBetweenRetriesInSeconds`, danach ein **fataler** Health-Zähler. (Default-Werte und
   Tick-Intervall: **unknown**.)
4. **Ungültiger Refresh-Token:** Autostart-Item abschalten (`SetApplicationStartupStatus(false)`),
   Tokens löschen, **App neu starten** → das Portal erscheint wieder.
5. **Der Schlüssel für uns:** die **Web-Cookies werden aus dem Refresh-Token erzeugt** —
   `POST <Panda-Base>/auth/token?source_token=<refresh>&source_token_type=refresh_token&requested_token_type=auth_cookies&domain=.<tld>&app_name=…&app_version=…`
   → Cookies in den `NSHttpCookieStorage`. Die Original-App **greift also nie Cookies aus einer Webview
   ab**, sie lässt sie sich ausstellen. Unser Weg (Cookies einmal kopieren) hat damit keine
   Erneuerungsquelle — genau die 401 von heute.
6. **Abmelden** = `POST /auth/deregister` + Tokens löschen; dazu Tamper-Erkennung per FileSystemWatcher
   auf dem Legacy-Pfad (Renamed/Changed/Deleted → Datei löschen bzw. abmelden).

**Dokumentiert in `docs/SPEC_LOGIN.md` § 3.8 (+ § 3.7/§ 5/§ 5.1/§ 7/§ 9) und `docs/PROTOCOL.md` § 2.1**
(Auftrag aus derselben User-Nachricht; Paket läuft).

**Entscheidung offen (2026-10-15, als Frage gestellt):** ob wir den Original-Weg nachbauen
(PKCE-Portal → Refresh-Token → Access-Token bei 80 % → Cookies bei Bedarf), nur dokumentieren, auf
Bearer-Token umstellen oder beim Cookie-Weg bleiben. **Bis zur Antwort gilt der bisherige Stand:**
`AGENTS.md` „Auth (upload engine): browser cookies …" und der Ledger-Eintrag „Device registration /
OAuth from scratch: replaced by browser-cookie auth" (unten unter „Verworfen") sind **nicht** aufgehoben.

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
- [x] **2.4b Doku + zwei Tests (a)–(l)** — ✅ erledigt 2026-10-15 (`40cf024`, Verifier Runde 1 =
      CHANGES REQUIRED mit einem reinen Doku-Fund, Ein-Zeilen-Fix danach; Abnahme ohne neue Runde
      zugesagt). 2.4a `36aee39`, Aufräumpaket `b0fa4c4`.

**Verworfen (2026-10-15):** `SaveAsNew` als **Application Default** (galt 2026-10-14 bis 2026-10-15).
Wer ihn zurückdreht, ändert eine getroffene Entscheidung — heute ist `Replace` die Vorgabe für neue
Ziele; der Enum-`#[default]` bleibt trotzdem `SaveAsNew` (Datei-Fallback, siehe Wellenplan). Ebenfalls
verworfen: den Application Default beim Drag&Drop still zu überschreiben (statt nur run-weit zu
merken).

**Verworfen (2026-10-15):** „Nicht noch einmal fragen" **pro Datei** als neues persistiertes Feld.
Geltende Regel (2.3a/2.3b): die Antwort wird in das **bestehende**
`BackupFolder.conflict_resolution` des Ordners der gefragten Datei geschrieben, plus **Application
Default** für neue Ziele und Drag&Drop — **kein** Schemawechsel, **kein** Per-Datei-Feld.

**Offene Befunde aus dem 2.4a-Verify (2026-10-15, nicht blockierend):**
- [ ] **Kein Test deckt die drei Vorbelegungsstellen** (`add_backup_folder`, transiente Einmal-Zeile,
      `UploadPlan::from_files`): der Verifier hat jede einzeln auf `true` zurückgedreht — alle 95 Tests
      blieben grün. Ursache: `PrimePhotoApp` braucht egui-Kontext bzw. Datei-Dialog. Ein **reiner
      Helfer** (z. B. „Config ⇒ (dedup, conflict) für die Einmal-Queue") würde die Entscheidung
      testbar machen — **noch nicht beauftragt.**
- [x] **`src/app.rs:346`** — **behoben in `b0fa4c4`:** die wiederhergestellte Drag&Drop-Zeile nutzt
      jetzt `config.dedup_default` / `config.conflict_resolution_default`, genau wie ein frischer Drop
      (`begin_one_time_run`). Vorher konnte ein frischer Drop `Replace` nutzen und ein
      wiederhergestellter `SaveAsNew`. Der dadurch unbenutzte `ConflictResolution`-Import in `app.rs`
      entfiel folgerichtig.
- [ ] **Ein Label-Test prüft deutschen Wortlaut** (`remember_label_names_the_folder` assertiert
      „-Ordner fragen"): unter `Lang::En` wäre er rot. Unkritisch, solange die Tests mit Deutsch
      laufen — beim nächsten Anfassen sprachneutral formulieren.
- [ ] **Das Häkchen-Gating ist eine Stufe großzügiger als das Schreiben:**
      `remember_checkbox_label` gated nur auf „Store kennt den Index", `remember_conflict_in_folder`
      zusätzlich auf `file_path.starts_with(&folder.path)`. Im Sonderfall eines verschobenen Index
      *mit* fremdem Pfad benennt das Häkchen einen Ordner, in den nichts geschrieben würde. Nur dort
      relevant, bewusst im beauftragten Umfang.
- [ ] **Toter i18n-Key „Als neue Dateien speichern"** (`src/i18n.rs`): außerhalb der i18n-Tabelle kein
      `tr`-Aufruf mehr, nur der Doku-Kommentar an `SaveAsNew` in `src/folders.rs` zitiert den alten
      Wortlaut — wird mit **2.4b (l)** entfernt bzw. korrigiert. Die bekannten **Doppel-Keys** in der
      i18n-Tabelle bleiben offen (es gibt keine Eindeutigkeitsprüfung; Kandidat für eine spätere
      Aufräumrunde).

**Offene Befunde aus dem 2.4b-Verify (2026-10-15, nicht blockierend):**
- [x] **`AGENTS.md:63`** führte die vier Optionen noch als „mapped to `NONE`/`RENAME`/`OVERWRITE`" (stale
      seit 2.2b und im Widerspruch zu `AGENTS.md:441`) — in dieser Ledger-Runde korrigiert; `OVERWRITE`
      steht dort jetzt als der Wert, der **nie** gesendet wird.
- [ ] **`docs/SPEC_DUPLICATES.md`:** die **Empfehlungszelle** neben der jetzt als umgesetzt markierten
      Zeile „Global dedup default setting" sagt noch „Optional. Only worth it if a preferences screen
      lands" — beim nächsten Anfassen auf den Nachbarzeilen-Stil („Done") ziehen; der Verifier hat die
      Zeile bewusst nicht mitgeändert.
- [ ] **`docs/SPEC_DUPLICATES.md:269`** — ✅ behoben (Ein-Zeilen-Fix vor dem Commit `40cf024`); die
      **Empfehlungszelle** neben der Zeile sagt aber weiter „Optional. Only worth it if a preferences
      screen lands" — beim nächsten Anfassen auf den Nachbarzeilen-Stil („Done") ziehen.
- [ ] **`docs/SPEC_DUPLICATES.md:268`** (Fund 2 des zweiten Verifiers, echt): die Zeile
      „**Conflict default**" nennt `SaveAsNew` („was `Ignore`; changed 2026-10-14") und lässt den
      **Application Default `Replace`** (seit 2.4a, 2026-10-15) weg — sie widerspricht damit dem
      eigenen §5-Absatz zwei Zeilen darüber. Auf den neuen Stand bringen (Application Default
      `Replace`, Enum-`#[default]` `SaveAsNew` als Datei-Fallback).
- [ ] **`docs/SPEC_DUPLICATES.md:273`** (Fund 3 des zweiten Verifiers, echt): die Zeile
      „**`Replace` wire value**" führt unseren Weg noch als `PUT /v2/upload/<nodeId>?conflictResolution=NONE`
      — genau die Form, die **dieselbe Datei** in §291-295 als **400 `MissingFileSize`** verwirft.
      Auf den POST-Query-Satz umstellen (wie §291-295).
- [ ] **Live-Abnahme des `PUT` (2.2b)** — weiterhin offen: die Tests pinnen URL-Bau und Dispatch, nicht
      die Leitung (POST → 409 `NameAlreadyExists` → PUT → 200, gleiche Node-Id); hängt an frischen
      Cookies, siehe Frage an den User (Sitzung/Refresh).

**Warnungen — ✅ erledigt 2026-10-15 (`b0fa4c4`; Baseline jetzt `0`):** User-Entscheidung „jetzt
abräumen, Ziel 0 Warnungen". Die sieben Build-Warnungen waren nie angeschaut, nur als Zahl
weitergereicht. Bestand:
1. **`src/icons.rs:26`** — `egui::Stroke::new(1.6, …)`, Lint `float_literal_f32_fallback` ⇒
   **future incompatible**, wird in einer künftigen Compiler-Version ein **harter Fehler**
   („will become a hard error in a future release") — das ist der Punkt mit Ablaufdatum; Fix:
   `1.6_f32`.
2. `src/upload.rs:848` unnötiges `mut`; 3. `src/login.rs:144` nie gelesener Ausgangswert von
   `outcome`.
4.–7. toter Code: `folders.rs:29 supported_extensions()` — **der einzige bewusst behaltene Haken**
   (laut Projektregel für einen künftigen Datei-Picker vorgesehen: bekommt `#[allow(dead_code)]`
   **mit** Begründung), `folders.rs:222 BackupFolder::percent()`, `folders.rs:278
   FolderStore::remove()`, `theme.rs:98 card_frame()`.
   **Regel für die drei echten Leichen:** entfernen, wenn ein gleichwertiger lebender Pfad existiert
   (die UI rechnet den Bruch inline; „Trennen" arbeitet mit `retain`) — sonst behalten **mit**
   Begründung. Nichts still mit `#[allow]` zudecken. Ziel: **Baseline 0 Warnungen**, damit ab hier
   jede neu entstehende Warnung sofort auffällt statt im Rauschen zu verschwinden.

**Verworfen (2026-10-15):** die sieben Warnungen pauschal mit `#[allow(...)]` stumm stellen — das
beseitigt das Rauschen, versteckt aber genau die toten Funktionen, um die es geht. Ebenfalls
verworfen: nur die future-incompatible Warnung zu fixen und die Baseline bei sechs zu lassen (der
User wollte 0 als neue Vergleichsbasis).

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
- [ ] **i18n-Hygiene:** doppelte Keys in der Tabelle — `"Ersetzen"` (`src/i18n.rs:209` **und** `:322`)
  und `"Ignorieren"` (`:213` **und** `:323`); je eine Kopie ist toter Text (gleicher Wert, daher
  harmlos). Dazu fehlt eine Prüfung, die doppelte Keys überhaupt aufdeckt. Nebenbei geklärt: die
  Config-Nits aus 2.3a sind **erledigt** (`src/config.rs:167-178` pinnt den Legacy-Default
  `SaveAsNew`, `:180-193` die kleingeschriebene Serde-Schreibweise `"replace"`).
- [ ] Download-Vorgang (Herunterladen-Tab des Originals) — relevant, aber explizit **nicht** Teil des
  MVP (siehe `AGENTS.md` → „Scope-Grenzen (In/Out)").

## Verworfen

- Separates Login-Fenster (`ppu-login` Helper-Prozess mit eigener `tao`-Eventloop): wirkte ungewohnt und passte nicht zum Original, das im Hauptfenster anmeldet. Ersetzt durch die eingebettete Child-WebView im Hauptfenster.
- Synchrone Cookie-Abfrage in `update` (wry `WebView::cookies`): Panic durch winit-Reentrancy (dreht die NSRunLoop). Ersetzt durch `dispatch_async` auf der Main-Queue.
- mitmproxy live capture: kept only as a debugging tool; app uses the cookie upload path.
- Device registration / OAuth from scratch: replaced by browser-cookie auth. — **Aufgehoben am 2026-10-15** (User-Entscheidung „so gut es geht wie das Original", siehe Welle „Sitzung wie das Original" oben und `AGENTS.md`): genau dieser Weg ist jetzt das **Ziel**. Verworfen bleibt daran nur der **CDP-Browser-Login** (externer Browser) und die **Paste-Variante** als Hauptweg.
- Standalone paste-only login: superseded by CDP browser login (paste stays as fallback).
- **Avatar-Thumbnail per Raten/Probieren (2026-10-14, verworfen):** `/drive/v1/nodes/{id}/content` (mit und ohne `tempLink=true`), `/drive/v1/nodes/{id}/thumbnail` (mit `width`/`height`/`transform`), `/drive/v1/nodes/{id}/transforms/IMAGE_THUMBNAIL` (mit und ohne `?tempLink=true`) sowie `/drive/v1/nodes/{id}/content?transform=IMAGE_THUMBNAIL&tempLink=true` — **alle HTTP 404**. Der Parameter `?transform=IMAGE_THUMBNAIL` wird **ignoriert** (der `tempLink` liefert weiterhin die Rohdatei). Das `transforms: ["IMAGE_THUMBNAIL"]` eines Suchtreffers ist eine **Deklaration**, keine URL. Stattdessen belegbar über das Assembly (`GetSmartCroppedThumbnail`/`GetThumbnailUrl`) — Recon läuft, siehe Welle 3.
- **Avatar lokal dekodieren/schneiden (2026-10-14, verworfen):** der Avatar-Quellknoten kann ein RAW sein (bei diesem Konto ein Canon CR3, 13,5 MB) — die `image`-Crate kann kein CR3. Der 60×60-Crop muss daher **serverseitig** passieren (wie im Original), nicht bei uns.
- **`deepseek-v4.1-flash` als Umsetzer-/Verifier-Modell (2026-10-14 gesetzt, 2026-10-15 zurückgenommen):** lief genau einen Tag, ohne dass ein Fehler oder Gate daran schuld war — der User hat schlicht wieder zurückgestellt („ab jetzt wieder step 5 preview für subagenten nutzen"). Gültig sind damit wieder `opencode-go/step-5-preview-free` für **beide** Rollen, dieselbe Vorgabe wie global. Nicht „reparieren": ein erneuter Wechsel auf ein Gratis-/Billig-Modell für Subagenten braucht eine **neue** User-Entscheidung, nicht den alten Eintrag.
