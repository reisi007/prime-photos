# Entwickler-Wissen: Fallstricke dieser Codebase

Hier steht **Implementation-Wissen**, das weder im Original noch in `AGENTS.md`
liegt: warum bestimmte Stellen so aussehen, wie sie aussehen, und was passiert,
wenn man sie „normal" schreibt. `AGENTS.md` enthält die **Regeln**; dieses Dokument
den **technischen Grund** dahinter mit Fundstelle und Beleg.

Kein Original-Recon — alles hier ist eigenes Verhalten oder Verhalten der
genutzten Crates (egui 0.31, wry 0.57, rfd 0.15, notify-rust 4.18).

## 1. Keine synchrone AppKit-/RunLoop-API direkt aus `update`

egui's `update` läuft in winit's Event-Handler. Eine ObjC-Callback, die dort
eine eigene RunLoop dreht, re-entert denselben Handler; unter einem ObjC-Callback
ist unwinding verboten ⇒ der Prozess bricht mit **SIGABRT** ab, nicht mit einem
Rust-Panic-Backtrace.

| Aufrufer | Befund | Umsetzung |
|---|---|---|
| `notify-rust` (macOS) | sendet synchron und pumpt dabei die RunLoop — **abgestürzt am 2026-10-14** | Notification auf die Main-Dispatch-Queue legen (`dispatch2`), Ergebnis im nächsten Frame (`src/notify.rs`) |
| `wry::WebView::cookies()` | blockiert den Hauptthread bis zu 1 s und läuft aus einem `DispatchQueue::main`-Block **immer** in den Timeout | **komplett entfernt** (Geräteweg ist der einzige Anmeldeweg); nicht „asynchron repariert" |
| `rfd::FileDialog::pick_folder()` (sync) | öffnet auf macOS ein **modales** `NSOpenPanel` mit eigener RunLoop — **dieselbe Klasse** | **ungeprüft**, ob es real abstürzt; geplant: `AsyncFileDialog` auf der bestehenden Tokio-Runtime (Welle 5.1) |

**Regel:** aus `update` heraus niemals blockierende/native Main-Thread-API.
Näheres (inkl. der Cookie-Probe auf der Main-Queue) in `AGENTS.md` → „Plattform /
Portierung".

## 2. `ctx.load_texture()` nie innerhalb von `ctx.data_mut(...)`

`data_mut` hält den egui-Context-Lock bereits; `load_texture` will denselben
Lock — **Selbstblockade**, kein Deadlock über zwei Threads, sondern ein Lock, den
der eigene Frame hält. Symptom: die App friert beim **ersten** Zeichnen einer
Backup-Zeilen-Icon ein, der Session-Worker hängt in `ctx.request_repaint`.

Drin seit `a6e2ed1` (2026-10-14), behoben mit Regressionstest (Timeout): Cache-
Treffer per **Read-Lock** holen, `load_texture` **außerhalb** des Locks aufrufen,
dann ablegen (`icons::icon_texture`).

## 3. Widget-IDs leiten sich aus dem Text ab — dynamische Labels zurücksetzen den Zustand

Ein `CollapsingHeader` bekommt seine ID vom Label. Enthält das Label eine Zahl,
die sich pro Update ändert („Warteschlange (3)" → „Warteschlange (2)" → …), ist
es **jedes Frame ein neues Widget** und der Auf-/Zu-Zustand fällt zurück — die
Liste klappt von selbst wieder zu.

**Fix:** feste ID vergeben, in egui 0.31 `CollapsingHeader::id_salt("queue")`.
**Dieselbe Fehlerklasse** trifft jedes `CollapsingHeader` mit dynamischen Zahlen —
beim UI-Umbau (Welle 3) mitprüfen.

## 4. `popup_below_widget` sized das Popup auf seinen Anker

Der Anker ist das ~26 px breite Kebab-Icon ⇒ jeder Menüeintrag bricht zeichen-
weise um. **Fix:** `ui.set_min_width(220.0)` plus
`ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend)` innerhalb des
Popup-Closures (`src/backup.rs`, Zeilenmenü).

## 5. `ProgressBar`: feste Breite clippt den Text

Ohne `desired_width` nimmt die Leiste den **gesamten restlichen Platz** der Zeile
(`progress_bar.rs:119-123`: `available_size_before_wrap().x.at_least(96.0)`, der
Spinner ist über den Layout-Cursor vorher abgezogen). Mit fester Breite wird der
Text **auf das Leisten-Rechteck geclippt** (`progress_bar.rs:201-208`
`with_clip_rect(outer_rect)`) — genau das war der abgeschnittene Zähler in der
Fußzeile. Deshalb steht dort **kein** `desired_width`.

## 6. Klick auf leeren Bereich = nichts

Nutzerregel (2026-10-15): ein Klick in den leeren Bereich einer Seite löst **keine**
Aktion aus — keine Ordnerwahl, kein Finder, kein sichtbares Feedback. Die **einzige**
erlaubte Interaktion mit „leerem" Raum ist **Drag & Drop** von Bildern
(`hovered_files`/`dropped_files` in `src/app.rs`).

Belegter Ist-Zustand: im gesamten `src/` gibt es **genau einen** `Sense::click()` auf
einer größeren Fläche — `nav_item` in `src/app.rs` (die Seitenleiste); `src/icons.rs`
nutzt nur `Sense::hover()`. Jeder Pfad, der einen Finder/Dialog öffnet, hängt an
einem **Widget** (Kebab-Popup, Buttons), nie an einer leeren Fläche. Keinen solchen
Handler also „der Vollständigkeit halber" ergänzen.

## 7. Für Sichtprüfungen und Screenshots immer das Bundle bauen

Das nackte, aus dem Terminal gestartete Binary hat **keine** `Info.plist` — es
fehlt `NSHighResolutionCapable`, die Darstellung erscheint pixelig. Das gebaute
`.app` (`packaging/make-app.sh`) ist retina-scharf.

**Konsequenz:** UI-Sichtprüfungen und Screenshots **immer** über das Bundle, nie
über die nackte Binary — sonst misst man Artefakte der Startmethode.

## 8. Zwei Gates, die es (bewusst) nicht gibt

| Gate | Stand |
|---|---|
| Warnungen | **Baseline 0**, maschiniell erzwungen über `[lints.rust] warnings = "deny"` in `Cargo.toml` — jede Warnung ist ein harter Fehler, nichts wird mit `#[allow]` zugedeckt |
| `cargo fmt --check` | **kein Gate** — das Repo ist nicht fmt-clean, ein fmt-Diff ist kein Regressionssignal |
