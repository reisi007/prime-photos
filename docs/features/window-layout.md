# Window layout

The main window uses a left sidebar (navigation), a bottom log panel, and a
central content area. The layout follows egui's panel order, virtual lists
with fixed row heights, and measured chrome dimensions.

## How it works

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
  of the free height. Only the measured chrome (`QUEUE_BELOW_H`,
  `QUEUE_HEADER_H`, `QUEUE_FOOTER_H`) remains absolute.
- **Scroll-bug fix:** two `ScrollArea::vertical()` in the same parent `Ui`
  shared the scroll state: egui sets without `id_salt` the **constant**
  `Id::new("scroll_area")` (`scroll_area.rs:534`). Fix: **every** ScrollArea
  gets its own `id_salt` (`"queue"`, `"folders"`).
- **Folder card Variant A:** row 1 = name left, **status word** right
  (colored, per `BackupStatusAlertLevel`) + the three row symbols; row 2
  small = **path · progress · last updated**. Each element appears **once**
  (previously duplicated percentage), path always visible. Six
  `FolderStatus` values with word and color (Paused → AMBER).
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
