# TODO / decisions ledger

**Nur offene Arbeit** — kurze Checkliste, **keine Doku**. Regel: `AGENTS.md` → „Arbeitsablauf" →
„Ledger-Hygiene". Doku/Recon → `docs/`; Dauer-Entscheidungen → `AGENTS.md`. „Verworfen" wird **nie**
gelöscht.

## Aktueller Auftrag (2026-10-15, Restbestand)

1. [ ] **`docs/`-Gerüst:** bestehende Spec-Dateien in Unterverzeichnisse sortieren
   (`docs/original/`, `docs/protocol/`, `docs/dev/` …), ein `docs/README.md` als Index — **alle**
   Referenzen in `AGENTS.md`, `AGENTS.todo.md`, `docs/` und Code mitziehen. Anlass: User-Wunsch
   „gerne einen docs Ordner mit einer größeren Struktur". **Bekannter Rest:** `docs/features/FEATURES.md:8`
   nennt `docs/original/` und `docs/protocol/`, die es **noch nicht gibt** — beim Umzug mitziehen.
2. [ ] **Restplan nach dem manuellen Test (User-Entscheidung 2026-10-16: „E vor dem DMG").**
   Reihenfolge: **D → E → DMG** — C, D und E sind **fertig und verifiziert** (`c6003ff`,
   `69b30c5`, `615d009`).
   - **Offen vor dem DMG:** der App-gesteuerte **Ende-zu-Ende-Live-Test** (Runde D) — blockiert,
     solange der Mac gesperrt ist (Keychain-Dialog unbedienbar). Nach dem Entsperren: Testdatei in
     einen überwachten Ordner legen, **signiertes** Bundle starten, Upload im Log/Konto beobachten,
     Knoten danach löschen. Live-Beleg der Korrektur bisher: der Host lehnt `chunked` mit **400**
     ab und verlangt `Content-Length` (⇒ 201); lokaler Listener-Test + Negativprobe.
   - **DMG** (Release-Build + Install): `cargo build --release` + `packaging/make-app.sh` (signiert
     mit **„Prime Photos Local Signer"**), `hdiutil create -volname "Prime Photos" -srcfolder
     <staging> -ov -format UDZO`, Installation nach `/Applications` + `xattr -dr
     com.apple.quarantine`. **Voraussetzung:** der Keychain-Dialog „… möchte den Schlüssel
     »refresh-token« verwenden" muss einmalig mit **„Immer erlauben"** bestätigen (stabiles
     Zertifikat, daher nur einmal).

## Manueller Test 2026-10-16 — abgeschlossen, offene Reste

App aus `02e0fca` installiert und gestartet; Start sauber (**keine** Warnung/kein Fehler im Log,
Default-Filter `warn`, `src/logging.rs:74`), Dedup-Index unverworfen mit **591 Zeilen** — Echtbeleg
für die Selbstheilung aus 5.4. Der Test ist **geschlossen**: die Rückmeldungen 1–3, 5, 6, 8–14 sind
umgesetzt, unabhängig verifiziert und gelöscht; die dauerhaften Entscheidungen stehen in `AGENTS.md`
→ „UI-Politur nach dem manuellen Test 2026-10-16". **Offen ist nur noch:**

1. [ ] **Upload-Ziel-Spiegelung wie das Original (offen nach der Spec).** Das Zielfeld ist aus dem
   Dialog **entfernt** (Welle 5.7); die App wählt das Ziel heute automatisch (`Pictures/Web`,
   `src/prime_photos.rs:820-843`). Offen ist der **eigene Auftrag** nach
   `docs/SPEC_UPLOAD_DESTINATION.md`: die Original-Spiegelung `Backup/<Macname>/<Quellpfad>`
   (Segmente bei Bedarf anlegen). **Offene Frage:** kann unser Cookie-Host Ordner anlegen
   (`POST /drive/v1/nodes`, `kind: "FOLDER"`)? Bestehende Uploads in `Pictures/Web` bleiben.

## Offene Punkte aus der Konflikt-/Duplikat-Kette (Welle 2)

- [ ] **Live-Abnahme der Rückfall-Regel (2026-10-16).** Ebenfalls nur durch Tests gepinnt, nicht
  durch die Leitung: ein `Replace`-Ziel, dessen Pfad der Index **nicht** an die vom 409 benannte
  Node bindet, setzt den Lauf als **`RENAME`** fort (lokaler Zweig `resolution = SaveAsNew;
  continue;` in `src/upload.rs`). Belegbar an einer **fremden** Datei gleichen Namens — sie muss
  danach **unverändert** im Konto stehen (kein Datenverlust). Sinnvoll in **einem** Lauf mit der
  PUT-Abnahme oben (derselbe Namenskonflikt, einmal **mit** Bindung ⇒ PUT, einmal **ohne** ⇒ RENAME).
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

## Offene Punkte (Bugs & Wünsche aus 2026-10-13)

Oberziel des Users: **nur neue Dateien hochladen, keine Bandbreite verschwenden.**

- [ ] **Icons nicht sauber ausgerichtet.** Nav-/Sidebar-Icons an die Doku-Werte ausrichten
  (Header 64 pt bzw. 104 pt mit Banner, 4 pt Akzentleiste, Nav-Spalte — `docs/UI_FLOWS.md` §3.1);
  braucht die Maßwerte/Design-Abmachung, kein Rate-Wert. **Stand 2026-10-16: verschoben in den
  manuellen Test** (Schritt 2 der Auftragsliste) — der Recon liefert für die Nav-Zeile keine
  Icon-Maße, und nach Welle 3c sitzen die Icons in einem festen 18×18-Rect. Ein blindes „Ausrichten"
  würde ändern, was der User noch gar nicht beurteilt hat.

## Weitere offene Punkte (dokumentiert, nicht eingeplant)

Ausdrücklich **ohne** Starttermin:
- **Kleiner Rest aus Welle 5.2 (nicht blockierend, Verifier-Befund 2026-10-16):** die
  `cancelled`-Zuweisung im Ablauf-Zweig (`src/login.rs:491`) liegt **innerhalb** des
  `session`-Guards — der Guard im `if`-Kopf wird über den ganzen Block gehalten, Ordnung also
  `session → upload`. Der Verifier hat das als **deadlock-frei** belegt (es gibt keine
  Gegenrichtung `upload → session`); beim nächsten Anfassen von `poll_session` nach außen ziehen,
  damit die Reihenfolge nicht von einer Abwesenheit abhängt.
- Queue-Paginierung (heute bricht das Panel bei 300 Zeilen ab, Original lädt seitenweise 1000).
- Fehlerfenster mit `readdtoqueue`/`retryall`; Semantik laut `SPEC_STATUS_DETAIL.md` §3.
- Pause **pro Ordner** (Zeilen-Symbol) — im Original vorhanden, bei uns Post-MVP.
- Sortierung/Filter der Tabelle; Spalte „Größe" + relative Zeit (`datetimeago_*`).
- Tray-Status-Popup mit Statistiken (`statuspopup_stats_*`).
- Kaltstart-Verhalten nach Login; Pro-Ziel-Limit; netzwerkgesteuerte Drosselung; Logout während
  laufendem Upload; Nav-Accessibility.
- **Nicht** eingeplant (bewusst Out of Scope): die vollständige In/Out-Liste mit Beleg-Keys steht in
  `docs/SPEC_SCOPE_INVENTORY.md`; Sync, Downloads/Restore, Datei-Management, Apple-Photos-Import,
  Videos, Autoplay, Upsell-/Quota-Banner, Auto-Update, Proxy, ausgeblendete Dateien,
  iOS-Dedupe-Prompt, Crash-Relaunch bleiben draußen.

## Geplante Wellen — **NUR PLAN**, keine Welle startet ohne ausdrücklichen Auftrag

Geschnitten nach **gemeinsamem Umfeld/Dateien**. **Wellen 1–5 sind abgeschlossen** (Historie in den
Commits, Entscheidungen in `AGENTS.md`); offen ist nur **Welle 6** — plus die Reste in den Listen
weiter oben.

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
4. **Farbe/Sprache des Multi-Fenster-Verhaltens** („Einmalige Sicherung"-Zeilendarstellung).

## Offen (Backlog)

- [ ] „Details anzeigen"-Panel (verschachtelt/flach wie das Original, `docs/UI_FLOWS.md` §4.7).
- [ ] Menüleisten-Status-Item (Status-Popup: Status, Benachrichtigungen, Speicher, §9).
- [ ] Videos hochladen (bisher nur Fotos). Offene Fragen dazu: `docs/SPEC_SCOPE_INVENTORY.md`.
- [ ] „Startseite"/Übersichtsseite? (Strings vorhanden, im Build ungenutzt.)
- [ ] `.app`-Icon für das Bundle — gehört zu `packaging/make-app.sh`.
- [ ] Rest aus `Amazon Photos.dll`: Quota-Form, exakter Extension-Filter, Watcher-Intervall,
  Konfliktbehandlung, Video-Support.
- [ ] Echte „Speichernutzung"-Anzeige (nur Prime; bis dahin bewusst nichts anzeigen).
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
  `opencode-go/deepseek-v4.1-flash` für beide Rollen (siehe `AGENTS.md` → „Umsetzer").
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
