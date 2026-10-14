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
- [x] Identität/Name auf **„Prime Photos"** umgestellt (2026-10-13): Paket/Binary `prime-photos`,
  Modul `src/prime_photos.rs` (`PrimePhotos`), Config-Ordner `prime_photos` (alter Ordner einmalig
  von Hand migriert, kein Migrationscode), Fenster-/UI-Titel „Prime Photos", Verzeichnis `prime-photos`.
  Funktional notwendige Amazon-Referenzen (Hosts/Header/Cookies/Login-Strings) bleiben bewusst.
  Verifiziert (Verifier: PASS). Original-App-Recon-Docs unangetastet.

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
  - [x] **F-2:** Pacing-Sleep über Pause/Resume behoben (Commit `029ff01`): der Throttle wird in
    ≤200-ms-Slices geschlafen, pausiert wird geparkt und die Restzeit bleibt über die Pause erhalten
    (Resume setzt den Throttle fort statt ihn zu überspringen). Verifiziert (Verifier: PASS).
  - [ ] **F-3:** Abbruch verwirft bereits gesendete Bytes (Datei startet bei 0) UND `conflictResolution=RENAME`
    erzeugt bei Abbruch nach Server-Anlage ein Duplikat statt 409-Dedupe — Widerspruch zum Ziel „keine
    Bandbreite verschwenden". Entscheidung nötig: „aktuelles Element fertig, dann ruhen" vs. sofortiger Abbruch; `RENAME` überdenken.

- [x] **P1 – Nur neue Dateien hochladen (Dedup).** Erledigt, Commit `b47bf9b` (persistenter
  SQLite-MD5-Index); Server-409 als Sicherheitsnetz) Vor dem Upload prüfen, ob dieselbe Datei (md5)
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
  **Umsetzung in zwei Paketen:** (A) Datenmodell + SQLite-Dedup-Index + Upload-Verdrahtung —
  ✅ erledigt, verifiziert (Verifier: APPROVE), Commit `b47bf9b`;
  (B) Konfigfenster (Create+Edit geteilt) auf das Modell — 🔧 läuft.
  **Paket B (User-Entscheidung 2026-10-13):** volle Original-Anordnung, aber Felder ohne Backend
  (**Ziel**, **Inhaltsfilter**, **Intervall**) ausgegraut mit Hover „Noch nicht verfügbar";
  funktionsfähig: Quelle (bei Bearbeiten read-only), „Duplikate vermeiden", Dateinamenkonflikte,
  Speichern/Abbrechen. Fenster öffnet aus „Datensicherung hinzufügen …" (Create) und dem neuen
  Zeilenmenü-Eintrag „Bearbeiten" (Edit).

- [x] **P1 – Upload-Fehler `error sending request for url (…)`** (Beispiel
  `…/27_ÖFB-Reisinger_09.jpg`). Behoben (Paket C): `upload_picture` liefert jetzt die **volle
  Fehlerkette** (`error_chain` über `source()`), **Retry mit Backoff** nur für transiente Fehler
  (Netzwerk connect/timeout/request/body, 5xx, 429, 408; bounded 4 Versuche, exponentiell + Jitter,
  `Retry-After` beachtet, Cap 8 s), 401/403 und 409 werden **nicht** wiederholt; `panic!()`-Stellen
  im Upload-/Node-Pfad durch `Err` ersetzt (node fetch → `Result`). Verifiziert (Verifier: PASS).
  **Residuum (offen):** weiter fragile Stellen außerhalb des Auftrags — `get_album` →
  `list_albums().await.unwrap()` und `web_dir_node.as_ref().unwrap()` in `prime_photos.rs`
  (Netzwerkfehler im Album-Schritt könnte den Koordinator-Task panicken lassen); als Folge-Ticket.

- [x] **.xmp-Befund geklärt (User-Meldung 2026-10-13).** Belegt in `src/folders.rs`: `.xmp` wird
  explizit nie als Bild behandelt Gemeldet: „.xmp wird als Bild erkannt — falsch".
  Code-Befund (nur gelesen): `is_supported_image` lehnt `xmp` explizit ab (`src/folders.rs:22`), der
  einzige Sammelpfad `scan_images` filtert damit (`src/folders.rs:40`, Aufruf aus `start_upload` in
  `src/main.rs`), der Dialog wählt nur Ordner (`FileDialog::pick_folder`). Offen: Upload-Log auf
  `.xmp`-Einträge prüfen (Log lebt nur in-memory im Panel) — bestätigt → Eintrittspfad suchen;
  nicht bestätigt → schließen.

- [x] **P2 – Login: Einfügen per ⌘V.** Erledigt mit `src/app_menu.rs` (natives AppKit-Hauptmenü,
  ⌘X/⌘C/⌘V/⌘A/Undo/Redo an den Fokusträger; Re-Entrancy-Guard in `16cfbe5`). Nacktes Binary ohne Hauptmenü → AppKit legt kein
  „Bearbeiten"-Menü an, `paste:` wird nicht geroutet (Passwort-Manager/Passwort einfügen unmöglich).
  Fix: `NSMenu` (App- + Bearbeiten-Menü: Ausschneiden/Kopieren/Einfügen/Alles auswählen) beim Start
  setzen, WebView als first responder. `objc2-app-kit 0.3.2` / `objc2-foundation 0.3.2` sind bereits
  im Abhängigkeitsbaum.

- [x] **P2 – Ordner-Backup konfigurierbar.** Erledigt mit `src/backup_config.rs` (Paket B,
  `68b6ad0`): Dialog „Datensicherungseinstellungen“ für Create und Edit; noch fehlende
  Felder (Ziel, Content-Filter, Intervall) sind bewusst ausgegraut. `BackupFolder` kennt nur
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

- [x] **P2 – `app_menu` Härtung.** Erledigt, Commit `16cfbe5`: `static Once` + extrahierte
  `build_main_menu()`. War vorher idempotent, jetzt explizit;
  ersetzt winit-Default-Menü (Window/Hide/About/Services fehlen). Beim `.app`-Bundle mitdenken (siehe Backlog).

- [ ] **P2 – Nav-Accessibility.** Selbstgezeichnete `nav_item`-Zeilen (`allocate_exact_size`) haben keine
  Keyboard-Fokus-/Accessibility-Semantik; optional `Sense`-Widget/`widget_info` ergänzen.

- [ ] **P2 – Profilbild fehlt.** Wenn **angemeldet**: das **echte Amazon-Profilbild** im Account-Block
  der Sidebar anzeigen. Wenn **nicht angemeldet**: ein Icon (Personen-Symbol) stattdessen. Die
  **farbliche Kodierung beibehalten** (grün = angemeldet, rot = nicht angemeldet) — User möchte sie
  ausdrücklich behalten. Woher das echte Profilbild kommt (Amazon-Account-/Photos-API), ist noch
  offen; bis dahin Avatar-Platzhalter. Original zeigt den Avatar (`avatar-placeholder34.png`) im
  Header und im Status-Popup (`docs/UI_FLOWS.md` §3.1 / §9.1).

## Portierung auf Windows/Linux (User-Entscheidung 2026-10-13)

**Ziel:** „jetzt voll cross-platform portieren" — macOS/Windows/Linux gleichwertig. Grundsatz und
Verifikationsgrenze stehen in `AGENTS.md` („Plattform / Portierung"). Neue Features laufen über
cross-platform Crates; macOS-Spezialisierungen (`app_menu`, Main-Queue-Cookie-Probe) bekommen
`#[cfg]`-Guards. Getestet/gebaut wird hier nur der macOS-Pfad; Windows/Linux per Cross-Check.

Pakete (Reihenfolge, jedes mit Build → Verify → Commit):

- [x] **PORT-1 – Plattform-Firewall (Deps + cfg).** Erledigt, Commit `1744e94`: macOS-only Crates
  (`objc2`, `objc2-app-kit`, `objc2-foundation`, `dispatch2`) unter
  `[target.'cfg(target_os = "macos")'.dependencies]`; `login.rs`-Probe: macOS =
  `DispatchQueue::main()` (unverändert), sonst direkt; `app_menu`-No-op bleibt. Verifiziert (PASS).
- [x] **PORT-2 – Cross-platform Benachrichtigungen** (`notify-rust` 4.18), Auslöser **Upload fertig** +
  **Fehler** (eine Notifikation pro Lauf, synchron im Main-Thread), Schalter
  „Desktop-Benachrichtigungen (Upload fertig / Fehler)" in Einstellungen; `config.notifications`
  mit `serde(default = true)` (alte `config.toml` lädt weiter). Commit `0e96798`. Verifiziert (PASS).
- [x] **PORT-3 – Tray/Menüleisten-Icon** (`tray-icon`): Fenster anzeigen, Pause/Fortsetzen, Beenden;
  „Fenster schließen = im Hintergrund weiterlaufen". Erledigt, Commit `b7fd1b2`. Verifiziert (PASS).
  **Residuum (MINOR, Win/Linux):** ohne sichtbares Tray-Icon gäbe es dort keinen In-App-Ausweg
  (macOS ist safe: ⌘Q terminiert). Fix wäre ein In-App-„Beenden" — offen.
- [x] **PORT-4 – Optionaler Autostart** (`auto-launch`). Erledigt, Commit `c8e1448`: Schalter „Nach dem Anmelden starten“ in den Einstellungen, cfg-freier `Autostart`-Wrapper (nicht unterstützte Plattform = dauerhaft aus, keine Panics), Wunsch wird sofort wirksam. **Runtime-only**, nicht in `Config` persistiert — der Login-Item-Zustand des Betriebssystems ist die Quelle der Wahrheit. Verify: Build grün, 7 Warnungen, keine Clippy-Neuzugänge.
- [x] **PORT-5 – Hintergrund-Sync**. Erledigt, Commit `8a461a6`: ein Supervisor-Task (erster Tick nach 15 min, dann alle 90 min — beides bewusst hart-kodiert; der Takt ist „Zurückgestellt“ #3), pro Tick neues Config/Store-Lesen und alle Guards (Schalter an, angemeldet, nicht pausiert, nicht bereits aktiv); ein manueller Upload resettet den Intervall. Gemeinsamer Startpfad für manuell/Hintergrund (`UploadPlan::build` + `publish_state`/`publish_plan`). Schalter „Hintergrund-Synchronisierung“ in den Einstellungen (persistiert via `background_sync`, serde-default true, lädt also eine alte `config.toml`). `was_active` wird jetzt auch für Hintergrund-Läufe gelatcht ⇒ die Abschluss-Benachrichtigung greift dort ebenfalls. Verify: Build grün, 7 Warnungen, `cargo test` 2/2.
- [x] **PORT-6 – Paketierung (macOS-Teil).** Erledigt, Commit `7c0d10c`: `packaging/Info.plist`
  (Bundle-ID `de.reisinger.prime-photos`, Executable `prime-photos`) + `packaging/make-app.sh`
  (Release-Build → `target/dist/Prime Photos.app` → `codesign`, **Default ad-hoc**, optional
  `CODESIGN_IDENTITY`) + README. Echter Lauf: `codesign --verify` = „valid on disk / satisfies its
  Designated Requirement", `EXIT=0`; Ausgabe unter dem ignorierten `target/`. Verifiziert (PASS).
  Erster Lauf fand zwei Bugs (BSD-`chmod` kennt kein `--`; Ausgabe nun `target/dist/`).
  **Offen:** selbstsignierte Identität (wird erst auf Zuruf angelegt — Schlüsselbund!), ob
  Benachrichtigungen im signierten Bundle erscheinen, ob `--options runtime` für ad-hoc nötig ist;
  Windows/Linux nur als Hinweis (keine Installer), `.app`-Backlog-Item geht hierin auf.
- [x] **I18N-1 – Internationalisierung (de+en).** Erledigt, Commit `45b654d`: `src/i18n.rs` (Lang,
  tr/tr1/tr2/tr3/tr_n, de = Quelle), `Config.language` (Option, Auto/Deutsch/English), alle
  UI-Strings der Feature-Module inkl. Fenstertitel, Tray-Labels, Benachrichtigungen, Log/Statuszeilen;
  `docs/I18N.md`. Verifiziert (PASS). Nach REF-2 (siehe `AGENTS.md` „Internationalisierung"):
  `src/i18n.rs` mit `Lang`-Enum + zentraler String-Tabelle (de = Quelle, en daneben), `Config.language`
  (serde-Default aus OS-Locale) + Sprachwahl in den Einstellungen, alle UI-Strings der Feature-Module
  (inkl. Marktplatz-Ländernamen in `countries.rs`, Fenstertitel, Log-/Statuszeilen) darüber führen.
  Kein hartkodierter UI-Text mehr außerhalb `i18n.rs`. Weitere lateinische Sprachen später additiv.
- [x] **PORT-7 – Shell-Aufrufe plattformneutral** (Befund aus PORT-1-Verify, nicht blockierend):
  nach dem Refactor liegen die Stellen in `src/backup.rs:63` („Im Ordner zeigen", `open -R`) und
  `src/login.rs:406` („im Browser öffnen", `open <url>`); auf Windows/Linux still wirkungslos. Soll:
  je OS `open`/`open -R` (macOS), `explorer`/`explorer /select,` (Windows), `xdg-open` (Linux) —
  bevorzugt als kleines Modul `src/platform.rs` ohne neue Abhängigkeit. `defaults read -g AppleLocale`
  (`src/countries.rs:99`) hat bereits einen `LANG`-Fallback — ok.

**Refactor-Vorlauf (User-Entscheidung 2026-10-13: `main.rs` splitten, damit parallel gearbeitet
werden kann — Details/Zielstruktur in `AGENTS.md` „Modul-Struktur"):**

- [x] **REF-1 – Blatt-Module extrahieren.** Erledigt (nicht committet→siehe Commit-Log): `src/countries.rs`
  (`COUNTRIES`, `country_label`, `system_country`, `ParsedCookies`, `parse_cookie_string`),
  `src/upload.rs` (`UploadShared` (+`log`), `upload_worker`, `run_upload`, `status_text`, `eta_secs`,
  `format_eta`, `eta_suffix`, `upload_progress_text`), `src/icons.rs` (`NavIcon`, `nav_icon`,
  `status_dot`, `draw_avatar`, `upload_logo`). Reiner Move, nur `pub(crate)` + 2 Import-Pfade.
  Verifiziert (Verifier: PASS, 497/497 Zeilen byte-identisch). `main.rs` 1780 → ~1260 Zeilen.
- [x] **REF-2 – Feature-Module.** Erledigt, Commit `71de242`: `src/app.rs` (State + `impl eframe::App`
  + Sidebar/Navigation), `login.rs` + Login-Screens, `src/backup.rs` (Liste/`Row`/Aktionen),
  `src/backup_config.rs` (Dialog „Datensicherungseinstellungen"), `src/settings.rs`
  (Einstellungen + Konto). Reiner Move; `update` byte-identisch, nur `pub(crate)`.
  Verifiziert (PASS). `src/main.rs` = 33 Zeilen.

## CI / Release (User-Entscheidung 2026-10-13)

Ziel: öffentliches GitHub-Repo (`AGENTS.md` „CI/Release" — hebt „Kein Push, kein CI" auf).

- [x] **CI-1 – `.github/workflows/ci.yml`.** Erledigt, Commit `badf043`: Trigger `push` (main) +
  `pull_request`: **nur Linux**; `cargo build --locked` + `cargo test --locked` (System-Deps:
  `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `libssl-dev`, `pkg-config`,
  `build-essential`); `concurrency` mit Cancel. Verifiziert (PASS). **Noch nicht gelaufen** (kein
  Remote) — die apt-Paketnamen zeigen sich erst im ersten echten CI-Lauf.
- [x] **CI-2 – `.github/workflows/release.yml` + `docs/RELEASE.md`.** Erledigt, Commit `badf043`:
  Trigger Tag `v*`: CI-Matrix
  **Windows + Linux** (`needs:`-Kette → erst alle grün, dann Release); Windows `.zip`, Linux `.tar.gz`;
  GitHub Release mit den Artefakten. **macOS nur lokal** (`packaging/make-app.sh`, arm64) — bewusst
  nicht im CI (User-Entscheidung 2026-10-13); `docs/RELEASE.md` beschreibt den lokalen macOS-Build,
  den Gatekeeper-Workaround für unsignierte Downloads und — falls später gewünscht — die nötigen
  Secrets/Voraussetzungen für Developer-ID + Notarisierung.

Danach können **PORT-4/5/6/7** (und Folge-Features) auf getrennten Dateien laufen.
Pakete, die `main.rs`/`Cargo.toml` anfassen, bleiben seriell.

Verworfen (siehe `AGENTS.md`): Benachrichtigungen nur über macOS-`UNUserNotificationCenter` +
selbstsigniertes `.app` (war die erste Teilantwort des Users).

## OpenCode-MCP: rea-Server (User-Entscheidung 2026-10-13)

- [x] **rea-MCP dauerhaft gegen Netzwerk-Flackern abgesichert.** Befund: Server-Start via
  `npx -y rea-agents@6.4.0 mcp` braucht bei *jedem* Connect die npm-Registry; die Maschine hatte
  heute 13:00–18:00 DNS-Fehler (`getaddrinfo ENOTFOUND`, 9–18/h — Model-Fetch und Update-Check
  scheiterten ebenfalls). Gegen eine unresolvable Registry retryt npm ~70 s, OpenCode's
  MCP-Connect-Timeout bricht vorher ab ⇒ Status „failed" / „Request timed out" (15:31, 16:48,
  17:36, 18:08). codegraph (lokales Binary, kein Netz) lief derselben Zeit durch — war der
  Auslöser-Eingrenzung dienlich. Vorgeschichte (vor 12:04): alter Server-Prozess ohne `npx` im
  PATH ⇒ `NotFound: ChildProcess.spawn` — durch Service-Neustart behoben, anderes Symptom.
  **Fix (User gewählt: minimal):** `command: ["npx","-y","--prefer-offline","rea-agents@6.4.0","mcp"]`
  (warmer npx-Cache ⇒ ~0,6 s Start, kein Registry-Kontakt). Verifiziert: rea connected, 170 Tools.
- [x] **Stale `HOPPER_LAUNCHER_PATH` entfernt** (User: „remove hopper"). Die Variable zeigte auf
  `/Users/florianreisinger/Applications/Hopper Disassembler.app/Contents/MacOS/hopper` — die App
  liegt seit heute im `~/.Trash`. Testbefund: fehlendes Hopper bricht das MCP-Handshake **nicht**
  (nur native-Analyse-Tools wären betroffen); die Variable war für den Startup-Timeout irrelevant.
  Wiederherstellung (falls native Analyse gebraucht wird): Hopper neu installieren, Variable in
  `~/.config/opencode/opencode.jsonc` ergänzen.
- [ ] **Optional/Reserve (nur bei Wiederauftreten):** `npm install -g rea-agents@6.4.0` und MCP-Command
  auf den absoluten Binary-Pfad stellen (`~/.nvm/versions/node/<v>/bin/rea mcp`) — null Netzwerk im
  Startpfad. Außerdem Maschinen-DNS prüfen (öffentliche Resolver 1.1.1.1/8.8.4.4/8.8.8.8 hinter dem
  10.x-LAN flackerten; ggf. Router-DNS statt Public-Resolvers).

- [x] **Modellwechsel Implementierer/Verifier (User-Entscheidung 2026-10-13):** beide Rollen nutzen jetzt
  `opencode-go/step-5-preview-free` (kein `#variant`). Ersetzt `opencode-go/deepseek-v4.1-flash#high`.
  Der Twin `opencode/step-5-preview-free` bleibt bewusst ungenutzt (Namespace-Regel).

## Upload-Queue (User-Entscheidung 2026-10-13)

- [x] **Slow-Mode (MVP):** Runtime-only `slow_mode` mit Checkbox in den Einstellungen und
  `concurrency=1` im Upload-Lauf; nicht persistiert (Reset beim App-Start). Commit `0be2a81`.
- [ ] **Post-MVP – Pro-Ziel-Limit:** optionales Feld im Backup-Ziel (Wert ≤ globaler Slider),
  das die Anzahl der gleichzeitigen Uploads **für dieses Ziel** begrenzt (globaler Pool bleibt).
- [ ] **Post-MVP – Netzwerkgesteuerte Drosselung:** automatische Reduktion/Pause anhand des
  OS-Netzwerkstatus (costed/metered) bzw. SSID/Name-basierter Regeln; nicht als MVP.
- [x] **Hintergrund-Sync (PORT-5)** umgesetzt (Commit `8a461a6`, siehe Abschnitt
  „Portierung“); `docs/SPEC_BACKGROUND_SYNC.md` war die Vorlage. Offen blieb bewusst:
  `notify`-Watcher + periodischen Timer + Start nach Login.

## Upload-Progress & Drag-and-Drop (Recon 2026-10-13)

- [x] **Upload-Progress, Slice 1+2 (Spec `docs/SPEC_UPLOAD_PROGRESS.md`).** Commits `7572e22` (Live-Warteschlange pro Datei: `FileState`/`QueueEntry`, Zustandsübergänge im Worker, Panel „Warteschlange (n)“) und `9a11e45` (Gruppierung je Backup-Ordner, „N offen“ je Gruppe; das `folder`-Feld wird gelesen, `#[allow(dead_code)]` entfällt). Byte-Level-Fortschritt ist UNKNOWN für uns (der Cookie-Pfad liest die Datei als Ganzes), daher nur grobe Zustände pro Datei. Verify: Build grün, 7 Warnungen. Globale Leiste um "N Artikel ... Noch etwa M Minuten" plus Per-Datei-Status (`Warten`, `Upload-Vorgang läuft (n)`, Warnzustand) und pro-Datei-Name/Zeit/Prozent ergänzen. Derzeit nur globale ETA + Ordnerzellen.
- [x] **Drag & Drop One-Time-Upload (Spec `docs/SPEC_ONE_TIME_UPLOAD.md`).** Commit `51a5acc`: Drop-Overlay, transientes Ziel „Einmalige Sicherung“ (nie persistiert), rekursiver Verzeichnis-Drop, gemeinsamer Upload-Pfad, eigene Zeile in der Liste ohne Menü/Pause. Verify: Build grün, 7 Warnungen, `cargo test` 2/2. Full-Window-Drop-Zone, Scanning-Overlay, Ad-hoc-Queue-Zeile "Einmalige Sicherung", Cloud-Zielpfad (Default /All/{0}) und Upload-Lauf ohne persistiertes Backup-Ziel. Nutzt eframe/egui DroppedFiles + `folders::scan_images`.
- [x] **Per-Datei-Warteschlange** (mit oder ohne byte-progress; je nachdem, ob der Cookie-Upload feingranulare Fortschritts-Events bietet).

## Scope-Inventar: vergessene Features & bewusste In/Out-Grenzen (User-Wunsch 2026-10-13)

Anlass: Beim Reverse-Engineering sind Features aufgetaucht, die in keiner TODO standen. Ziel ist eine
**absichtliche** Scope-Entscheidung — **nicht alles umsetzen**. „Scope" = Vorschlag/Empfehlung;
**nicht umsetzen** heißt: dokumentiert und bewusst draußen. Nichts hiervon ist implementiert.

Belege: `Localizable.strings` (Keys) + `Amazon Photos.dll` (Symbole), `docs/*`.

| # | Feature | Beleg | Status bei uns | Scope (Vorschlag) |
|---|---|---|---|---|
| 1 | **Bidirektionale Synchronisation** (Sync-Ordner) | `enablesync`/`disablesync`, `navigationMenu_sync`, `failedsyncitemswindow_*`, `SyncEngineQueueTimeoutInSeconds` | fehlt komplett | **Out / Post-MVP** — anderer Produktkern (Sync ≠ Backup) |
| 2 | **Interaktiver Dateinamens-Konflikt-Dialog** | `filenameconflict_overlayview_{title,body,ignore/replace/saveasnew}_*`, `_donotaskagain_`, `filenameconflict_toast` | „Nachfragen" existiert nur als Option, **kein** Dialog | **In (MVP-Lücke)** — sonst ist die Option wirkungslos |
| 3 | **Finder-/Kontextmenü-Service** „Bei Amazon Photos hochladen" | `context_menu_upload`, `menu_services`, `ServicesMenu.strings` | fehlt | **Out / Post-MVP** — OS-Integration |
| 4 | **Apple-Photos-/iPhoto-/Aperture-Import** | `mediaimport_*`, `onboard_noautosave_*` | fehlt | **Out** — nicht unser Datenmodell |
| 5 | **Ausgeblendete Dateien** (Setting) | `ExcludeHiddenFilesAndFolders` (Default true), `preferences_(in|ex)cludehiddenfiles` | fehlt | **Out / optional** — kleine, spätere Einstellung |
| 6 | **Proxy-Einstellungen** | `preferences_proxy_*` | fehlt | **Out** |
| 7 | **Speicher-/Quota-Balken** | `youraccount_*` | bewusst nicht (MVP) | **Out (bereits entschieden)** |
| 8 | **Videos hochladen** | ContentFilter-Bits, `no_photos_or_videos_found` | nur Bilder | **Out / Post-MVP (bereits getrackt)** |
| 9 | **Downloads / Restore** | `navigationMenu_download`, `displayitememptymessage_download_*` | fehlt | **Out (bereits entschieden: nicht MVP)** |
| 10 | **Datei-Management** (Albums, Favoriten, Papierkorb, Teilen) | `album`, `favorite`, `trash`, `restore` | fehlt | **Out** — Uploader-Scope |
| 11 | **Autoplay / Media-Player** | `mediaimport_autoplayaction` | fehlt | **Out** |
| 12 | **Upsell-/Trial-/Overquota-Banner** | `notification_{overquota,trialexpired,campaignwindow}_*`, `outage_banner_text` | fehlt | **Out** |
| 13 | **Auto-Update** | `DefaultAutoUpdateAfterTimeInSeconds` | fehlt | **Out** |
| 14 | **About / Privacy / ToS / Third-Party-Lizenzen** | `aboutwindow_*` | fehlt | **Out für MVP**, **In für ein öffentliches Release** (Lizenzhinweise) |
| 15 | **Menüleisten-Status-Popup** (Statistiken, Benachrichtigungen) | `statuspopup_stats_*`, `contextmenu_*` | Tray existiert (PORT-3), Popup-Detail fehlt | **Post-MVP** |
| 16 | **Tabellen-Sortierung/-Filter** (Name/Letzte/Status/Größe) | `statuswindow_{status,size,lastmodified}columntitle`, `statusall` | fehlt (keine Sortierung/Filter) | **Post-MVP** |
| 17 | **Größe-Spalte + relatives „Letzte Aktualisierung"** | `datetimeago_*`, `sizecolumntitle` | fehlt | **In** (in `SPEC_UPLOAD_PROGRESS.md` enthalten) |
| 18 | **iOS-Dedupe-Prompt** | `iosdedupeprompt_*` | fehlt | **Out** (Apple-Photos-Kontext) |
| 19 | **Mehrere Konten** | `folderconflict_*` „anderes Benutzerkonto" | fehlt | **Out / UNKNOWN** |
| 20 | **Crash-Relaunch (LaunchAgent)** | `WorkerMaxRestarts*`, macOS-LaunchAgent | fehlt | **Out** (macOS-spezifisch) |
| 21 | **Retry-All / Re-add-to-queue / Failed-Fenster** | `readdtoqueue`, `retryall`, `failedsyncitemswindow_retry` | fehlt | **Teilweise In** — im Upload-Progress-TODO (Retry) |
| 22 | **Sync-Mass-Delete-Benachrichtigungen** | `notification_syncmassdeletes_*` | fehlt | **Out** (Sync) |
| 23 | **„Alle Aktivitäten anhalten" global (Upload+Download)** | `activitywindow_*` | nur Backup-Pause | **In bleibt** (Download Teil davon Out) |
| 24 | **„Gleichzeitige Uploads"-Default = min(4, Cores)** | `UploadMaxQueues=4`, `MaxNumberOfQueuesPerProcessorCore=1.0` | Slider 1..8, Default **3** | **Entscheidung offen** — Original: `clamp(1, min(4, Cores×1), 8)` |

### Concurrency-Cap „8" — Research-Ergebnis (belegt, `Infrastructure.AppSettings`/`CustomerSettings`)

- `CustomerSettingsMaxSupportedUploadQueues = 8`, `…Min… = 1`; Downloads symmetrisch (8/1).
  Die „8" ist **kein technisches Limit**, sondern die Obergrenze einer **kunden-/server-gelieferten**
  Einstellung (`SettingsRefreshTimeInSeconds = 3600`), die der `CustomerSettings`-Getter per
  `CheckMinMax` klemmt (`IgnoreCustomerSettingsUploadQueuesSetting` = false ⇒ gilt).
- Der **Geräte-Default** ist kleiner: `UploadMaxQueues = 4` und `MaxNumberOfQueuesPerProcessorCore = 1.0`,
  also `UploadQueues = max(1, min(4, ProcessorCount × 1))` — typisch **4** (bei ≤4 Kernen entsprechend
  weniger). Download identisch (`DownloadMaxQueues = 4`).
- **UNKNOWN:** eine inhaltliche Begründung der 8 gibt es im Code nicht — nur den gesetzten Wert.
  Unser Slider `1..=8` trifft die Obergrenze; unser Default 3 weicht vom Original-Default ab.

## Zurückgestellt — braucht eine bewusste User-Entscheidung (nicht autonom lösbar)

Nichts da ist umgesetzt; alle Punkte sind bewusst **nicht** angefasst, weil sie eine Vorgabe brauchen.

1. **Konflikt-Dialog „Nachfragen" (größte offene MVP-Lücke).** `ConflictResolution::Ask` ist heute
   ein No-Op (mappt wie `Ignore` auf `conflictResolution=NONE`). Für einen echten Dialog braucht es
   die Nutzerentscheidung: pro **Datei** oder pro **Lauf** fragen? Welche Optionen anbieten (Original:
   Ignorieren / Als neue Datei speichern / Ersetzen + „Nicht noch einmal fragen")? Und was passiert
   während der Frage mit dem Upload-Lauf (Pause, Rest zurückstellen, Retry als neuer Job)?
2. **Autostart-Default.** PORT-4 liefert den Schalter; ob er bei Installation **an** oder **aus**
   sein soll, ist eine Produktentscheidung (Default bisher: aus).
3. **Hintergrund-Sync-Takt (PORT-5-Detail).** Event-driven ist spezifiziert (deutig aus dem User-
   Entscheidungssatz); ein **Mindestabstand** zwischen automatischen Läufen (z. B. 15/60 min)
   und ob ein Lauf nur bei bestehendem WLAN startet, ist nicht festgelegt.
4. **Pro-Ziel-Limit / netzwerkgesteuerte Drosselung** (bereits als Post-MVP markiert).
5. **macOS-Signatur:** selbstsignierte Identität (Keychain-Zugriff), ob Benachrichtigungen im
   signierten Bundle erscheinen, ob `--options runtime` für ad-hoc nötig ist.
6. **Öffentliches Remote/Push** (Repo ist public-zielend, aber noch kein Remote konfiguriert).
7. **Nav-Accessibility + Icon-Ausrichtung** (`nav_item`, Status-/Upload-Icons): brauchen die
   Doku-Maßwerte/Design-Abmachung, kein Rate-Wert.
8. **Echtes Profilbild** im Account-Block: keine API identifiziert.
9. **Logout während laufendem Upload:** `logout()` löscht die Auth-Cookies, stoppt aber den laufenden Lauf nicht. Soll der Upload stoppen/pausieren oder weiterlaufen und mit 401 scheitern? (Eine saubere Antwort bräuchte einen Abbruch-Kanal in der Upload-Pipeline.)
10. **F-3 – Pause/Abbruch verwirft gesendete Bytes.** Ein Abbruch im laufenden Upload startet die
   Datei später bei 0 (die bereits gesendeten Bytes sind verloren), und `conflictResolution=RENAME`
   erzeugt bei Abbruch nach Server-Anlage ein Duplikat statt eines 409-Dedupe — beides widerspricht
   dem Ziel „keine Bandbreite verschwenden". Entscheidung nötig: **„aktuelles Element fertig, dann
   ruhen"** vs. sofortiger Abbruch; dazu, ob `RENAME` der richtige Wire-Wert ist.
11. **Farbe/Sprache des Multi-Fenster-Verhaltens** („Einmalige Sicherung"-Zeilendarstellung) — folgt
   aus Punkt 1/3.

## Offen (Backlog)

- [x] **Neu (User-Wunsch 2026-10-13) – Fortschrittsbalken mit Restmenge + Zeitschätzung.**
  Erledigt (Paket E, Commit `0ca6dcf`): `egui::ProgressBar` (`done/total`) in der Kopfzeile mit Text
  „{pct}% · noch {n} Dateien · ca. {m:ss}", ETA aus der Datei-Rate, plus dieselbe Angabe in der
  Statuszeile; robuster Fallback „Schätzung läuft …". Verifiziert (Verifier: APPROVE).
- [x] **Neu (User-Wunsch 2026-10-13) – Hintergrund-Helfer + Desktop-Benachrichtigungen.** Recon +
  Clean-room-Spec `docs/SPEC_NOTIFICATIONS.md` (verifiziert) abgeschlossen. **Umsetzung geht in der
  Cross-Platform-Portierung auf** (PORT-2/PORT-3/PORT-4/PORT-5) — nicht mehr macOS-nativ.
  Recon-Befund: Original hat **keinen** Helfer/XPC/LoginItem-Bundle (`LSUIElement=true`); ein
  LaunchAgent dient nur dem Crash-Relaunch; `NSStatusItem`-Menüleiste + Popover; App bleibt ohne
  Fenster am Leben; Benachrichtigungen nur über das **deprecated `NSUserNotification`**, **kein**
  Toast bei Upload-Fertig/Fehler → für uns ein **neues** Feature.
- [ ] **Optional/Branding:** Account-Block zeigt unter „Angemeldet als:" noch die Zeile „Amazon Photos"
  (Label des angemeldeten Amazon-Kontos, kein Eigen-Branding). Falls keine Amazon-Wortmarke im eigenen
  Fenster gewünscht ist: durch die Domain (`amazon.<cc>`) ersetzen. Bewusst nicht im Rename-Lauf geändert.
- [ ] „Details anzeigen"-Panel (verschachtelt/flach wie das Original, `docs/UI_FLOWS.md` §4.7).
- [ ] Menüleisten-Status-Item (Status-Popup: Status, Benachrichtigungen, Speicher, §9).
- [ ] Videos hochladen (bisher nur Fotos — `.mp4`/`.mov`/… werden wie alle Nicht-Bilder ignoriert).
  Original-Befund (`docs/`, nur gelesen): gleicher Upload-Endpoint (`kind=FILE`, nur `FILE` beobachtet),
  Modus nur über `content_filter`-Bitmaske (`None=0, Photos=2, Videos=4, Other=8, PhotosAndVideo=6,
  Everything=14`; dazu `DeduplicateOptions`, `docs/UI_STATES.md`-Tabellen). Unknown: welche
  Video-Extensions das Original akzeptiert; ob sich Upload-Parameter für Video unterscheiden;
  Quota-Frage (Prime: Fotos unbegrenzt, Videos begrenzt? — `…_notification_overquota_unlimitedphotos_*`,
  Videospeicher-Upsell-String).
- [x] Auto-Refresh der Auth-Daten bei 401. Erledigt, Commit `fff2dc7`: der Upload-Worker erkennt
  „Authentication failed“, erneuert den Token und versucht die Datei einmal erneut (nur ein
  Renew pro Lauf, Wiederholung zaehlt nicht als Fehler).
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
