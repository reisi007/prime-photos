# TODO / decisions ledger

Regel (User-Entscheidung 2026-10-15, steht in `AGENTS.md` → „Arbeitsablauf"): dieses Protokoll
hält **nur offene Arbeit**. Umgesetztes **und** unabhängig Verifiziertes wird **gelöscht**, nicht
abgehakt — die Historie steht in den Commits. Wissen, das **nur** hier stand, ist vor dem Löschen
in `docs/` ausgelagert (Stand 2026-10-15: `docs/PROTOCOL.md` § 5, `docs/SPEC_SCOPE_INVENTORY.md`,
`docs/DEV_NOTES.md`, `docs/features/LIVE_ACCEPTANCE.md`, `docs/RELEASE.md` § 10). „Verworfen" wird
**nie** gelöscht.

## Aktueller Auftrag (2026-10-15, Restbestand)

Erledigt und gelöscht: Commits der verifizierten Runde (`9cad019`), Tray-Icon erst nach Login
(`746392d`, Verifier APPROVE), Fortschritts-Leiste volle Breite (`0363bf1`, Verifier APPROVE).

1. [ ] **`docs/`-Gerüst:** bestehende Spec-Dateien in Unterverzeichnisse sortieren
   (`docs/original/`, `docs/protocol/`, `docs/dev/` …), ein `docs/README.md` als Index — **alle**
   Referenzen in `AGENTS.md`, `AGENTS.todo.md`, `docs/` und Code mitziehen. Anlass: User-Wunsch
   „gerne einen docs Ordner mit einer größeren Struktur". **Bekannter Rest:** `docs/features/FEATURES.md:8`
   nennt `docs/original/` und `docs/protocol/`, die es **noch nicht gibt** — beim Umzug mitziehen.
2. [ ] **Release-Build + Install:** `cargo build --release` + `packaging/make-app.sh`, lokal mit
   **„Prime Photos Local Signer"** signiert, in eine **DMG** verpacken
   (`hdiutil create -volname "Prime Photos" -srcfolder <staging> -ov -format UDZO`), nach
   `/Applications` installieren und `xattr -dr com.apple.quarantine` setzen, damit der User sie
   sofort nutzen kann. **Voraussetzung:** der Keychain-Dialog „… möchte den Schlüssel
   »refresh-token« verwenden" muss einmalig mit **„Immer erlauben"** bestätigt sein (stabiles
   Zertifikat, daher nur einmal).

## Prio 1 — Bugs aus der Live-Nutzung 2026-10-15 (User-Meldungen)

Erledigt und gelöscht (jeweils implementiert, unabhängig verifiziert = Verifier APPROVE, committet):
Bug 1 „Where is `use_default?" (`2899bc5`; Wissen in `docs/DEV_NOTES.md` § 1), Bug 7 „Klick auf den
leeren Bereich = Nichts" (nichts zu bauen — Belege in `docs/DEV_NOTES.md` § 6), Bug 9 „Warnungen als
Fehler" (`c019528`; Regel in `AGENTS.md`), Bug 8 „Sichere Ersetz-Regel" (`2e2f183`; Regel in
`AGENTS.md` → „Sichere Ersetz-Regel", Belege in `docs/SPEC_DUPLICATES.md` § 5 und
`docs/SPEC_CONFLICT_OPTIONS.md`), Bug 3 „Logo neben „Prime Photos" in der Seitenleiste" (`ecb9bb8`;
nur `src/app.rs`, Verifier APPROVE) und Bug 6 „Aufnahmedatum/Online-Sortierung" (`fcbc14d`; Regel in
`AGENTS.md` → „Aufnahmedatum", Live-Belege in `docs/SPEC_CAPTURE_DATE.md` § 12, PUT-Abnahme in
`docs/features/LIVE_ACCEPTANCE.md`).

2. **Dock-/Taskbar-Icon fehlt (verkleinertes Logo nicht dargestellt).** `packaging/Info.plist`
   kennt **keinen** `CFBundleIconFile` und `Contents/Resources` enthält **keine** `.icns` — macOS
   zeigt ein generisches Platzhalter-Icon im Dock und im verkleinerten Zustand. Die Logos liegen
   als SVG-Quellen in `assets/` und werden über `build.rs`/`resvg` bereits gerastert; fehlt ist nur
   die `.icns`-Bündelung (mehrere Größen) und der Schlüssel im Info.plist.
   **In Umsetzung (User-Entscheidung 2026-10-16):** die `.icns` wird **im Build** aus
   `assets/logo.svg` erzeugt — `build.rs` schreibt die zehn iconset-Größen nach
   `target/pp-appicon.iconset`, `packaging/make-app.sh` bündelt sie per `iconutil` (macOS-Bordmittel)
   zu `Contents/Resources/AppIcon.icns`, `CFBundleIconFile = AppIcon`. **Kein** eingechecktes
   Binary, keine neue Abhängigkeit. Die Form bleibt **vollflächig** (belegt: auch das Original-Bundle
   `Contents/Resources/AppIcon.icns` ist vollflächig — kein Rand/Squircle-Umbau).

4. **Warteschlange und Fortschrittsleiste gehören physisch zusammen.** „▸ Warteschlange (n)" sitzt
   oben im Kopfbereich, die Gesamt-Leiste unten in der Fußzeile — der User erwartet beide
   zusammenhängend.

5. **Protokoll-Bereich unten abgeschnitten.** Die Log-Ausliste wird am unteren Fensterrand gekappt,
   die letzten Zeilen sind nicht erreichbar (Screenshot „Letzter Upload: 54/54 erfolgreich" mit
   abgeschnittenen OK-Zeilen).


## Offene Punkte aus der Konflikt-/Duplikat-Kette (Welle 2)

Die Umsetzung selbst ist abgeschlossen und gelöscht (Commits `42149bb`, `4ec98e7`, `f3c773b`,
`db68cff`, `a71191b`, `261501e`, `36aee39`, `40cf024`, `506df58`, `b0fa4c4`, `2803aa9` — alle
Verifier APPROVE). Geblieben ist **offene** Arbeit:

- [ ] **Live-Abnahme der Rückfall-Regel (2026-10-16).** Ebenfalls nur durch Tests gepinnt, nicht
  durch die Leitung: ein `Replace`-Ziel, dessen Pfad der Index **nicht** an die vom 409 benannte
  Node bindet, setzt den Lauf als **`RENAME`** fort (lokaler Zweig `resolution = SaveAsNew;
  continue;` in `src/upload.rs`). Belegbar an einer **fremden** Datei gleichen Namens — sie muss
  danach **unverändert** im Konto stehen (kein Datenverlust). Sinnvoll in **einem** Lauf mit der
  PUT-Abnahme oben (derselbe Namenskonflikt, einmal **mit** Bindung ⇒ PUT, einmal **ohne** ⇒ RENAME).
- [ ] **Zwei Befunde aus der Hänger-Diagnose (angelegt, nicht beauftragt).** (a) Ein Ziel, dessen
  Dateien **alle** fehlschlagen, behält `status = "uploading"` dauerhaft:
  `finalize_background_store` (`src/backup.rs`) schreibt Done/Error nur für Indizes aus
  `folder_done` — bei 0 Erfolgen fehlt der Index. (b) `FolderStore::load()` (`src/folders.rs`)
  macht bei **jedem** TOML-Parse-Fehler `unwrap_or_default()`: **ein** fehlerhafter Eintrag in
  `folders.toml` leert **still** den ganzen Store — danach läuft **kein** Hintergrund-Lauf mehr,
  ohne eine einzige Meldung.
- [ ] **Kein Test deckt die drei Vorbelegungsstellen** (`add_backup_folder`, transiente Einmal-Zeile,
  `UploadPlan::from_files`): der Verifier hat jede einzeln auf `true` zurückgedreht — alle 95 Tests
  blieben grün. Ursache: `PrimePhotoApp` braucht egui-Kontext bzw. Datei-Dialog. Ein **reiner
  Helfer** („Config ⇒ (dedup, conflict) für die Einmal-Queue") würde die Entscheidung testbar machen.
- [ ] **Ein Label-Test prüft deutschen Wortlaut** (`remember_label_names_the_folder`): unter
  `Lang::En` wäre er rot. Unkritisch, solange die Tests mit Deutsch laufen — beim nächsten Anfassen
  sprachneutral formulieren.
- [ ] **Das Häkchen-Gating ist eine Stufe großzügiger als das Schreiben:**
  `remember_checkbox_label` gated nur auf „Store kennt den Index", `remember_conflict_in_folder`
  zusätzlich auf `file_path.starts_with(&folder.path)`. Im Sonderfall eines verschobenen Index
  *mit* fremdem Pfad benennt das Häkchen einen Ordner, in den nichts geschrieben würde.
- [ ] **i18n-Hygiene:** toter Key „Als neue Dateien speichern" (`src/i18n.rs`) außerhalb der Tabelle
  unbenutzt; dazu **Doppel-Keys** in der Tabelle — `"Ersetzen"` (zweimal) und `"Ignorieren"`
  (zweimal), je eine Kopie ist toter Text (gleicher Wert, daher harmlos). Es gibt keine
  Eindeutigkeitsprüfung — Kandidat für eine Prüfung im Test.
- [ ] **`docs/SPEC_DUPLICATES.md` zwei stale Empfehlungszellen + eine neue Zeile** (Verifier-Funde,
  echt): (1)+(2) die **Empfehlungszellen** neben „Global dedup default setting" bzw. `:269` sagen
  noch „Optional. Only worth it if a preferences screen lands" — auf den Nachbarzeilen-Stil („Done")
  ziehen. (3) **neu (2026-10-16, vom Umsetzer gemeldet):** die Zeile `:275` „**Global conflict
  setting** | *Does not exist* | **We also have none**" ist auf unserer Seite überholt — seit
  `Config.conflict_resolution_default` gibt es die globale Konflikt-Standardwahl (Einstellungen →
  Upload); die Original-Spalte bleibt „gibt es dort nicht". **Erledigt und damit aus dieser Liste
  raus** (Runde 2026-10-16, Commit `83376ba`): die Zeilen `:268` und `:273`/`:278` (App-Default
  `Replace`, voller POST-Query-Satz) sowie der §5-Text.

## Offene Kandidaten derselben Absturzklasse (2026-10-14) — **ungeprüft**

Regel aus dem Absturz am Laufende (2026-10-14, behoben; Details in `docs/DEV_NOTES.md` § 1):
**keine synchrone AppKit-/RunLoop-API direkt aus `update`.**
- [ ] **`rfd::FileDialog::pick_folder()`** (`src/backup.rs`, `src/backup_config.rs`) — öffnet auf
  macOS ein **modales** `NSOpenPanel` mit eigener RunLoop. Dieselbe Klasse wie der Absturz
  (`notify-rust`). **Ungeprüft:** ob es real abstürzt, ist **nicht** belegt. Prüfen und ggf. auf
  `AsyncFileDialog` + `self.rt` umstellen (die Tokio-Runtime existiert bereits).

## Offene Punkte (Bugs & Wünsche aus 2026-10-13)

Oberziel des Users: **nur neue Dateien hochladen, keine Bandbreite verschwenden.**

- [ ] **Icons nicht sauber ausgerichtet.** Nav-/Sidebar-Icons an die Doku-Werte ausrichten
  (Header 64 pt bzw. 104 pt mit Banner, 4 pt Akzentleiste, Nav-Spalte — `docs/UI_FLOWS.md` §3.1);
  braucht die Maßwerte/Design-Abmachung, kein Rate-Wert.
- [ ] **Logout-Residuen.** `logout()` löscht die 5 Auth-Felder, stoppt aber weder eine laufende
  Login-WebView noch einen laufenden Upload (der Worker hält eigenen `cfg`-Klon und arbeitet mit
  dem alten Cookie weiter); `config_error` nach fehlgeschlagenem `save()` wird auf dem Login-Screen
  nicht angezeigt. Soll: Login-/Upload-Status zurücksetzen, Fehler sichtbar machen.
- [ ] **Nav-Accessibility.** Selbstgezeichnete `nav_item`-Zeilen (`allocate_exact_size`) haben keine
  Keyboard-Fokus-/Accessibility-Semantik; optional `Sense`-Widget/`widget_info` ergänzen.
- [ ] **Deutsche Doku-Kommentare in `src/notify.rs`** (Altbestand in HEAD, Verifier-Befund
  2026-10-16, kosmetisch): dort stehen deutsche `///`-Doku-Texte, während der Projektstil in `src/`
  englische Kommentare vorsieht — die inline-`//`-Kommentare sind außerhalb von `i18n.rs`/`tr(...)`
  inzwischen **frei** von Deutsch (Runde `83376ba`). Kein eigener Auftrag; beim nächsten Anfassen
  von `notify.rs` mitziehen.

## Weitere offene Punkte (dokumentiert, nicht eingeplant)

Ausdrücklich **ohne** Starttermin:
- Queue-Paginierung (heute bricht das Panel bei 300 Zeilen ab, Original lädt seitenweise 1000).
- Fehlerfenster mit `readdtoqueue`/`retryall`; Semantik laut `SPEC_STATUS_DETAIL.md` §3.
- Pause **pro Ordner** (Zeilen-Symbol) — im Original vorhanden, bei uns Post-MVP.
- Sortierung/Filter der Tabelle; Spalte „Größe" + relative Zeit (`datetimeago_*`).
- Tray-Status-Popup mit Statistiken (`statuspopup_stats_*`).
- Kaltstart-Verhalten nach Login; Pro-Ziel-Limit; netzwerkgesteuerte Drosselung; Logout während
  laufendem Upload; Nav-Accessibility.
- **Toter Dependency-Eintrag `kamadak-exif`** (Befund des Verifiers der contentDate-Runde,
  2026-10-16): seit `fcbc14d` liest kein Code mehr EXIF, der Eintrag in `Cargo.toml` steht aber
  weiter und wird mitgebaut. Entfernen wäre ein eigener kleiner Auftrag (`Cargo.toml` +
  `Cargo.lock`, `cargo build`/`cargo test` als Gate) — bewusst **nicht** in der contentDate-Runde
  mitgemacht, damit dort ein Auftrag = ein Commit bleibt.
- **Nicht** eingeplant (bewusst Out of Scope): die vollständige In/Out-Liste mit Beleg-Keys steht in
  `docs/SPEC_SCOPE_INVENTORY.md`; Sync, Downloads/Restore, Datei-Management, Apple-Photos-Import,
  Videos, Autoplay, Upsell-/Quota-Banner, Auto-Update, Proxy, ausgeblendete Dateien,
  iOS-Dedupe-Prompt, Crash-Relaunch bleiben draußen.

## Referenz: was **nicht** neu erfunden werden darf

Alle Rechte-/Verhaltensfragen sind entweder aus den Clean-Room-Specs belegt oder als UNKNOWN
markiert (`docs/SPEC_*.md`, `docs/PROTOCOL.md`, `docs/openapi.yaml`). Wo eine Spec einen
Original-Wert nennt, gilt er als Vorgabe, nicht als Ratewert. Unser **eigenes** Verhalten —
Fallstricke, Ops-Tricks, Gates — steht in `docs/DEV_NOTES.md`; Live-Nachweise in
`docs/features/LIVE_ACCEPTANCE.md`.

## Geplante Wellen — **NUR PLAN**, keine Welle startet ohne ausdrücklichen Auftrag

Geschnitten nach **gemeinsamem Umfeld/Dateien**, damit jede Welle in einem Durchgang ohne
Merge-Konflikt läuft; die Reihenfolge ist eine Empfehlung.

### Welle 3 — Progress-/UI-Umbau + Layout-Punkte aus `docs/SPEC_UPLOAD_PROGRESS.md` § 7.8
- **3.1** Detail in den **Hauptbereich**, Klapp-Panel „Warteschlange" (`CollapsingHeader` in
  `src/backup.rs`) entfernen — Baum/Liste mit Spalten wie im Recon.
- **3.2** `CollapsingHeader`-Zustand hält nicht: feste ID (`id_salt`); dieselbe Fehlerklasse bei
  **jedem** `CollapsingHeader` mit dynamischen Zahlen prüfen (`docs/DEV_NOTES.md` § 3).
- **3.3** Zeile zeigt „Fehler" statt „Upload läuft": Status **live** ableiten (läuft `folder_total`
  für dieses Ziel ⇒ „Upload läuft"; erst nach `finalize` gilt der persistierte Ausgang).
- **3.4** Die zwei noch **gezeichneten** Glyphen (Nav-Icons, Flagge) auf Material Symbols umstellen
  (User-Regel: nichts selbst zeichnen außer dem Logo).
- **3.5** Layout-Punkte aus § 7.8, die der User ausdrücklich mit in diese Welle genommen hat:
  feste Spaltenköpfe, **Kürzen + Tooltip** bei langen Namen, Liste füllt die Höhe, 300-Zeilen-Kappe
  entfällt, leerer Zustand zentriert.
- **Ein Paket:** 3.1–3.5 zusammen in **einen** UI-Umbau — die Zustandsfehler (3.2/3.3) und der
  Fortschritt (3.1) sitzen an denselben Stellen.

### Welle 4 — Release-Pflicht: About + Third-Party-Lizenzen
Recon liegt vor (`docs/SPEC_ABOUT.md`), Attributionen sind gepflegt (`docs/THIRD_PARTY.md`, u. a.
Material Symbols). Das Original hat **keine** Lizenzliste — wir erzeugen sie selbst.

### Welle 5 — Robustheit & Politur (klein, unabhängig)
- **5.1** `rfd::FileDialog::pick_folder()` auf `AsyncFileDialog` (siehe „Offene Kandidaten" oben).
- **5.2** P2-Reste: Icon-Ausrichtung (`docs/UI_FLOWS.md` §3.1), Logout-Residuen, Nav-Accessibility.
- **5.3** Optional/Branding: die Zeile „Amazon Photos" im Account-Block durch die Domain ersetzen
  (Projektregel: kein Amazon-Branding im eigenen Fenster).
- **5.4** Bei fehlgeschlagener `ensure_origin_column`-Migration `conn: None` zurückgeben: heute
  bleibt eine **halb funktionierende** Verbindung — `record`/`should_skip`/`stat_matches` scheitern
  dann still, der **ganze** Dedup-Index fällt aus (jede Datei würde wieder gelesen und neu
  gesendet), nicht nur die Herkunfts-Unterscheidung. Dazu den überzeichnenden Kommentar in
  `src/dedup.rs` korrigieren.

### Welle 6 — Post-MVP / bewusst später (dokumentiert, **nicht** eingeplant)
Pro-Ziel-Limit, netzwerkgesteuerte Drosselung, Cooldown nach Event-Läufen, Watcher-Sicherheitsnetz,
Queue-Paginierung, Fehlerfenster (`readdtoqueue`/`retryall`), Pause **pro Ordner**, Sortierung/Filter
+ Größen-Spalte, Tray-Status-Popup, „Details anzeigen"-Panel, Videos, echte Speichernutzung (Quota),
`.app`-Bundle + Icon, „Startseite".

## Zurückgestellt — braucht eine bewusste User-Entscheidung (nicht autonom lösbar)

Nichts davon ist umgeszt; alle Punkte brauchen eine Vorgabe.

1. **Hintergrund-Sync-Rest:** (a) ob nach einem Event-Lauf ein Cooldown gelten soll (mehrere Drops
   kurz hintereinander) und (b) ob ein Lauf nur bei bestehendem WLAN/nicht-kostenpflichtigem Netz
   starten soll — beides Plattform-APIs, noch nicht festgelegt.
2. **Pro-Ziel-Limit / netzwerkgesteuerte Drosselung** (bereits als Post-MVP markiert).
3. **macOS-Signatur:** die selbstsignierte Identität „Prime Photos Local Signer" ist importiert und
   getrustt (damit der Keychain-Dialog nur **einmal** erscheint). Offen bleibt, ob **Benachrichtigungen**
   im signierten Bundle erscheinen und ob `--options runtime` für ad-hoc nötig ist.
4. **Logout während laufendem Upload:** `logout()` löscht die Auth-Cookies, stoppt aber den
   laufenden Lauf nicht. Soll der Upload stoppen/pausieren oder weiterlaufen und mit 401 scheitern?
   (Eine saubere Antwort bräuchte einen Abbruch-Kanal in der Upload-Pipeline.)
5. **Farbe/Sprache des Multi-Fenster-Verhaltens** („Einmalige Sicherung"-Zeilendarstellung).

## Offen (Backlog)

- [ ] **Optional/Branding:** Account-Block zeigt unter „Angemeldet als:" noch die Zeile „Amazon
  Photos" (Label des angemeldeten Amazon-Kontos, kein Eigen-Branding). Falls keine Amazon-Wortmarke
  im eigenen Fenster gewünscht ist: durch die Domain (`amazon.<cc>`) ersetzen.
- [ ] „Details anzeigen"-Panel (verschachtelt/flach wie das Original, `docs/UI_FLOWS.md` §4.7).
- [ ] Menüleisten-Status-Item (Status-Popup: Status, Benachrichtigungen, Speicher, §9).
- [ ] Videos hochladen (bisher nur Fotos). Offene Fragen dazu: `docs/SPEC_SCOPE_INVENTORY.md`.
- [ ] „Startseite"/Übersichtsseite? (Strings vorhanden, im Build ungenutzt.)
- [ ] `.app`-Icon für das Bundle (siehe Prio-1-Bug 2) — gehört zu `packaging/make-app.sh`.
- [ ] Rest aus `Amazon Photos.dll`: Quota-Form, exakter Extension-Filter, Watcher-Intervall,
  Konfliktbehandlung, Video-Support.
- [ ] Echte „Speichernutzung"-Anzeige (nur Prime; bis dahin bewusst nichts anzeigen).
- [ ] **`pct` in der Ordner-Zeile kappen** (`src/backup.rs` rechnet `done/total` ungekappt): ein
  inkonsistenter Alt-Zustand (`total_files` < `uploaded_files` — genau so wurden einmal „320 %"
  angezeigt) würde wieder über 100 % anzeigen.
- [ ] **Win/Linux: In-App-„Beenden".** Ohne sichtbares Tray-Icon gibt es dort keinen Weg zurück
  (macOS ist safe, ⌘Q) — Residuum aus PORT-3.
- [ ] **Gezeichnete UI-Glyphen ersetzen:** `nav_icon`, Flagge und Logo auf die Material-Symbols-
  Pipeline umstellen (Icons werden nicht handgezeichnet, siehe `AGENTS.md`); nicht beauftragt.

## Verworfen

Kein Eintrag hier darf nachträglich „doch noch" umgesetzt werden — verworfen heißt verworfen.

- **Separates Login-Fenster** (`ppu-login` Helper-Prozess mit eigener `tao`-Eventloop): wirkte
  ungewohnt und passte nicht zum Original, das im Hauptfenster anmeldet. Ersetzt durch die
  eingebettete Child-WebView im Hauptfenster.
- **Synchrone Cookie-Abfrage in `update`** (wry `WebView::cookies`): Panic durch winit-Reentrancy
  (dreht die NSRunLoop). Ersetzt durch `dispatch_async` auf der Main-Queue; später **komplett
  entfernt**, weil sie aus einem Main-Queue-Block immer in den Timeout lief (Geräteweg ist der
  einzige Anmeldeweg).
- **mitmproxy live capture:** kept only as a debugging tool; app uses the cookie upload path.
- **Device registration / OAuth from scratch: replaced by browser-cookie auth** — **Aufgehoben am
  2026-10-15** (User-Entscheidung „so gut es geht wie das Original"): genau dieser Weg ist jetzt
  das **Ziel**. Verworfen bleibt daran nur der **CDP-Browser-Login** (externer Browser) und die
  **Paste-Variante** als Hauptweg.
- **Standalone paste-only login:** superseded by CDP browser login (paste stays as fallback).
- **Avatar-Thumbnail per Raten/Probieren (2026-10-14):** `/drive/v1/nodes/{id}/content` (mit und
  ohne `tempLink=true`), `/drive/v1/nodes/{id}/thumbnail` (mit `width`/`height`/`transform`),
  `/drive/v1/nodes/{id}/transforms/IMAGE_THUMBNAIL` (mit und ohne `?tempLink=true`) sowie
  `/drive/v1/nodes/{id}/content?transform=IMAGE_THUMBNAIL&tempLink=true` — **alle HTTP 404**. Der
  Parameter `?transform=IMAGE_THUMBNAIL` wird **ignoriert** (der `tempLink` liefert weiterhin die
  Rohdatei). Das `transforms: ["IMAGE_THUMBNAIL"]` eines Suchtreffers ist eine **Deklaration**, keine
  URL. Stattdessen belegbar über das Assembly (`GetSmartCroppedThumbnail`/`GetThumbnailUrl`).
- **Avatar lokal dekodieren/schneiden (2026-10-14):** der Avatar-Quellknoten kann ein RAW sein
  (bei diesem Konto ein Canon CR3, 13,5 MB) — die `image`-Crate kann kein CR3. Der 60×60-Crop muss
  daher **serverseitig** passieren (wie im Original), nicht bei uns.
- **`deepseek-v4.1-flash` als Umsetzer-/Verifier-Modell (2026-10-14 gesetzt, 2026-10-15
  zurückgenommen):** lief genau einen Tag, ohne dass ein Fehler oder Gate daran schuld war — der
  User hat schlicht wieder zurückgestellt. Ein erneuter Wechsel auf ein Gratis-/Billig-Modell für
  Subagenten braucht eine **neue** User-Entscheidung, nicht den alten Eintrag. Gegenwärtig gilt
  `opencode/mimo-v2.6-flash-free` für beide Rollen.
- **`SaveAsNew` als Application Default** (galt 2026-10-14 bis 2026-10-15). Wer ihn zurückdreht,
  ändert eine getroffene Entscheidung. Der Enum-`#[default]` bleibt trotzdem `SaveAsNew`
  (Datei-Fallback für `folders.toml`-Einträge ohne Schlüssel) — **zwei verschiedene Defaults, nicht
  verschmelzen**. *Aufgelöst 2026-10-16:* die sichere Ersetz-Regel ist entschieden (Bug 8) — der
  Application Default **bleibt `Replace`** (bestätigt, „alles auf replace"), die Sicherheit kommt aus
  der **Bindung** (`node_id` an den Pfad), nicht aus einem anderen Default; der Enum-`#[default]`
  bleibt `SaveAsNew`. **Verworfen:** `Replace` nur wegen Bug 8 zurückdrehen.
- **Den Application Default beim Drag&Drop still zu überschreiben** (statt nur run-weit zu merken).
- **„Nicht noch einmal fragen" pro Datei als neues persistiertes Feld.** Geltende Regel: die Antwort
  wird in das **bestehende** `BackupFolder.conflict_resolution` des Ordners der gefragten Datei
  geschrieben, plus Application Default für neue Ziele und Drag&Drop — **kein** Schemawechsel,
  **kein** Per-Datei-Feld.
- **Die sieben Warnungen pauschal mit `#[allow(...)]` stumm stellen** — das beseitigt das Rauschen,
  versteckt aber genau die toten Funktionen, um die es geht. Ebenfalls verworfen: nur die
  future-incompatible Warnung zu fixen und die Baseline bei sechs zu lassen (der User wollte 0 als
  Vergleichsbasis).
- **Der reine Web-/Cookie-Weg als Ziel** — und mit der Live-Abnahme auch als **Rückfall** (ganz
  entfernt). Der **CDP-Browser-Login** (externer Browser) bleibt ebenso verworfen wie die
  **Paste-Variante** als Hauptweg. Ebenfalls verworfen: die Original-Login-**UI** (zwei Spalten,
  Willkommens-Panel, Fenster 880 × 530) zu übernehmen — unsere UI bleibt.
- **Den Cookie-Rückfall asynchron über `objc2` neu zu bauen** (mehr Aufwand, kein Bedarf).
- **Den Fehlschlag „es bleibt `com.apple.Finder` stehen" als Ursache des `use_default`-Panels**
  (2026-10-15): er trifft nicht zu — der Dialog entsteht **vor** dem Fallback.
- **Die Vermutung, der User habe das LaunchServices-Panel für den Finder gehalten** (2026-10-15):
  ausdrücklich vom User korrigiert — es geht **kein** Finder auf. Keine Klick-Logik daraus ableiten.
