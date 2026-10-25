# Upload queue and progress

All backup targets selected in one run share a single upload queue with a
configurable concurrency. While a run is active, aggregate progress covers only
that run; while idle, it sums all targets. Each target row reports its own
outcome.

## How it works

- **Global pool:** all backup targets selected in one run become a single
  upload queue, sharing at most `concurrency` (1..8, setting) workers; more
  than 8 **total** makes no sense.
- **Per-target limit is only "not more than global"** and is **Post-MVP**:
  an optional, persistently stored value (0/None = global default; not
  greater than global). In the MVP it is deliberately **not** implemented.
- **Network awareness (MVP):** "Slow mode" is a temporary, **not**
  persisted run-time setting (`PrimePhotoApp.slow_mode`, checkbox in Settings
  → Upload), which forces `concurrency = 1` for the current run; reset at
  app restart. Automatic throttling based on network/SSID/name is
  **deferred** (platform APIs, Post-MVP).
- **Progress/ETA:** for scan-planned runs (manual and background scans via
  `build_since`), the run-level total, aggregate bar/fraction, queue rows and ETA
  cover only files that remain candidates after the local stat/dedup check. A
  manual scan still checks every supported file, but a stat match is counted in
  the result breakdown instead of becoming a queue row. Explicit file lists are
  different: drag & drop (`from_files`) and watcher events (`build_from_events`)
  enter the queue and run total without this pre-filter; a stat skip in the
  worker is then settled and counted as done. Per-target counts reflect the
  corresponding run plan; non-participating rows may retain persisted counts but
  do not affect the live aggregate. The aggregate count and bar use the run
  counters (`done`/`total`), not a sum of target rows. While idle, the aggregate
  sums every target row, including the one-time row. ETA uses the run's known
  bytes and current transfer rate.
- **Result breakdown:** "done" alone would hide the difference between a
  real upload and a skip. Results are split into **newly uploaded / already
  in the account / unchanged / errors**. The breakdown lives in memory for
  the run/session; `folders.toml` is **not** extended (no schema change);
  after a restart only the total count is there. **Location:** the **footer
  aggregates** across all targets **and** the **individual folder row**
  shows its own three numbers; the file detail keeps as today (both skip
  cases share one state there).
- **Live status of the folder row (Welle 3.3):** whether a row shows
  "Upload läuft" or "Fehler" is derived **live** — if a run is live for this
  target, it shows "läuft"; the **persistent** outcome counts only after
  `finalize`. The live checks depend on **`folder_total` AND `s.active`**
  (two places in `src/backup.rs`); without `s.active` the display flips to
  "Fehler" in the frame between run end and `finalize`. If a short background
  run starts and finishes between UI frames, the UI reloads the persisted store
  on the next idle frame and never reloads it during a run.
- **Aggregate progress bar:** the blue aggregate bar ("Alles aktuell") must
  never exceed 100 %. Fix: remember the line width **before** the
  `ui.horizontal`, give the bar an explicit `desired_width` (without spinner
  = line width, with spinner = width − spinner width − `item_spacing.x`);
  the spinner width is `interact_size.y` = 26 in the theme.
- **Responsive heights:** `page_heights` as **one** source; the **folder
  list is content-aware** (as many cards as fit), the **queue** keeps at
  least `QUEUE_MIN_FRAC` (20 %) and at most `QUEUE_MAX_FRAC` (50 %) of the
  free height. Only the measured chrome (`QUEUE_BELOW_H`,
  `QUEUE_HEADER_H`, `QUEUE_FOOTER_H`) remains absolute.
- **Scroll-bug fix:** two `ScrollArea::vertical()` in the same parent `Ui`
  shared the scroll state: egui sets without `id_salt` the **constant**
  `Id::new("scroll_area")` (`scroll_area.rs:534`). Fix: **every** ScrollArea
  gets its own `id_salt` (`"queue"`, `"folders"`).
- **Folders in the queue:** folder **column** per row **and** in the group
  header; with **two folders of the same name** name **+ path**
  (disambiguation computed once per frame), full path in the tooltip.
- **Folder card Variant A:** row 1 = name left, **status word** right
  (colored, per `BackupStatusAlertLevel`) + the three row symbols; row 2
  small = **path · progress · last updated**. Each element appears **once**
  (previously duplicated percentage), path always visible. Six
  `FolderStatus` values with word and color (Paused → AMBER).
- **Empty folder versus all-current scan:** a plan with no supported photos or
  videos anywhere starts no run; for a single-folder scan the user sees
  **"Keine Fotos oder Videos in diesem Ordner gefunden."** In a multi-folder
  plan, an empty folder is finalized as **"nichts zu tun"** (Done, 0/0) if
  another selected folder has supported files. A scan that finds supported
  files but whose files are all unchanged also starts a run with zero
  candidates, shows **"Alles aktuell"**, and is finalized via `folder_total`
  (Done, 0/0). Never-scanned folders remain **untouched**. At 100 % the wording
  is **"Vollständig"** (original wording), not "100 % hochgeladen".

## Decisions

- **Nur echte Upload-Kandidaten zählen (User-Entscheidung 2026-10-25):** der
  manuelle Lauf bleibt eine Voll-Prüfung aller unterstützten Dateien, damit
  geänderte Dateien trotz unveränderter Ordner-Wasserzeichen gefunden werden.
  Der Scan liest dafür nur Metadaten und fragt den lokalen Dedup-Index ab —
  ohne Netzwerk und ohne Hashing. Dateien mit passendem Stat-Eintrag werden
  als „unverändert" bzw. „bereits im Konto" im Ergebnis gezählt, aber nicht
  in Queue, Lauf-Total oder ETA aufgenommen. Sind alle Dateien aktuell, bleibt
  die Queue leer und der teilnehmende Ordner wird als abgeschlossen (0/0)
  finalisiert.

- **Upload-Queue & Concurrency (User-Entscheidung 2026-10-13):**
  - **Globaler Pool bleibt** (status quo): alle in einem Lauf ausgewählten
    Sicherungsziele werden zu **einer** gemeinsamen Warteschlange, die sich
    maximal `concurrency` (1..8, Einstellung) Arbeiter teilt; mehr als 8
    **gesamt** macht keinen Sinn.
  - **Pro-Ziel-Limit ist nur „nicht mehr als global"** und erst **Post-MVP**:
    ein optionaler, persistent gespeicherter Wert (0/None = globaler
    Default; nicht größer wie global) wie bereits bei Dedup/Conflict
    vorbereitet. Im MVP bewusst **nicht** umgesetzt.
  - **Netzwerkabhängigkeit (MVP):** „Slow-Mode" als temporäre, **nicht**
    persistierte Run-Zeit-Einstellung (`PrimePhotoApp.slow_mode`, Checkbox
    in Einstellungen → Upload), die für den aktuellen Lauf `concurrency = 1`
    erzwingt; bei App-Neustart zurückgesetzt. Automatische Drosselung anhand
    des Netzwerks/SSID/Namen ist **nachgeordnet** (Plattform-APIs,
    Post-MVP).

- **Queue-Paginierung (Abweichung vom Original, Stand 2026-10-17):** unsere
  Warteschlange kappt das Rendering bei **300 Zeilen** — das Original
  paginiert im View-Model mit **1000 Items/Seite**
  (`../original/SPEC_STATUS_DETAIL.md` §3,
  `../original/SPEC_UPLOAD_PROGRESS.md` §7.4). egui kann stattdessen
  virtualisieren (`ScrollArea::show_rows`); die Kappung ist damit eine
  **Abweichung**, die Umsetzung bleibt offene Arbeit im Ledger.

- **Zeilen-Aktionen & Ergebnis-Anzeige (User-Entscheidung 2026-10-14):**
  Anlass: Der User suchte einen Rescan-Button und erwartete ein **vertikales
  „…"-Icon** mit Popup bei Linksklick; außerdem zweifelte er die Zahl „480
  fertig" an („passt die Anzahl wirklich von heute?").

  - **Icons kommen aus einer professionellen Icon-Sammlung, nicht selbst
    gezeichnet** (User-Korrektur 2026-10-14, nachdem er „selbst zeichnen"
    abgelehnt hat: „so gut wie es geht nicht selbst zeichnen, sondern icons
    nutzen … besser was nehmen, was lizenzrechtlich passt und professionell
    ist"). Konkret: **Material Symbols (Google, Apache-2.0)**, als **SVG** in
    `assets/` über die **vorhandene** Pipeline (`build.rs`/`resvg` →
    `OUT_DIR`-Raster → `include_bytes!`), zur Laufzeit per **Tint** in die
    Theme-Farbe. **Kein Icon-Font** (User: „potentiell nicht die Iconfont
    selbst einbinden, sondern die PNGs in der richtigen Größe") — Raster
    statt Font, keine neue Abhängigkeit. Belege:
    `assets/icon-more-vert.svg` (Kebab) und `assets/icon-refresh.svg`,
    Quellen im Material-Symbols-Repo
    (`symbols/web/<name>/materialsymbolsoutlined/<name>_24px.svg`), Hashes
    und Attribution in `../dev/THIRD_PARTY.md`. **Fallstrick, die dabei zu
    beachten ist:** die Quellen haben **kein** `fill`/`currentColor`
    (Standard = schwarz), der vorhandene Asset-Override `*{color:…}` färbt
    sie also **nicht** — für die Icon-Gruppe greift ein **fill**-Override.
    Das wird durch einen Test belegt (Raster ist gedeckt und weiß).
  - **Zeilen-Menü: vertikaler Kebab, immer sichtbar**, Tooltip „Aktionen",
    Linksklick öffnet das Popup (egui 0.31 hat **kein**
    `From<Image> for WidgetText` ⇒ `ui.menu_button` scheidet aus; es wird
    der dokumentierte Weg `Button::image` + `popup_below_widget` +
    `memory_mut(toggle_popup)` benutzt). Aktuell existiert genau **eine**
    `⋯`-Stelle: `src/backup.rs:1501`.
  - **Rescan wird sichtbar:** pro Ordner-Zeile ein **eigener
    Refresh-Icon-Button**, der direkt „Jetzt scannen und hochladen" auslöst.
    Der Kebab-Popup behält den Rest (Im Dateimanager anzeigen, Bearbeiten,
    Trennen, Pausieren). Grund: der Rescan existierte, war aber nur im
    Text-`⋯` versteckt. **Global bleibt es weiterhin ohne „Alles neu prüfen"**
    (nicht beauftragt).
    **Verworfen (2026-10-14):** das Zeilen-Icon **selbst zu zeichnen** (in
    `src/icons.rs`) — der User will keine handgemachten Formen, sondern eine
    lizenzierte Icon-Bibliothek. Ebenfalls verworfen: das Textzeichen `⋮`
    (U+22EE **fehlt in allen vier** Fonts von `epaint_default_fonts-0.31.1`
    — Hack, Ubuntu-Light, NotoEmoji, emoji-icon-font — und erschiene als □;
    `⋯` U+22EF gibt es nur in Hack-Regular) und ein **Icon-Font-Crate** als
    zweite Icon-Quelle.
  - **Ergebnis wird aufgeschlüsselt:** „fertig" darf **nicht** länger
    echt-Upload, 409 „schon im Konto" und lokalen Dedup-Skip zusammenwerfen.
    Anzeige fortan getrennt: **neu hochgeladen / bereits im Konto /
    unverändert / Fehler**. Belegte Ursache: `src/upload.rs` zählt in allen
    drei Fällen `s.done += 1` (`:496` Uploaded, `:552` ConflictSkipped,
    `:376`/`:421` lokaler Skip) — die Ausgänge liegen dort schon getrennt
    vor und werden nur nicht berichtet. **Konsequenz für die Regel „eine
    aggregierte Anzeige":** das bleibt für den *Fortschritt* eines laufenden
    Uploads richtig, gilt aber nicht für das *Ergebnis* — das Ergebnis wird
    aufgeschlüsselt. **Speicherung: nur flüchtig** (User-Entscheidung
    2026-10-14) — die Aufschlüsselung lebt im Speicher für den Lauf/die
    Sitzung, `folders.toml` wird **nicht** erweitert (keine
    Schemaänderung); nach einem Neustart steht dort weiterhin nur die
    Gesamtzahl. **Ort (User-Entscheidung 2026-10-14):** die **Fußzeile
    aggregiert** über alle Ziele **und** die **einzelne Ordner-Zeile** zeigt
    ihre eigenen drei Zahlen; das Datei-Detail bleibt wie heute (beide
    Skip-Fälle teilen sich dort einen Zustand).
  - **Live-Status der Ordner-Zeile (Welle 3.3, 2026-10-16):** ob eine Zeile
    „Upload läuft" oder „Fehler" zeigt, wird **live** abgeleitet — läuft für
    dieses Ziel gerade ein Lauf, gilt „läuft"; der **persistierte** Ausgang
    zählt erst **nach** `finalize`. Umsetzung: die Live-Prüfungen hängen an
    **`folder_total` UND `s.active`** (zwei Stellen in `src/backup.rs`);
    fehlt das `s.active`, kippt die Anzeige im Frame zwischen Lauf-Ende et
    `finalize` kurz auf „Fehler". Anlass: eine Zeile stand auf „Fehler",
    während der Upload noch lief.

- **Gesamt-Fortschritt über alle Uploads** (Wunsch 2026-10-14): genau
  **eine** aggregierte Anzeige. Während ein Lauf aktiv ist, zeigt sie dessen
  Lauf-Zähler; im Ruhezustand summiert sie alle Backup-Ziele inkl. One-Time-
  Zeile. So bleiben nicht beteiligte Ziele sichtbar, ohne den Fortschritt des
  laufenden Plans zu verfälschen. Manuelle Läufe starten weiterhin pro Ordner.

- **Fortschrittsbalken nie > 100 % (Welle 5.15).** Der blaue Aggregat-Balken
  („Alles aktuell") lief während eines Laufs sichtbar über den rechten Rand
  („es hat ausgesehen wie hätte er kurz 105 % Breite"). Ursache: der
  `ProgressBar` ohne `desired_width` nimmt die volle Breite der Zeile, obwohl
  der **Spinner** davor schon Platz verbraucht. Fix: Zeilenbreite **vor**
  dem `ui.horizontal` merken, dem Balken eine explizite `desired_width` geben
  (ohne Spinner = Zeilenbreite, mit Spinner = Breite − Spinnerbreite −
  `item_spacing.x`); die Spinnerbreite ist `interact_size.y` = 26 im Theme.

- **Ordner-Zelle kurz (Welle 5.15).** Die Disambiguierung in der Warteschlange
  zeigt nur den **Namen des Elternordners** („Sport (Pictures)") statt des
  vollen Pfads; der voller Pfad bleibt im Tooltip. Anlass: „Sport
  (/Users/florianreisinger/Pictures)" wurde in der 120-px-Spalte
  abgeschnitten. **Verworfen:** nur den Namen zeigen (gleichnamige Ordner
  wären in der Zeile ununterscheidbar); die Spalte breiter machen.

- **Kein „Wöchentlich aktiviert um …" (Welle 5.15).** Der Block im Dialog
  „Datensicherungseinstellungen" ist **entfernt** (samt der i18n-Keys
  „Wöchentlich aktiviert" und „um") — wir unterstützen kein wöchentliches
  Hochladen (User-Wortlaut: „wöchentlich um XX hochladen unterstützen wir
  nicht, entfernen").

- **„Nichts hochzuladen" ⇒ abgeschlossener Zustand (F3, Welle 5.8,
  `c6003ff`).** Nur ein vollständig leerer Plan (keine unterstützten Fotos
  oder Videos in irgendeinem ausgewählten Ordner) startet keinen Lauf. Ist ein
  einzelner Ordner leer, zeigt der Scan „Keine Fotos oder Videos in diesem
  Ordner gefunden."; in einem gemischten Mehrordner-Lauf wird der leere
  teilnehmende Ordner dagegen als „nichts zu tun" mit Done (0/0)
  **finalisiert**. Ein Scan mit unterstützten Dateien, die alle unverändert
  sind, startet ebenfalls einen Lauf mit 0 Kandidaten, meldet „Alles aktuell"
  und wird über `folder_total` als Done (0/0) **finalisiert**. Nie gescannte
  Ordner bleiben **unberührt**. Bei 100 % lautet der Wortlaut
  **„Vollständig"** (Original-Wortlaut), nicht „100% hochgeladen".

- **Responsive Höhen (F8/F11, Welle 5.8).** `page_heights` als **eine**
  Quelle; die **Ordnerliste inhaltbewusst** (so viele Karten, wie passen), die
  **Warteschlange** mindestens `QUEUE_MIN_FRAC` (20 %) und höchstens
  `QUEUE_MAX_FRAC` (50 %) der freien Höhe. Nur das gemessene Chrome
  (`QUEUE_BELOW_H`, `QUEUE_HEADER_H`, `QUEUE_FOOTER_H`) bleibt absolut.
  **Verworfen:** ziehbare Trennlinie (Position überlebt den Neustart nicht);
  eigener Reiter für die Warteschlange (widerspricht „Warteschlange et
  Fortschrittsleiste gehören zusammen", Welle 3a).

- **Scroll-Bug (F12, Welle 5.8).** Zwei `ScrollArea::vertical()` im selben
  Eltern-`Ui` teilten sich den Scroll-Zustand: egui setzt ohne `id_salt` die
  **Konstante** `Id::new("scroll_area")` (`scroll_area.rs:534`). Fix:
  **jede** ScrollArea ein eigenes `id_salt` (`"queue"`, `"folders"`).

- **Ordner in der Warteschlange (F13, Welle 5.8).** Ordner-**Spalte** je
  Zeile **und** im Gruppenkopf; bei **zwei Ordnern mit gleichem Namen** Name
  **+ Pfad** (Mehrdeutigkeit einmal pro Frame berechnet), voller Pfad im
  Tooltip. Ersetzt den früheren Wunsch „Ordnerpfad im Zeilennamen".

- **Ordner-Karte Variant A (F14, Welle 5.8).** Zeile 1 = Name links,
  **Statuswort** rechts (farbig, je `BackupStatusAlertLevel`) + die drei
  Zeilen-Symbole; Zeile 2 klein = **Pfad · Fortschritt · Zuletzt
  aktualisiert**. Jeder Baustein steht **einmal** (vorher doppelte
  Prozentzahl), der Pfad ist immer sichtbar. Sechs `FolderStatus`-Werte mit
  Wort und Farbe (Paused → AMBER).
  **Verworfen:** Variant B (kompakt, eine Zeile, Pfad nur bei
  Mehrdeutigkeit); Variant C (nur die Dopplung entfernen, Pfad in die graue
  Zeile).
