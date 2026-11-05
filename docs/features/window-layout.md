# Window layout

The main window uses a left sidebar (navigation), a bottom log panel, and a
central content area. The layout follows egui's panel order, virtual lists
with fixed row heights, and measured chrome dimensions.

## How it works

- **Style application:** `theme::apply` runs `ctx.all_styles_mut(apply_style)`,
  so the custom style lives in **both** the light and the dark style map. A
  per-frame `set_style` of only the active theme is not enough: egui switches
  to the system theme on the first frame and would drop every custom value.
- **Page scale:** every page's `CentralPanel` (and the login screen) uses a
  frame with `theme::PAGE_BG` and the 16/12 inner margin
  (`theme::PAGE_MARGIN_X/Y`); the sidebar uses 16/12 (`SIDEBAR_MARGIN_X/Y`) and
  `theme::SIDEBAR_BG`. The bottom log panel uses the same 16 px horizontal
  gutter and the page background and is separated only by its panel line.
- **Queue table:** `queue_columns(content_w, gap, bar_w)` is the single source
  for the table width and the four column widths. Header and every row are
  built from `table_cell(…)` (one `allocate_ui_with_layout` per cell,
  left/right aligned), so the columns cannot drift. The table ends 18 px before
  the page gutter; a 1 px `theme::BORDER` line spans exactly that width under
  the header.
- **Folder cards:** the card frame uses the page gutter as inner margin, a 6 px
  corner radius and the table width; the status/action block is measured in an
  invisible probe so the left text column wraps before it can paint under the
  icons. The probe now only bounds the text column: folder cards draw at their
  natural height inside the folder scroll area (`auto_shrink(false)` keeps the
  split-assigned viewport), so wrapped names never need a content-height budget.
- **Sidebar account block:** measured with an invisible probe, then placed by
  consuming the free space above it — no fixed height that can clip the logout
  button.
- **Panel order in `App::update`:** `SidePanel::left("nav")` →
  `TopBottomPanel::bottom("log")` → `CentralPanel` (egui standard:
  **panels first**, `CentralPanel` last). **Do not reverse.** If
  `CentralPanel` came before, the central area does **not** include the log
  strip: `ui.available_height()` is too large by the panel height, and
  every page that fills its height (the queue does since Welle 3a) ends up
  behind the log panel — headless measured **+26 px** overlap at 760×520 and
  1000×700, after the change **−8 px**. The settings page does not depend on
  `available_height`/`screen_rect` and is unaffected.
- **Virtual list:** `ScrollArea::show_rows(ui, row_h, n, …)` gets the
  **real** row stride **without** `item_spacing`. In a flat `ui.horizontal`
  this is not true: the frame is `interact_size.y` high (theme 26),
  `add_sized` children are **centred** in it (stride 31), a **bare**
  `ui.label` expands to the full frame (stride 34). Correct is
  `ui.allocate_ui_with_layout(vec2(w, row_h), left_to_right(Center), …)`
  for **every** row type — then the stride is exactly `row_h +
  item_spacing.y` (in the queue 28 = 20 + 8) and the declared height
  matches the actual height (before: Δ 252 px ⇒ last row cut off).
- **Heights (Issue #33):** `page_heights_for_sections` is the single source for
  both scroll viewports. With **both sections open**, `Config::backup_queue_ratio`
  (default 0.35, clamped 0.20–0.80) splits the height left after the fixed chrome:
  `QUEUE_HEADER_TOTAL_H` = **40 px**, headlessly measured through the production
  table path: the 20 px header-cell response begins 3 px below the layout cursor,
  followed by 8 px before the 1 px divider and 8 px before the first `show_rows`
  row. That makes 31 px from the cursor to the divider, then 1 + 8 px to the
  first row; the 31 px figure is not the height of the cell response. Below the
  header, `QUEUE_BELOW_H` = **145 px** = table trailing 8 +
  progress zone 40 (6 px gap + 26 px row + 8 px spacing) + splitter zone 24
  (3 + 10 + 8 + 3) + folder heading zone 34 (the horizontal layout row is
  26 px, although the label response is 16 px, + 8 px spacing) + heading/list
  gap 6 +
  footer area 33 (table trailing 8 + post-list 6 + footer gap 6 + 13 px line).
  With **only the queue** open, `QUEUE_ONLY_BELOW_H` = **107 px** = 8 + 40 + 6 +
  34 + 6 + 13; with **only the folder list** open, `FOLDER_ONLY_BELOW_H` =
  **119 px** = 40 + 6 + 34 + 6 + 33. Both sections can never be closed at once.
  With enough room the queue keeps 72 px and the folder list 96 px. Tiny windows
  degrade to zero-height viewports without negative sizes; the allocation itself
  never overruns its budget, but a window smaller than the fixed chrome cannot
  show every control. The folder list follows a disclosure toggle one frame
  later: the transition frame keeps the previous layout budget (so the footer
  never overruns), and the next frame applies the new split.
- **Scroll-bug fix:** two `ScrollArea::vertical()` in the same parent `Ui`
  shared the scroll state: egui sets without `id_salt` the **constant**
  `Id::new("scroll_area")` (`scroll_area.rs:534`). Fix: **every** ScrollArea
  gets its own `id_salt` (`"queue"`, `"folders"`).
- **Backup-section toggle focus (Issue #36):** the disclosure triangle and
  section label share one keyboard focus target. When focused, it has a small,
  rounded outline in `theme::TEAL`; hover alone does not show the outline. The
  sole open section remains disabled and cannot receive keyboard focus.
- **Folder card Variant A:** row 1 = name left, **status word** right
  (colored, per `BackupStatusAlertLevel`) + the three row symbols; row 2
  small = **path · progress · last updated**. Each element appears **once**
  (previously duplicated percentage), path always visible. Six
  `FolderStatus` values with word and color (Paused → AMBER). The left text
  column is bounded by the measured status/action block, so a long row 2 wraps
  inside the card instead of running under the icons.
- **Folder list used the remaining height (historical, superseded by #33):**
  Earlier layout iterations reserved a queue lower bound and gave the remainder
  to the cards. Today both scroll viewports follow the user-adjustable split.
- **Progress bar never > 100 %:** the blue aggregate bar ("Alles aktuell")
  visibly exceeded the right edge during a run. Fix: remember the line width
  **before** the `ui.horizontal`, give the bar an explicit `desired_width`
  (without spinner = line width, with spinner = width − spinner width −
  `item_spacing.x`); the spinner width is `interact_size.y` = 26 in the
  theme.
- **Folder cell short:** the disambiguation in the queue shows only the
  **parent folder name** ("Sport (Pictures)") instead of the full path; the
  full path remains in the tooltip. Reason: "Sport
  (/Users/florianreisinger/Pictures)" was cut off in the 120-px column.
  **Rejected:** show only the name (same-name folders would be
  indistinguishable in the row); make the column wider.
- **Folders in the queue:** folder **column** per row **and** in the group
  header; with **two folders of the same name** name **+ path**
  (disambiguation computed once per frame), full path in the tooltip.
  Replaces the earlier wish "folder path in the row name".
- **UI reference:** `../original/UI_FLOWS.md` + `../original/UI_STATES.md`
  (clean-room spec from the decompiled app + resources). Use them when
  changing the UI.
- **ProgressBar width clipping:** a fixed `desired_width` clips the text to
  the bar rectangle (`progress_bar.rs:201-208` `with_clip_rect(outer_rect)`)
  — exactly the cut-off counter in the footer. That is why the footer uses
  **no** `desired_width` (see `../dev/DEV_NOTES.md` §5).

## Decisions

- **Panel-Reihenfolge in `App::update` (Befund 2026-10-16):**
  `SidePanel::left("nav")` → `TopBottomPanel::bottom("log")` → `CentralPanel`
  (egui-Standard: **Panels zuerst**, `CentralPanel` zuletzt). **Nicht
  zurückdrehen.** Stand das `CentralPanel` davor, schließt der Zentralbereich
  den Log-Streifen **nicht** aus: `ui.available_height()` ist um die
  Panelhöhe zu groß, und jede Seite, die ihre Höhe füllt (die Warteschlange tut
  das seit Welle 3a), endet hinter dem Protokoll-Panel — headless gemessen
  **+26 px** Überlapp bei 760×520 und 1000×700, nach der Umstellung **−8 px**.
  Die Einstellungs-Seite hängt nicht an `available_height`/`screen_rect` et
  ist unberührt.

- **Virtuelle Liste (Befund 2026-10-16):** `ScrollArea::show_rows(ui, row_h,
  n, …)` bekommt den **echten** Zeilen-Vorschub **ohne** `item_spacing`. In
  einem flachen `ui.horizontal` stimmt das nicht: das Frame ist
  `interact_size.y` hoch (Theme 26), `add_sized`-Kinder werden darin
  **zentriert** (Vorschub 31), ein **nacktes** `ui.label` expandiert auf das
  volle Frame (Vorschub 34). Richtig ist
  `ui.allocate_ui_with_layout(vec2(w, row_h), left_to_right(Center), …)` für
  **jede** Zeilenart — dann ist der Vorschub exakt `row_h + item_spacing.y`
  (in der Warteschlange 28 = 20 + 8) und die deklarierte Höhe stimmt mit der
  Ist-Höhe überein (vorher Δ 252 px ⇒ letzte Zeile abgeschnitten).

- **Responsive Höhen (frühere Version, durch Issue #33 ersetzt).** Früher waren
  die Ordnerliste inhaltbewusst und die Warteschlange auf 20–50 % der freien
  Höhe begrenzt. Jetzt teilt `Config::backup_queue_ratio` (Default **0.35**,
  geklemmt auf **0.20–0.80**) die beiden Viewports, solange beide Bereiche offen
  sind; bei genau einem offenen Bereich bekommt dieser die gesamte Resthöhe und
  das gespeicherte Verhältnis bleibt unangetastet. Das fixe Chrome ist
  vollständig im Budget berücksichtigt: `QUEUE_HEADER_TOTAL_H` = **40**,
  `QUEUE_BELOW_H` = **145**, `QUEUE_ONLY_BELOW_H` = **107** und
  `FOLDER_ONLY_BELOW_H` = **119**. Der separate Warteschlangen-Reiter bleibt
  verworfen.

- **Scroll-Bug (F12, Welle 5.8).** Zwei `ScrollArea::vertical()` im selben
  Eltern-`Ui` teilten sich den Scroll-Zustand: egui setzt ohne `id_salt` die
  **Konstante** `Id::new("scroll_area")` (`scroll_area.rs:534`). Fix:
  **jede** ScrollArea ein eigenes `id_salt` (`"queue"`, `"folders"`).

- **Ordner in der Warteschlange (F13, Welle 5.8).** Ordner-**Spalte** je
  Zeile (der frühere Gruppenkopf entfällt seit der flachen Statussortierung,
  2026-10-26); bei **zwei Ordnern mit gleichem Namen** Name **+ Name des
  Elternordners** in Klammern (z. B. „Sport (Pictures)"; Mehrdeutigkeit einmal
  pro Frame berechnet), voller Ordnerpfad im Tooltip, sofern einer vorliegt
  (bei der Einmal-Sicherung nicht).
  Ersetzt den früheren Wunsch „Ordnerpfad im Zeilennamen".

- **Ordner-Karte Variant A (F14, Welle 5.8).** Zeile 1 = Name links,
  **Statuswort** rechts (farbig, je `BackupStatusAlertLevel`) + die drei
  Zeilen-Symbole; Zeile 2 klein = **Pfad · Fortschritt · Zuletzt
  aktualisiert**. Jeder Baustein steht **einmal** (vorher doppelte
  Prozentzahl), der Pfad ist immer sichtbar. Sechs `FolderStatus`-Werte mit
  Wort und Farbe (Paused → AMBER).
  **Verworfen:** Variant B (kompakt, eine Zeile, Pfad nur bei
  Mehrdeutigkeit); Variant C (nur die Dopplung entfernen, Pfad in die graue
  Zeile).

- **Ordnerliste bekam die Resthöhe (historisch, durch #33 ersetzt).** Frühere
  Layoutversionen reservierten eine Queue-Untergrenze und gaben den übrigen
  Platz den Karten. Beide Scroll-Viewports folgen jetzt dem einstellbaren Split.

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

- **Nav-Icon-Maße (Stand 2026-10-16): kein Rate-Wert.** Die Doku-Werte
  (`../original/UI_FLOWS.md` §3.1: Header 64 pt bzw. 104 pt mit Banner, 4 pt
  Akzentleiste, Nav-Spalte) sind für die **Icon-Maße** der Nav unzureichend —
  der Recon liefert dafür **keine** Pixelwerte. Entscheidung: **kein**
  geratener Wert; nach Welle 3c sitzen die Icons in einem festen
  **18×18-Rect**. Ein blindes „Ausrichten" würde ändern, was der User noch
  nicht beurteilt hat. Die offene Arbeit (Nav-/Sidebar-Icons ausrichten) bleibt
  im Ledger.

- **Abstands-Regel (Entscheidung 2026-10-17, UI-Review 0.1.1 F2/F3).**
  *Innerhalb* einer logischen Gruppe gilt nur das `item_spacing` des Themes
  (**8 px**, `src/theme.rs:89`); *zwischen* Gruppen kommt zusätzlich
  `ui.add_space(8.0)` dazu (**16 px gesamt**). Umgesetzt in `src/settings.rs` —
  `general_settings`: vier Checkbox-Gruppen, jede Checkbox mit ihrem eigenen
  `theme::muted`-Hinweis darunter (die Benachrichtigungen-Checkbox ohne
  Hinweis); `bandwidth_settings`: Widget → eigener Hinweis **ohne**
  Zusatzabstand, Hinweis → neues Gruppen-Label `8.0` — und in `src/about.rs`
  (`about_window`): Blocktrennung Version-Zeile → Schaltflächen-Block `8.0`.
  Die **engen** Werte *innerhalb* eines Blocks bleiben bewusst (Name → Version
  `4.0`, zwischen den zwei Schaltflächen `6.0`) — bewusste Kopplung, **kein**
  Befund. Anlass: eine Checkbox und ihr eigener Hinweis lagen genauso weit
  auseinander wie der Hinweis und die nächste Checkbox. Gemessen
  (Einstellungen/Allgemeines): vorher innen ≈ 6 px / außen ≈ 5–7 px (optisch
  nicht unterscheidbar), nachher innen ≈ 6 px / außen ≈ 13–15 px. **Nicht
  zurückkorrigieren:** die 16 px zwischen Gruppen sind die Entscheidung — und
  die engen Werte innerhalb eines Blocks ebenso.

- **Überschrift → Ordnerliste: 6 px (Befund 2026-10-17, Issue #33).** Zwischen
  dem Ordnerkopf und der Karten-`ScrollArea` liegen 6 px. Ist genau ein Bereich
  offen, kommen vor dem Ordnerkopf weitere 6 px (`SECTION_GAP`) hinzu; sind
  beide offen, ersetzt der Splitter mit 3 px Luft, 10 px Griff und 3 px Luft
  diesen Abstand. Alle Anteile stecken im Höhenbudget (`QUEUE_BELOW_H`,
  `QUEUE_ONLY_BELOW_H` bzw. `FOLDER_ONLY_BELOW_H`), sodass die Liste weder
  Überschrift noch Splitter berührt.

- **Kompaktere Backup-Abstände (Issue #33, 2026-11-03).** Die Fortschrittszone
  verbraucht in jedem Zweig **40 px** (6 px Abstand + feste 26 px Zeile + 8 px
  `item_spacing`; der Balken selbst ist 20 px hoch), auch wenn keine
  Fortschrittszeile gezeichnet wird. Die Splitterzone ist **24 px** (3 + 10 +
  8 + 3), die Ordnerkopfzone **34 px** (26 px Zeile + 8 px Abstand), der
  Überschrift/Listen-Abstand **6 px**. Die aggregierte Zähler-/Ergebniszeile
  und der Log-Kopf „Letzter Upload“ stehen in dunkler Tinte bei 11 pt; die
  Karten-Meta-/Ergebniszeilen bleiben bei 11 pt und `theme::muted`. Der
  Footer-Bereich ist **33 px** (8 px Tabellen-Nachlauf + 6 px nach der Liste +
  6 px vor dem Ergebnis + 13 px Zeilenbox). `QUEUE_BELOW_H` ist **145 px**
  (8 + 40 + 24 + 34 + 6 + 33), `QUEUE_ONLY_BELOW_H` **107 px** (8 + 40 + 6 +
  34 + 6 + 13) und `FOLDER_ONLY_BELOW_H` **119 px** (40 + 6 + 34 + 6 + 33);
  `QUEUE_HEADER_TOTAL_H` ist **40 px** (headless am Produktionspfad gemessen:
  Die 20-px-Zellen-Response beginnt 3 px unter dem Layout-Cursor; es folgen
  8 px bis zur 1-px-Trennlinie und 8 px bis zur ersten virtuellen Zeile). Die
  31 px bis zur Trennlinie sind nicht die Höhe der Zellen-Response. Beim
  Ordnerkopf misst die Text-Label-Response 16 px, die umschließende egui-
  Layoutzeile aber 26 px; mit 8 px Abstand bleibt die Zone daher **34 px** —
  nicht 25 px. Karten haben 3 px äußeren Vertikalrand und 6 px Listenabstand.

- **Persistierter Backup-Splitter (Issue #33, #36).** Beide Bereiche starten offen;
  Disclosure-Dreieck und Bereichslabel aktivieren denselben fokussierbaren,
  barrierefrei beschrifteten Toggle; bei Tastaturfokus wird er mit einem
  abgerundeten Rahmen in `theme::TEAL` sichtbar, bloßes Hover zeigt keinen
  Rahmen. Der letzte offene Bereich bleibt auch per Label deaktiviert und
  erhält keinen Tastaturfokus. Die Disclosure-Knöpfe bleiben als ergänzendes
  Accordion. In `config.toml` wird das Queue-Verhältnis
  (`Config::backup_queue_ratio`) gespeichert: standardmäßig **35 %**, begrenzt
  auf **20–80 %**; ältere Konfigurationen ohne Schlüssel erhalten 35 %,
  NaN/unendlich fallen ebenfalls darauf zurück. Der **10-px-Griff**
  ist per Drag und per Up/Down-Tasten bedienbar (Shift = 10 % statt 3 %);
  gespeichert wird beim Loslassen bzw. bei einer Tastaturänderung, nicht pro
  Frame. Bei ausreichend Höhe haben Queue- und Ordner-Viewport Mindestgrößen von
  **72 und 96 px**; kleinere Fenster degradieren zu 0 hohen Viewports ohne
  negative Größen. Die Allokation selbst überschreitet ihr Budget nicht, ein
  Fenster kleiner als das feste Chrome kann aber nicht alle Bedienelemente
  zeigen. Ist einer eingeklappt, belegt der andere den gesamten flexiblen Raum;
  das gespeicherte Verhältnis wird **nicht angewendet** und nicht verändert und
  gilt wieder, wenn beide geöffnet sind. Die Ordnerliste folgt einem
  Disclosure-Toggle einen Frame später: Der Übergangs-Frame behält das bisherige
  Layout-Budget (damit die Fußzeile nie überläuft), und der nächste Frame wendet
  die neue Aufteilung an. Der letzte offene Bereich kann nicht eingeklappt werden.

- **Scrollbalken-Reserve ist jetzt dauerhaft (Issue #30, ersetzt den Stand
  2026-10-17).** Die in `src/theme.rs` reservierte Breite für den schwebenden
  Balken (`floating_allocated_width = bar_width + SCROLL_BAR_GAP` = **18 px**;
  Griff = `bar_width` 10 px + 8 px Abstand) war nur dann wirksam, wenn egui den
  Balken einblendete — ohne Hover schrumpfte der reservierte Streifen auf 0, die
  Tabelle/Karten wurden breiter und sprangen beim Hover. Jetzt setzen die
  Warteschlange und die Ordnerliste `ScrollBarVisibility::AlwaysVisible`: der
  18-px-Streifen ist **immer** reserviert, die rechte Kante bleibt konstant. Der
  Griff selbst bleibt im Ruhezustand unsichtbar (Floating-Style,
  `dormant_handle_opacity = 0`), sodass ohne Hover weiterhin kein Balken zu
  sehen ist. **Verworfen:** den Streifen verkleinern oder auf der Karte
  asymmetrisch ausgleichen.

- **Theme gilt für alle Styles (Issue #30).** `theme::apply` schrieb mit
  `ctx.set_style` nur den **aktiven** Style. egui wechselt im ersten Frame auf
  das System-Theme (Light) und wählt dabei seinen Default-Light-Style — alle
  eigenen Werte (8/8-`item_spacing`, 18-px-Scrollbar-Reserve, Panel-Füllung,
  Schriftskala) gingen verloren. Das erklärte die Screenshots aus #29
  (Zeilen-Vorschub 23 statt 28, Karten bis an den Fensterrand, abgeschnittene
  Statusspalte). Fix: `ctx.all_styles_mut(apply_style)` schreibt Light- **und**
  Dark-Style (`dark_mode = false` in beiden, die App ist bewusst nur hell). Ein
  Regressionstest prüft nach `apply` beide `ThemePreference`-Werte.

- **Abstands-Skala (Issue #30).** Seitenrand: **16 px** horizontal, **12 px**
  oben/unten (`theme::PAGE_MARGIN_X/Y`); Sidebar: **16 px** horizontal, **12 px**
  vertikal (`theme::SIDEBAR_MARGIN_X/Y`); Seitenhintergrund `PAGE_BG = #F7F8F8`,
  Karten bleiben weiß, die Sidebar `#F3F5F5`. Die Werte stehen als Konstanten in
  `src/theme.rs` und werden für **Seiten- und Sidebar-Gutter** an allen
  Aufrufstellen verwendet; Kontext-Abstände bleiben bewusst eigene Werte (der
  Login-Header `16/8` in `src/login.rs`, das Log-Panel vertikal `2` in
  `src/app.rs`). Dadurch beginnen Sidebar-Inhalt (x = 16) und Seiteninhalt
  (x = 220 + 16 = 236) jeweils in ihrem Gutter, und Logo sowie erste
  Inhaltszeile liegen auf derselben y-Höhe (12). Das Log-Panel übernimmt
  denselben horizontalen Gutter (16) und den Seitenhintergrund, damit es optisch
  zweitrangig bleibt und die Inhaltskante nicht unterbricht.

- **Kontoblock der Sidebar wird gemessen, nicht reserviert (Issue #30).** Die
  frühere feste Reserve (`ACCOUNT_BLOCK_H = 104`) schnitt „Abmelden“ ab, sobald
  die Konfigurations-Fehlermeldung auf zwei Zeilen umbrach (Zustand `failures`,
  760×520). Jetzt wird der Block zuerst in einer unsichtbaren, deaktivierten
  Probe mit exakter Inhaltsbreite vermessen; die freie Höhe darüber wird
  verbraucht, der echte Block endet exakt am unteren Sidebar-Rand. Kein
  fester Wert, keine `bottom_up`-Sonderfälle.

- **Warteschlange: eine Geometrie, echte Zellen (Issue #30).** `queue_columns`
  (rein, getestet) liefert Tabellenbreite und Spaltenbreiten: Tabelle endet
  `content_w - bar_w`, Ordner 120, Größe 80, Status **140** (passt
  „In Bearbeitung · 100 %“ bei 12 pt), Name flexibel mit Minimum 80. Kopf und
  jede Zeile benutzen dieselben `QueueColumns` und einen `table_cell`-Helfer
  (`allocate_ui_with_layout` mit `left_to_right`/`right_to_left`): Ordner/Name/
  Status links, Größe rechts. 1-px-Trennlinie (`theme::BORDER`) unter dem Kopf,
  exakt über die Tabellenbreite; darunter 8 px bis zur ersten Zeile. Der
  Gesamtbalken und der Leerzustand der Warteschlange verwenden die
  Tabellenbreite, nicht die volle Seitenbreite.

- **Ordnerkarte (Issue #30, spacing update #33).** Innenabstand
  `symmetric(16, 10)`, Außenabstand `(0, 3)`, Listenabstand 6 px, Eckenradius 6,
  Breite `cols.table_w` (nicht die animierte
  ScrollArea-Breite), damit die rechte Rahmenlinie sichtbar ist und beide Listen
  auf derselben Kante enden. Der Status-/Aktionsblock wird unsichtbar vermessen;
  die linke Textspalte bricht innerhalb der Restbreite um und kann nicht unter
  die Symbole laufen. Die Ordner-ScrollArea erhält ihre Höhe vom Splitter und
  scrollt, sobald Karteninhalte den Viewport übersteigen; `auto_shrink(false)`
  hält die zugeteilte Split-Höhe auch bei wenigen oder keinen Karten ein.

- **Anmeldebildschirm zentriert (Issue #30, F2).** `ui.horizontal` innerhalb
  von `ui.vertical_centered` nimmt die volle Breite ein und beginnt am linken
  Fensterrand; die Marktplatz-Zeile klebte dort. Jetzt wird die Zeilenbreite
  (Label im Body-Font + 8 px + ComboBox 200 px) gemessen und als Breite des
  Formularblocks verwendet: der Block ist damit exakt so breit wie sein Inhalt
  und wird als Ganzes zentriert (Layout-Mitte 700.0 bei 1400, 380.0 bei 760).
  Das Login-CentralPanel hat denselben 16/12-Seitenrahmen wie die Hauptseite.
