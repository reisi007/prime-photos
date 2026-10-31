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
  icons. The same probe feeds the card-height reservation: name/meta/outcome
  are laid out at the card's real column width, and each extra wrapped line
  raises the reserved height (see the F1 decision below).
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
- **Heights (`page_heights`):** `page_heights` as **one** source; the
  **folder list is content-aware** (as many cards as fit), the **queue**
  keeps at least `QUEUE_MIN_FRAC` (20 %) and at most `QUEUE_MAX_FRAC` (50 %)
  of the free height. Only the measured chrome (`QUEUE_HEADER_TOTAL_H`,
  `QUEUE_BELOW_H`, `QUEUE_FOOTER_H`) remains absolute; the header total
  includes both item spacings and the 1 px divider.
- **Scroll-bug fix:** two `ScrollArea::vertical()` in the same parent `Ui`
  shared the scroll state: egui sets without `id_salt` the **constant**
  `Id::new("scroll_area")` (`scroll_area.rs:534`). Fix: **every** ScrollArea
  gets its own `id_salt` (`"queue"`, `"folders"`).
- **Folder card Variant A:** row 1 = name left, **status word** right
  (colored, per `BackupStatusAlertLevel`) + the three row symbols; row 2
  small = **path · progress · last updated**. Each element appears **once**
  (previously duplicated percentage), path always visible. Six
  `FolderStatus` values with word and color (Paused → AMBER). The left text
  column is bounded by the measured status/action block, so a long row 2 wraps
  inside the card instead of running under the icons.
- **Folder list gets all remaining height:** the bottom card was cut off
  because `page_heights` capped the list at `FOLDER_LIST_MAX_FRAC = 0.45`
  of the free height. Now: the list gets all space except the **queue
  lower bound** (`QUEUE_MIN_FRAC`) and the chrome — the queue keeps its
  minimum, the cards are fully visible.
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

- **Responsive Höhen (F8/F11, Welle 5.8).** `page_heights` als **eine**
  Quelle; die **Ordnerliste inhaltbewusst** (so viele Karten, wie passen), die
  **Warteschlange** mindestens `QUEUE_MIN_FRAC` (20 %) und höchstens
  `QUEUE_MAX_FRAC` (50 %) der freien Höhe. Nur das gemessene Chrome
  (`QUEUE_HEADER_TOTAL_H`, `QUEUE_BELOW_H`, `QUEUE_FOOTER_H`) bleibt absolut;
  die Header-Summe enthält beide `item_spacing`-Abstände und die 1-px-Linie.
  **Verworfen:** ziehbare Trennlinie (Position überlebt den Neustart nicht);
  eigener Reiter für die Warteschlange (widerspricht „Warteschlange et
  Fortschrittsleiste gehören zusammen", Welle 3a).

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

- **Ordnerliste bekommt die ganze Resthöhe (User-Entscheidung 2026-10-16).**
  Die unterste Karte war angeschnitten, weil `page_heights` die Liste auf
  `FOLDER_LIST_MAX_FRAC = 0.45` der freien Höhe kappte. Neu: die Liste bekommt
  allen Platz außer der **Queue-Untergrenze** (`QUEUE_MIN_FRAC`) und dem
  Chrome — die Queue behält ihr Minimum, die Karten sind vollständig
  sichtbar. **Verworfen:** die Listen-Höhe auf ganze Karten zu runden; es so
  zu lassen („scrollt halt").

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

- **Überschrift → Ordnerliste: 8 px (Befund 2026-10-17, UI-Review F1).** Auf
  der Seite „Datensicherung" steht vor der Ordner-Karten-`ScrollArea` jetzt
  `ui.add_space(8.0)`. Ohne diese Luft klebte die Überschrift „Automatisch
  gesicherte Ordner:" mit 0 px an der Liste, und im gescrollten Zustand klebte
  eine halb sichtbare Karte daran. Die Luft ist Teil des gemessenen
  Layout-Puffers `QUEUE_BELOW_H` (`src/backup.rs`), der deshalb von **88.0 auf
  96.0** ging (gemessen: Abstand Überschrift → erste Karte +8 px).

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

- **Ordnerkarte (Issue #30).** Innenabstand `symmetric(16, 10)`, Außenabstand
  `(0, 4)`, Eckenradius 6, Breite `cols.table_w` (nicht die animierte
  ScrollArea-Breite), damit die rechte Rahmenlinie sichtbar ist und beide Listen
  auf derselben Kante enden. Der Status-/Aktionsblock wird unsichtbar vermessen;
  die linke Textspalte bricht innerhalb der Restbreite um und kann nicht unter
  die Symbole laufen.
  **Nachtrag F1/R1 (Review-Runden 2–3):** Die frühere Konstante
  `FOLDER_CARD_H = 88` gilt nur für einzeilige Texte und wurde für gewrappte
  Namen sogar um ~4.2 pt pro Zeile unterschätzt (13 statt der echten 15-pt-Zeile
  mit 17 pt). Deshalb gibt es **keine Zeilenkonstante mehr**: `backup_ui` misst
  vor der Höhenbudgetierung je Karte den Status-/Aktionsblock mit derselben
  unsichtbaren Probe wie im Draw und legt die **genau dieselben Labels** (Name
  15 pt, Meta 11 pt, Outcome 11 pt) mit `fonts.layout(...)` bei der echten
  Spaltenbreite aus. Die Reservierung ist
  `Galley-Höhe(Name) + Galley-Höhe(Meta) + [Galley-Höhe(Outcome)]`
  `+ 8 px je Blockabstand + 30 px festes Chrome` (Innenrand 10/10, 1-px-Rahmen
  und Außenrand 4/4, dieselben Konstanten wie der Frame); der Inhalt ist das
  Maximum aus Textspalte und gemessenem Aktionsblock. Das **Outcome zählt nur,
  wenn die Karte es wirklich zeichnet**. Eine gewrappte 15-pt-Namenszeile
  addiert damit real 17 pt, eine 11-pt-Meta-Zeile 13 pt. Diese Höhen fließen in
  `page_heights` ein, sodass die Liste exakt die Karten reserviert, die sie
  zeichnet. Bei 760×700 ist die zweite Karte vollständig sichtbar; bei 760×520
  ist der Viewport (148.5 pt) kleiner als zwei Karten (~199–223 pt) — dort
  bleibt die Liste bewusst scrollbar.

- **Anmeldebildschirm zentriert (Issue #30, F2).** `ui.horizontal` innerhalb
  von `ui.vertical_centered` nimmt die volle Breite ein und beginnt am linken
  Fensterrand; die Marktplatz-Zeile klebte dort. Jetzt wird die Zeilenbreite
  (Label im Body-Font + 8 px + ComboBox 200 px) gemessen und als Breite des
  Formularblocks verwendet: der Block ist damit exakt so breit wie sein Inhalt
  und wird als Ganzes zentriert (Layout-Mitte 700.0 bei 1400, 380.0 bei 760).
  Das Login-CentralPanel hat denselben 16/12-Seitenrahmen wie die Hauptseite.
