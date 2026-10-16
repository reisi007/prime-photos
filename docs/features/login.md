# Sign-in

Prime Photos signs in with **its own device flow** (PKCE against Amazon's auth
endpoints), not by scraping a browser session. The sign-in page opens **inside
the app** in an embedded web view — same window, no external browser, no CDP.

## How it works

- A country/marketplace selector picks the TLD (`amazon.de`, `amazon.com`, …);
  on first run it is pre-filled from the system locale.
- The embedded `WKWebView` child view hosts Amazon's sign-in page directly
  with the original client's parameter set (`openid.assoc_handle`,
  `pageId = amzn_photos_desktop_<region>`, PKCE S256 challenge,
  `openid.return_to=https://www.<domain>/ap/maplanding`, …).
- While Amazon's page is open, a slim branded header (our upload logo +
  "Prime Photos" + "Abbrechen") sits above the web view and the window title
  reads "Login – Prime Photos". This is a deliberate deviation from the
  measured original: the app is a third-party tool, not Amazon's own, so its
  identity and a way to cancel must stay reachable.
- Every time the sign-in view opens, its browsing data is **cleared first**.
  Without that wipe, Amazon's browser cookies survived a sign-out and let the
  next sign-in skip two-factor authentication.
- Success is detected via `openid.oa2.authorization_code` in an `/ap/…`
  navigation (the navigation itself is discarded). The code is exchanged at
  `POST https://api.amazon.com/auth/register` for an access token and a
  refresh token.
- Only the **refresh token** is stored — in the OS keychain (macOS Keychain /
  Windows Credential Manager / Linux Secret Service), under our own service
  name. The access token and the four session cookies live **in memory only**;
  nothing session-related is written to `config.toml`.
- The app **starts straight into the main UI** when a refresh token is stored.
  Access token and cookies are renewed in the background; the sign-in screen
  is only shown when there is no stored session.
- An invalidated session clears the tokens and returns to the sign-in screen —
  no app restart required.

## Decisions

- **Name/Rename (user decision 2026-10-13):** Project, git repo and app are
  **"Prime Photos"**. Cargo package/binary `prime-photos`; upload module
  `src/prime_photos.rs` with struct `PrimePhotos`; config dir
  `~/Library/Application Support/prime_photos/` (was `amzn_photos_uploader`);
  the project directory is renamed to `prime-photos`. The old config folder
  is migrated **once, locally, by hand** (no migration code in the app).

- **GUI framework:** egui/eframe (native ARM, single binary).

- **Auth (GUI):** Embedded WKWebView login, **in the same window as the app**
  (like the original). egui's window hosts a wry `WKWebView` child view; no
  extra window, no external browser, no CDP. Amazon's sign-in page is loaded
  directly (`/ap/signin?…` with `assoc_handle=amzn_photos_web_<cc>`).
  Cookie capture must **not** run inside egui's `update`: wry's
  `WebView::cookies` is synchronous and spins the macOS run loop, which
  re-enters winit's event handler and panics — so probes are dispatched to the
  main queue and the result is picked up on the next frame. A
  country/marketplace selector picks the TLD (default from system locale on
  first run).

- **Auth (login chrome):** Deliberate deviation from the measured original
  (`../original/UI_FLOWS.md` §2.1). While Amazon's page is open, a slim
  branded header (our upload logo + "Prime Photos" + "Abbrechen") sits above
  the web view and the window title reads "Login – Prime Photos". Reason: the
  app is a third-party tool, not Amazon's own, so its identity and a way to
  cancel must stay reachable. The measured window sizes stay the benchmark for
  the main app window.

- **Sitzung wie das Original — Login einmal, dann Refresh-Token (User-Entscheidung
  2026-10-15):** „Ziel wäre so gut es geht so wie im Original mit der Anmeldung /
  Update der Tokens zu verfahren" und „Ziel: nur einmal Anmeldung nötig, refresh
  token speichern. Nachbau Original hier mit Rea clean room spec für Login /
  auth. Gerne aber unsere aktuelle Login Ui so gut wie es geht beibehalten."
  Damit ist der reine Web-/Cookie-Weg **nicht mehr das Ziel** — der Rückfall
  wurde am 2026-10-15 ganz **entfernt** (siehe „Kein Cookie-Rückfall mehr"
  unten). Clean-Room-Grundlage des Nachbaus:
  `../original/SPEC_LOGIN.md` § 3.8 und `../protocol/PROTOCOL.md` § 2.1
  (Recon 2026-10-15 am DLL).

  1. **Anmelden — unsere UI bleibt:** das eingebettete Webview mit unserem
     Header, „Abbrechen", der Marktplatz-Auswahl und dem Fenstertitel „Login –
     Prime Photos" **bleibt**; geändert wird nur die **URL** auf den
     Parametersatz des Originals: `openid.assoc_handle` und `pageId` =
     `amzn_photos_desktop_<region>` (gb→uk), `openid.oa2.scope=device_auth_access`,
     `openid.oa2.response_type=code`, `openid.oa2.code_challenge_method=S256`
     + `openid.oa2.code_challenge` (S256 eines frischen 32-Byte-Verifiers),
     `openid.oa2.client_id=device:<hex aus „<deviceSerial>#<deviceTypeId>">` (lowercased),
     `openid.return_to=https://www.<domain>/ap/maplanding`,
     `openid.pape.max_auth_age=0`, `marketPlaceId`, `language`,
     `forceMobileLayout=true`.
  2. **Erfolgserkennung:** nicht mehr „Cookies sind da", sondern
     `openid.oa2.authorization_code` in einer `/ap/…`-Navigation (die
     Navigation selbst wird verworfen — wie im Original). Die Marktplatz-Wahl
     aus dem Cookie-Suffix (`x-acb<cc>`) entfällt damit als Nebenwirkung.
  3. **Einlösen:** `POST https://api.amazon.com/auth/register` mit
     `registration_data` (app_name/version, device_type/serial/model,
     os_version) und `auth_data` (`use_global_authentication=false`,
     `authorization_code`, `code_verifier`, `code_algorithm="SHA-256"`,
     `client_id` **ohne** `device:`-Präfix, `client_domain="DeviceLegacy"`)
     ⇒ `tokens.bearer.access_token` + `refresh_token`; ein leerer
     Access-Token gilt als Fehlschlag.
  4. **Ablage:** der **Refresh-Token** liegt im **OS-Schlüsselbund** — macOS
     Keychain mit **eigenem** Service/Account, **nie**
     `com.amazon.clouddrive.mac`/`amzp-rt` (das ist der Schlüssel des
     Originals); Windows Credential Manager / Linux Secret Service über eine
     cross-platform Crate. Der Access-Token **und** die vier Session-Cookies
     (`session-id`, `x-acb*`, `at-acb*`, `ubid-acb*`) leben **nur im
     Speicher** — die `Config`-Cookie-Felder sind `#[serde(skip)]` und
     stehen nicht mehr in `config.toml` (Kehrtwende zu S-B, User-Entscheidung
     2026-10-15: Klartext-Cookies sind dieselbe Autorität wie der
     Refresh-Token, nur ungeschützt; das Original hält sie ebenso nur im
     Speicher, `NSHttpCookieStorage`). **Kein** Klartext-Sessionmaterial in
     `config.toml`.
  5. **Ungültiger Refresh-Token:** Tokens löschen und zurück auf unseren
     Login-Screen — **kein** App-Neustart (den braucht nur das Original für
     seinen Startpfad).

- **Login-Start leert den Webview-Speicher (User-Entscheidung 2026-10-15):**
  bei **jedem** Öffnen des Login-Webviews wird der Cookie-/Cache-Speicher des
  eingebauten WKWebView **vor** dem Laden der Anmeldeseite geleert — wie das
  Original (`../original/SPEC_LOGIN.md` § 3.3). Umsetzung: `WebViewBuilder`
  **ohne** `with_url` bauen, `WebView::clear_all_browsing_data()` rufen,
  dann `WebView::load_url(url)`; der `Err`-Pfad bleibt.
  **Verworfen:** `with_incognito(true)` (nonPersistent DataStore) und ein
  benannter DataStore (`with_data_store_identifier` + `remove_data_store`).
  Hintergrund: „Abmelden" löscht nur **unser** Sessionmaterial — ohne diesen
  Wipe überlebt Amazons Browser-Sitzung und lässt die nächste Anmeldung **ohne
  2FA** durchlaufen. (Zeilen-Icons in der Zeile: Kebab/Refresh/Pause sind
  durchgängig **ohne** Rahmen; das Kebab-Popup bekommt eine Mindestbreite von
  220 px und kein Zeilenumbruch der Menüeinträge.)

- **Start-Gate (User-Entscheidung 2026-10-15):** mit gespeichertem
  Refresh-Token startet die App **direkt in die Haupt-UI** — **kein**
  Anmelde-Screen; Access-Token und Cookies werden im Hintergrund geholt (wie
  das Original, das das Auth-Portal nur ohne Refresh-Token zeigt). **Einlöse-Pfad:**
  `/auth/register` bekommt die belegten **3** Versuche
  (`REGISTER_MAX_ATTEMPTS`); ein Netzwerkfehler zählt auch hier nicht als
  Versuch. Der `authorization_code` ist **einmalig** — der Pfad wiederholt
  **nie** nach einer Serverantwort, **nur** wenn der Request den Server nicht
  erreicht hat.

- **Geräte-Identität (User-Entscheidung 2026-10-15, nach der Suche):** Wir
  senden **die Werte, die das Original aktuell sendet**: `app_name = "Amazon
  Photos"`, `app_version = "11.3.0.1d7941b5"` (Stand 2026-10-15),
  `device_type = "A2UXDCSFQXA1M7"`. Die Suche hat belegt, dass im Bundle
  **genau ein** Gerätetyp existiert (keine zweite Wahl), der User hat danach
  entschieden: „aber sonst den aktuellen nehmen". **Nur der `device_serial` ist
  unserer** — 32 Zeichen, einmalig lokal erzeugt, in `config.toml` gespeichert.
  Diese Werte stehen **ausschließlich** im Anfragekörper des Geräte-Wegs; UI,
  Fenstertitel, Bundle-Name und Branding bleiben unberührt (Projektregel
  „Name & Branding"): wir geben uns nicht als Amazon-App aus, sondern sprechen
  auf der Leitung das Protokoll dieses Clients.

- **Grenze („so gut es geht"):** wir treten als **eigenes** Gerät mit eigener
  `client_id` auf. Ein abgelehnter Device-Weg wird als **klare Meldung**
  gezeigt, nicht still verschluckt.

- **Kein Cookie-Rückfall mehr (User-Entscheidung 2026-10-15):** Der Cookie-**Probe**
  und der Web-/Cookie-Rückfall sind **entfernt**; der Geräteweg (PKCE) ist der
  **einzige** Anmeldeweg, ein abgelehnter Weg erscheint als klare Meldung.
  Anlass: `wry::WebView::cookies()` blockiert auf macOS den Hauptthread bis zu
  **1 s** (`wait_for_blocking_operation`, wry 0.57) und läuft aus einem
  `DispatchQueue::main`-Block **immer** in den Timeout — die Login-UI fror
  ein und zeigte „Cookies konnten nicht gelesen werden: IO error: timed out
  waiting for cookies response". Da der Geräteweg live akzeptiert wird und der
  Rückfall ohnehin nur sitzungsweit wäre (Cookies werden nicht persistiert),
  entfällt er. Mit ihm entfallen der Browser-Link „Amazon Photos im Browser
  öffnen" (`urls::login_url`, `platform::open_url`) und die zugehörigen
  Texte. **Verworfen:** den Rückfall asynchron über `objc2` am `WKWebView`
  neu zu bauen (mehr Aufwand, kein Bedarf).

- **Flow note:** this is the **web** flow (cookie-based). The desktop client's
  **device** flow (`amzn_photos_desktop_*` signin -> `/auth/register` ->
  refresh token -> `/auth/token` ->
  `upload-photos.amazon.de/v2/upload`) is documented in
  `../protocol/openapi.yaml` but not used.

- **Account/avatar block in the sidebar:** when signed in, show the Amazon
  profile picture (no API identified yet — placeholder until then) plus
  "Angemeldet als:" und "Abmelden"; when **not** signed in, an icon replaces the
  picture. The green/red status color coding is kept deliberately (user likes
  it).

## Original reference

- `../original/SPEC_LOGIN.md` — Login-/Auth-Flow of the original (device flow, PKCE, token).
- `../original/UI_FLOWS.md` §2.1 — Login UI flow of the original.
