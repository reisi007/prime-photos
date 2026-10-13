# TODO / decisions ledger

## Implemented

- [x] Rust app boots natively on macOS ARM (egui GUI).
- [x] Theme modeled on Amazon Photos (white content, grey sidebar, teal accent, dark primary button).
- [x] Sidebar navigation: Datensicherung / Einstellungen.
- [x] Persistent backup folders (`folders.toml`), survive restarts.
- [x] Backup list with per-folder progress, status, ⋯ menu (jetzt scannen, Im Finder zeigen, Wiederholen, stoppen).
- [x] "Alle Sicherungsvorgänge anhalten/fortsetzen" (global pause; seit 2026-10-13 stoppt sie auch
  laufende Übertragungen sofort — `watch`-Kanal, Commit `4a47795`).
- [x] Concurrent uploads (Workers + JoinSet), per-file rate limit (Bandbreite tab).
- [x] Login: embedded WKWebView **in the main window** (wry child view of the egui window); cookies captured via an async main-queue probe; manual cookie entry removed. Window title "Login – Prime Photos", slim branded header above the page (documented deviation from the original).
- [x] Country / marketplace selector before login (default from system locale on first run).
- [x] `.xmp` sidecars rejected; RAW formats supported (single source in `folders.rs`).
- [x] OpenAPI spec (`docs/openapi.yaml`) + protocol notes (`docs/PROTOCOL.md`).
- [x] `ilspycmd` installed for .NET decompilation; Screen-Recording grant works (self-screenshots).

## MVP-Scope (User-Entscheidung 2026-10-13)

Im MVP: **Login**, **Upload-Ziel erstellen** (Backup-Konfiguration: Ziel, Inhaltsfilter,
Intervall, Dedup, Dateinamenkonflikte), **Upload-Vorgang** (inkl. Pause/Dedup/Retry),
**Einstellungsdialog**. Aktuelle Abdeckung: Login ✅ (eingebettet, siehe oben),
Upload-Vorgang 🔧 (P1-Items unten), Upload-Ziel/Einstellungen 🔧 (P2-Items unten).
Nicht im MVP, aber relevant: **Download-Vorgang** (Herunterladen-Tab des Originals) → Backlog.

## Spec-Backfill (läuft, 2026-10-13)

Clean-room-Specs nachholen für bereits Implementiertes (Umsetzer-Modell seit 2026-10-13
`opencode-go/deepseek-v4.1-flash#high` — ersetzt `opencode/cat-jam-free`, das „rate limit exceeded"
lieferte; Mapping auf git-HEAD, nicht Working-Tree):
- [x] `docs/SPEC_LOGIN.md` (ap/signin-URL + assoc-Handles je Marketplace, Cookie-Satz, async Probe, Länderwahl). Verifiziert (Verifier: PASS mit Minors, angewendet).
- [x] `docs/SPEC_UPLOAD.md` (Web-Upload-Endpoint, Parameter/Header, EXIF-Datums-Kette, 409-Verhalten, Album). Verifiziert (Verifier: PASS mit Minors, angewendet).
- [x] `docs/SPEC_DEDUP.md` (Duplikat-Identität, ContentFilter-Extension-Taxonomie, Conflict-Pfade). Verifiziert (Verifier-Lauf läuft/gelaufen).
- [x] `docs/SPEC_FOLDERS.md` (Scan/Filter-Semantik, xmp-Ausschluss, folders.toml, Fortschritts-States; Extension-Taxonomie → SPEC_DEDUP). Verifiziert (Verifier: PASS nach 10 Korrekturen).

## Gemeldete Bugs & Wünsche (priorisiert, 2026-10-13)

Oberziel des Users: **nur neue Dateien hochladen, keine Bandbreite verschwenden.**

- [x] **P1 – Pause hält den Upload nicht auf.** Behoben (Commit `4a47795`): `paused` ist jetzt ein
  `tokio::sync::watch`-Kanal; `upload_worker` racet jede Datei-Übertragung (`biased select!`) und den
  Rate-Limit-Sleep gegen `pause_rx.changed()`, bricht die laufende Future ab und legt das Element per
  `push_front` genau einmal zurück. Verifiziert (Verifier: APPROVE, nur `src/main.rs`).
  **Residuen (nicht blockierend, offen):**
  - [ ] **F-2:** Wenn Pause während des Pacing-Sleeps landet, kann der Throttle über die Pause/Resume-Grenze
    übersprungen werden (effektives Rate-Limit kurzzeitig überschritten).
  - [ ] **F-3:** Abbruch verwirft bereits gesendete Bytes (Datei startet bei 0) UND `conflictResolution=RENAME`
    erzeugt bei Abbruch nach Server-Anlage ein Duplikat statt 409-Dedupe — Widerspruch zum Ziel „keine
    Bandbreite verschwenden". Entscheidung nötig: „aktuelles Element fertig, dann ruhen" vs. sofortiger Abbruch; `RENAME` überdenken.

- [ ] **P1 – Nur neue Dateien hochladen (Dedup).** Vor dem Upload prüfen, ob dieselbe Datei (md5)
  bereits im Konto bzw. im lokalen Index liegt → überspringen. Persistenter md5-Index, damit
  Neustarts nichts erneut laden. Original-Strings: „Duplikate vermeiden: Keine Fotos und Videos
  sichern, die sich bereits in meinem Konto befinden" (`…_scheduleconfiguration_dedupeoption`),
  „Dateien, die sich bereits in meinem Konto befinden, nicht sichern"
  (`…_upload_settings_skip_duplicates_option_subtitle`).
  Spec liegt vor: `docs/SPEC_DEDUP.md` (verifiziert). **Entscheidungen (User, 2026-10-13):**
  (a) **persistenter MD5-Index in SQLite** (wie Original; überlebt Neustart; gleicher Pfad + gleiche
  MD5 ⇒ skip, gleicher Pfad + andere MD5 ⇒ neu hochladen, neue Datei ⇒ hochladen);
  (b) **alle 4 Konfliktoptionen**, einstellbar im **vollen Konfigfenster** (siehe P2-Item; geteilte
  Formularlogik mit dem Erstellen-Flow), gemappt auf `conflictResolution`
  (`NONE`/`RENAME`/`OVERWRITE`); 409 bleibt nur Sicherheitsnetz (wird wie im Original als
  „Ignored/Zuvor hochgeladen" behandelt, kein Fehler).
  Original-Mechanik: client-seitige `PreUploadCheck` gegen MD5-Index, Gating über
  Dedup-Toggle; Namenskonflikt separat über `ConflictResolution`. Siehe `docs/SPEC_DEDUP.md`.
  **Umsetzung in zwei Paketen:** (A) Datenmodell + SQLite-Dedup-Index + Upload-Verdrahtung;
  (B) Konfigfenster (Create+Edit geteilt) auf das Modell.

- [ ] **P1 – Upload-Fehler `error sending request for url (…)`** (Beispiel
  `…/27_ÖFB-Reisinger_09.jpg`). `upload_picture` gibt `Err(e.to_string())` zurück → die
  reqwest-Ursache (DNS / Connection-Reset / Body-Stream / Timeout) geht verloren. Soll: komplette
  Fehlerkette loggen (`{e:?}` / `source()`), **Retry mit Backoff** für transiente Fehler
  (Netzwerk, 5xx, 429, 408). Außerdem `conflictResolution=RENAME` hinterfragen — RENAME erzeugt
  bei vorhandener Datei eine Kopie; das Original nutzt `NONE` (Duplikate vermeiden).

- [ ] **.xmp-Befund klären (User-Meldung 2026-10-13).** Gemeldet: „.xmp wird als Bild erkannt — falsch".
  Code-Befund (nur gelesen): `is_supported_image` lehnt `xmp` explizit ab (`src/folders.rs:22`), der
  einzige Sammelpfad `scan_images` filtert damit (`src/folders.rs:40`, Aufruf aus `start_upload` in
  `src/main.rs`), der Dialog wählt nur Ordner (`FileDialog::pick_folder`). Offen: Upload-Log auf
  `.xmp`-Einträge prüfen (Log lebt nur in-memory im Panel) — bestätigt → Eintrittspfad suchen;
  nicht bestätigt → schließen.

- [ ] **P2 – Login: Einfügen per ⌘V geht nicht.** Nacktes Binary ohne Hauptmenü → AppKit legt kein
  „Bearbeiten"-Menü an, `paste:` wird nicht geroutet (Passwort-Manager/Passwort einfügen unmöglich).
  Fix: `NSMenu` (App- + Bearbeiten-Menü: Ausschneiden/Kopieren/Einfügen/Alles auswählen) beim Start
  setzen, WebView als first responder. `objc2-app-kit 0.3.2` / `objc2-foundation 0.3.2` sind bereits
  im Abhängigkeitsbaum.

- [ ] **P2 – Ordner-Backup nicht konfigurierbar wie im Original.** `BackupFolder` kennt nur
  path/name/enabled/…; es fehlt das Fenster „Datensicherungseinstellungen"
  (`…_backupconfiguration_windowtitle`, `docs/UI_FLOWS.md` §4.5): Ziel, „Was soll gesichert werden"
  (Alles / Fotos und Videos / Fotos), „Änderungen hochladen" (Intervall inkl. „Sofort"),
  Dedup-Option, „Umgang mit Dateinamenkonflikten", Speichern/Abbrechen. Plus Zeilen-Aktionen
  „Jetzt scannen und hochladen", „Anhalten/Fortsetzen", „Fehler erneut versuchen", „Bearbeiten",
  „Datensicherung stoppen" (§4.6) und Auto-Upload-Toggle „Neue Dateien automatisch hochladen" (§4.7).

- [ ] **P2 – Icons nicht sauber ausgerichtet.** Nav-/Sidebar-Icons an die Doku-Werte ausrichten
  (Header 64 pt bzw. 104 pt mit Banner, 4 pt Akzentleiste, Nav-Spalte — `docs/UI_FLOWS.md` §3.1).

- [~] **P2 – An-/Abmelden-Bereich.** Working Tree (uncommitted) liefert Sidebar-Account-Block mit
  „Angemeldet als:", „Abmelden" (`…_preferences_account_title` / `…_preferences_signout`, `docs/UI_FLOWS.md`
  §3.1 / §7) und grün/roter Status-Farbe (Text + Punkt). **Offen:** Avatar-Button im Header,
  echte Farb-/Layout-Parität zum Original (siehe Profilbild-Item).

- [ ] **P2 – Logout-Residuen.** `logout()` löscht die 5 Auth-Felder, stoppt aber weder eine laufende
  Login-WebView noch einen laufenden Upload (Worker hält eigenen `cfg`-Klon und arbeitet mit dem alten
  Cookie weiter); `config_error` nach fehlgeschlagenem `save()` wird auf dem Login-Screen nicht angezeigt.
  Soll: Login-/Upload-Status zurücksetzen, Fehler sichtbar machen.

- [ ] **P2 – `app_menu` Härtung.** Kein Re-Entrancy-Guard (idempotent, aber defensiv `static Once` sinnvoll);
  ersetzt winit-Default-Menü (Window/Hide/About/Services fehlen). Beim `.app`-Bundle mitdenken (siehe Backlog).

- [ ] **P2 – Nav-Accessibility.** Selbstgezeichnete `nav_item`-Zeilen (`allocate_exact_size`) haben keine
  Keyboard-Fokus-/Accessibility-Semantik; optional `Sense`-Widget/`widget_info` ergänzen.

- [ ] **P2 – Profilbild fehlt.** Wenn **angemeldet**: das **echte Amazon-Profilbild** im Account-Block
  der Sidebar anzeigen. Wenn **nicht angemeldet**: ein Icon (Personen-Symbol) stattdessen. Die
  **farbliche Kodierung beibehalten** (grün = angemeldet, rot = nicht angemeldet) — User möchte sie
  ausdrücklich behalten. Woher das echte Profilbild kommt (Amazon-Account-/Photos-API), ist noch
  offen; bis dahin Avatar-Platzhalter. Original zeigt den Avatar (`avatar-placeholder34.png`) im
  Header und im Status-Popup (`docs/UI_FLOWS.md` §3.1 / §9.1).

## Offen (Backlog)

- [ ] „Details anzeigen"-Panel (verschachtelt/flach wie das Original, `docs/UI_FLOWS.md` §4.7).
- [ ] Menüleisten-Status-Item (Status-Popup: Status, Benachrichtigungen, Speicher, §9).
- [ ] Videos hochladen (bisher nur Fotos — `.mp4`/`.mov`/… werden wie alle Nicht-Bilder ignoriert).
  Original-Befund (`docs/`, nur gelesen): gleicher Upload-Endpoint (`kind=FILE`, nur `FILE` beobachtet),
  Modus nur über `content_filter`-Bitmaske (`None=0, Photos=2, Videos=4, Other=8, PhotosAndVideo=6,
  Everything=14`; dazu `DeduplicateOptions`, `docs/UI_STATES.md`-Tabellen). Unknown: welche
  Video-Extensions das Original akzeptiert; ob sich Upload-Parameter für Video unterscheiden;
  Quota-Frage (Prime: Fotos unbegrenzt, Videos begrenzt? — `…_notification_overquota_unlimitedphotos_*`,
  Videospeicher-Upsell-String).
- [ ] Auto-Refresh der Cookies bei 401.
- [ ] „Startseite"/Übersichtsseite? (Strings vorhanden, im Build ungenutzt.)
- [ ] Als `.app`-Bundle paketieren (Info.plist, Icon) — saubere Menüleiste/Doppelklick (.app und Menü hängen zusammen, siehe ⌘V).
- [ ] Rest aus `Amazon Photos.dll`: Quota-Form, exakter Extension-Filter, Watcher-Intervall, Concurrency-Defaults, Konfliktbehandlung, Video-Support.
- [ ] Echte „Speichernutzung"-Anzeige (nur Prime; bis dahin bewusst nichts anzeigen).
- [ ] Download-Vorgang (Herunterladen-Tab des Originals) — relevant, aber explizit **nicht** Teil des MVP (siehe MVP-Scope oben).

## Verworfen

- Separates Login-Fenster (`ppu-login` Helper-Prozess mit eigener `tao`-Eventloop): wirkte ungewohnt und passte nicht zum Original, das im Hauptfenster anmeldet. Ersetzt durch die eingebettete Child-WebView im Hauptfenster.
- Synchrone Cookie-Abfrage in `update` (wry `WebView::cookies`): Panic durch winit-Reentrancy (dreht die NSRunLoop). Ersetzt durch `dispatch_async` auf der Main-Queue.
- mitmproxy live capture: kept only as a debugging tool; app uses the cookie upload path.
- Device registration / OAuth from scratch: replaced by browser-cookie auth.
- Standalone paste-only login: superseded by CDP browser login (paste stays as fallback).
