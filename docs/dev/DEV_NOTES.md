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
| `cargo fmt --check` | **kein Gate**, solange das Repo nicht fmt-clean ist — ein fmt-Diff ist dann kein Regressionssignal. **Nach** der einmaligen Formatierung des Altbestands gilt er als Gate. Die Handlungsanweisung steht in `AGENTS.md` („Gates"). |

**Warum die Warnungs-Baseline 0 ist (User-Entscheidung):** die Warnungen wurden **nicht** pauschal
mit `#[allow(...)]` stumm gestellt — das beseitigt das Rauschen, versteckt aber genau die toten
Funktionen, um die es geht. Ebenfalls verworfen: nur die **future-incompatible** Warnung zu fixen
und die Baseline bei sechs zu lassen — der User wollte **0** als Vergleichsbasis. Deshalb ist
„Warnungen" ein hartes Gate und keine Sammlung von Ausnahmen.

## 9. Modul-Struktur (User-Entscheidung 2026-10-13)

`src/main.rs` war ein Monolith (~1780 Zeilen: State + Rendering + Upload-Worker + Formular +
Icons). Das ist der Engpass des Ports: fast jedes Paket ändert dieselbe Datei, also läuft alles
seriell. Deshalb wurde ein **reiner Refactor ohne Verhaltensänderung** in Module gesplittet;
danach trifft jedes Paket möglichst eine Datei und Subagenten können parallel arbeiten.

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

## 10. OpenCode-MCP: rea-Server (2026-10-13, User-Entscheidung)

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
- Protokoll/Begründung: Git-Historie (`git log -p -- AGENTS.todo.md`); der damalige
  Ledger-Abschnitt „OpenCode-MCP: rea-Server" wurde mit `dace89b` entfernt.

## 11. Umsetzer-/Verifier-Modell: `deepseek-v4.1-flash` (2026-10-14 gesetzt, 2026-10-15 zurückgenommen)

Das Modell war am **2026-10-14** als Umsetzer- und Verifier-Modell gesetzt und am **2026-10-15**
wieder zurückgenommen — es lief genau einen Tag, ohne dass ein Fehler oder Gate daran schuld war;
der User hat schlicht wieder zurückgestellt.

**Stand 2026-10-16:** seit der Kürzung von `AGENTS.md` ist das Modell dort **nicht mehr** fixiert.

**Regel:** ein Wechsel — zurück auf dieses oder auf ein anderes Modell — braucht eine **neue**
User-Entscheidung, nicht den alten, zurückgenommenen Eintrag.

## 12. Zeilenverweise auf unseren Code in den Recon-Specs sind Stichtage

Die `docs/original/SPEC_*.md` zitieren unseren Code als `src/<datei>:<zeile>` — diese
Nummern sind **Momentaufnahmen** und driften mit jedem Umbau (z. B.
`is_content_duplicate_code` in `SPEC_FILE_IDENTITY.md`: als `:368` genannt, richtig ist
`:449`; der `content_seen`-Aufruf als `:602` genannt, richtig ist `:661`). **Regel:**
beim Lesen eines solchen Verweises die **Symbolstelle** nachschlagen
(Codegraph/`grep -n`) statt der Zahl zu vertrauen; wer so eine Datei anfasst, korrigiert
die Verweise, die er verifiziert hat. Bestehende Abschnitte werden nicht umnummeriert.

## 13. README bleibt versionsfrei (User-Entscheidung 2026-10-17)

Die `README.md` nennt **keine** Release-Anzahl und **keine** Versionsnummern; Releases werden dort
nur über den Link auf die Release-Seite sichtbar (dort steht jeweils das Neueste). **Zweck:** So
veraltet die README bei keinem Release — die Versionspflege beschränkt sich auf `Cargo.toml`,
`packaging/` und `docs/dev/RELEASE.md`. Die handlungsleitende Regel steht in `AGENTS.md`
(„README versionsfrei halten").
