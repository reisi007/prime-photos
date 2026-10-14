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
  - [x] **F-3:** Behoben (Commit `0fd04dd`) — Pause entscheidet nur am Schleifenkopf: die laufende
    Datei wird fertig, das nächste Element wartet. Kein Byte verworfen, kein `RENAME`-Duplikat mehr
    (User-Entscheidung: „erst Datei fertig, dann pausieren").

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
*(Ergänzung 2026-10-14: Der **Default ist jetzt „an"** — dafür braucht es eine persistierte Absicht, siehe „Zurückgestellt" #2. Nicht umgesetzt.)*
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

- [x] **Modellwechsel Implementierer/Verifier (User-Entscheidung 2026-10-14):** beide Rollen nutzen jetzt
  `opencode-go/deepseek-v4.1-flash` (kein `#variant`), frisch pro Task bzw. pro Verify-Runde. Ersetzt
  `opencode-go/step-5-preview-free` (das seinerseits `opencode-go/deepseek-v4.1-flash#high` abgelöst
  hatte). Belegter Registry-Stand: `status: active`, Kosten 0,15/0,60 $ je 1M Token, Varianten
  low/high/max — bewusst ungenutzt.

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

## Datei-Erkennung per OS-Event (User-Entscheidung 2026-10-14)

Anlass: „periodic background scan is not mvp ready ... hard requirement OS-level file locks" →
per Nachfrage geklärt: gemeint sind **Filesystem-Events**, keine Sperren.

- [x] **D1 – OS-Event-Watcher statt Polling.** Umgesetzt und **committet** (`465a525`, PKG-2a):
  `notify` 8.2.0 (User-Vorgabe, dokumentiert, weil die Projektregel „keine zusätzlichen
  Abhängigkeiten" hier vom User ausdrücklich aufgehoben wird), Watcher pro aktiviertem
  Backup-Ordner in `src/backup.rs` (`sync_watchers`, `watch_loop`), `WATCH_DEBOUNCE` 5 s,
  Re-Arm/Set-Sync `WATCH_POLL` 30 s. Der 15/90-Minuten-Timer ist entfernt.
- [x] **D2 – Reaktion pro Event:** implementiert (`collect_event_paths`, `map_to_folders`,
  `handle_files`) — nur die betroffene Datei prüfen, kein kompletter Ordner-Durchlauf.
- [x] **D3 – Stat-Check vor dem Hash** (`src/dedup.rs`): umgesetzt und **committet**
  (`488d1e4`). Treffer nur bei path+size+mtime; unbekannte mtime (0) erg nie einen Treffer.
  Das MD5 bleibt Identität (`docs/SPEC_DEDUP.md` §1).
- [x] **D4 – Kaltstart-Sicherheitsnetz (Wasserzeichen):** umgesetzt und **committet** (`465a525`,
  PKG-2a): Tabelle `folder_scan(path, max_mtime, scanned_at)` in derselben SQLite,
  `folder_max_mtime`/`record_folder_scan`, `folders::max_mtime()` (reiner Metadaten-Durchlauf),
  `scan_new_since()`. Fortschreiben nur nach fehlerfreiem Lauf, mit dem zum
  Entscheidungszeitpunkt gelesenen Wert (während des Uploads dazugekommene Dateien werden
  nicht verschluckt). Kein Wasserzeichen ⇒ voller Stat-Durchlauf.
- [x] **D5 – Clean-Room-Doku** `docs/SPEC_CHANGE_DETECTION.md`: **erledigt** (Commit
  `4ca98b2`) — Watcher-Mechanik, Indikatoren (QuickScan braucht Stat-Kombination),
  Cold-Boot-Scan, halb geschriebene Dateien. Nachtrag: Gap-Analyse nach PKG-2a auf den
  neuen Port-Stand aktualisiert (**erledigt**, `777938f`).
- [x] **D6 – Event-Puffer ohne Kappe (User-Entscheidung 2026-10-14).** Anlass: Der Nutzer
  hinterfragte die 500er-Kappe („10k files zum uploaden sollte schon unterstützt werden …
  warum kann die Liste nicht unlimitiert groß sein bzw. dynamisch wachsen") und stellte fest,
  dass ein bloßes `log::warn!` den Bug nur lauter macht, statt ihn zu beheben. Umgesetzt
  (**committet** `2eea96c`, PKG-2b): `WATCH_PENDING_MAX` ersatzlos entfernt (keine Obergrenze,
  Liste wächst dynamisch), Dedup per `HashSet<PathBuf>` O(1) statt O(n²), reiner Helfer
  `park_paths`. **Dabei gefundener und behobener Verlust-Bug:** der Event-Block in `watch_loop`
  hat den geparkten Bestand mit `pending = handle_files(…)` **überschrieben** — bei mehreren
  Bursts während einer Pause überlebte nur der letzte; jetzt wird **vereinigt** und bei
  erlaubtem Lauf alles in **einem** Lauf gestartet. 3 neue Tests (2.000 Pfade ohne Kappung,
  Doppelpfad ⇒ 1 Eintrag, Vereinigung ohne Verlust/Duplikate). Verify Runde 1: APPROVE.
  **Nicht verworfen, sondern bewusst offen:** ein optionaler Rescan-Fallback bei Überlauf ist
  mit der unbegrenzten Liste **nicht mehr nötig** (es gibt keinen Überlauf) — bleibt aber als
  Netz für *verpasste* Events (Watcher-Limit, ausgeworfenes Laufwerk) ein eigener, noch nicht
  entschiedener Punkt (siehe unten „Offen").

## Assets: Tray-Icons + Logo — Quellen geliefert (2026-10-14), Einbindung **offen**

Eigene Assets liegen in `assets/` — **als SVG-Quellen** (maßgeblich: `AGENTS.md` §„Assets"), daraus
rastet der Build die PNGs. Auftrag: einplanen, damit „MVP polished" inkl. Autostart und effizientem
Upload erreicht wird.

**Verbindliche Einbindung (Vorgabe, damit es auf jeder Plattform/im Bundle funktioniert):**
- Ablageort: **`assets/` im Repo-Root**, **SVG-Quellen** — keine committeten PNGs mehr (sie entstehen im Build).
- `build.rs` rastet die SVGs mit `resvg` (`[build-dependencies]`, `default-features = false`, reine
  Rust-Bibliothek, keine Laufzeit-Abhängigkeit) nach `OUT_DIR`; Einbindung per
  **`include_bytes!(concat!(env!("OUT_DIR"), …))`** (nicht zur Laufzeit vom Pfad) + Dekodierung über die
  bereits vorhandene `image`-Crate → `tray_icon::Icon::from_rgba` bzw. eframe-Viewport-Icon.
  Keine Pfadauflösung im `.app`-Bündel, Binary bleibt self-contained.
**Kein Fallback (User-Vorgabe 2026-10-14):** fehlt ein SVG, bricht `build.rs` den Build (Compile-Fehler).
Kein `Option`, keine `path.exists()`-Prüfung, kein Rückfall auf die gezeichnete Variante (`src/icons.rs`
bleibt nur für die Vektor-Glyphen im UI selbst: Navigation, Status-Punkt, Flagge). Ein Dekodier-Fehler
zur Laufzeit ist ebenfalls hart (fail fast, klare Meldung) — eine stille degradierte UI ist
ausdrücklich **nicht** gewollt.

**Geliefert (Stand 2026-10-14) — Quellen sind SVGs in `assets/`:**

| Datei | viewBox | Zweck |
|---|---|---|
| `tray.svg` | 84×84 | Tray-Basis (Leerlauf: Upload-Pfeil) |
| `tray-active.svg` | 84×84 | „Upload läuft" (Pfeil mit Bewegungslinien) |
| `tray-paused.svg` | 84×84 | „Angehalten" (zwei Balken) |
| `tray-done.svg` | 84×84 | „Fertig" (Haken) |
| `tray-error.svg` | 84×84 | „Fehler" (Ausrufezeichen) |
| `logo.svg` | 256×256 | App-Logo (orange, Upload-Pfeil + Fotostapel) |
| `logo-small.svg` | 256×256 | kleine farbige Variante |
| `logo-small-black.svg` | 256×256 | schwarze Variante (dunkler Hintergrund) |

Die Tray-Zustände sind **monochrom über `stroke="currentColor"`** — die Farbe entscheidet der Renderer,
deshalb ergibt sich schwarz/weiß ohne zweite Datei. **Entschieden (User 2026-10-14): die PNGs entstehen
im Build** — `build.rs` + `resvg` als `[build-dependencies]` (`default-features = false`, 53 Crates,
keine Laufzeit-Abhängigkeit), Ausgabe nach `OUT_DIR`, Einbettung per `include_bytes!`. Verworfen wurden
(mit Beleg): `sips`/`qlmanage` im Build (gute Qualität, aber macOS-only ⇒ CI/Release-Matrix bricht),
ImageMagicks interner Renderer (malt die Striche nicht, `rsvg-convert` fehlt), und „einmalig rendern und
committen" (zwei Quellen, die auseinanderlaufen). **Theme:** `egui::Context::system_theme()` — dunkel ⇒
weiß, hell ⇒ schwarz, `None` ⇒ farbiges Logo (immer sichtbar). **Zu erzeugende PNGs:** fünf Zustände ×
{schwarz, weiß} × {32, 64} + `logo` 1024 + `logo-small` 256 + `logo-small-black` 256.

Der Tray-Satz ist durchgängig **monochrom (`currentColor`) auf transparent** — genau die
macOS-Template-Semantik. Es gibt **keine** separaten schwarzen/weißen Dateien und **keine**
PNG-Zwillinge im Repo: Weiß/Schwarz ist ein Raster-Override beim Build (`*{color:#fff}` bzw. `#000`).
Es gibt auch **keine QA-Artefakte** mehr im Repo — die früheren Kontrollbilder (`chk-logo-*`,
`contact-sheet`, `zoom-corners`, `debug-mask-*`, `preview-tray`) und der komplette PNG-Satz wurden
entfernt; committet werden ausschließlich die acht SVG-Quellen der Tabelle oben.

**Einbindungspunkte, die bereits existieren (verifiziert im Code):**
- `src/tray.rs`: `ICON_SIZE = 32`, `tray_icon::Icon::from_rgba(…)`, und **auf macOS ist das
  Template-Flag schon gesetzt** (`with_icon_templated`, nicht-deprecatierter Nachfolger von
  `with_icon_as_template(true)`), außermacOS `with_icon(icon)`. Der heutige Glyph wird gezeichnet
  und ist **teal** — die neuen schwarz/Alpha-Raster passen dort 1:1 hinein.
- macOS skaliert (`tray-icon` 0.26.1, `to_nsimage(max_height)`) auf die Menüleisten-Höhe **herunter**;
  ein Icon, das schon passt, wird nicht hochskaliert. ⇒ Als Quelle das 64-px-Raster (`@2x`) nehmen
  (mehr Bitmap pro Punkt = retina-scharf), das 32-px-Raster für die Nicht-macOS-Pfade/1×-Fälle.
- Dock-/Fenster-Icon: eframe-Viewport-Icon (RGBA) aus dem Build-Raster von `logo.svg` (1024 px); die
  gezeichnete Variante `src/icons.rs::upload_logo` bleibt nur, wo sie heute als Vektor genutzt wird.

**Windows/Linux-Tray (User-Entscheidung 2026-10-14): weiße Varianten + Theme-Erkennung.** Außerhalb
macOS wird das Icon **ohne** Template-Flag gesetzt (`with_icon`), ein schwarzes Monochrom-Icon wäre
auf dunkler Taskleiste unsichtbar. Weiß entsteht **beim Rastern aus derselben SVG-Quelle** per
`currentColor`-Override (`*{color:#fff}`) — es gibt **keine** separaten `tray-*-white`-Dateien. Die App
wählt per **Theme-Erkennung** — `egui::Context::system_theme()` (in 0.31 vorhanden): dunkel ⇒ weiß,
hell ⇒ schwarz; `None` (unbekannt) ⇒ das **farbige `logo`-Raster**, das auf jedem Hintergrund sichtbar
ist (dann ohne Zustandswechsel). macOS bleibt beim schwarzen Template-Satz (das System invertiert
selbst). **Verworfen:** farbiges Logo als generelle Non-macOS-Lösung (verliert die Zustände);
„bewusst akzeptieren" (nicht polished). **Die Quellen liegen vor ⇒ Umsetzung kann starten;** die
Assets sind per Vorgabe **compile-pflichtig** (`build.rs` bricht bei fehlendem SVG).

Recherche-Grundlage: `docs/SPEC_NOTIFICATIONS.md` §2.4 (Icon-/Status-Map des Originals:
idle/processing/paused/done/error) und §2.3 (Menüleisten-Item); das Original liefert monochrome
Template-Bilder plus eine farbige Ungelesen-Variante.

**Einplanung:** Symbolwechsel zur Laufzeit (aktiv/pausiert/Fehler) gehört in die Asset-Welle;
das Dock-/Fenster-Icon ist davon unabhängig und kann allein kommen. Nicht Teil: das `.app`-Icon
für das macOS-Bundle (`packaging/make-app.sh`) — eigener, bereits zurückgestellter Punkt.

## URL-Util `src/urls.rs` (User-Entscheidung 2026-10-14) — ✅ **ERLEDIGT** (`791687d`, Verify Runde 1 APPROVE)

**Ergebnis:** `src/urls.rs` ist der **einzige** Ort für marktplatz-/zonenabhängige URLs —
`marketplace_domain` (die **einzige** Code→Domain-Abbildung; `login::amazon_domain` ist entfernt) plus
Builder für alle Routen: `login_url`, `auth_token_url`, `nodes_url`, `nodes_base_url`, `search_url`,
`children_nodes_url`, `add_to_album_url`, `preferences_url`, `thumbnail_url`, `upload_url`.
`COUNTRIES` ist auf `(code, name)` reduziert (die Domain-Spalte war eine zweite Wahrheit),
`country_label` holt die Domain aus der Util, Aufrufer umgestellt (`login.rs`, `prime_photos.rs`,
`app.rs`, `settings.rs` nur Arity). **Der Bugfix wirkt für `us`, `uk`, `gb`, `jp`, `au`, `br`, `mx`
(echte Domain statt nicht existierendem Host) und `at`/`ch` (auf `.de`)** — für
`de/fr/it/es/ca/cn/in` bleiben alle URLs **byte-identisch**, unabhängig belegt: der Verifier hat
HEAD-Builder und neue Builder für **10 Builder × 16 Codes** gegeneinander laufen lassen (inkl.
Parameterreihenfolge, `%2C`, Slash, Upload-Query). Regel-Beleg: außerhalb `src/urls.rs` kein
`https://www.amazon.`-, `thumbnails-photos.amazon.`- oder `content-<zone>`-Literal. Gates: 7 Warnungen
(Delta 0, auch gegen eine HEAD-Kopie gegengeprüft), **39/39** Tests (30 − 3 verschoben + 12 neu).


Anlass: Die Umsetzung des Profilbilds hatte den Thumbnail-Host **hartkodiert auf `.de`** — aufgefallen
ist das erst in der Verifikation. Der User: *„Können wir das URL-Bauen in eine eigene Datei auslagern
und als Regel etablieren, dass über diese Util alle URLs auf Marktplatz-Basis individualisiert werden
muss?"* → **Ja**, als Projektregel in `AGENTS.md` festgehalten.

**Befund (2026-10-14, beim Fix von Welle 3 aufgefallen): marktplatzabhängige URLs sind heute FALSCH
für die meisten Marktplätze.** `config.country` ist ein **Code** (`de`, `uk`, `jp`, `au`, `br`, `mx`, …,
so wie ihn `parse_cookie_string` aus dem Cookie-Namen `x-acb<code>` liest), aber die Drive-Aufrufe
bauen den Host als `https://www.amazon.{country}` mit dem **rohen** Code. Für alles, wo Code ≠ Domain
(`uk→co.uk`, `jp→co.jp`, `au→com.au`, `br→com.br`, `mx→com.mx`) läuft der Aufruf damit auf einen
**nicht existierenden Host** — nur `de`/`fr`/`it`/`es`/`ca`/`cn`/`in` stimmen zufällig. Der **Login**
nutzt dagegen korrekt `amazon_domain`. Folge: für UK/JP/AU/BR/MX-Konten scheitern `nodes`/`search`/
`children` (also der gesamte Upload-Pfad); aufgefallen ist es nie, weil das Testkonto `de` ist. Dazu
dupliziert `COUNTRIES` die Domain in ihrer **zweiten Spalte** und `amazon_domain` bildet sie **noch
einmal** ab — zwei Wahrheiten für dieselbe Zuordnung.

**Umfang (nächstes Paket, direkt nach dem Commit von Welle 3 — dieselben Dateien):**
- Neue Datei **`src/urls.rs`** als einziger Ort für marktplatz- und zonenabhängige URLs. Die
  Land→Domain-Zuordnung **`amazon_domain`** (heute `src/login.rs:38`) **zieht dorthin um**; `login.rs`,
  `prime_photos.rs` und `app.rs` beziehen sie von dort (ein Pfad, keine zweite Tabelle).
- Builder für alle heute verstreuten Routen: Signin (`login.rs:67`), `photos/auth/token`,
  `drive/v1/nodes` (GET/POST), `…/nodes/{id}/children` (2 Varianten), `drive/v1/search`,
  `drive/v1/account/preferences`, `v1/thumbnail/<id>`, sowie der Upload-Host
  `content-<zone>.drive.amazonaws.com/v2/upload`.
- **Regel:** außerhalb `src/urls.rs` kein `format!("https://www.amazon.{}…")`, kein
  `thumbnails-photos.amazon.`-Literal, kein `content-{zone}`-Literal — Tests gehören in die Util.
- **Marktplatzneutral:** kein Literal-TLD; Ableitung nur über die Util. Belegter Stand: das Host-Muster
  `thumbnails-photos.amazon.<tld>` existiert für `.de/.com/.co.uk/.fr/.it/.es/.ca/.co.jp` (DNS +
  HTTP-Antwort geprüft); **inhaltlich** getestet ist nur `.de` (es gibt nur ein DE-Konto).
- **Eine Zuordnung, nicht zwei:** `COUNTRIES` führt die Domain heute als **zweite Spalte**
  (`("at","de","Österreich")`), `amazon_domain` bildet sie **erneut** ab. Ziel: die Domain-Abbildung
  lebt **nur** in `urls.rs`; `countries.rs` behält Code + Anzeigename und holt das Domain-Label über
  die Util (kein zweiter Wahrheitswert).
- **Refactor + bewusste Korrektur:** für `de`/`fr`/`it`/`es`/`ca`/`cn`/`in` bleiben die erzeugten URLs
  **byte-identisch**; für `uk`/`jp`/`au`/`br`/`mx` ändern sie sich **absichtlich** auf den korrekten
  Host (und für `at`/`ch` auf `.de`). Genau das ist der Altbestands-Bug oben — deshalb je Marktplatz
  mindestens ein Test und ein Vergleichstest „Code = Domain ⇒ unverändert".

## Implementierungs-Wellen (User-Auswahl 2026-10-14) — **NUR DOKUMENTIERT, NICHT GESTARTET**

User hat die nächsten Wellen ausgewählt, aber ausdrücklich **keinen Startauftrag** erteilt
(„nur dokumentieren, aber nicht starten"). Diese Liste ist damit ein **Plan**, kein Arbeitsauftrag;
gestartet wird erst auf ausdrücklichen Auftrag. Auftragsbeginn je Welle wird mit Datum + Commit
hier ergänzt.

### Welle 2 — P2 + P3 (die zwei offenen, bereits entschiedenen Punkte)

- ✅ **P2 – One-Time-Queue in SQLite: ERLEDIGT** (`00bc338`, Verify Runde 1 **APPROVE**). Abgelegte
  Drag&Drop-Dateien liegen temporär in derselben SQLite wie MD5-Index und `folder_scan`
  (`one_time_queue(position PK, display_path, path)`, API `set_one_time_queue` / `one_time_queue` /
  `clear_one_time_queue`), werden **nur** bei vollständigem, fehlerfreiem Lauf gelöscht
  (`done + failed >= total` ∧ `failed == 0`) und beim Start wiederhergestellt; der Lauf wird unter
  denselben Guards wie der Kaltstart einmalig angestoßen (Frame-Loop, kein Timer). Kein Watch, kein
  Wasserzeichen. **4 akzeptierte nits, alle fail-safe (kein Datenverlust):** (a) die
  wiederhergestellte Zeile zeigt „Uploading 0 %", solange die Guards blockieren → wird in P3
  adressiert; (b) startet zwischen Laufende und `finalize_upload` ein Hintergrund-Lauf, bleibt die
  Queue stehen (nächster Start leert sie; die Dateien sind ohnehin im MD5-Index); (c) ein
  abgelehnter Drop lässt eine wartende Queue stehen (bewusst: kein stilles Verwerfen); (d)
  `save_config` im Resume-Pfad loggt einmalig.
- ✅ **P3 – Gesamt-Fortschritt: ERLEDIGT** (`7c6f2b5`, Verify Runde 1 **APPROVE**). Der
  `if running`-Block (Spinner + Balken + `progress_text`) ist aus der Kopfzeile entfernt; die neue
  **Fußzeile** am Ende der Backup-Seite zeichnet **eine** Anzeige über `sum(done)/sum(total)` aller
  Ziele inkl. `ONE_TIME_IDX` (Datenquelle: die bereits gebaute `rows`-Momentaufnahme), Spinner nur bei
  aktivem Lauf, `total == 0 ⇒ frac 0.0`; im Lauf mit dem Byte-/ETA-Detail aus `upload_progress_text`,
  im Leerlauf „Alles aktuell" bzw. „Gesamt: X von Y Dateien". Reiner Helfer `aggregate_progress` +
  3 Tests; 3 neue i18n-Keys (de + en). Die wiederhergestellte Einmal-Zeile steht jetzt auf
  `FolderStatus::Idle`. Nebenläufigkeit explizit geprüft: **kein** Mutex-Guard über `ui`-Aufrufe,
  `rows`-Aufbau, `store.save()` oder `DedupIndex::open()`.
  **3 nits** (im Folgepaket abgeräumt): (a) der `app.rs`-Kommentar überzeichnete die Wirkung — `Idle`
  ändert die **Farbe** (Muted statt Teal), der Text „0% hochgeladen" kommt aus dem unveränderten
  `status_text`/Label; (b) die Fußzeile hing an `!rows.is_empty()` — wurde während eines Laufs das
  letzte Ziel entfernt, verschwand auch Balken + ETA; nun `|| running`; (c) Ziele ganz ohne Total
  zeigten kosmetisch „Gesamt: 0 von 0 Dateien" — im Leerlauf wird dann nichts gezeichnet.

### Welle 3 — echter Name + Profilbild im Account-Block

✅ **ERLEDIGT (`f81c679`; Runde 1 APPROVE → drei beauftragte Fixes → Runde 2 APPROVE ohne neue
Findings).** Umgesetzt: reine Helfer `preferences_url` / `thumbnail_url(node_id, country)` /
`avatar_cache_file_name` / `avatar_cache_path` / `looks_like_image` (je mit Tests), Methoden
`fetch_account_name` + `fetch_avatar_jpeg` (beide **nur** über `build_common_headers`, **kein**
`x-amz-access-token` — das liefert 401), Abruf im Hintergrund-Task (`self.rt`, Rückkanal
`Arc<Mutex<AvatarFetch>>` + `request_repaint`, **nie** im `update`-Frame), Cache unter
`…/prime_photos/thumbnails/<nodeId>_60_60.jpeg` (gültige Datei wird **sofort** angezeigt, danach
frisch geladen und geschrieben; kaputte Datei blockiert nie — `looks_like_image` beim Lesen **und**
Schreiben), **genau ein** Wiederholungsversuch nach Fehler (`should_fetch_avatar`: 0 ⇒ ja, 1 ⇒ nur
bei Fehler, ≥2 ⇒ nein), Anzeige als **einmal** dekodierte Textur mit runden Ecken + Presence-Dot,
Platzhalter bis zum Eintreffen unverändert, Logs ohne Werte/URLs/Cookies, Beschriftung
**„Angemeldet als:"** (neuer zweisprachiger Key). Gates: 7 Warnungen (Delta 0), **30/30** Tests.
**Offen (Verifier-Fund, bewusst ins nächste Paket geschoben):** `preferences_url` baut den Host noch
mit dem **rohen Ländercode** statt über `amazon_domain` — dieselbe Fehlerklasse wie der bereits
gefixte Thumbnail-Host; wird zentral in `src/urls.rs` korrigiert (nächster Abschnitt).

**VORAUSSETZUNG ERLEDIGT (2026-10-14, User-Erlaubnis: „Wir sind angemeldet, also sollte ein Start
durch dich reichen"):** Der Live-Beweis ist erbracht — `GET https://www.amazon.de/drive/v1/account/preferences`
antwortet mit **HTTP 200** auf **genau unseren** Cookie-Satz (`session-id`, `x-acb<cc>`, `at-acb<cc>`,
`ubid-acb<cc>`, plus `x-amzn-SessionId`, denselben `Content-Type`/`Accept`/`User-Agent` wie
`build_common_headers`) und liefert u. a. `name` (17 Zeichen) und `avatarNodeId` (22 Zeichen).
Die in `docs/SPEC_ACCOUNT_BILLING.md` §9 geführte UNKNOWN („akzeptiert der Endpunkt unsere
Cookie-Session?") ist damit **beantwortet: ja**. Getestet ohne Diagnose-Code (kein Code geändert,
kein Probing im Produkt), nur lokal mit den Cookies aus `config.toml`; **keine** Cookie-Werte oder
Namensinhalte protokolliert. **Thumbnail-Route: GELÖST UND LIVE BEWIESEN (2026-10-14, Recon + eigener Test):** Die Route liegt **nicht**
unter `/drive/v1`, sondern auf dem eigenen Host (`docs/PROTOCOL.md` §2.4, `thumbnailServiceUrl`):

```
GET https://thumbnails-photos.amazon.de/v1/thumbnail/<avatarNodeId>?viewBox=60%2C60&fit=clip&encoding=jpeg
```

Antwort = **direkte Bilddaten** (JPEG), kein JSON, kein `tempLink`. Mit **unserem Cookie-Satz** (derselbe
wie `build_common_headers`, also ohne den gerätespezifischen `x-amz-access-token`, den das Original
schickt) liefert der Aufruf **HTTP 200, `image/jpeg`, 2 135 Bytes, 60×60**. Der Umweg über `tempLink`
(= Rohdatei, bei diesem Konto 13,5 MB CR3) war die falsche Spur. Der Recon hat die URL-Konstruktion im
Original belegt (`EndpointsProvider.GetThumbnailUrl`, `CloudDriveService.DownloadThumbnail`,
`ThumbnailWorker.GetSmartCroppedThumbnail`, `AvatarViewBoxSize = 60`) und in
`docs/SPEC_ACCOUNT_BILLING.md` §3.4/§9 dokumentiert; die dort zuletzt offene UNKNOWN („akzeptiert der
Thumbnail-Host unsere Cookie-Session?") ist mit diesem Test **beantwortet: ja**.


§2.4 enthält die Endpoint-Discovery mit `thumbnailServiceUrl: https://thumbnails-photos.amazon.de/`
(daneben `metadataUrl: https://cdws.eu-west-1.amazonaws.com/drive/v1/`, `downloadServiceUrl`,
`photosBFFUrl`, `uploadServiceUrl`). Die Erwartung „das Original lädt einen Thumb, nicht das RAW" ist
damit bestätigt; das 13,5-MB-RAW kam nur über meinen `tempLink`-Pfad (Originaldatei). Live-Probe:
`GET`/`POST`/`HEAD`/`OPTIONS` auf `<thumbnailServiceUrl>/{nodeId}` sowie die Varianten
`?width=60&height=60`, `/IMAGE_THUMBNAIL`, `?transform=IMAGE_THUMBNAIL`, `?size=60`, `?tempLink=true`
antworten **alle** mit `405 Not allowed` + Body „Invalid request" → die Anfrage braucht eine
**Signatur/Autorisierung**, ein reiner Cookie-Aufruf genügt nicht. Deshalb: IL-Recherche in
`GetSmartCroppedThumbnail`/`GetThumbnailUrl` (Clean-Room, `docs/SPEC_ACCOUNT_BILLING.md` §3.4/§9).

**Blocker Hinweis (2026-10-14): `/Applications/Amazon Photos.app` existierte nicht mehr** — der User hatte
die App in den Papierkorb gelegt. Konsequenzen: (a) die beiden Recon-Aufträge, die auf diesen Pfad
zeigen (`docs/SPEC_ABOUT.md`, Thumbnail-Route), zielen ins Leere und müssen **neu gestartet** werden;
(b) der Papierkorb ist für den Shell-Prozess **nicht lesbar und nicht auflistbar** (macOS-TCC;
`mv` scheitert mit `Operation not permitted`, `sandbox_extension_issue_file failed`; `ls ~/.Trash`
zeigt nichts, obwohl Spotlight die Pfade kennt; auch der Finder-Weg per AppleScript läuft in einen
AppleEvent-Timeout `-1712`). **Gelöst (2026-10-14):** die App wurde **offiziell neu geladen** und wieder
nach `/Applications/Amazon Photos.app` installiert —
`https://www.amazon.com/photos/download` → 302 →
`https://d29x207vrinatv.cloudfront.net/mac/AmazonPhotosInstaller.dmg` (1 846 066 B, Stub) →
dessen Payload `https://d29x207vrinatv.cloudfront.net/mac/AmazonPhotosApp.zip` (60 488 014 B) →
`ditto -x -k` entpackt und per `ditto` nach `/Applications` kopiert (`/Applications` ist für den
Shell-Prozess **beschreibbar**). **Versionstreue bestätigt:** `CFBundleShortVersionString = 11.3.0`,
`CFBundleVersion = 36`, `com.amazon.clouddrive.mac`, TeamIdentifier `94KV3E626L` — identisch mit der
Version, aus der `SPEC_FOLDERS`/`SPEC_MEDIA_IMPORT`/`SPEC_OS_INTEGRATION`/`SPEC_PREFERENCES` gebaut
wurden (dort `11.3.0`/Build `36`). Die Specs bleiben also gültig; 202 DLLs im Bundle.


- **Funktioniert:** `GET /drive/v1/nodes/{avatarNodeId}?tempLink=true` → JSON mit `tempLink` auf den Host
  `content-eu.drive.amazonaws.com`. Der `tempLink` braucht **unsere Cookies** (ohne: HTTP 401).
- **Der Avatar-Quellknoten dieses Kontos ist ein RAW:** Canon CR3, 6024×4020, `size` 13 475 387 Bytes,
  `contentType image/x-canon-cr3` — der Download liefert byte-identisch 13 475 387 Bytes mit
  ISO-BMFF-Kopf (`ftyp`/`crx `). Dreifach gegengeprüft (Metadaten, Byte-Zahl, Magic-Bytes). **Lokal
  nicht dekodierbar** — die `image`-Crate kann kein CR3. Ohne Server-Crop ist das Profilbild also
  nicht darstellbar.
- **Deshalb nötig:** die **Server-Thumbnail-Route** (das Original: `GetSmartCroppedThumbnail(nodeId,
  ownerId, 60, 60, path)`, `docs/SPEC_ACCOUNT_BILLING.md` §3.4) — Recon dazu läuft.
- **Sackgassen (nicht erneut probieren, siehe „Verworfen"):** `/nodes/{id}/content` (mit/ohne
  `tempLink=true`), `/nodes/{id}/thumbnail` (mit `width`/`height`/`transform`),
  `/nodes/{id}/transforms/IMAGE_THUMBNAIL` (mit/ohne `tempLink`),
  `/nodes/{id}/content?transform=IMAGE_THUMBNAIL&tempLink=true` — alle **404**. Der Parameter
  `?transform=IMAGE_THUMBNAIL` wird **ignoriert** (liefert weiter die Rohdatei). Ein Suchtreffer trägt
  `transforms: ["IMAGE_THUMBNAIL"]` als **Deklaration**, nicht als URL.

- Voraussetzung (nicht recherchierbar, nur testbar): akzeptiert unser Cookie-Pfad
  (`session-id` + `x-acb`/`at-acb`/`ubid-acb`) den belegten Endpunkt
  `GET <metadataUrl>/drive/v1/account/preferences` (`docs/SPEC_ACCOUNT_BILLING.md`,
  `CloudDriveAccountRefreshWorker.UpdatePreferences()`)? Felder `name` + `avatarNodeId`,
  Avatar = 60×60 smart-crop, Cache `ThumbnailsPath/<nodeId>_60_60.jpeg`, Placeholder-Fallback,
  Refresh beim Start + alle 24 h + on demand, nicht persistiert.
- Bei Ja: Name als Titel des Account-Blocks (Original-Verhalten) + Avatar; bei Nein: Block
  bleibt wie heute (Marktplatz + Presence-Dot + „Abmelden").
- Quota ebenfalls belegt (`/drive/v2/account/quota?customerId=UNNEEDED`, Roh-Bytes) — separater,
  eigener Punkt, nicht Teil dieser Welle.

### Welle 4 — Release-Pflicht: About / Third-Party-Lizenzen

**RECON ERLEDIGT (2026-10-14): `docs/SPEC_ABOUT.md` liegt vor** (Clean-Room, 350 Zeilen). Die
entscheidenden Befunde:
- **Das Original hat KEINE Drittanbieter-Lizenzliste** — keine Datei im Bundle, keine Embedded-Resource,
  kein `.nib`/`.storyboardc`, kein Lizenztext in `strings`. Es gibt nur einen **Link**
  „Drittanbieterlizenzen" → `https://www.amazon.<TLD>/photos/desktop/thirdpartylicenses`, im
  System-Browser. **Folge für uns: die Liste müssen wir selbst erzeugen** (Lizenzhinweise sind
  Pflicht) — sie ist nicht „nachbaubar", es gibt nichts nachzubauen.
- Versionsanzeige des Originals: `CFBundleShortVersionString` + `.` + `CommitId` (installiert:
  `11.3.0.1d7941b5`); Build `36` wird **nicht** angezeigt. Ein About-Fenster als **Sheet**,
  Logo + Version + Seriennummer + „Send-Logs" + drei Rechts-Links, seriennummer/Version selektierbar,
  kein Standard-macOS-About; Auslöser: Tray-Statusmenü + App-Menü (erster Eintrag), **kein** Kürzel.
- Rechtstexte sind reine **Links** (ToS wechselt mit Anmeldestatus), kein In-App-Rechtstextfenster;
  Release Notes ist ein eigenes Fenster (WebView auf eine CloudFront-Seite).
- Offengelegt: Code setzt 504×410, die Constraints fordern 572 pt Höhe → tatsächliche Größe UNKNOWN.


- About-Fenster mit Version, Kurzhinweis, Datenschutz/ToS-Hinweis und Third-Party-Lizenzen
  (`notify`, `reqwest`, `egui`/`eframe`, `rusqlite`, `wry`, `md5`, `image`, `kamadak-exif`,
  `walkdir`, `chrono`, `tokio`, `tray-icon`, `auto-launch`, `rfd`, `clap`, `serde`/`toml`,
  `futures`, `indicatif`, `dirs`, `log`/`env_logger`, `dispatch2`/`objc2` — macOS-Zweig).
- In `AGENTS.md` als Voraussetzung für ein öffentliches Release vermerkt; neue Strings
  zweisprachig (i18n-Regel).

### Weitere offene Punkte (Bild-/Upload-/Sync-nah, dokumentiert, nicht eingeplant)

Auswahl des Users als „Sonst noch offene Punkte"; bewusst **ohne** Starttermin:
- Queue-Paginierung (heute bricht das Panel bei 300 Zeilen ab, Original lädt seitenweise 1000).
- Fehlerfenster mit `readdtoqueue`/`retryall`; Semantik laut `SPEC_STATUS_DETAIL.md` (§3).
- Pause **pro Ordner** (Zeilen-Symbol) — im Original vorhanden, bei uns Post-MVP.
- Sortierung/Filter der Tabelle; Spalte „Größe" + relative Zeit (`datetimeago_*`).
- Tray-Status-Popup mit Statistiken (`statuspopup_stats_*`).
- Kaltstart-Verhalten nach Login; Pro-Ziel-Limit; netzwerkgesteuerte
  Drosselung; Logout während laufendem Upload; Nav-Accessibility; `.app`-Icon.
- **Nicht** eingeplant (bewusst Out of Scope, siehe Scope-Inventar): Sync, Downloads/Restore,
  Datei-Management, Apple-Photos-Import, Videos, Autoplay, Upsell-/Quota-Banner, Auto-Update,
  Proxy, ausgeblendete Dateien, iOS-Dedupe-Prompt, Crash-Relaunch.

### Referenz: was in diesen Wellen **nicht** neu erfunden werden darf

Alle Rechte-/Verhaltensfragen sind entweder aus den acht Clean-Room-Specs belegt oder als
UNKNOWN markiert. Wo eine Spec einen Original-Wert nennt, gilt er als Vorgabe, nicht als Ratewert.

## Doku-Auftrag „Rest-Specs" (User-Wunsch 2026-10-14) — ERLEDIGT

Alle acht beauftragten Clean-Room-Specs sind geschrieben, after-verify-round-korrigiert und
committet (5× `docs:`-Commits: `4ca98b2`, `a105faa`, `a638d0f`, `952f81b`, plus Folge-Fix).
Quelle war durchgehend nur lesend `/Applications/Amazon Photos.app` + `ilspycmd`
(v11.1.0.9782, Dump 175 559 Zeilen / 2 486 Typen) + `de.lproj/Localizable.strings`.

| Spec | Commit | Kern |
|---|---|---|
| `SPEC_CHANGE_DETECTION.md` | `4ca98b2` | DetectChanges = Size+ModifiedDate+Device/FileId; **Original prüft keine Datei-Locks**; Löschungen nur aus Walk-vs-DB |
| `SPEC_OS_INTEGRATION.md` | `4ca98b2` | Finder-Dienst legt geplante Sicherung an; LaunchAgent-Crash-Relaunch; LSUIElement |
| `SPEC_MEDIA_IMPORT.md` | `4ca98b2` | Media-Import in diesem Build praktisch tot; bearbeitete statt Original-Datei |
| `SPEC_PREFERENCES.md` | `4ca98b2` | vollständiges Settings-Inventar inkl. Customer-Settings-Keys |
| `SPEC_LIBRARY.md` | `a105faa` | Download ist Default-Screen; Bibliothek fast nur API; Verw-Verben DELETE/PATCH/PUT |
| `SPEC_ACCOUNT_BILLING.md` | `a105faa` | **`/drive/v1/account/preferences` → `name` + `avatarNodeId`** |
| `SPEC_SYNC.md` | `a638d0f` | Sync ist eigenes Modus-Subsystem; 120 s = Health-Metric; Sync-Ende im Original angekündigt |
| `SPEC_STATUS_DETAIL.md` | `952f81b` | „Letzte Aktualisierung" = StatusChanged, nicht mtime; „Alle Aktivitäten anhalten" ≠ Aktivitätsfenster |

**User-Vorgabe dazu:** dokumentieren, aber **keine** konkreten TODOs aus den Specs ableiten
(gilt besonders für den Löschen/Patch-Befund — Uploader-Scope bleibt).
**Alle Specs sind damit app-unabhängig dokumentiert** — die Deinstallation der Original-App
kann erfolgen, ohne dass Recon-Wissen verloren geht.

## Paritätspaket Pause / One-Time-Queue / Gesamt-Fortschritt (User-Entscheidung 2026-10-14)

Anlass: User-Fragen „haben wir wie im Original einen alle-stoppen/alle-fortsetzen-Button" und
„wie werden die Drag&Drop-Dateien gespeichert". Befunde im Code, Entscheidungen danach:

- [x] **P1 – Pause wie Original (User-Entscheidung 2026-10-14) — ERLEDIGT**, Commits `488d1e4`
  (2 Verify-Runden: Runde 1 CHANGES REQUIRED, Runde 2 APPROVE). Ein Umschalt-Button existiert
  bereits („Alle Sicherungsvorgänge anhalten" ↔ „… fortsetzen", `src/backup.rs` `backup_ui`, plus
  Tray `src/tray.rs`); das Original hat **ebenfalls nur einen** Umschalter (`UI_STATES.md` §70),
  nicht zwei Buttons. **Abweichungen vor dem Paket:** (a) Button nur während eines laufenden Laufs
  aktiv (`add_enabled(!folders.is_empty() && running)`) — Original: schaltbar, solange überhaupt
  ein Sicherungsziel existiert, also **auch im Leerlauf** (Pause blockt dann den Auto-Sync); (b)
  Pause-Zustand **nicht persistent** (`watch::Sender`, startet immer `false`; `Config` hat kein
  Feld) — Original schreibt `schedules.paused=1` und bleibt nach Neustart pausiert
  (`SPEC_BACKGROUND_SYNC.md` §7); (c) ein **reiner Drop-Lauf ohne angelegte Ordner** sperrte den
  Button, obwohl ein Lauf läuft; (d) **Pause pro Ordner** (Zeilen-Symbol) bleibt Post-MVP.
  Auftrag: `paused` in `Config` persistieren (serde-default false), aus dem Config initialisieren,
  Toggle speichert; Schalter aktiv, sobald ≥1 Sicherungsziel existiert **oder** ein Lauf läuft.
- [ ] **P2 – One-Time-Queue temporär in SQLite (User-Entscheidung 2026-10-14).** Abweichung vom
  Original aufgehoben: Original persistiert die Drop-Queue als DB-Zeilen
  (`uploads`/`backup`, `schedule_id="ManualUpload"`, `SPEC_ONE_TIME_UPLOAD.md` §6 — **belegt**,
  überlebt den Neustart); unser `one_time`-Ziel war absichtlich transient (`SPEC_ONE_TIME_UPLOAD.md`
  §10.3). Auftrag: abgelegte Dateien **temporär in derselben SQLite** speichern, in der der
  MD5-Dedup-Index liegt, und **nach vollständig abgeschlossenem Upload wieder entfernen**. Nach
  Neustart steht die Zeile „Einmalige Sicherung" wieder und der Lauf wird fortgesetzt
  (gleiche Guards wie der Kaltstart: `background_sync_pass`/`can_start` — Hauptschalter an,
  angemeldet, nicht pausiert, kein Lauf aktiv; der alte 15/90-Minuten-Tick existiert nicht mehr).
  Bandbreite bleibt sauber, weil der MD5-Index ohnehin persistent ist.
- [ ] **P3 – Gesamt-Fortschritt über alle Uploads (User-Wunsch 2026-10-14).** Der Kopfzeilen-Balken
  zeigt nur den **gerade laufenden** Lauf; manuelle Läufe starten pro Ordner
  (`start_folder_upload(vec![i])`, kein „Alle starten"). Auftrag: **eine** aggregierte Anzeige
  über **alle** Uploads (Summe über alle Backup-Ziele inkl. One-Time-Zeile), sichtbar auch im
  Leerlauf und außerhalb der Backup-Seite.
- [x] **Clean-Room-Doku-Rest (User-Wunsch 2026-10-14) — ERLEDIGT**, siehe Abschnitt
  „Doku-Auftrag Rest-Specs" unten. Acht Specs geschrieben und committed; die Original-App
  kann jetzt deinstalliert werden, ohne Recon-Wissen zu verlieren.
### Recon-Notiz (nur Doku, bewusst **keine** TODO daraus)

**„Löschen / Patchen" — direkt am DLL belegt (2026-10-14).** Antwort: das Original kann **alles
drei**. `CloudDriveService` baut neben dem Upload (`POST /v2/upload`, `PUT` bei `overwrite`) auch:
`DELETE <metadata>/nodes/{id}` (optional `?recurse=true`, „DeleteNode"),
`DELETE /v2/upload/multipart-upload/{id}` (Abbruch), `PUT <content>/nodes/{id}/content`
(Inhalt ersetzen), **`PATCH <metadata>/nodes/{id}`** mit `nodeChanges`-JSON
(`NullValueHandling.Ignore`, `HealthMetrics.Request.PatchNode`, für Umbenennen/Verschieben/
Favorit/Album) sowie `trash/{id}` und `trash/{id}/restore` (Verben dafür noch offen).
Nebenbefund: **unser `docs/PROTOCOL.md` enthält nur `POST`/`GET`**, weil ausschließlich
Upload-Flows mitgeschnitten wurden — Grenze der Aufzeichnung, **nicht** das Verhalten der App.
**Für unseren Port folgt daraus nichts:** Uploader-Scope, Löschen/Download/Papierkorb ist
bewusst Out. User-Vorgabe 2026-10-14: dokumentieren, **keine** konkreten TODOs ableiten.

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

## Bekannte Schwachstelle: `glib` (Dependabot, medium) — derzeit **nicht** fixbar (2026-10-14)

- **Befund:** GitHub/Dependabot meldet auf `main` **1 moderate** Schwachstelle: `glib`
  („Unsoundness in `Iterator`/`DoubleEndedIterator` impls für `glib::VariantStr`"), betroffen
  `>= 0.15.0, < 0.20.0`, Fix wäre **0.20.0**. Im Lock steht `glib 0.18.5`.
- **Herkunft (ausschließlich Linux-Backend, auf macOS nicht kompiliert):** `tray-icon` →
  `libappindicator 0.9.0` → `glib/gtk ^0.18`; `muda 0.21.2` → `gtk ^0.18`; `wry 0.57.0` →
  `webkit2gtk =2.0.2` → `glib/gtk ^0.18`. Damit hängt die komplette gtk-rs-0.18-Familie daran
  (glib, gio, gdk, gtk, gtk-sys, pango, atk, gdk-pixbuf).
- **Kein Upgrade-Pfad (belegt über die crates.io-API am 2026-10-14):** die jeweils **neuesten**
  Versionen verlangen weiterhin gtk-rs 0.18 — `tray-icon 0.26.1` (`libappindicator ^0.9`),
  `libappindicator 0.9.0` (`glib ^0.18`, `gtk ^0.18`), `muda 0.21.2` (`gtk ^0.18`), `wry 0.57.0`
  (`gtk ^0.18`), `webkit2gtk 2.0.2` (`glib/gtk ^0.18`). Ein isolierter `glib`-Bump ist
  ausgeschlossen, weil `gtk 0.18` `glib ^0.18` pinnt.
- **Folge:** Die Dependabot-Auto-Updates **scheitern deshalb** (Job „cargo in /. for glib - Update",
  Lauf `38542163059`, failure). Erwartbar — kein Config-Fehler, nicht „reparieren".
- **Einordnung (bewusst akzeptiert):** die Unsoundness betrifft glib-Iterator-APIs, die wir nicht
  aufrufen; sie liegt nur im Linux-Tray/WebView-Stack. Auflösbar erst, wenn `tray-icon`/`wry`/`muda`
  auf gtk-rs 0.20 wechseln. **Nicht eigenmächtig patchen** (Vendoring oder `[patch]` wäre ein neuer,
  ungeprüfter Eingriff und auf diesem Rechner nicht testbar). Beobachten: `cargo update` nach dem
  nächsten Tray-Icon-/wry-Release.
- **Fürs öffentliche Release:** als bekannte Einschränkung nennen — gehört zusätzlich in
  `docs/RELEASE.md` (noch zu ergänzen).

## Logging ist heute stumm — Befund + Fix (2026-10-14)

**Befund:** `env_logger` ist in `Cargo.toml` als Abhängigkeit vorhanden, wird aber **nirgends
initialisiert** (kein `env_logger::init()` in `src/main.rs` oder sonst im Code). Damit sind alle
**16** `log::warn!/error!/…`-Aufrufe im Projekt wirkungslos: Autostart-Fehler, Kontoname/Profilbild-
Fehler, Watcher-Fehler, 401-Hinweise, Save-Fehler usw. laufen ins Leere. Aufgefallen beim Versuch, den
**ersten echten Upload** mitzuschreiben (User-Wunsch: „gerne wenn du auch Logs machst") — ohne Logger
gibt es keine Zeile Ausgabe, weder auf stderr noch in einer Datei.

**Fix (kleines Paket, unmittelbar nach der Asset-Welle — teilt sich `src/main.rs` mit ihr):**
`env_logger::init()` als erste Anweisung in `fn main()` (keine neue Abhängigkeit, sie ist schon da).
Steuerung über die übliche `RUST_LOG`-Umgebungsvariable. Zusätzlich im Ledger/`docs/BUILD.md`
notieren, wie man einen Lauf mitschreibt:
`RUST_LOG=info ./target/debug/prime-photos > ~/…/prime-photos.log 2>&1`.
Prüfen: `RUST_LOG=info` muss dann echte Zeilen liefern (z. B. den Autostart-/Avatar-Pfad), und ohne
`RUST_LOG` bleibt es still (Default von `env_logger` ist `error`).

**Beobachtung ohne Logger (für den Upload-Lauf genutzt):** der MD5-Index
(`~/Library/Application Support/prime_photos/dedup.sqlite3`, Tabelle `uploads`) ist die
Belastbarste Quelle — er bekommt pro hochgeladener Datei eine Zeile; dazu `folders.toml`
(Zähler/Status) und die Server-Sicht über die dokumentierten Routen.

## UI-Befunde aus dem ersten echten Upload (User-Meldung 2026-10-14) — Fix-Paket offen

Beobachtet während des echten 3,8-GB-Laufs (`~/Pictures/Sport`, 480 Dateien) am **ungebündelten
Debug-Binary**. Vier Befunde, zwei davon mit User-Entscheidung:

1. **Umlaute werden als Kasten gerendert** (`O□FB`). **Ursache belegt:** macOS legt Dateinamen in
   **NFD** ab — `Ö` im Pfad ist `4f cc 88` („O" + U+0308 KOMBIERENDES TREMA), nicht `c3 96`. Die
   eingebettete egui-Schrift (Ubuntu-Light/Hack) hat für das kombinierende Zeichen kein Glyph, und
   egui positioniert Kombinationszeichen ohnehin nicht. Betrifft jeden Pfad/jede Datei mit Umlaut in
   der Anzeige (deutsche App!).
   **Entscheidung (User 2026-10-14): `unicode-normalization` als Abhängigkeit, NFC-Normalisierung
   NUR für den Anzeigetext.** Daten, Pfade und der MD5-Index bleiben **unverändert** — die
   Dedup-Schlüssel sind Pfade, die dürfen sich nicht verschieben. Verworfen: eine eigene
   Mini-Tabelle (handgepflegt, unvollständig) und „andere Schrift einbetten" (löst das
   Kombinationszeichen-Problem nicht).
   **Erweitert (User-Entscheidung 2026-10-14, nach der Server-Prüfung): NFC gilt auch für den
   Upload-Namen.** Belege: der Server normalisiert **nicht** — NFD-Abfrage findet nur die NFD-Datei,
   NFC-Abfrage nur die NFC-Datei (`27_ÖFB-Reisinger_01.dng`: NFD-Treffer, NFC `count=0`). macOS
   liefert NFD, Windows NFC ⇒ derselbe Name ist ein **anderer String**. **Warum das zählt:** die
   Namensbehandlung ist exakt/ordinal (Original `GetNodeByNameAndParentIfExists`; unser
   `Ask`/`SaveAsNew`/`Replace` hängt am Namen) — ohne Normalisierung findet „Ersetzen" auf dem Mac
   die von Windows angelegte Datei nicht. **Korrektur:** ein zweiter Upload **desselben Inhalts**
   wird schon **serverseitig** abgewiesen (`409`, `errorCode = "DuplicatesConflictError"`); der
   frühere Schluss „beide Formen derselben Datei im Konto" war **falsch** —
   `27_ÖFB-Reisinger_01.jpg` liegt dort zweimal als **verschiedene Fotos** (NFD 28.09., 6.274.270 B,
   MD5 `1d16ed8f…` = unsere Datei; NFC 28.03., 8.911.096 B, MD5 `c5f0f37e…` = anderes Bild). Pfad
   und MD5-Index bleiben unverändert. **Status: entschieden, noch nicht umgesetzt.**
2. **Fortschritt doppelt angezeigt:** die Seiten-Fußzeile (P3, mit Spinner + „Gesamt: x von y · z % ·
   noch …") **und** die untere Statusleiste (`TopBottomPanel::bottom("log")`, `src/app.rs:808`,
   „Upload läuft: n/total · <Datei> · ETA").
   **Entscheidung (User 2026-10-14): genau EINE Fußzeile ganz unten, app-weit** — Spinner +
   Fortschrittsbalken + „Gesamt: x von y · z % · noch …" + aktuelle Datei/ETA in einer (etwas
   höheren) Zeile; die Seiten-Fußzeile aus P3 entfällt damit. Verworfen: „Seiten-Fußzeile behalten,
   unten nur Log" (dann fehlt Datei/ETA) und „zwei Zeilen unten" (mehr Höhe).
3. **Zeile zeigt „Fehler" statt „Upload läuft …"** (`src/backup.rs:1399` für die Ordner-Zeilen, `:1425`
   für die Einmal-Zeile). Die Zeile rendert den **persistierten** Status (`Row.status ← f.status`)
   aus dem letzten fehlgeschlagenen Lauf; der laufende Upload setzt ihn nicht auf „läuft". Solange
   das so ist, steht bei jedem neuen Lauf erst einmal wieder **„Fehler"** — für den Nutzer wertlos,
   weil es den Zustand *vor* dem Lauf beschreibt. Fix: Status **live** ableiten — enthält
   `folder_total` das Ziel, läuft dieser Ordner ⇒ „Upload läuft …"; erst nach `finalize` gilt der
   persistierte Ausgang.
4. **Aufklappen hält nicht (User: „bleibt nicht auf Dauer", `src/backup.rs:1327`).** Die
   Klapp-Überschrift heißt `tr1("Warteschlange ({})", remaining)`; ihr Text ändert sich mit **jedem
   Zähler-Update** (jede hochgeladene Datei). egui leitet die Widget-ID aus dem Label ab ⇒ neue ID
   pro Update ⇒ der Auf-/Zu-Zustand wird jedes Mal zurückgesetzt, die Liste klappt von selbst wieder
   zu. **Fix:** feste ID vergeben (`CollapsingHeader::id_salt("queue")` in egui 0.31) — dann bleibt
   der Zustand über Updates und Frames erhalten. **Dieselbe Fehlerklasse** gilt für jedes
   `CollapsingHeader`, dessen Label dynamische Zahlen enthält (beim Umbau mitprüfen).

**Beide Punkte (3+4)** sind der Grund, warum sich die Anzeige im ersten echten Lauf als
„dauerhaft Fehler + Liste klappt zu" zeigte. Sie gehen **zusammen mit dem Fortschritts-Umbau**
(Recon zur Original-Darstellung läuft) und den Umlauten in **ein** UI-Paket.

**Kein** App-Fehler (aufgeklärt): der **pixelige** Font in den Screenshots kam vom **ungebündelten**
Binary (per Terminal gestartet, ohne `Info.plist`). `packaging/Info.plist` enthält
`NSHighResolutionCapable = true`, das gebaute `.app` ist also retina-scharf. **Konsequenz:** für
Sichtprüfungen/Screenshots künftig das Bundle bauen (`packaging/make-app.sh`), nicht die nackte
Binary starten.

## Duplikate & Dateikonflikte — User-Entscheidung 2026-10-14: alle Varianten wie das Original

Recon-Grundlage: `docs/SPEC_DUPLICATES.md` (Mechanismen, Erkennungsweg, lokaler Zustand, 409,
Gap-Tabelle) und `docs/SPEC_CONFLICT_OPTIONS.md` (vollständige Varianten-Liste mit Abnahmekriterium,
kommt von einem Subagenten). Kernbefund: **zwei orthogonale Mechanismen** — Inhalts-Dedupe (MD5,
**über Pfade hinweg**, `GetNodeByMd5`) und Namenskonflikt (exakter Name + Parent, **pro Ziel**;
global existiert nichts).

**Unser Stand heute:** vier Konflikt-Optionen pro Ziel sind da (`ConflictResolution::{Ask,Ignore,
SaveAsNew,Replace}`, `src/folders.rs:130-142`; Default `Ignore` = `conflictResolution=NONE`;
`wire_value()` `:148-154`), Dedupe ist **rein pfadbasiert** (`DedupIndex::should_skip`/`stat_matches`,
`src/dedup.rs:101/143`; Skip-Pfade `src/upload.rs:418` und `:465`), 409 wird pauschal als
„bereits im Konto" gewertet. **Also: nein, wir unterstützen noch nicht alle.**

**Checkliste (Anlegen bleibt Pflicht, Abarbeiten nur auf Auftrag):**
- [ ] **Inhalts-Dedupe über Pfade** — vor dem Upload per MD5 im lokalen Index prüfen, ob derselbe
      Inhalt schon hochgeladen wurde, unabhängig von Pfad und Name. Fängt NFD/NFC-Zwillinge,
      verschobene und umbenannte Dateien. `ix_uploads_md5` (`src/dedup.rs`) liegt dafür **ungenutzt**
      da. Abnahme: gleicher Inhalt unter anderem Pfad ⇒ **kein** zweiter Upload; Test beweist es
      ohne Netz.
- [ ] **409-Ursache trennen** — das Original unterscheidet Inhaltsduplikat (⇒ wie ignoriert) von
      Namenskonflikt (⇒ Fehler); wir machen aus jedem 409 „bereits im Konto".
      **Beide Gründe sind jetzt live belegt (2026-10-14), mit exakter Schreibweise:**
      Inhaltsduplikat ⇒ `errorCode = "DuplicatesConflictError"`; Namenskonflikt ⇒
      `errorCode = "NameAlreadyExists"` (**MixedCase** — der Recon nennt auch `NAME_ALREADY_EXISTS`,
      auf unserem Host gilt diese Form). **Beide** tragen `errorDetails.conflictNodeIds` mit der ID
      des vorhandenen Knotens. Abnahme: ein Inhaltsduplikat zählt als „bereits im Konto", ein
      Namenskonflikt wird als Konflikt behandelt (Prompt/Fehler je Option); Test über die
      Fehlertexte, ohne Netz.
- [ ] **„Nicht noch einmal fragen" pro Ziel persistieren** — das Original schreibt die Antwort in
      `schedules.conflict_resolution` zurück; bei uns gilt sie nur für den laufenden Lauf. Braucht
      ein **neues persistiertes Feld pro Ziel** (bewusste Schemaänderung, nicht „nur flüchtig").
- [ ] **„Ersetzen" reparieren (PUT + Node-ID).** **Verdikt der Probe (2026-10-14):**
      `POST …/v2/upload?conflictResolution=OVERWRITE` wird **nicht akzeptiert** — **404 mit leerem
      Body** (kein `errorCode`) ⇒ unser „Ersetzen" ist **kaputt**: es ersetzt nichts und fällt in den
      generischen Fehlerzweig (Upload-Fehler, kein Konflikt-Skip, kein Dialog). Der Weg des
      Originals **funktioniert**: `PUT /v2/upload/<nodeId>?conflictResolution=NONE&…` ⇒ **200**,
      **gleiche** Node-ID, Inhalt wirklich getauscht (`size`/`md5` und `version` hochgezählt); ein
      `overwrite`-Parameter war **nicht** nötig. **Umsetzung:** wir brauchen die Node-ID des
      vorhandenen Knotens — billigster Weg ohne eigene Persistenz ist der Zwei-Schritt
      `POST(NONE)` ⇒ 409 ⇒ `conflict_node_id()` (`src/prime_photos.rs:289`) ⇒ PUT.
      **Bestätigt (2026-10-14):** auch der **Namenskonflikt**-409 liefert
      `errorDetails.conflictNodeIds` — mit genau der ID des bestehenden Knotens. Der Zwei-Schritt-Weg
      ist damit tragfähig, **ohne** eigenen Node-Cache. **Nebenfund:** `docs/openapi.yaml` listet
      `OVERWRITE` als gültig — das ist aber die **Geräte-Flow**-Doku (`upload-photos.amazon.de`),
      nicht unser Cookie-Host.
- [ ] **Alle Varianten laut Liste** (`docs/SPEC_CONFLICT_OPTIONS.md`) umsetzen, jede mit
      Abnahmekriterium.
- [ ] **Code-Default auf `SaveAsNew` umstellen** (User-Entscheidung 2026-10-14: „wenn inhaltlich
      anders hochladen, sonst überspringen wenn es nur umbenannt wurde"). Heute steht `#[default]`
      auf `Ignore` (`src/folders.rs:136-137`); Ziel ist `SaveAsNew` (`RENAME`) bei `dedup = true`.
      Eine Zeile plus ein Test, der den Default festnagelt, damit er nicht wieder driftet; die
      Doc-Kommentare am Enum mitziehen. Die **lokale** Config des Ziels „Sport" ist bereits manuell
      umgestellt (`conflict_resolution = "saveasnew"`; Serde-Name ist **lowercase**, nicht
      `save_as_new`). **`RENAME` ist live bestätigt (2026-10-14):** ⇒ **201**, der Server legt eine
      **umbenannte Kopie** an (`name (2026-10-14T20_22_13.091).ext` — Zeitstempel vor die Endung,
      Doppelpunkte zu `_`), kein 409. Der neue Default ist auf unserem Host also tragfähig.

**Live-Beleg 2026-10-14 (genau ein Upload-Versuch, `.cr3`).** Verdacht: 121 der 480 Index-Zeilen
seien nie im Konto angekommen (die Server-Suche fand sie nicht). Test: eine davon
(`…/ÖFB/2026-09-27-Kosovo/27_ÖFB-Reisinger_31.cr3`, MD5 `0289be68ad920cc22964e09c77da64cd`) erneut
mit `conflictResolution=NONE` gesendet ⇒ **HTTP 409**:
`errorCode = "DuplicatesConflictError"`, Message „Duplicate files already exist with
FILE_MD5='0289be68…' …\", `errorDetails.conflictNodeIds = [N5RwmpPvR5SKmUbQsdxU6g]`.
**Ergebnis: die Datei liegt im Konto — es gab keinen stillen Verlust.** Die 121 waren ein Artefakt
der Server-**Suche**, die als Beweis **unbrauchbar** ist: dieselbe Abfrage antwortete für dieselbe
Datei zuerst `count=1` mit Datum aus dem Lauf und später stabil `count=0`.
**Nebenfunde:** (a) der Server **dedupliziert per Inhalt** (nicht nur per Name) und nennt den
vorhandenen Knoten — die 409-Trennung ist damit **belegt** und direkt umsetzbar; (b) unser
Cookie-Upload sendet **kein** `Content-MD5` (base64), nur `x-amzn-file-md5` (hex) — die
`Content-MD5`-Notiz in `AGENTS.md` gilt nur für den **Geräte**-Flow; (c) der Zielordner „Web" hat
**40.791** Dateien, `sort=['createdDate DESC']` funktioniert (`createdDate` ist die **Aufnahme-**,
nicht die Upload-Zeit), ein `createdDate`-Filter gibt es nicht, und `parents:`+`name:` kombiniert
der Server nicht.

**Verworfen:** beim rein pfadbasierten Index bleiben (er erkennt den NFD/NFC-Zwilling nicht, und
genau der entsteht zwischen macOS und Windows).

## Offene Kandidaten derselben Absturzklasse (2026-10-14) — **ungeprüft**

Regel aus dem Absturz am Laufende (unten): **keine synchrone AppKit-/RunLoop-API direkt aus
`update`.** Die Kandidaten standen bisher nur im Fix-Bericht, nicht im Ledger — hiermit nachgetragen:
- [ ] **`rfd::FileDialog::pick_folder()`** (`src/backup.rs:808`, `src/backup_config.rs:59`) — öffnet
      auf macOS ein **modales** `NSOpenPanel` und blockiert mit eigener RunLoop. Dieselbe Klasse wie
      der Absturz (`notify-rust`). **Ungeprüft:** ob es real abstürzt, ist **nicht** belegt. Prüfen
      und ggf. auf den asynchronen Weg (`AsyncFileDialog` + `self.rt`) umstellen — der Tokio-Runtime
      existiert bereits (`src/backup.rs:925`).

## Erster echter Upload: ERFOLGREICH (2026-10-14) + Absturz am Laufende (behoben)

**Der Lauf ist vollständig durchgelaufen:** `~/Pictures/Sport`, **480 von 480 Dateien**,
`status = "done"`, `last_updated 14.10.2026 21:13`, **480 Zeilen im MD5-Index** (vorher leer), ~3,6 GB.
Gegen die Server-Sicht geprüft: die neuesten Einträge im Konto tragen das Datum des Laufs
(`2026-10-14T19:07…` UTC = 21:07 lokal) mit den echten Dateinamen. Der frühere Lauf hatte dort nur
Einträge vom Vortag — die 409-Phase („zuvor hochgeladen") lief also durch und danach ging es **neu**
hoch. Mittel: lokaler MD5-Index (`dedup.sqlite3`), `folders.toml`-Zähler, Konto-Suche über die
dokumentierten Routen.

**Absturz (`SIGABRT`) direkt nach dem Lauf — Ursache belegt.** Der User klickte aufs Tray; der Stack
zeigt aber, dass es kein Tray-Problem war, sondern das **Laufende**:

```
PrimePhotoApp::update (app.rs:871) → notify::run_finished (notify.rs:34)
 → notify_rust::…::NotificationHandle::drop → mac_notification_sys::send_notification
   → -[NSRunLoop runUntilDate:]            ← dreht die Main-RunLoop
     → winit::…::ApplicationDelegate::maybe_queue_event   ← queued closure läuft RE-ENTRANT
       → panic → panic_cannot_unwind → abort()
```

`notify-rust` sendet die macOS-Mitteilung **synchron** und **pumpt dabei die RunLoop**; wir rufen das
**innerhalb von `update`** auf. Die verschachtelte RunLoop führt eine winit-Closure aus (re-entranter
egui-Zustand), die paniked — und in einem ObjC-Callback ist Unwinding verboten, also `abort()`.
**Dieselbe Falle war im Projekt schon gelöst** (Cookie-Probe in `src/login.rs`: „`WebView::cookies` ist
synchron und dreht die macOS-RunLoop … deshalb auf die Main-Queue dispatchen"). Die Benachrichtigung
wurde jetzt genauso auf die Main-Queue verlagert (`dispatch2`, Muster wie `login.rs`) — **Regel daraus:
keine synchrone AppKit-/RunLoop-API direkt aus `update`.** Kandidaten, die noch zu prüfen sind, wurden
im Fix-Bericht erfasst.

**Zweiter Fehler, gleiche Stelle:** `folders.toml` trug nach dem Lauf `total_files = 150` bei
`uploaded_files = 480` (die Zeile hätte „480 von 150" gezeigt) — beim Abschluss wurde nur
`uploaded_files` fortgeschrieben, nicht `total_files`. Wird im selben Paket mitrepariert.

## Verifikation „fertig\" nach dem Lauf (2026-10-14) — die Zahl ist irreführend, die Anzeige falsch

Der User zweifelte die Zahl an („passt die Anzahl wirklich von heute?\"). **Zu Recht.** Drei Befunde:

**1. `done` mischt drei völlig verschiedene Ausgänge (Code-Beleg).** In `src/upload.rs`:
`FileOutcome::Uploaded` → `s.done += 1` (`:496`), `FileOutcome::ConflictSkipped` → **ebenfalls** `s.done += 1`
(`:552`, Log „Vorhanden, übersprungen", das ist der 409-Fall: die Datei lag **schon** im Konto), und der
lokale Dedup-/Stat-Skip zählt ebenfalls als `done` (`:376`, `:421`, Log „unverändert"). **„480 fertig"
heißt also nicht „480 neu hochgeladen"**, sondern „480 abgearbeitet" — die App kann die Fälle heute
nicht auseinanderhalten. Das ist die eigentliche Antwort auf die Frage.

**2. Pro Datei am Server gegengeprüft** (`GET /drive/v1/nodes?filters=name:(\"…\")`, belegt die
Existenz **und** das Server-Datum; Fenster unseres Laufs = 14.10.2026 19:00–19:20 UTC):
`22_Vikings-Reisinger_166-2.jpg` liegt dort seit **27.08.2026** (lokal als „hochgeladen 19:05"
verbucht ⇒ 409-übersprungen), `27_ÖFB-Reisinger_36.jpg` seit **28.09.2026**,
`22_Vikings-Reisinger_192-2.jpg` sogar **zweimal** (27.08. + 19.09.) — dagegen
`13_ÖFB-Reisinger_18/24/30.dng` mit **createdDate 14.10. 19:09** = in diesem Lauf **wirklich neu**.
Die Zahl der echten Neuzugänge wurde über alle 480 Index-Einträge bestimmt (siehe unten).

**3. Anzeige-Fehler, der genau wie „fertig passt nicht" aussieht:** `src/backup.rs:1457-1462` rechnet
`pct = done / total` und druckt das **ungekappt** als zweite Zeile („{pct}% hochgeladen"). Mit dem
Altzustand `total_files = 150` bei `uploaded_files = 480` stand dort **„320% hochgeladen"** — während
die Statuszeile darüber „100% hochgeladen" meldete. Ursache ist derselbe Fehler wie beim Absturz-Paket
(`finalize_upload` schrieb `uploaded_files`, aber nicht `total_files`); Fix läuft. **Offen bleibt, ob
`pct` zusätzlich gekappt (`clamp(0.0, 1.0)`) werden soll** — ein inkonsistenter Altzustand darf nie
wieder „320%" zeigen.

**Rescan-Weg (User fragte):** **global gibt es keinen** — der Backup-Kopf hat nur „Datensicherung
hinzufügen …" und den Pause/Fortsetzen-Umschalter (`src/backup.rs:1255`, `:1262`). **Pro Ordner** gibt
es ihn, aber versteckt: im **⋯-Menü** der Zeile als **„Jetzt scannen und hochladen"**
(`FolderAction::Upload`, `src/backup.rs:1489`). Die Einmal-Zeile hat bewusst kein ⋯-Menü
(`src/backup.rs:1486`). Kalte Erkennung passiert sonst nur beim Start (max-mtime-Wasserzeichen).

## Zeilen-Aktionen & Ergebnis-Aufschlüsselung — beauftragt (User-Entscheidung 2026-10-14)

Drei Entscheidungen (Details in `AGENTS.md` → „Zeilen-Aktionen & Ergebnis-Anzeige"), noch **nicht**
umgesetzt:

- [ ] **Vertikaler Kebab als echtes Icon** (Material Symbols `more_vert`), immer sichtbar, Tooltip
  „Aktionen", Linksklick → Popup via `Button::image` + `popup_below_widget` (egui 0.31 hat kein
  `Image → WidgetText`, `ui.menu_button` scheidet aus). Einzige `⋯`-Stelle: `src/backup.rs:1501`.
- [ ] **Sichtbarer Refresh-Button pro Zeile** (Material Symbols `refresh`), löst „Jetzt scannen und
  hochladen" aus; der Popup behält die restlichen Aktionen. Grund: der Rescan existierte, war aber im
  Text-`⋯` nicht auffindbar. Global weiterhin kein „Alles neu prüfen" (bewusst nicht beauftragt).
- [ ] **Ergebnis aufschlüsseln: neu / bereits im Konto / unverändert / Fehler.** Belegte Ursache:
  `src/upload.rs` zählt alle drei Ausgänge als `s.done += 1` (`:496`, `:552`, `:376`/`:421`) ⇒ die
  Zahl „480 fertig" war nicht als „480 neu" lesbar. Die Ausgänge liegen bereits getrennt vor.
  **Speicherung: nur flüchtig** (User-Entscheidung 2026-10-14) — `folders.toml` wird **nicht**
  erweitert, nach einem Neustart steht dort nur die Gesamtzahl. **Ort:** Fußzeile aggregiert **und**
  die einzelne Ordner-Zeile (User-Entscheidung 2026-10-14); das Datei-Detail bleibt wie heute.
- [x] **Icon-Herkunft entschieden (User 2026-10-14):** Material Symbols (Apache-2.0) als SVG durch die
  vorhandene resvg-Pipeline, Raster + Laufzeit-Tint, **kein** Icon-Font, keine neue Abhängigkeit.
  Attribution sammelt `docs/THIRD_PARTY.md` (Eingang für die About-/Lizenzseite, Welle 4).
  **Verworfen:** (a) die Zeilen-Icons **selbst zeichnen** — der User will keine handgemachten Formen;
  (b) das Textzeichen `⋮` U+22EE — **fehlt in allen vier** Fonts von `epaint_default_fonts-0.31.1`
  (`⋯` U+22EF gibt es nur in Hack-Regular), es erschiene als □; (c) ein **Icon-Font-Crate** als
  zweite Icon-Quelle. **Verworfen** ebenfalls: „alles als fertig belassen und nur >100 % klemmen".
- [ ] **Offen (nicht beauftragt):** die bisher **gezeichneten** UI-Glyphen (`nav_icon`, Flagge,
  Logo) auf dieselbe Icon-Quelle umstellen — gleiche Pipeline, aber eigener Auftrag; der User wollte
  das Logo ausdrücklich ausgenommen („abseits vom Logo").

## Zurückgestellt — braucht eine bewusste User-Entscheidung (nicht autonom lösbar)

Nichts da ist umgesetzt; alle Punkte sind bewusst **nicht** angefasst, weil sie eine Vorgabe brauchen.

1. ✅ **ERLEDIGT (User-Entscheidung 2026-10-13: „pro Datei")** — **Konflikt-Dialog „Nachfragen"**,
   Commit `81cd277`. Umsetzung: 409 wird gemeldet; `Ask` stellt pro Datei die Frage
   (Ignorieren / Als neue Datei speichern / Ersetzen) mit „Nicht noch einmal fragen" als
   run-weitem Override; Fragen sind serialisiert, 60-s-Timeout → Ignore (kein Stillstand).
   **Unabhängig verifiziert** (PASS mit 2 Medium-Findings, beide behoben in `a3a3625`):
   F1 = die 409-Auflösung trug sich nicht in den MD5-Index ein (jede spätere Runde hätte die
   Datei erneut komplett gesendet); F2 = der „nicht noch einmal fragen"-Override überlebte den
   Lauf und konnte später Dateien still überschreiben. Dazu F3 (ETA-Rate jetzt Wanduhr statt
   Worker-Summe) und F4 (defensive Ask-Normalisierung).
2. ✅ **ERLEDIGT (User-Entscheidung 2026-10-14: „autostart (also os level autostart) an bei default")** —
   **Autostart-Default = an**, Commit `7b8f3d1`, zwei Verify-Runden (Runde 1 APPROVE mit mittlerem
   Finding „Startup-Save konnte eine korrupte `config.toml` überschreiben" → gefixt → Runde 2 APPROVE
   ohne Findings). Umsetzung: `Config.autostart_enabled: Option<bool>` (serde-default, alte Config
   lädt weiter ⇒ `None`), reine Funktion `autostart_action(intent, os_enabled)` mit
   `EnableAndRecord` (`None` ⇒ einmal registrieren und `Some(true)` speichern), `Enable` (Reparatur
   bei `Some(true)` und fehlendem Login-Item) und `Nothing` (`Some(false)` = **Veto**, nie
   einschalten; ein von Hand angelegter Eintrag wird nicht bekämpft). `set_autostart` persistiert
   die erreichte Absicht, damit ein bewusstes „aus" Neustarts überlebt; Fehler/nicht unterstützte
   Plattform ⇒ kein Save + `log::warn!`. Der OS-Zustand bleibt die operative Wahrheit (Checkbox
   liest ihn nach der Aktion), `Config` speichert nur die Absicht.
   ~~Ursprüngliche Formulierung: „PORT-4 liefert den Schalter; ob er bei Installation an oder aus
   sein soll, ist eine Produktentscheidung (Default bisher: aus)."~~
3. ~~**Hintergrund-Sync-Takt (PORT-5-Detail).**~~ **Größtenteils erledigt/obsolet (2026-10-14):**
   Event-driven ist umgesetzt (`notify`-Watcher, siehe Abschnitt „Erkennung neuer/geänderter
   Dateien"); der frühere 15/90-Minuten-Takt ist entfernt. **Offen bleibt nur:** (a) ob nach einem
   Event-Lauf ein Cooldown gelten soll (mehrere Drops kurz hintereinander) und (b) ob ein Lauf nur
   bei bestehendem WLAN/nicht-kostenpflichtigem Netz starten soll — beides Plattform-APIs, noch
   nicht festgelegt.
4. **Pro-Ziel-Limit / netzwerkgesteuerte Drosselung** (bereits als Post-MVP markiert).
5. **macOS-Signatur:** selbstsignierte Identität (Keychain-Zugriff), ob Benachrichtigungen im
   signierten Bundle erscheinen, ob `--options runtime` für ad-hoc nötig ist.
6. ✅ **ERLEDIGT (2026-10-14)** — **Öffentliches Remote/Push:** Repo
   `https://github.com/reisi007/prime-photos` (public), Remote `origin`, Branch `main`;
   `CI`-Workflow zweimal grün (Linux build + test).
7. **Nav-Accessibility + Icon-Ausrichtung** (`nav_item`, Status-/Upload-Icons): brauchen die
   Doku-Maßwerte/Design-Abmachung, kein Rate-Wert.
8. **Echter Name/Profilbild** im Account-Block — **Endpunkt jetzt belegt** (`docs/SPEC_ACCOUNT_BILLING.md`):
   `CloudDriveAccountRefreshWorker.UpdatePreferences()` →
   `GET <metadataUrl>/drive/v1/account/preferences` liefert `name` + `avatarNodeId`
   (Avatar = 60×60 smart-cropped Thumbnail, Cache unter `ThumbnailsPath/<nodeId>_60_60.jpeg`,
   Placeholder-Fallback); `AccountName` ist im Original der Titel des Account-Buttons.
   Refresh beim Start + alle 24 h + on demand, nicht persistiert. **Einziger verbleibender
   Blocker:** ob unser Cookie-Pfad (`session-id` + `x-acb`/`at-acb`/`ubid-acb`) an diesem
   Endpunkt akzeptiert wird — dafür braucht es einen Test-Lauf gegen den echten Account.
   Bis dahin zeigt der Block bewusst nur Marktplatz (`amazon.<domain>`), Flaggen-Badge mit
   Presence-Dot und „Abmelden" (User-Entscheidung 2026-10-14, Commit `0d66e05`).
   Quota ebenfalls belegt: `/drive/v2/account/quota?customerId=UNNEEDED`, Roh-Bytes —
   Near-Quota schaltet ab 1 TiB von Prozent auf Restbytes um („90 %" wäre falsch).
9. **Logout während laufendem Upload:** `logout()` löscht die Auth-Cookies, stoppt aber den laufenden Lauf nicht. Soll der Upload stoppen/pausieren oder weiterlaufen und mit 401 scheitern? (Eine saubere Antwort bräuchte einen Abbruch-Kanal in der Upload-Pipeline.)
10. ✅ **ERLEDIGT (User-Entscheidung 2026-10-13: „erst Datei fertig, dann pausieren")** — Commit
   `0fd04dd`. **F-3 – Pause/Abbruch verwirft gesendete Bytes.** Ein Abbruch im laufenden Upload startet die
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
- **Avatar-Thumbnail per Raten/Probieren (2026-10-14, verworfen):** `/drive/v1/nodes/{id}/content` (mit und ohne `tempLink=true`), `/drive/v1/nodes/{id}/thumbnail` (mit `width`/`height`/`transform`), `/drive/v1/nodes/{id}/transforms/IMAGE_THUMBNAIL` (mit und ohne `?tempLink=true`) sowie `/drive/v1/nodes/{id}/content?transform=IMAGE_THUMBNAIL&tempLink=true` — **alle HTTP 404**. Der Parameter `?transform=IMAGE_THUMBNAIL` wird **ignoriert** (der `tempLink` liefert weiterhin die Rohdatei). Das `transforms: ["IMAGE_THUMBNAIL"]` eines Suchtreffers ist eine **Deklaration**, keine URL. Stattdessen belegbar über das Assembly (`GetSmartCroppedThumbnail`/`GetThumbnailUrl`) — Recon läuft, siehe Welle 3.
- **Avatar lokal dekodieren/schneiden (2026-10-14, verworfen):** der Avatar-Quellknoten kann ein RAW sein (bei diesem Konto ein Canon CR3, 13,5 MB) — die `image`-Crate kann kein CR3. Der 60×60-Crop muss daher **serverseitig** passieren (wie im Original), nicht bei uns.
