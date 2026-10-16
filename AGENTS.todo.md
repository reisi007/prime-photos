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
2. [ ] **Manueller Test VOR dem DMG** (User-Vorgabe 2026-10-16: „vor DMG neu bauen bitte manuellen
   test einplanen"). Nach Welle 4b/5 die App lokal bauen und nach `/Applications` installieren,
   dann **vom User am echten Fenster** prüfen lassen — headless ist nur die Geometrie belegt, nicht
   die Optik:
   - **About** aus dem **Tray-Menü** *und* dem **macOS-App-Menü** öffnen (der AppKit-Pfad ist
     headless grundsätzlich nicht beweisbar): Version-/Commit-Anzeige, „Schließen", Fenster-✕.
   - **Lizenz-Fenster:** beide Reiter, Scrollen durch Komponentenliste und Lizenztexte, Lesbarkeit,
     **Ruckeln?** (1,2 MB dürfen nur sichtbar gesetzt werden), Fenster vergrößern/verkleinern.
   - **Nav-Icons** (gefüllte Wolke + Zahnrad) bei 18 px — sitzen sie sauber in der Icon-Spalte?
   - **Warteschlange** (Welle 3a): Gruppenköpfe, feste Spaltenköpfe, lange Namen mit Tooltip,
     leerer Zustand, Liste füllt die Höhe, Scrollen bis zur letzten Zeile.
   - **Protokoll-Panel** (Welle 3b): ziehbar, nichts abgeschnitten, **kein** Überlappen des
     Seiteninhalts; Fenster auf das Minimum 760×520 ziehen.
   Rückmeldungen abarbeiten — **erst danach** Schritt 3.
3. [ ] **Release-Build + Install (DMG).** Am 2026-10-16 einmal gelaufen: `cargo build --release` +
   `packaging/make-app.sh` (signiert mit **„Prime Photos Local Signer"**),
   `hdiutil create -volname "Prime Photos" -srcfolder <staging> -ov -format UDZO`, Installation nach
   `/Applications` + `xattr -dr com.apple.quarantine`. **Dieser Stand ist inzwischen veraltet** —
   Welle 3a–3c haben die UI geändert (Warteschlange, Log-Panel, Nav-Icons). **Offen: nach Welle 4/5
   neu bauen**, DMG nach `target/dist/` legen und installieren. **Voraussetzung:** der
   Keychain-Dialog „… möchte den Schlüssel »refresh-token« verwenden" muss einmalig mit **„Immer
   erlauben"** bestätigt sein (stabiles Zertifikat, daher nur einmal).

## Prio 1 — Bugs aus der Live-Nutzung 2026-10-15 (User-Meldungen)

Erledigt und gelöscht (jeweils implementiert, unabhängig verifiziert = Verifier APPROVE, committet):
Bug 1 „Where is `use_default?" (`2899bc5`; Wissen in `docs/DEV_NOTES.md` § 1), Bug 7 „Klick auf den
leeren Bereich = Nichts" (nichts zu bauen — Belege in `docs/DEV_NOTES.md` § 6), Bug 9 „Warnungen als
Fehler" (`c019528`; Regel in `AGENTS.md`), Bug 8 „Sichere Ersetz-Regel" (`2e2f183`; Regel in
`AGENTS.md` → „Sichere Ersetz-Regel", Belege in `docs/SPEC_DUPLICATES.md` § 5 und
`docs/SPEC_CONFLICT_OPTIONS.md`), Bug 3 „Logo neben „Prime Photos" in der Seitenleiste" (`ecb9bb8`;
nur `src/app.rs`, Verifier APPROVE) und Bug 6 „Aufnahmedatum/Online-Sortierung" (`fcbc14d`; Regel in
`AGENTS.md` → „Aufnahmedatum", Live-Belege in `docs/SPEC_CAPTURE_DATE.md` § 12, PUT-Abnahme in
`docs/features/LIVE_ACCEPTANCE.md`), Bug 2 „Dock-Icon fehlt" (`02f5aa3`; iconset im Build →
`iconutil` in `packaging/make-app.sh`, Entscheidung in `AGENTS.md` → „Assets") sowie Bug 4
„Warteschlange und Fortschrittsleiste gehören zusammen" und Bug 5 „Protokoll-Bereich unten
abgeschnitten" (`4c0f337` = Welle 3a, `a55f8e3`/`a22c713` = 3b, `b9141dd` = Panel-Reihenfolge;
Wissen in `AGENTS.md` → „Panel-Reihenfolge", „Virtuelle Liste", „Live-Status der Ordner-Zeile"
und `docs/SPEC_UPLOAD_PROGRESS.md` § 7.8).

**Prio 1 ist damit vollständig abgearbeitet — hier ist nichts offen.**

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
  braucht die Maßwerte/Design-Abmachung, kein Rate-Wert. **Stand 2026-10-16: verschoben in den
  manuellen Test** (Schritt 2 der Auftragsliste) — der Recon liefert für die Nav-Zeile keine
  Icon-Maße, und nach Welle 3c sitzen die Icons in einem festen 18×18-Rect. Ein blindes „Ausrichten"
  würde ändern, was der User noch gar nicht beurteilt hat.
- [ ] **Logout-Residuen — aufgeteilt (Stand 2026-10-16).** Hinfällig: die laufende Login-WebView
  (der Login-Screen *ersetzt* die Haupt-UI, der Abmelden-Knopf ist dort nicht erreichbar) und die
  Annahme „`logout()` löscht nur die 5 Auth-Felder" (es stoppt den Session-Worker und löscht
  Token/Keychain längst). **Offen und beauftragt:** der **laufende Upload** (Abbruch-Flag, User-
  Entscheidung 2026-10-16, siehe `AGENTS.md`) und `config_error` nach fehlgeschlagenem `save()`
  (verständlich formulieren **und** auf dem Login-Screen zeigen).
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

### Welle 4 — Release-Pflicht: About + Third-Party-Lizenzen
Entscheidungen vom 2026-10-16 stehen in `AGENTS.md` → „About + Drittanbieter-Lizenzen"; Recon in
`docs/SPEC_ABOUT.md`. Zwei Pakete:
- **4a** Daten + eigene Lizenz: `LICENSE-MIT`/`LICENSE-APACHE`, `license = "MIT OR Apache-2.0"` in
  `Cargo.toml`, Generator `packaging/gen-third-party-licenses.sh` → `assets/third-party-licenses.txt`
  (alle Abhängigkeiten aus `Cargo.lock`, vollständige Lizenztexte, deterministisch, offline).
- **4b** UI + Menü: About-Fenster (Logo, Version, Kurztext, Copyright „© <Jahr> Prime Photos"),
  eigenes scrollbares Lizenz-Fenster (generierte Liste **und** `docs/THIRD_PARTY.md`), Eintrag
  „Über Prime Photos" im Tray- **und** App-Menü (AppKit-Eintrag über ein ObjC-Zielobjekt).

### Welle 5 — Robustheit & Politur (klein, unabhängig)
- **erledigt und verifiziert** (`7cc3699`): **5.4** — Selbstheilung statt `conn: None`, siehe
  `AGENTS.md` → „Abmelden bei laufendem Upload" (dort steht auch die Entscheidung).
- **hinfällig: 5.3** — der Account-Block zeigt seit `0d66e05` „Marktplatz / amazon.<domain>", die
  Zeile „Amazon Photos" existiert dort nicht mehr. Der Login-Untertitel „Fotos & Videos zu Amazon
  Photos hochladen" **bleibt** (faktische Nennung des Ziels, nach der Branding-Regel zulässig, wie
  „Bei Amazon anmelden").
- **5.1** `rfd::FileDialog::pick_folder()` auf `AsyncFileDialog` (siehe „Offene Kandidaten" oben).
- **5.2** Rest: **Abmelden bei laufendem Upload** (Abbruch-Flag, User-Entscheidung 2026-10-16) +
  `config_error` verständlich und auch auf dem Login-Screen. **Hinfällig** ist die dritte alte
  Residue (laufende Login-WebView: der Login-Screen *ersetzt* die Haupt-UI, `src/app.rs:950`).
- **5.2c** Nav-Accessibility: `widget_info`/`Sense` für die selbstgezeichneten `nav_item`-Zeilen.
- **5.5 (neu, aus dem 4b-Verify)** den überzeichnenden Kommentar in `src/about.rs` korrigieren (die
  „757 px / ~34 unerreichbare Zeilen"-Angabe reproduziert sich nicht; real sind es 777 px und ein
  <1-px-Beschnitt) und dem Lizenz-Fenster eine **Mindestbreite** geben (unter ~707 px würden
  100-Zeichen-Zeilen geklippt).
- **Verschoben in den manuellen Test:** die Icon-Ausrichtung (keine belegten Maßwerte im Recon).

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
