# Upload queue and progress

> **Hinweis:** Einzelne Implementierungsnotizen unten beschreiben die frühere
> eframe-Oberfläche. In der Rune-Ansicht (`src/rune_backup_live/`) sind die
> gemeinsame Warteschlange, der lauf-aggregierte Fortschritt, die Sicherungsziele
> mit eigenen Zählern und die flache Queue portiert. Die Queue wird in
> Plan-/Einfügereihenfolge gerendert (`upload.queue`, kein `sort_by`); die frühere
> Status-Sortierung (In Bearbeitung → Fehlgeschlagen → Warten → Abgeschlossen →
> Übersprungen) ist **nicht** portiert. Der Dateistatus erscheint je Zeile als
> Inline-Text. **Nicht portiert** sind die ETA, die Größen-/Status-Tabelle der
> Queue-Zeilen und die Ergebniszeile (neu hochgeladen / bereits im Konto /
> unverändert / Fehler); diese Punkte bleiben offen.

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
  → Upload) that applies **immediately**, including to a running upload:
  transfers already in flight finish, no new one starts while more than one
  is running, then exactly one file uploads at a time. Switching it off
  restores the configured concurrency without restarting the run; reset at
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
  sums every target row, including the one-time row. The earlier eframe view
  derived an ETA from the run's known bytes and current transfer rate; the Rune
  view currently shows **no ETA** (open).
- **Result breakdown (in der Rune-Ansicht offen):** "done" alone would hide the
  difference between a real upload and a skip. In the earlier eframe UI results
  were split into **newly uploaded / already in the account / unchanged /
  errors**. The breakdown lives in memory for the run/session; `folders.toml` is
  **not** extended (no schema change); after a restart only the total count is
  there. In that UI the **footer aggregates** across all targets **and** the
  **individual folder row** showed its own three numbers; the file detail kept
  both skip cases in one state. The Rune view has not ported the result line or
  the four counters yet.
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
- **Real draggable splitter (Issue #33):** the queue/progress pane and the
  folder pane are separated by a **10 px drag handle** with 3 px of air above
  and below it. The handle is keyboard-focusable (Up/Down adjust the split;
  Shift changes in 10 % steps instead of 3 %), shows the vertical-resize cursor
  and a localized accessibility label/tooltip. Both sections are **open by
  default**; the disclosure triangle and section label share one focusable
  toggle and remain as a supplemental accordion. Keyboard focus on the shared
  toggle is shown with a small rounded theme-teal outline (not on hover alone).
- **Persistent split ratio:** `Config::backup_queue_ratio` stores the queue's
  share of the splittable height in `config.toml` (serde default **0.35**,
  clamped to **0.20–0.80**; legacy files without the key get 0.35). It is saved
  when the drag handle is released or when a keyboard step changes it — never
  once per frame — through the ordinary `save_config()` path (demo fixtures are
  skipped). The ratio is used **only while both sections are open**. If exactly
  one section is collapsed, it receives all free viewport height and the saved
  ratio is **not applied** and never overwritten, so reopening restores it
  unchanged. The folder header toggles after that frame's layout snapshot. On
  opening, the queue gets its queue-only allocation and the folder cards wait
  until the next frame applies the both-open split; drawing them early would add
  content outside that frame's budget and could push the footer down. On
  closing, the cards remain visible for one final frame in their already
  allocated both-open viewport, then disappear when the queue-only layout is
  applied on the next frame. The list is capped to actual remaining space minus
  the footer and permits a zero-height scroll viewport, so a tiny window cannot
  make the list itself overflow; fixed chrome can still exceed what such a
  window can show.
- **At least one section stays open:** the disclosure controls enforce a hard
  invariant. A section may only close while the other one is open; if both
  persisted states are somehow closed, the queue header reopens itself before
  painting. The aggregate progress summary and the folder-result footer stay
  reachable in either state. The sole open section's toggle is disabled and
  cannot receive keyboard focus.
- **Viewport floors:** with enough height the queue keeps **72 px** and the
  folder list **96 px**. Tiny windows degrade to zero-height viewports without
  negative sizes; the viewport allocation itself never overruns its budget, but
  a window smaller than the fixed chrome cannot show every control. The folder
  list scrolls at its natural card height and the empty-folder hint lives inside
  that scroll area.
- **Compact backup-page spacing (Issue #33):** the queue progress zone consumes
  **40 px** in every branch (6 px gap + fixed 26 px row + 8 px trailing item
  spacing; the bar itself is 20 px high), even when no progress row is shown.
  The splitter zone is **24 px** (3 px + 10 px handle + 8 px trailing spacing
  + 3 px), the folder heading zone **34 px** (the production horizontal layout
  row measures 26 px, although its text label response is 16 px, plus 8 px
  spacing), and the heading/list gap **6 px**. Cards use 3 px outer vertical
  margins with a 6 px list gap. The aggregate outcome footer and bottom-panel
  “Letzter Upload” summary use dark theme ink at 11 pt; folder-card meta/outcome lines stay
  muted at 11 pt. The progress-bar label keeps its theme color
  (`visuals.selection.stroke`). The footer area is **33 px** (8 px table
  trailing spacing + 6 px post-list gap + 6 px footer gap + 13 px line box).
  Both-open chrome is `QUEUE_BELOW_H` = **145 px** = 8 + 40 + 24 + 34 + 6 + 33.
  With only the queue open, `QUEUE_ONLY_BELOW_H` = **107 px** = 8 + 40 + 6 + 34
  + 6 + 13. With only the folder list open, `FOLDER_ONLY_BELOW_H` = **119 px**
  = 40 + 6 + 34 + 6 + 33. The table header chrome `QUEUE_HEADER_TOTAL_H` is
  **40 px**, measured headlessly through the production table: a 20 px cell row
  beginning 3 px below the layout cursor, 8 px before the 1 px divider, then 8
  px before the first virtual row. This is distinct from the folder heading: its
  text label response bounds are 16 px, but the production horizontal layout
  row is 26 px and the consumed zone is 34 px with trailing spacing.
- **Scroll-bug fix:** two `ScrollArea::vertical()` in the same parent `Ui`
  shared the scroll state: egui sets without `id_salt` the **constant**
  `Id::new("scroll_area")` (`scroll_area.rs:534`). Fix: **every** ScrollArea
  gets its own `id_salt` (`"queue"`, `"folders"`).
- **Queue order (user decision 2026-10-26):** the queue is **global and
  flat**, sorted by file status — In Bearbeitung → Fehlgeschlagen → Warten →
  Abgeschlossen → Übersprungen — and stable in insertion order within a status
  bucket. There are **no folder group headers**.
- **Folders in the queue:** folder **column** per row; with **two folders of
  the same name** name **+ parent name** (disambiguation computed once per
  frame), full path in the tooltip.
- **Folder card Variant A:** row 1 = name left, **status word** right
  (colored, per `BackupStatusAlertLevel`) + the three row symbols; row 2
  small = **path · progress · last updated**. Each element appears **once**
  (previously duplicated percentage), path always visible. Six
  `FolderStatus` values with word and color (Paused → AMBER).
- **Empty folder versus all-current scan:** a plan with no supported photos
  anywhere starts no run; for a single-folder scan the user sees
  **"Keine Fotos in diesem Ordner gefunden."** In a multi-folder
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
    in Einstellungen → Upload), die **sofort** wirkt — auch auf einen bereits
    laufenden Upload: laufende Transfers werden zu Ende geführt, solange noch
    mehr als einer läuft startet kein neuer, danach läuft genau 1 Datei
    gleichzeitig. Ausschalten stellt die konfigurierte Parallelität ohne
    Neustart des Laufs wieder her; bei App-Neustart zurückgesetzt.
    Automatische Drosselung anhand des Netzwerks/SSID/Namen ist
    **nachgeordnet** (Plattform-APIs, Post-MVP).

- **Statussortierung der Warteschlange (User-Entscheidung 2026-10-26):** die
  Live-Warteschlange wird **global und flach** nach Dateistatus sortiert statt
  nach Backup-Ordner gruppiert; die Gruppenkopf-Zeilen („x offen") entfallen
  vollständig. Reihenfolge: In Bearbeitung → Fehlgeschlagen → Warten →
  Abgeschlossen → Übersprungen, innerhalb eines Status in Einfügereihenfolge
  (stabil, keine weitere Priorisierung). Die **Ordner-Spalte bleibt** (inkl.
  Gleichnamigkeits-Disambiguierung und Tooltip), ebenso die Spalte
  „Warteschlange (N)". Die Reihenfolge ist **unsere eigene Festlegung**; die
  Originalsortierung (§3.3: Processing/Errored/andere/Completed **plus
  `SortDate`**, ohne stabile Einfügereihenfolge) dient höchstens als loser
  Anhalt (Processing zuerst, Errored danach) und wird **nicht** übernommen.
  Das Original gruppiert seine Status-Tabellen nicht — die frühere
  Ordner-Gruppierung war eine eigene Idee, die hier entfällt.

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
    **Verworfen (2026-10-14):** das Zeilen-Icon **selbst zu zeichnen** (im
    Icon-Modul) — der User will keine handgemachten Formen, sondern eine
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

- **Responsive Höhen (früher F8/F11, Welle 5.8; durch #33 ersetzt).** Früher
  war die Ordnerliste inhaltsbewusst und die Warteschlange auf 20–50 % der
  freien Höhe begrenzt. Issue #33 ersetzt diese feste Verteilung durch ein in
  `config.toml` gespeichertes Verhältnis (`Config::backup_queue_ratio`) und
  einen echten ziehbaren Trenner. Der separate Warteschlangen-Reiter bleibt
  verworfen; die frühere Begründung „die Position überlebt den Neustart nicht"
  gilt nicht mehr, weil das Verhältnis jetzt persistiert wird.

- **Ziehbarer, persistierter Splitter (Issue #33, 2026-11-03).** Beide Bereiche
  starten offen; Disclosure-Dreieck und Bereichslabel teilen sich einen
  fokussierbaren Toggle und bleiben als ergänzendes Accordion. Bei
  Tastaturfokus wird der gemeinsame Toggle mit einem abgerundeten Rahmen in
  Theme-Türkis hervorgehoben (nicht bei bloßem Hover). Der
  **10-px-Griff** (mit je 3 px Luft) verändert das Höhenverhältnis der Queue- und
  Ordner-Scrollbereiche. Default ist **35 %** Queue, begrenzt auf **20–80 %**
  (NaN/unendlich fallen auf den Default zurück). Gespeichert wird beim Loslassen
  des Griffs bzw. nach einem Tastatur-Schritt (Up/Down, Shift = 10 %), nicht pro
  Frame; ältere Konfigurationen ohne Schlüssel erhalten 35 %. Bei ausreichend
  Platz gelten **72/96 px** Mindesthöhen für Queue und Ordnerliste. Ist genau ein
  Bereich geschlossen, bekommt der offene den gesamten freien Viewport; das
  gespeicherte Verhältnis wird **nicht angewendet** und nicht verändert und gilt
  beim Wiederöffnen beider Bereiche unverändert. Der Ordnerkopf schaltet erst
  nach dem Layout-Snapshot des Frames um. Beim Öffnen erhält die Queue zunächst
  das Queue-only-Budget; die Ordnerkarten warten bis zum nächsten Frame mit der
  neuen Aufteilung, damit sie nicht außerhalb des aktuellen Budgets liegen und
  die Fußzeile nach unten drücken. Beim Schließen bleiben die Karten im letzten
  Frame noch im bereits zugeteilten Beide-offen-Viewport sichtbar; im nächsten
  Frame gilt das Queue-only-Layout. Die Listenhöhe wird zusätzlich auf den
  tatsächlichen Platz abzüglich Fußzeile begrenzt und darf bis auf null sinken:
  so kann die Liste in winzigen Fenstern nicht überlaufen, auch wenn das feste
  Chrome dort nicht vollständig sichtbar ist. Der letzte offene Bereich kann nicht geschlossen
  werden; sein Toggle ist deaktiviert und nicht tastaturfokussierbar. Sind beide
  gespeicherten Zustände geschlossen, öffnet der Queue-Kopf sich selbst.
  Fortschrittszusammenfassung und Ergebnisfußzeile
  bleiben sichtbar, die Ordnerliste scrollt bei ihrem natürlichen Kartenmaß.

- **Scroll-Bug (F12, Welle 5.8).** Zwei `ScrollArea::vertical()` im selben
  Eltern-`Ui` teilten sich den Scroll-Zustand: egui setzt ohne `id_salt` die
  **Konstante** `Id::new("scroll_area")` (`scroll_area.rs:534`). Fix:
  **jede** ScrollArea ein eigenes `id_salt` (`"queue"`, `"folders"`).

- **Ordner in der Warteschlange (F13, Welle 5.8).** Ordner-**Spalte** je
  Zeile; bei **zwei Ordnern mit gleichem Namen** Name **+ Pfad**
  (Mehrdeutigkeit einmal pro Frame berechnet), voller Pfad im Tooltip. Ersetzt
  den früheren Wunsch „Ordnerpfad im Zeilennamen". Der damalige Gruppenkopf
  entfällt seit der flachen Statussortierung (2026-10-26).

- **Ordner-Karte Variant A (F14, Welle 5.8).** Zeile 1 = Name links,
  **Statuswort** rechts (farbig, je `BackupStatusAlertLevel`) + die drei
  Zeilen-Symbole; Zeile 2 klein = **Pfad · Fortschritt · Zuletzt
  aktualisiert**. Jeder Baustein steht **einmal** (vorher doppelte
  Prozentzahl), der Pfad ist immer sichtbar. Sechs `FolderStatus`-Werte mit
  Wort und Farbe (Paused → AMBER).
  **Verworfen:** Variant B (kompakt, eine Zeile, Pfad nur bei
  Mehrdeutigkeit); Variant C (nur die Dopplung entfernen, Pfad in die graue
  Zeile).
