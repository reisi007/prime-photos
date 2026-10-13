# TODO / decisions ledger

## Implemented

- [x] Rust app boots natively on macOS ARM (egui GUI).
- [x] Theme modeled on Amazon Photos (white content, grey sidebar, teal accent, dark primary button).
- [x] Sidebar navigation: Datensicherung / Einstellungen.
- [x] Persistent backup folders (`folders.toml`), survive restarts.
- [x] Backup list with per-folder progress, status, ⋯ menu (jetzt scannen, Im Finder zeigen, Wiederholen, stoppen).
- [x] "Alle Sicherungsvorgänge anhalten/fortsetzen" (global pause).
- [x] Concurrent uploads (Workers + JoinSet), per-file rate limit (Bandbreite tab).
- [x] Login: embedded WKWebView **in the main window** (wry child view of the egui window); cookies captured via an async main-queue probe; manual cookie entry removed. Window title "Login – Prime Photo Uploader", slim branded header above the page (documented deviation from the original).
- [x] Country / marketplace selector before login (default from system locale on first run).
- [x] `.xmp` sidecars rejected; RAW formats supported (single source in `folders.rs`).
- [x] OpenAPI spec (`docs/openapi.yaml`) + protocol notes (`docs/PROTOCOL.md`).
- [x] `ilspycmd` installed for .NET decompilation; Screen-Recording grant works (self-screenshots).

## Gemeldete Bugs & Wünsche (priorisiert, 2026-10-13)

Oberziel des Users: **nur neue Dateien hochladen, keine Bandbreite verschwenden.**

- [ ] **P1 – Pause hält den Upload nicht auf.** Trotz „Alle Sicherungsvorgänge anhalten" wurden
  Fotos weiter hochgeladen. Ursache in `src/main.rs`: `upload_worker` prüft `paused` nur *vor* dem
  nächsten Queue-Element; die bis zu `concurrency` bereits laufenden Requests und der
  Rate-Limit-Sleep laufen zu Ende. Soll: Pause stoppt sofort (laufende Übertragungen abbrechen),
  Queue ruht, Resume setzt fort. (Original: `…_backupscreen_pauseallbackups`,
  `…_scheduletab_pauseschedule` / `…_resumeschedule`.)

- [ ] **P1 – Nur neue Dateien hochladen (Dedup).** Vor dem Upload prüfen, ob dieselbe Datei (md5)
  bereits im Konto bzw. im lokalen Index liegt → überspringen. Persistenter md5-Index, damit
  Neustarts nichts erneut laden. Original-Strings: „Duplikate vermeiden: Keine Fotos und Videos
  sichern, die sich bereits in meinem Konto befinden" (`…_scheduleconfiguration_dedupeoption`),
  „Dateien, die sich bereits in meinem Konto befinden, nicht sichern"
  (`…_upload_settings_skip_duplicates_option_subtitle`).

- [ ] **P1 – Upload-Fehler `error sending request for url (…)`** (Beispiel
  `…/27_ÖFB-Reisinger_09.jpg`). `upload_picture` gibt `Err(e.to_string())` zurück → die
  reqwest-Ursache (DNS / Connection-Reset / Body-Stream / Timeout) geht verloren. Soll: komplette
  Fehlerkette loggen (`{e:?}` / `source()`), **Retry mit Backoff** für transiente Fehler
  (Netzwerk, 5xx, 429, 408). Außerdem `conflictResolution=RENAME` hinterfragen — RENAME erzeugt
  bei vorhandener Datei eine Kopie; das Original nutzt `NONE` (Duplikate vermeiden).

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

- [ ] **P2 – An-/Abmelden-Icon kaputt.** Konto-Bereich wie Original: Avatar-Button im Header,
  „Angemeldet als:", „Abmelden" (`…_preferences_account_title` / `…_preferences_signout`,
  `docs/UI_FLOWS.md` §3.1 / §7).

- [ ] **P2 – Profilbild fehlt.** Wenn **angemeldet**: das **echte Amazon-Profilbild** im Account-Block
  der Sidebar anzeigen. Wenn **nicht angemeldet**: ein Icon (Personen-Symbol) stattdessen. Die
  **farbliche Kodierung beibehalten** (grün = angemeldet, rot = nicht angemeldet) — User möchte sie
  ausdrücklich behalten. Woher das echte Profilbild kommt (Amazon-Account-/Photos-API), ist noch
  offen; bis dahin Avatar-Platzhalter. Original zeigt den Avatar (`avatar-placeholder34.png`) im
  Header und im Status-Popup (`docs/UI_FLOWS.md` §3.1 / §9.1).

## Offen (Backlog)

- [ ] „Details anzeigen"-Panel (verschachtelt/flach wie das Original, `docs/UI_FLOWS.md` §4.7).
- [ ] Menüleisten-Status-Item (Status-Popup: Status, Benachrichtigungen, Speicher, §9).
- [ ] Videos hochladen (bisher nur Fotos).
- [ ] Auto-Refresh der Cookies bei 401.
- [ ] „Startseite"/Übersichtsseite? (Strings vorhanden, im Build ungenutzt.)
- [ ] Als `.app`-Bundle paketieren (Info.plist, Icon) — saubere Menüleiste/Doppelklick (.app und Menü hängen zusammen, siehe ⌘V).
- [ ] Rest aus `Amazon Photos.dll`: Quota-Form, exakter Extension-Filter, Watcher-Intervall, Concurrency-Defaults, Konfliktbehandlung, Video-Support.
- [ ] Echte „Speichernutzung"-Anzeige (nur Prime; bis dahin bewusst nichts anzeigen).

## Verworfen

- Separates Login-Fenster (`ppu-login` Helper-Prozess mit eigener `tao`-Eventloop): wirkte ungewohnt und passte nicht zum Original, das im Hauptfenster anmeldet. Ersetzt durch die eingebettete Child-WebView im Hauptfenster.
- Synchrone Cookie-Abfrage in `update` (wry `WebView::cookies`): Panic durch winit-Reentrancy (dreht die NSRunLoop). Ersetzt durch `dispatch_async` auf der Main-Queue.
- mitmproxy live capture: kept only as a debugging tool; app uses the cookie upload path.
- Device registration / OAuth from scratch: replaced by browser-cookie auth.
- Standalone paste-only login: superseded by CDP browser login (paste stays as fallback).
