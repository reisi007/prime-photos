# Prime Photos

Cross-platform Rust desktop app (egui) for uploading photos to Amazon Photos.
Primärtarget ist **macOS (Apple Silicon)**; **Windows und Linux sind vollwertige Ziele**
(User-Entscheidung 2026-10-13: „Jetzt voll cross-platform portieren" — Kehrtwende vom
früheren reinen macOS-Ziel, siehe „Plattform / Portierung").

## Source of truth

- GUI: `src/main.rs` (egui/eframe), theme in `src/theme.rs`
- Upload engine: `src/prime_photos.rs` (`PrimePhotos`, cookie-based upload API)
- Persistent backup folders: `src/folders.rs` -> `~/Library/Application Support/prime_photos/folders.toml`
- Login: `src/login.rs` (embedded wry WKWebView as a child view of the main window; cookies captured with an asynchronous main-queue probe)
- Config: `src/config.rs`, stored at `~/Library/Application Support/prime_photos/config.toml`
- Protocol notes: `docs/PROTOCOL.md`

## Projektregeln

- **Name & Branding (Projektregel, 2026-10-13):** Projekt und git-Repo heißen **„Prime Photos"**.
  Nichts im Projekt darf den Eindruck erwecken, es sei eine offizielle App von Amazon: kein
  „Amazon Photos" als *unsere* App-/Produkt-/Bundle-/Wortmarken-Identität, keine Amazon-Logos oder
  Markenauftritte in der Chrome/UI des eigenen Fensters. **Zulässig bleibt alles funktional
  Notwendige:** echte Hosts/URLs (`amazon.de`, `www.amazon.de`, `amazonaws.com`, `upload-photos…`),
  Header-/Cookie-Namen (`x-amzn-*`), und Hinweise wie „Bei Amazon anmelden"/„Mit Amazon anmelden"
  (dort wird tatsächlich angemeldet). In Docs, die die Original-App beschreiben, bleibt die
  faktische Nennung des Referenz-Produkts erlaubt (das ist Recon eines echten Produkts); nur
  **unser** Projekt/Code/UI darf sich nirgends als offizielle Amazon-App darstellen.
  **Folge für den Account-Block (2026-10-16):** die Zeile „Amazon Photos" ist dort seit `0d66e05`
  durch „Marktplatz / amazon.<domain>" ersetzt. Der **Login-Untertitel** „Fotos & Videos zu Amazon
  Photos hochladen" **bleibt** — er nennt das Ziel faktisch (wie „Bei Amazon anmelden") und
  beschreibt, wohin unsere App hochlädt, nicht wer sie ist.
- **Marktplatz-URLs zentral (Projektregel, User-Entscheidung 2026-10-14):** Jede vom **Marktplatz**
  (oder der Zone) abhängige URL wird **ausschließlich in `src/urls.rs`** gebaut. Außerhalb dieser Datei
  darf **kein** `format!("https://www.amazon.{}…")`, **kein** `thumbnails-photos.amazon.`-Literal und
  **kein** `content-{zone}`-Literal stehen (Tests gehören in die Util). Anlass: die Umsetzung des
  Profilbilds hatte den Thumbnail-Host auf `.de` hartkodiert — das fiel erst in der Verifikation auf,
  weil URLs über mehrere Dateien verstreut waren. Die Land→Domain-Zuordnung (`amazon_domain`, heute
  `src/login.rs`) zieht in die Util um; `login.rs`, `prime_photos.rs` und `app.rs` beziehen sie von
  dort. **Jede neue Route:** Util-Funktion + Test, der mindestens `de` **und** einen Nicht-DE-Marktplatz
  prüft. Marktplatzneutral heißt: kein Literal-TLD im Code, die Ableitung passiert nur in der Util.
- **Subagenten führen nichts Neues ein (Projektregel, 2026-10-13):** Jeder Auftrag ist strikt auf die
  beauftragte Änderung begrenzt — keine zusätzlichen Features, Dateien, Abhängigkeiten, Refactorings
  oder „Verbesserungen" auf Verdacht. Unklarheiten werden zurückgemeldet, nicht eigenmächtig gelöst.

## Decisions

- Name/Rename (user decision 2026-10-13): project, git repo and app are **"Prime Photos"**. Cargo package/binary `prime-photos`; upload module `src/prime_photos.rs` with struct `PrimePhotos`; config dir `~/Library/Application Support/prime_photos/` (was `amzn_photos_uploader`); the project directory is renamed to `prime-photos`. The old config folder is migrated **once, locally, by hand** (no migration code in the app).
- GUI framework: egui/eframe (native ARM, single binary).
- Auth (GUI): embedded WKWebView login, **in the same window as the app** (like the original). egui's window hosts a wry `WKWebView` child view; no extra window, no external browser, no CDP. Amazon's sign-in page is loaded directly (`/ap/signin?…` with `assoc_handle=amzn_photos_web_<cc>`). Cookie capture must **not** run inside egui's `update`: wry's `WebView::cookies` is synchronous and spins the macOS run loop, which re-enters winit's event handler and panics — so probes are dispatched to the main queue and the result is picked up on the next frame. A country/marketplace selector picks the TLD (default from system locale on first run).
- Auth (login chrome): deliberate deviation from the measured original (`docs/UI_FLOWS.md` §2.1). While Amazon's page is open, a slim branded header (our upload logo + "Prime Photos" + "Abbrechen") sits above the webview and the window title reads "Login – Prime Photos". Reason: the app is a third-party tool, not Amazon's own, so its identity and a way to cancel must stay reachable. The measured window sizes stay the benchmark for the main app window.
- **Keine technischen Fehlermeldungen im Endnutzer-UI (User-Regel 2026-10-15):** Im sichtbaren UI (v. a. im Login-Header) erscheinen nur **verständliche** Meldungen; technische Details (`IO error: …`, wry-Fehler, URLs, Fehlercodes) gehen ins Log (`log::warn!`), **nie** in die Oberfläche. Anlass: „Cookies konnten nicht gelesen werden: IO error: timed out waiting for cookies response" stand im Login-Header. Auch der Header-Rest (Logo + „Prime Photos" + Untertitel) wird vertikal sauber zentriert.
- Auth (upload engine): browser cookies (session-id + x-acb/at-acb/ubid-acb) -> `content-<zone>.drive.amazonaws.com/v2/upload`.
- Flow note: this is the **web** flow (cookie-based). The desktop client's **device** flow (`amzn_photos_desktop_*` signin -> `/auth/register` -> refresh token -> `/auth/token` -> `upload-photos.amazon.de/v2/upload`) is documented in `docs/openapi.yaml` but not used.
- **Sitzung wie das Original — Login einmal, dann Refresh-Token (User-Entscheidung 2026-10-15):** „Ziel wäre so gut es geht so wie im Original mit der Anmeldung / Update der Tokens zu verfahren" und „Ziel: nur einmal Anmeldung nötig, refresh token speichern. Nachbau Original hier mit Rea clean room spec für Login / auth. Gerne aber unsere aktuelle Login Ui so gut wie es geht beibehalten." Damit ist der reine Web-/Cookie-Weg **nicht mehr das Ziel** — der Rückfall wurde am 2026-10-15 ganz **entfernt** (siehe „Kein Cookie-Rückfall mehr" unten). Clean-Room-Grundlage des Nachbaus: `docs/SPEC_LOGIN.md` § 3.8 und `docs/PROTOCOL.md` § 2.1 (Recon 2026-10-15 am DLL).
  1. **Anmelden — unsere UI bleibt:** das eingebettete Webview mit unserem Header, „Abbrechen", der Marktplatz-Auswahl und dem Fenstertitel „Login – Prime Photos" **bleibt**; geändert wird nur die **URL** auf den Parametersatz des Originals: `openid.assoc_handle` und `pageId` = `amzn_photos_desktop_<region>` (gb→uk), `openid.oa2.scope=device_auth_access`, `openid.oa2.response_type=code`, `openid.oa2.code_challenge_method=S256` + `openid.oa2.code_challenge` (S256 eines frischen 32-Byte-Verifiers), `openid.oa2.client_id=device:<hex aus „<deviceSerial>#<deviceTypeId>">` (lowercased), `openid.return_to=https://www.<domain>/ap/maplanding`, `openid.pape.max_auth_age=0`, `marketPlaceId`, `language`, `forceMobileLayout=true`.
  2. **Erfolgserkennung:** nicht mehr „Cookies sind da", sondern `openid.oa2.authorization_code` in einer `/ap/…`-Navigation (die Navigation selbst wird verworfen — wie im Original). Die Marktplatz-Wahl aus dem Cookie-Suffix (`x-acb<cc>`) entfällt damit als Nebenwirkung.
  3. **Einlösen:** `POST https://api.amazon.com/auth/register` mit `registration_data` (app_name/version, device_type/serial/model, os_version) und `auth_data` (`use_global_authentication=false`, `authorization_code`, `code_verifier`, `code_algorithm="SHA-256"`, `client_id` **ohne** `device:`-Präfix, `client_domain="DeviceLegacy"`) ⇒ `tokens.bearer.access_token` + `refresh_token`; ein leerer Access-Token gilt als Fehlschlag.
  4. **Ablage:** der **Refresh-Token** liegt im **OS-Schlüsselbund** — macOS Keychain mit **eigenem** Service/Account, **nie** `com.amazon.clouddrive.mac`/`amzp-rt` (das ist der Schlüssel des Originals); Windows Credential Manager / Linux Secret Service über eine cross-platform Crate. Der Access-Token **und** die vier Session-Cookies (`session-id`, `x-acb*`, `at-acb*`, `ubid-acb*`) leben **nur im Speicher** — die `Config`-Cookie-Felder sind `#[serde(skip)]` und stehen nicht mehr in `config.toml` (Kehrtwende zu S-B, User-Entscheidung 2026-10-15: Klartext-Cookies sind dieselbe Autorität wie der Refresh-Token, nur ungeschützt; das Original hält sie ebenso nur im Speicher, `NSHttpCookieStorage`). **Kein** Klartext-Sessionmaterial in `config.toml`.
  5. **Auffrischen:** bei **80 % der Restlaufzeit** (der Ablauf ist `expires_in` − 60 s) durch einen **Hintergrund-Worker** auf der bestehenden Tokio-Runtime (3-s-Tick wie `TickTockTimer`, bis zu **5** Versuche im **15-s**-Abstand — die belegten `AppSettings`-Werte). **Offline zählt nicht** (bewusste Abweichung vom Original): ein Verbindungsfehler (`AuthError::Network`) verbraucht **keinen** Versuch, der Termin bleibt, der nächste Tick probiert erneut — **offline meldet nie ab**. Fataler Ausgang = Sitzung ungültig.
  6. **Web-Cookies erneuern:** `POST https://api.amazon.com/auth/token?source_token=<refresh>&source_token_type=refresh_token&requested_token_type=auth_cookies&domain=.<tld>&app_name=…&app_version=…` — **beim Start und nach jeder erfolgreichen Auffrischung** aus dem Refresh-Token gemintet und in die bestehende Cookie-Kette (im Speicher) eingespeist; die **Upload-Engine bleibt cookie-basiert**, die live belegte Kette aus Welle 2 bleibt damit gültig.
  7. **Ungültiger Refresh-Token:** Tokens löschen und zurück auf unseren Login-Screen — **kein** App-Neustart (den braucht nur das Original für seinen Startpfad).
  8. **Abmelden:** `POST https://api.amazon.com/auth/deregister` mit dem Access-Token, danach die Tokens löschen.
  **S-C-Umsetzung:** der Auffrischungs-Worker läuft als **ein** langlebiger Task auf der bestehenden Tokio-Runtime (Muster wie `avatar_fetch`), Ergebnis über einen geteilten Slot, den die UI pro Frame pollt. **Folge der Speicher-only-Regel:** der reine Cookie-Rückfall (Geräteweg abgelehnt) gilt nur für die **laufende** Sitzung, nicht über einen Neustart — der Geräteweg ist der persistente Pfad.
  **Login-Start leert den Webview-Speicher (User-Entscheidung 2026-10-15):** bei **jedem** Öffnen des
  Login-Webviews wird der Cookie-/Cache-Speicher des eingebauten WKWebView **vor** dem Laden der
  Anmeldeseite geleert — wie das Original (`docs/SPEC_LOGIN.md` § 3.3). Umsetzung: `WebViewBuilder`
  **ohne** `with_url` bauen, `WebView::clear_all_browsing_data()` rufen, dann `WebView::load_url(url)`;
  der `Err`-Pfad bleibt. **Verworfen:** `with_incognito(true)` (nonPersistent DataStore) und ein
  benannter DataStore (`with_data_store_identifier` + `remove_data_store`). Hintergrund: „Abmelden"
  löscht nur **unser** Sessionmaterial — ohne diesen Wipe überlebt Amazons Browser-Sitzung und lässt
  die nächste Anmeldung **ohne 2FA** durchlaufen. (Zeilen-Icons in der Zeile: Kebab/Refresh/Pause
  sind durchgängig **ohne** Rahmen; das Kebab-Popup bekommt eine Mindestbreite von 220 px und kein
  Zeilenumbruch der Menüeinträge.)
  **Start-Gate (User-Entscheidung 2026-10-15):** mit gespeichertem Refresh-Token startet die App **direkt in die Haupt-UI** — **kein** Anmelde-Screen; Access-Token und Cookies werden im Hintergrund geholt (wie das Original, das das Auth-Portal nur ohne Refresh-Token zeigt). **Einlöse-Pfad:** `/auth/register` bekommt die belegten **3** Versuche (`REGISTER_MAX_ATTEMPTS`); ein Netzwerkfehler zählt auch hier nicht als Versuch. Der `authorization_code` ist **einmalig** — der Pfad wiederholt **nie** nach einer Serverantwort, **nur** wenn der Request den Server nicht erreicht hat.
  **Geräte-Identität (User-Entscheidung 2026-10-15, nach der Suche):** Wir senden **die Werte, die
  das Original aktuell sendet**: `app_name = "Amazon Photos"`, `app_version = "11.3.0.1d7941b5"`
  (Stand 2026-10-15), `device_type = "A2UXDCSFQXA1M7"`. Die Suche hat belegt, dass im Bundle **genau
  ein** Gerätetyp existiert (keine zweite Wahl), der User hat danach entschieden: „aber sonst den
  aktuellen nehmen". **Nur der `device_serial` ist unserer** — 32 Zeichen, einmalig lokal erzeugt,
  in `config.toml` gespeichert. Diese Werte stehen **ausschließlich** im Anfragekörper des
  Geräte-Wegs; UI, Fenstertitel, Bundle-Name und Branding bleiben unberührt (Projektregel
  „Name & Branding"): wir geben uns nicht als Amazon-App aus, sondern sprechen auf der Leitung das
  Protokoll dieses Clients.
  **Grenze („so gut es geht"):** wir treten als **eigenes** Gerät mit eigener `client_id` auf. Ein abgelehnter Device-Weg wird als **klare Meldung** gezeigt, nicht still verschluckt.
- **Kein Cookie-Rückfall mehr (User-Entscheidung 2026-10-15):** Der Cookie-**Probe** und der Web-/Cookie-Rückfall sind **entfernt**; der Geräteweg (PKCE) ist der **einzige** Anmeldeweg, ein abgelehnter Weg erscheint als klare Meldung. Anlass: `wry::WebView::cookies()` blockiert auf macOS den Hauptthread bis zu **1 s** (`wait_for_blocking_operation`, wry 0.57) und läuft aus einem `DispatchQueue::main`-Block **immer** in den Timeout — die Login-UI fror ein und zeigte „Cookies konnten nicht gelesen werden: IO error: timed out waiting for cookies response". Da der Geräteweg live akzeptiert wird und der Rückfall ohnehin nur sitzungsweit wäre (Cookies werden nicht persistiert), entfällt er. Mit ihm entfallen der Browser-Link „Amazon Photos im Browser öffnen" (`urls::login_url`, `platform::open_url`) und die zugehörigen Texte. **Verworfen:** den Rückfall asynchron über `objc2` am `WKWebView` neu zu bauen (mehr Aufwand, kein Bedarf).
- **`expires_in` kommt als JSON-String** (`"3600"`): der Parser akzeptiert String **und** Zahl (Register **und** Refresh).
- UI reference: `docs/UI_FLOWS.md` + `docs/UI_STATES.md` (clean-room spec from the decompiled app + resources). Use them when changing the UI.
- **Panel-Reihenfolge in `App::update` (Befund 2026-10-16):** `SidePanel::left("nav")` → `TopBottomPanel::bottom("log")` → `CentralPanel` (egui-Standard: **Panels zuerst**, `CentralPanel` zuletzt). **Nicht zurückdrehen.** Stand das `CentralPanel` davor, schließt der Zentralbereich den Log-Streifen **nicht** aus: `ui.available_height()` ist um die Panelhöhe zu groß, und jede Seite, die ihre Höhe füllt (die Warteschlange tut das seit Welle 3a), endet hinter dem Protokoll-Panel — headless gemessen **+26 px** Überlapp bei 760×520 und 1000×700, nach der Umstellung **−8 px**. Die Einstellungs-Seite hängt nicht an `available_height`/`screen_rect` und ist unberührt.
- **Virtuelle Liste (Befund 2026-10-16):** `ScrollArea::show_rows(ui, row_h, n, …)` bekommt den **echten** Zeilen-Vorschub **ohne** `item_spacing`. In einem flachen `ui.horizontal` stimmt das nicht: das Frame ist `interact_size.y` hoch (Theme 26), `add_sized`-Kinder werden darin **zentriert** (Vorschub 31), ein **nacktes** `ui.label` expandiert auf das volle Frame (Vorschub 34). Richtig ist `ui.allocate_ui_with_layout(vec2(w, row_h), left_to_right(Center), …)` für **jede** Zeilenart — dann ist der Vorschub exakt `row_h + item_spacing.y` (in der Warteschlange 28 = 20 + 8) und die deklarierte Höhe stimmt mit der Ist-Höhe überein (vorher Δ 252 px ⇒ letzte Zeile abgeschnitten).
- Folder reconciliation (observed in the client's local DBs under `~/Library/Application Support/Amazon Cloud Drive`): local SQLite state, not an HTTP call. `backup` table (schedule_path, relative_path, node_id, size, modified_date, status, last_scan_time) + `uploads` table (path, status, precomputed_md5, node_id) + `cloud.db` node cache, synced via `/drive/v1/changes` and `/drive/v1/nodes`. `BackupScanIntervalInHours = 336`.
- Desktop-client upload protocol (observed): `POST https://upload-photos.amazon.de/v2/upload?name=<urlenc>&fileSize=<n>&conflictResolution=NONE&kind=FILE&parentNodeId=<id>` with raw body, `x-amz-access-token`, `x-amzn-file-md5` (hex), `Content-MD5` (base64), `x-amz-clouddrive-appid`, `x-amzn-clouddrive-source`. Not used by the app yet (we use the cookie path); documented for parity.
- Upload response (cookie path, live observed 2026-10-15): a successful upload returns JSON whose **metadata is not always present** — for an image without EXIF resolution (live: a PNG) the body carries `id`/`name`/`kind`/`version`/… but **no `resolutionUnit`**. `ResponseUpload` requires it today, so a **successful** upload is reported as a parse error (counted as an error, no index row ⇒ re-read every run). Those fields must be treated as optional; the bug and the repro are in the ledger (Welle 2, live findings), fix not yet commissioned.
- **Aufnahmedatum: wir senden kein `contentDate` (User-Entscheidung 2026-10-16, „das Wichtige ist, dass die Sortierung online nach Aufnahmedatum geht").** Das Aufnahmedatum leitet **der Server** aus den hochgeladenen Bytes ab — wie beim Original-Client, der das Feld nie setzt (REA: `docs/SPEC_CAPTURE_DATE.md` §1–§7). Live belegt am 2026-10-16 an unserem Cookie-Host (`content-<zone>.drive.amazonaws.com/v2/upload`): ein `POST` **ohne** `contentDate` ⇒ **201** und das Konto trägt `DateTimeOriginal` + `SubSecTimeOriginal` aus der Datei (`2026-10-13T19:47:36.123Z`) — nicht `ModifyDate`, nicht mtime, nicht die Upload-Zeit; ein **mitgesendeter** Wert wird **wörtlich übernommen** und überschreibt die Ableitung (`…36.000Z`); der `PUT /v2/upload/<nodeId>` funktioniert **ohne** `contentDate` (**200**, gleiche Node-Id, Inhalt getauscht, Datum bleibt). Der Server legt die EXIF-Wanduhrzeit naiv als `Z` ab (keine Offset-Umrechnung). **Der Query-Satz ist damit `conflictResolution`, `fileSize`, `name`, `parentNodeId`** — POST und PUT identisch (ohne `fileSize` ⇒ 400 `MissingFileSize`). **Verworfen:** den Wert weiterhin selbst ermitteln und mitsenden — die eigene Ermittlung hing an unserem EXIF-Leser und fiel ohne EXIF auf die **Dateizeit** zurück (bei einem Lightroom-Export = Exportzeit ⇒ falscher Tag); mit dem Feld entfallen EXIF-Priorität, Dateizeit-Rückfall und die Regel „jünger als 10 Minuten ⇒ Fehler". Doku: `docs/SPEC_UPLOAD.md` §2.4, `docs/PROTOCOL.md` §2.17, Live-Abnahme in `docs/features/LIVE_ACCEPTANCE.md`.
- Desktop auth chain (observed): `POST api.amazon.com/auth/token` (`source_token_type=refresh_token` -> `access_token`) -> `GET drive.amazonaws.com/drive/v2/account/endpoint` -> `POST cdws.../drive/v1/account/source`.
- Supported photos: jpg, jpeg, png, heic, heif, tif, tiff, gif, webp, bmp plus common RAW (cr2, cr3, arw, nef, dng, raf, rw2, orf, srw, pef, and others). `.xmp` sidecars are explicitly rejected.
- Folder scans enforce `is_supported_image` (single source in `folders.rs`). There is **no file picker** in HEAD (only `FileDialog::pick_folder()`) and `supported_extensions()` is currently unwired.
- Uses reference implementation from entourloop/amazon-photo-uploader (MIT) for API details.
- GUI language: German (matches the original Amazon Photos UI).
- "Speichernutzung"/storage bar is intentionally omitted for now: it only applies to Prime members and needs a real quota fetch; no placeholder is shown.
- Account/avatar block in the sidebar: when signed in, show the Amazon profile picture (no API identified yet — placeholder until then) plus "Angemeldet als:" and "Abmelden"; when **not** signed in, an icon replaces the picture. The green/red status color coding is kept deliberately (user likes it).
- MVP scope (user decision 2026-10-13): login, upload-target creation ("Upload-Ziel erstellen": backup configuration with destination, content filter, interval, dedup, conflict handling), upload flow, settings dialog. The download flow is relevant but explicitly NOT part of the MVP.
- Dedup (user decision 2026-10-13): a **persistent local MD5 index in SQLite** (like the original) decides skips (same path + same MD5 → skip; same path + different MD5 → re-upload; new file → upload). Server 409 is only a safety net, treated as "Ignored/Zuvor hochgeladen", not an error (mirrors the original's client-side `PreUploadCheck`).
- Content identity (user decision 2026-10-14): a file's identity is its **content**. A file whose **MD5 is already in the index under another path** (renamed/moved) is **skipped locally, before the network**, and counted as **"bereits im Konto"** (`conflict_skipped`) — not as "unverändert", because the file itself was never uploaded. Same path + same MD5 stays "unverändert"; same path + a different MD5 is uploaded. The local check demotes the server's `DuplicatesConflictError` to a pure safety net; `dedup = true` is the switch for both. **Persistence (user decision 2026-10-14):** such a path is **recorded in the index with a marker** (`origin = 'content_skip'`, next to the real uploads) so the fast stat path applies on later scans and the display keeps saying "bereits im Konto" in **every** run — otherwise the folder (~3,6 GB for "Sport") would be read and hashed again on each scan. That is a deliberate one-column schema change and matches the original, whose `uploads` table also carries a `status` column.
- Conflict handling (user decision 2026-10-13): all **four original options** per upload target (Nachfragen / Ignorieren / Als neue Datei speichern / Ersetzen). Wire mapping (corrected 2026-10-15 — the earlier `NONE`/`RENAME`/`OVERWRITE` line was stale, see the 2.2/2.2b decisions): `Nachfragen`/`Ignorieren`/`Ersetzen` ⇒ `conflictResolution=NONE`; `SaveAsNew` ⇒ `RENAME`. **`OVERWRITE` is rejected by the cookie host** (`POST` ⇒ 404, empty body) and is never sent; "Ersetzen" is carried out in place with `PUT /v2/upload/<nodeId>` instead. Replaces the current hard-coded `RENAME`.
- Conflict-handling **default** (user decision **2026-10-15**, changed from the 2026-10-14 `SaveAsNew` default): a file's identity is its **content**. `dedup = true` keeps meaning *same content ⇒ never uploaded* (MD5: server-side `409` + `errorCode = "DuplicatesConflictError"` and the local cross-path MD5 check). For a **name** conflict (same name, **different** content) new targets now default to **`Replace`**: the POST sends `conflictResolution=NONE`, the 409 `NameAlreadyExists` names the existing node, and the follow-up `PUT /v2/upload/<nodeId>` (POST query set, §2.2b) overwrites it **in place** — same node id, same name, the old content is gone. Chosen deliberately with the risk known: two **different** photos sharing a name (documented case: `27_ÖFB-Reisinger_01.jpg` exists twice with different MD5) ⇒ the one already in the account is replaced. `SaveAsNew` (wire `RENAME`) stays selectable per target. **Two different defaults — do not merge them:** the *Application Default* (`Config.conflict_resolution_default`) is `Replace`, while the *file* fallback — the enum's `#[default]`, used by `#[serde(default)]` for a `folders.toml` entry **without** the key — stays `SaveAsNew`, so pre-existing targets are never silently switched to overwriting. **Beide lokalen Ablagen stehen auf `Replace` (User-Entscheidung 2026-10-16, „alles auf replace"):** `config.toml` trägt `conflict_resolution_default = "replace"`, „Sport" trägt `conflict_resolution = "replace"` — von Hand umgestellt, **kein** Migrationscode. Anlass: ein zweiter JPEG-Export oder ein DNG mit geänderten Metadaten gehört **ersetzt**, nicht dupliziert. **Verworfen:** den Application Default dafür wieder auf `SaveAsNew` drehen. **Watch out:** `ConflictResolution` uses `#[serde(rename_all = "lowercase")]`, so the file value is `saveasnew`/`replace`, **not** `save_as_new` — renaming the serde rule would break existing config files.
- **Sichere Ersetz-Regel (User-Entscheidung 2026-10-16):** der `PUT /v2/upload/<nodeId>` darf den vom 409 `NameAlreadyExists` benannten Knoten **nur** überschreiben, wenn der **lokale Index genau diesen Pfad an genau diese Node** bindet — dann wurde dieselbe lokale Datei geändert (User-Wortlaut: „nie was oben ist löschen, außer es ist die gleiche Datei physisch relativ zum gesicherten Pfad, dann überschreiben"). Trifft der Name auf eine **fremde** Datei aus einem anderen Pfad, wird **umbenannt** (`SaveAsNew`/Wire `RENAME`), nie gelöscht. Träger der Bindung ist die neue Spalte **`node_id`** im Dedup-Index (Parität zum Original, dessen `uploads`-Tabelle ebenfalls `node_id` führt): `record_upload` schreibt die Node des Erfolgs, `COALESCE(excluded.node_id, node_id)` lässt eine bekannte Bindung bestehen, `node_id_for(path)` ist die Abfrage für die Regel. **Altdaten ohne Node-Id sind ungebunden** und werden beim ersten Inhaltswechsel einmalig umbenannt — vom User ausdrücklich akzeptiert (`docs/DEV_NOTES.md`). Die Regel beschränkt **nur die automatische Ordner-Politik**: eine **ausdrückliche** Antwort „Ersetzen" im Konflikt-Dialog ist die Zustimmung für *diese* Datei und überschreibt sie (User-Entscheidung 2026-10-16). Ein **Inhaltsduplikat** bleibt auch dort „bereits im Konto" — dieselben Bytes sind schon da, es gibt nichts zu überschreiben. **Verworfen:** den Application Default nur deshalb auf `SaveAsNew` zurückdrehen (die Sicherheit kommt aus der Bindung, nicht aus einem anderen Default); bei fehlender Bindung **nichts** tun und den Server entscheiden lassen.
- **Drei Ebenen der Konflikt-Standardwahl (User-Entscheidung 2026-10-15, bestätigt 2026-10-16):**
  1. **Application Default** — neues Feld in `Config` (`config.toml`), einstellbar in
     **Einstellungen → Upload** (Radiogruppe mit den vier vorhandenen Optionslabels, das Muster der
     Seite; umgesetzt in `db68cff`), Default **`Replace`**. Er ist die
     **Vorauswahl beim Anlegen eines Ziels** und gilt für die **Einmal-Queue (Drag&Drop)**.
  2. **Ordner-Default** — das bestehende `BackupFolder.conflict_resolution` (`folders.toml`).
     Hierher schreibt „**Nicht noch einmal fragen**" seine Antwort (in den Ordner der gefragten
     Datei); der nächste Lauf fragt dort nicht mehr.
  3. **Kein Per-Datei-Default** — einzelne Dateien haben keine eigene Einstellung. Innerhalb eines
     Laufs bleibt ein angekreuztes „Nicht noch einmal fragen" **run-weit** wirksam (unverändert);
     persistiert wird es **pro Ordner**.
  - **Ausnahme Einmal-Queue (Drag&Drop, User-Entscheidung 2026-10-15):** dort gibt es **keinen**
    Ordner in `folders.toml` (synthetischer Index `ONE_TIME_IDX = usize::MAX`, `src/app.rs:26`,
    transienter `BackupFolder`). Das Häkchen wirkt hier **nur run-weit**; beim nächsten Drop
    entscheidet wieder der Application Default. Bewusst **nicht** gewählt: den Application Default
    still überschreiben (greift global statt nur für diesen Drop). **Unberührt davon bleibt die
    2026-10-14 getroffene Regel, dass die Drop-Liste selbst persistiert wird** (SQLite-Tabelle
    `one_time_queue`, beim Start wiederhergestellt, gelöscht erst nach vollständig abgeschlossenem
    Lauf) — nicht persistiert wird hier ausschließlich die **Konflikt-Antwort**.
- **Globaler Dedupe-Default (User-Entscheidung 2026-10-15):** neues Feld `Config.dedup_default`
  (Default **`true`**, ebenso der serde-Default, damit eine alte `config.toml` weiter lädt),
  einstellbar in **Einstellungen → Upload** neben der Konflikt-Standardwahl. Er ist die
  **Vorauswahl beim Anlegen eines Ziels** und gilt für die **Einmal-Queue (Drag&Drop)** — dieselbe
  Rolle wie der Application Default bei den Konflikten. Der User will **nie Duplikate** hochladen,
  `true` bleibt also die Vorgabe. (Das Original hat nach dem Onboarding `false`; unsere
  `true`-Vorgabe ist die dokumentierte, bewusste Abweichung.) Bestehende Ziele behalten ihren
  gespeicherten `dedup`-Wert.
- **Wortlaut-Parität beim Konflikt-Dialog (User-Entscheidung 2026-10-15):** die Option `SaveAsNew`
  heißt in den **Ziel-/Einstellungs-Radiogruppen** wie im Original „**Neue Datei automatisch
  umbenennen**", der Knopf im **Konflikt-Dialog** bleibt „Als neue Datei speichern" — auch das ist
  Original-Wortlaut (das Original benennt dieselbe Option an beiden Stellen unterschiedlich).
  Das Häkchen nennt den **Ordner**: „Nicht noch einmal nach ‚<Ordner>'-Ordner fragen"
  (Original-Muster `…donotaskagain_checkbox_text`); bei der **Einmal-Queue** (kein Ordner) bleibt der
  neutrale Text „Nicht noch einmal fragen", weil dort nichts persistiert wird.
  **Bewusst abweichend (dokumentiert in `docs/SPEC_CONFLICT_OPTIONS.md` §5):** wir fragen **pro
  Datei** (60-s-Timeout ⇒ Ignorieren), das Original zeigt **nach dem Lauf** ein gebündeltes Sheet
  („N Dateinamenkonflikte in …", eine Antwort für alle) plus Toast; ein nachträgliches Sheet wäre ein
  UI-Umbau und gehört zu Welle 3.
- Backup configuration window (user decision 2026-10-13): build the full "Datensicherungseinstellungen" window now (destination, content filter, interval, dedup, conflict handling, save/cancel), and **share the form logic** with the create-backup flow (same dialog for create and edit) instead of a separate row-menu. Fields whose backend does not exist yet (destination, content filter, interval) are shown **disabled** with a "Noch nicht verfügbar" hover (user decision 2026-10-13: "volle Anordnung, Rest ausgegraut"); functional now: source folder (read-only when editing), dedup, name-conflict policy.

## Upload-Queue & Concurrency (User-Entscheidung 2026-10-13)

- **Globaler Pool bleibt** (status quo): alle in einem Lauf ausgewählten Sicherungsziele werden zu
  **einer** gemeinsamen Warteschlange, die sich maximal `concurrency` (1..8, Einstellung) Arbeiter
  teilt; mehr als 8 **gesamt** macht keinen Sinn.
- **Pro-Ziel-Limit ist nur „nicht mehr als global"** und erst **Post-MVP**: ein optionaler,
  persistent gespeicherter Wert (0/None = globaler Default; nicht größer als global) wie bereits
  bei Dedup/Conflict vorbereitet. Im MVP bewusst **nicht** umgesetzt.
- **Netzwerkabhängigkeit (MVP):** „Slow-Mode" als temporäre, **nicht** persistierte
  Run-Zeit-Einstellung (`PrimePhotoApp.slow_mode`, Checkbox in Einstellungen → Upload), die
  für den aktuellen Lauf `concurrency = 1` erzwingt; bei App-Neustart zurückgesetzt. Automatische
  Drosselung anhand des Netzwerks/SSID/Namen ist **nachgeordnet** (Plattform-APIs, Post-MVP).

## Assets: Tray-Icons + Logo (User 2026-10-14) — **implementiert** (Befund 2026-10-15: `build.rs`
rastert die SVGs bereits, `src/tray.rs` bettet sie ein; der frühere Stand „geplant, nicht
implementiert" war veraltet)

Eigene Assets des Users liegen in **`assets/`** — als **SVG-Quellen** (eine Quelle der Wahrheit).
**User-Entscheidung 2026-10-14: im Build entstehen daraus PNGs, die mit ausgeliefert werden.** Umsetzung:
`build.rs` rastert sie mit **`resvg`** (reine Rust-Bibliothek, `default-features = false` ⇒ 53 Crates,
**nur** `[build-dependencies]`, keine Laufzeit-Abhängigkeit, Binary bleibt unverändert klein) nach
`OUT_DIR`, eingebettet per `include_bytes!(concat!(env!("OUT_DIR"), …))` und dekodiert mit der
vorhandenen `image`-Crate. **Warum resvg:** `sips`/`qlmanage` liefern auf dem Mac gute Qualität, bauen
aber nur dort — unser **CI und die Release-Matrix sind Linux/Windows**; ImageMagicks interner Renderer
(`rsvg-convert` fehlt) malt die Striche nachweislich nicht. resvg-Qualität wurde an genau diesen SVGs
gesichtet (alle fünf Tray-Zustände + beide Logos, schwarz/weiß, Farbverlauf korrekt).
**Theme ohne neue Abhängigkeit:** `egui::Context::system_theme()` (in 0.31 vorhanden) — dunkel ⇒ weiße
Tray-Varianten, hell ⇒ schwarze; `None` (unbekannt) ⇒ das farbige Logo, das auf jedem Hintergrund
sichtbar ist. macOS bleibt beim schwarzen Template-Satz.
**Kein Fallback (User 2026-10-14):** fehlt ein SVG, bricht `build.rs` den Build (Compile-Fehler) —
kein `Option`, keine `exists()`-Prüfung, kein Rückfall auf eine gezeichnete Variante. Dekodier-Fehler
zur Laufzeit = fail fast.

**UI-Icons: Bibliothek statt selbst gezeichnet (Welle 3c, 2026-10-16).** `src/icons.rs` enthält nach
Welle 3c **keine** selbst gezeichneten Navigations-Glyphen mehr. Die beiden Sidebar-Icons sind
**Material Symbols**-SVGs (Apache-2.0, Google LLC) über **dieselbe** Pipeline:
`assets/icon-cloud.svg` („Sicherung") und `assets/icon-settings.svg` („Einstellungen") — **gefüllte**
Variante (`…_fill1_24px.svg`), 64-px-Raster, zur Laufzeit auf `TEAL` (ausgewählt/gehover) bzw.
`MUTED` getönt, im festen 18 × 18-Rect der Icon-Spalte. Motiv **und** Stil folgen dem Original-Bundle
(gesichtet: `navmenu-backup@3x.png` = massiv gefüllte Wolke, `navmenu-settings@3x.png` = gefülltes
Zahnrad mit Loch) — Projektregel Original-Parität. **Gefüllt gilt nur für die Navigation:** die
Zeilen-Icons (Kebab, Refresh) bleiben als **Linie** (Outlined), weil sie im Original ebenfalls Linien
sind. **Selbst gezeichnet bleiben:** die Marktplatz-Flaggen und unser Logo (`upload_logo`); der
Status-Punkt ist ein Kreis, kein Glyph. **Verworfen:** `backup` (Wolke mit Hochpfeil) + `tune` (drei
Schieberegler) — nicht die Motive des Originals; die **Outlined**-Variante für die Navigation — weicht
sichtbar vom Original ab.

MVP-Zielbild des Users: **polished** — Autostart (Default **an**, siehe §Autostart), effizienter
funktionierender Upload (Event-Watcher + Stat-Check + Wasserzeichen) und echte Tray-/Logo-Assets.

## About + Drittanbieter-Lizenzen (Welle 4, User-Entscheidungen 2026-10-16)

Recon: `docs/SPEC_ABOUT.md`. Kernbefund: das Original hat **keine** eingebettete Lizenzliste — nur
einen Link auf eine Web-Seite; About ist ein Sheet mit Logo, Version, **Geräte-Seriennummer**,
Send-Logs-Knopf, drei Rechtslinks und Copyright. Wir bauen das About **ohne** Amazon-Bezüge
(Projektregel „Name & Branding").

- **Eigene Lizenz: `MIT OR Apache-2.0`** (User-Entscheidung 2026-10-16) — `LICENSE-MIT` +
  `LICENSE-APACHE` im Repo, `license = "MIT OR Apache-2.0"` in `Cargo.toml`. Vorher hatte das Repo
  **gar keine** Lizenz: für ein öffentliches Release war das eine offene Lücke.
- **Lizenzliste = alle Abhängigkeiten aus `Cargo.lock`** (User-Entscheidung: „alle Deps"), bewusst
  **übervollständig** — auch transitiv und auch Windows-/Linux-only-Crates, die nicht im
  macOS-Binary landen. Erzeugt per `cargo metadata --format-version 1 --locked` aus
  **`packaging/gen-third-party-licenses.sh`** nach **`assets/third-party-licenses.txt`**;
  das Skript ist **deterministisch** (kein Zeitstempel) und läuft **offline**. `cargo-license`/
  `cargo-about` sind **nicht** installiert und werden **nicht** aufgenommen.
- **Vollständige Lizenztexte gehören mit hinein** (User-Entscheidung): Apache-2.0 §4 und MIT
  verlangen das **Beilegen** der Lizenz. Die Texte werden aus den **lokalen Crate-Kopien** gesammelt
  (über `manifest_path` der Metadaten), identische Texte per sha256 **dedupliziert**; fehlt ein Text,
  wird das **ausgewiesen** statt erfunden. Kein Netzwerk im Generator.
- **Darstellung:** eigenes, **scrollbares Lizenz-Fenster**, erreichbar über einen Knopf im
  **About**-Fenster (bei ~760 Komponenten die einzige lesbare Form). Zwei Quellen werden gezeigt:
  die generierte Liste **und** die kuratierte Icon-Attribution aus `docs/THIRD_PARTY.md`
  (dort steht: „Die About-/Lizenzseite (Welle 4) liest aus dieser Datei" — eine Quelle der Wahrheit).
- **Auslöser: Tray-Menü + macOS-App-Menü**, Eintrag „**Über Prime Photos**", **ohne** Tastenkürzel —
  genau die zwei Pfade des Originals (`clouddrive_desktop_contextmenu_support_about`), nur mit
  unserem Namen. Im App-Menü ist er der **erste** Eintrag (wie im Original). **Neue Mechanik dafür:**
  ein winziges ObjC-Zielobjekt (`objc2::define_class!`, objc2 0.6 ist bereits Dependency) setzt ein
  statisches Flag und weckt die UI — die AppKit-Menüpunkte haben `nil`-Target und laufen über die
  Responder-Chain, ein eigener Selektor braucht deshalb ein eigenes Target. Kein `muda`-Umbau des
  Hauptmenüs (das würde die per Responder-Chain funktionierende Bearbeiten-Gruppe riskieren).
- **Copyright-Zeile: „© <Jahr> Prime Photos"**, Jahr zur Laufzeit aus der Systemzeit (wie das
  Original sein laufendes Jahr einsetzt). Die Amazon-Formulierung wird **nicht** übernommen.
- **Version:** `CARGO_PKG_VERSION`; die Commit-ID nur, wenn sie zur Build-Zeit verfügbar ist —
  sonst weglassen, **nichts erfinden** (das Original zeigt `AssemblyVersion.CommitId`).
- **Bewusst nicht nachgebaut** (Branding-Regel bzw. Post-MVP): Nutzungsbedingungen- und
  Datenschutz-Link (zeigen im Original auf Amazon-Seiten), der Drittanbieter-**Web**-Link,
  die **Geräte-Seriennummer**, „Protokolle senden" und die Versionshinweise (`SPEC_ABOUT.md` §9).
- **Verworfen:** nur die **direkten** Abhängigkeiten listen (erfüllt die Attributionspflicht für
  transitive Crates nicht); die Liste als **Textdatei ins Bundle** legen statt sie zu rendern;
  About nur im **App-Menü**; **ohne** Copyright-Zeile; nur Komponentenliste **ohne** Lizenztexte;
  die eigene Lizenz später zu klären.

## Autostart (User-Entscheidung 2026-10-14) — Default **an**

Der Login-Item-Schalter „Nach dem Anmelden starten" ist bei erster Nutzung **ein** (PORT-4 baute
nur den Schalter, Default aus). Begründung: ohne Login-Item läuft nach Neustart/Ausloggen **kein
Watcher** — die Event-Erkennung würde schlafen (Wasserscan ist nur Sicherheitsnetz). Das Original
schaltet Autostart beim Onboarding ebenfalls selbst ein (`docs/SPEC_OS_INTEGRATION.md` §3.6).
`Config.autostart_enabled:
Option<bool>` (`None` = nie entschieden ⇒ einmal registrieren + `Some(true)` speichern; `Some(false)`
= Veto, nie einschalten, ein von Hand angelegter Eintrag wird nicht bekämpft; `Some(true)` ⇒
fehlendes Login-Item wird repariert). Der OS-Zustand bleibt die operative Wahrheit, `Config`
speichert nur die Absicht. Details: `AGENTS.todo.md` → „Zurückgestellt" #2.

## Geplante Implementierungs-Wellen

**Stand 2026-10-15 (Neuordnung, User-Auftrag):** Der User hat die Restliste **gruppiert** haben
wollen („dann rest in wellen gruppieren, ausgeben und stoppen") und danach **alle Wellen
nacheinander** beauftragt. Der verbindliche Plan — Schnitt nach gemeinsamem Umfeld, damit jede Welle
in einem Durchgang ohne Merge-Konflikt läuft — steht im Ledger (`AGENTS.todo.md` →
„Wellenplan (Neufassung 2026-10-14, nach der Duplikat-/Konflikt-Kette)"), und wird dort
fortgeschrieben (Abnahme + Commit je Welle). Kurzform:

- **Welle 1 — echter Lauf** (kein Code) — Belege im Ledger.
- **Welle 2 — Konflikt-/Duplikat-Kette abschließen:** 409-Ursache trennen (2.1), „Ersetzen" via
  `PUT /v2/upload/<nodeId>` (2.2), „Nicht noch einmal fragen" pro Ziel persistieren (2.3), Rest nach
  `docs/SPEC_CONFLICT_OPTIONS.md` (2.4). **Läuft ab 2026-10-15.**
- **Welle 3 — Progress-/UI-Umbau** (Detail in den Hauptbereich, Klapp-Panel weg, Live-Status)
  **plus die Layout-Punkte aus `docs/SPEC_UPLOAD_PROGRESS.md` § 7.8** — feste Spaltenköpfe,
  Kürzen + Tooltip, Liste füllt die Höhe, die 300-Zeilen-Kappe entfällt, leerer Zustand zentriert
  (User-Entscheidung 2026-10-15: „3.1–3.4 **plus** §7.8-Layout" als **ein** Paket).
- **Welle 4 — About / Third-Party-Lizenzen** (Voraussetzung für ein öffentliches Release).
- **Welle 5 — Robustheit & Politur** (`rfd::FileDialog` → `AsyncFileDialog`, P2-Reste, Branding).
- **Welle 6 — Post-MVP / bewusst später** (dokumentiert, **nicht** eingeplant).

Frühere Auswahl von 2026-10-14 („P2+P3", „Echter Name + Profilbild", „About"): die beiden ersten
sind umgesetzt, das dritte ist jetzt **Welle 4** — die alte Nummerierung ist damit abgelöst.

Weitere offene Punkte (Queue-Paginierung, Fehlerfenster, Pause pro Ordner, Sortierung/Filter,
Tray-Status-Popup, Pro-Ziel-Limit, Drosselung, Logout im Upload) sind im Ledger
dokumentiert, aber **nicht** eingeplant. Regel bleibt: „Anlegen bleibt Pflicht — Abarbeiten nicht";
ein Listeneintrag allein löst keinen Umsetzungs- oder Verify-Lauf aus.

## Erkennung neuer/geänderter Dateien (User-Entscheidung 2026-10-14)

- **Primärer Mechanismus: OS-Dateisystem-Events, kein Polling.** Die 15/90-Minuten-Runde
  (`background_sync`: erster Tick 15 min, dann 90 min) ist **nicht** MVP-fertig und entfällt als
  Hauptpfad. Pro überwachtem Backup-Ordner ein OS-Watch über eine plattformneutrale Crate
  (macOS FSEvents / Windows ReadDirectoryChangesW / Linux inotify).
  Original-Parameter (`docs/SPEC_BACKGROUND_SYNC.md` §6, belegt): FSEvents-Latenz 3 s, Throttle
  100 ms, `Throttle(FiveSeconds)`-Debounce, Poll `BackupWatcherPollIntervalInSeconds` 30 s,
  Re-arm bei Fehler, pausierte Ziele bekommen keinen Watcher.
- **Reaktion pro Event: „file add / edit per Event abfragen"** (User-Vorgabe) — nur die betroffene
  Datei prüfen, kein kompletter Ordner-Durchlauf und kein erneutes Hashen aller Dateien.
- **Stat-Check vor dem Hash:** die Skip-Entscheidung fragt zuerst (path, size, mtime) gegen den
  persistierten Index; nur unbekannte/geänderte Dateien werden gelesen und MD5-gehasht. `size` und
  `mtime` stehen in der Tabelle bereits, blieben aber ungenutzt (`src/dedup.rs`). Das MD5 bleibt die
  Identität (`docs/SPEC_DEDUP.md` §1); der Stat-Treffer ist nur der schnelle Pfad.
- **Kaltstart (Event-Ersatz fürs Ausschalten):** beim Start bzw. nach Anmeldung wird **einmal** pro
  Ordner enumeriert und per Stat-Check entschieden, was fehlt. Kein Timer.
- **Sicherheitsnetz beim Start (User-Entscheidung 2026-10-14):** pro Backup-Ordner wird das
  **größte Änderungsdatum** (`max mtime`) des letzten Durchlaufs **in der SQLite gespeichert**
  (neue Tabelle, neben dem MD5-Index). Beim Start genügt ein **reiner Metadaten-Durchlauf**
  (`walkdir` ohne Lesen der Dateien): ist das aktuelle `max mtime` **nicht größer** als das
  gespeicherte, ist der Ordner nachweislich unverändert und es wird **nichts** gehasht, keine
  DB-Abfrage pro Datei, kein Upload-Plan. Wird es größer, läuft nur für diesen Ordner der
  Stat-Check-Pfad (und nur geänderte Dateien werden gelesen + gehasht).
  **Angeborene Grenze (wie im Original, `SPEC_CHANGE_DETECTION.md`):** Kopien, die die mtime
  erhalten (`rsync --times`, Finder-Import), erhöhen `max mtime` nicht. Wer das braucht, startet
  manuell — der Wasserzeichen-Treffer ist der schnelle Pfad, keine Garantie.
- **Drag & Drop ist ein einmaliger Upload** (User-Klarstellung 2026-10-14): kein Watch, kein
  Wasserzeichen, keine dauerhafte Sicherung — nur die temporäre Queue bis zum Abschluss
  (siehe unten).
- **Pause blockt auch Events** (globale Pause, siehe oben).
- **Event-Puffer: unbegrenzt, kein Verwerfen (User-Entscheidung 2026-10-14).** Events, die während
  eines laufenden Uploads **oder einer Pause** eintreffen, werden geparkt und beim nächsten
  Durchlauf vollständig abgearbeitet. Es gibt **keine Obergrenze** (die frühere 500er-Kappe
  `WATCH_PENDING_MAX` ist entfernt): die Liste wächst dynamisch mit der Zahl der **verschiedenen**
  geänderten Pfade (~1,2 MB bei 10.000, ~12 MB bei 100.000 — vernachlässigbar), Dedup per
  `HashSet<PathBuf>` in O(1) statt vorher O(n²). Bursts von **10.000+** Dateien (Kamera-Import,
  Ordner-Kopie, lange Pause) sind damit vollständig abgedeckt; verworfen wird nichts.
  Ein neuer Burst **ersetzt** den geparkten Bestand nicht mehr, sondern wird mit ihm **vereinigt**
  (das war ein echter Verlust-Bug: bei mehreren Bursts während einer Pause überlebte nur der
  letzte); erlaubt `can_start` einen Lauf, geht der vereinigte Bestand in **einen** Lauf.
  Umsetzung: `src/backup.rs` (`park_paths`, `pending`/`pending_seen`), Commit `2eea96c`, Verify
  Runde 1 APPROVE.
- **Verworfen:** der periodische Hintergrund-Scan als Erkennungsmechanismus (User 2026-10-14:
  „file add / edit per event abfragen"); ein `BackupScanIntervalInHours`-Takt.
- **Offen:** Sicherheitsnetz für verpasste Events (Watcher-Limit, ausgeworfenes Laufwerk,
  Netzwerk-Mount) — als optionaler Schalter, noch nicht entschieden.
- **Doku:** `docs/SPEC_CHANGE_DETECTION.md` (Clean-Room recon, in Arbeit).

## Pause, One-Time-Queue & Gesamt-Fortschritt (User-Entscheidung 2026-10-14)

- **Stoppen/Fortsetzenist ein Umschalter** (wie im Original, `UI_STATES.md` §70): „Alle
  Sicherungsvorgänge anhalten" ↔ „… fortsetzen" in der Backup-Kopfzeile plus derselbe Eintrag im
  Tray. Kein zweiter Button.
- **Pause wie Original (Entscheidung 2026-10-14):** der Zustand wird in `config.toml` gespeichert
  (`paused`, serde-default `false`, lädt also eine alte Config) und beim Start daraus
  initialisiert. Der Button ist **immer** schaltbar, sobald mindestens ein Sicherungsziel existiert
  **oder** ein Lauf läuft — früher nur während eines laufenden Laufs, was (a) Pause im Leerlauf
  verhinderte und (b) einen reinen Drag&Drop-Lauf ohne angelegte Ordner sperrte. Eine gesetzte
  Pause blockt auch den Hintergrund-Sync (Guard existierte bereits). **Pause pro Ordner** bleibt
  Post-MVP (Original hat ein Zeilen-Symbol).
- **One-Time-Queue (Drag&Drop) wird temporär in SQLite gespeichert** (Entscheidung 2026-10-14): die
  abgelegten Dateien landen in derselben SQLite, in der der MD5-Dedup-Index liegt, und werden
  **nach vollständig abgeschlossenem Upload wieder gelöscht**. Damit überlebt die Zeile „Einmalige
  Sicherung" einen Neustart und der Lauf wird fortgesetzt (Guards wie Hintergrund-Sync). Das
  entspricht dem Original, das die Drop-Queue persistiert
  (`uploads`/`backup`, `schedule_id="ManualUpload"`, `docs/SPEC_ONE_TIME_UPLOAD.md` §6).
  **Verworfen:** die alte Regel „Drop-Queue ist absichtlich transient/nie persistiert"
  (`SPEC_ONE_TIME_UPLOAD.md` §10.3) — sie galt nur, bis der User die Parität verlangte.
- **Gesamt-Fortschritt über alle Uploads** (Wunsch 2026-10-14): genau **eine** aggregierte Anzeige
  über alle Uploads (Summe aller Backup-Ziele inkl. One-Time-Zeile), nicht nur über den gerade
  laufenden Lauf. Manuelle Läufe starten weiterhin pro Ordner; die Anzeige summiert trotzdem über
  alle.

## Abmelden bei laufendem Upload (User-Entscheidung 2026-10-16)

- **Ein Abbruch-Mechanismus wird gebaut** (User-Wahl aus drei Optionen). Anlass: `logout()` beendet
  den Session-Worker bereits (Generation-Bump) und löscht Token/Keychain — aber **nicht** einen
  laufenden Upload: der Worker hält einen eigenen `Config`-Klon mit den **alten** Cookies und lief
  mit widerrufenen Zugangsdaten in Fehler. **Die dritte Residue der alten Notiz ist hinfällig:** der
  Login-Screen *ersetzt* die Haupt-UI (`src/app.rs:950`), der Abmelden-Knopf ist bei offener
  Login-WebView also gar nicht erreichbar.
- Umsetzung: **Abbruch-Flag in `UploadShared`** (kein neuer Kanal — die App hält `self.upload`
  bereits, `logout()` erreicht es ohne neues Plumbing). `logout()` setzt es; der Worker prüft es an
  seiner **Wartestelle** (Pause-Park, `src/upload.rs:393`) **und** beim Konflikt-Dialog und kehrt
  zurück. Ein laufender HTTP-Request läuft zu Ende. `run_upload` setzt das Flag beim Start zurück.
- **Ein abgebrochener Lauf gilt nicht als erledigt:** Wassermarke **nicht** vorschieben (sonst gilt
  der Ordner beim nächsten Start als „unverändert" und die Reste werden nie nachgeholt),
  `FolderStatus`/Zähler **nicht** verbuchen (heute würde die Zeile „fertig" mit halber Zahl
  behaupten), Einmal-Queue behalten — das war schon korrekt (`backup.rs:1460` löscht nur bei
  `done + failed >= total`). Bereits hochgeladene Dateien bleiben im Dedup-Index und werden beim
  nächsten Lauf über den Stat-Pfad übersprungen.
- **Verworfen:** nur dokumentieren; den Lauf über die vorhandene Pause **parken** (er nimmt nach
  erneutem Anmelden mit den **alten** Cookies wieder auf — nicht sauber); `conn: None`-Analogie als
  Vorbild (der Abbruch braucht keinen neuen Kanal).
- **Kein Abbruch-Knopf in der UI** (nicht beauftragt) — der Abbruch hängt allein am Abmelden.
- **`config_error` verständlich machen** (User-Regel „keine technischen Fehlermeldungen im
  Endnutzer-UI"): das technische Detail geht ins Log, im UI steht ein verständlicher Satz — und er
  wird **auch auf dem Login-Screen** angezeigt (nach dem Abmelden ist er sonst unsichtbar).

## Zeilen-Aktionen & Ergebnis-Anzeige (User-Entscheidung 2026-10-14)

Anlass: Der User suchte einen Rescan-Button und erwartete ein **vertikales „…"-Icon** mit Popup bei
Linksklick; außerdem zweifelte er die Zahl „480 fertig" an („passt die Anzahl wirklich von heute?").

- **Icons kommen aus einer professionellen Icon-Sammlung, nicht selbst gezeichnet** (User-Korrektur
  2026-10-14, nachdem er „selbst zeichnen" abgelehnt hat: „so gut wie es geht nicht selbst zeichnen,
  sondern icons nutzen … besser was nehmen, was lizenzrechtlich passt und professionell ist").
  Konkret: **Material Symbols (Google, Apache-2.0)**, als **SVG** in `assets/` über die **vorhandene**
  Pipeline (`build.rs`/`resvg` → `OUT_DIR`-Raster → `include_bytes!`), zur Laufzeit per **Tint** in die
  Theme-Farbe. **Kein Icon-Font** (User: „potentiell nicht die Iconfont selbst einbinden, sondern die
  PNGs in der richtigen Größe") — Raster statt Font, keine neue Abhängigkeit.
  Belege: `assets/icon-more-vert.svg` (Kebab) und `assets/icon-refresh.svg`, Quellen im
  Material-Symbols-Repo (`symbols/web/<name>/materialsymbolsoutlined/<name>_24px.svg`), Hashes und
  Attribution in `docs/THIRD_PARTY.md`.
  **Falle, die dabei zu beachten ist:** die Quellen haben **kein** `fill`/`currentColor` (Standard =
  schwarz), der vorhandene Asset-Override `*{color:…}` färbt sie also **nicht** — für die Icon-Gruppe
  greift ein **fill**-Override. Das wird durch einen Test belegt (Raster ist gedeckt und weiß).
- **Zeilen-Menü: vertikaler Kebab, immer sichtbar**, Tooltip „Aktionen", Linksklick öffnet das Popup
  (egui 0.31 hat **kein** `From<Image> for WidgetText` ⇒ `ui.menu_button` scheidet aus; es wird der
  dokumentierte Weg `Button::image` + `popup_below_widget` + `memory_mut(toggle_popup)` benutzt).
  Aktuell existiert genau **eine** `⋯`-Stelle: `src/backup.rs:1501`.
- **Rescan wird sichtbar:** pro Ordner-Zeile ein **eigener Refresh-Icon-Button**, der direkt
  „Jetzt scannen und hochladen" auslöst. Der Kebab-Popup behält den Rest (Im Dateimanager anzeigen,
  Bearbeiten, Trennen, Pausieren). Grund: der Rescan existierte, war aber nur im Text-`⋯` versteckt.
  **Global bleibt es weiterhin ohne „Alles neu prüfen"** (nicht beauftragt).
  **Verworfen (2026-10-14):** das Zeilen-Icon **selbst zu zeichnen** (in `src/icons.rs`) — der User
  will keine handgemachten Formen, sondern eine lizenzierte Icon-Bibliothek. Ebenfalls verworfen:
  das Textzeichen `⋮` (U+22EE **fehlt in allen vier** Fonts von `epaint_default_fonts-0.31.1` — Hack,
  Ubuntu-Light, NotoEmoji, emoji-icon-font — und erschiene als □; `⋯` U+22EF gibt es nur in
  Hack-Regular) und ein **Icon-Font-Crate** als zweite Icon-Quelle.
- **Ergebnis wird aufgeschlüsselt:** „fertig" darf **nicht** länger echt-Upload, 409 „schon im Konto"
  und lokalen Dedup-Skip zusammenwerfen. Anzeige fortan getrennt:
  **neu hochgeladen / bereits im Konto / unverändert / Fehler**. Belegte Ursache: `src/upload.rs` zählt
  in allen drei Fällen `s.done += 1` (`:496` Uploaded, `:552` ConflictSkipped, `:376`/`:421` lokaler
  Skip) — die Ausgänge liegen dort schon getrennt vor und werden nur nicht berichtet.
  **Konsequenz für die Regel „eine aggregierte Anzeige":** das bleibt für den *Fortschritt* eines
  laufenden Uploads richtig, gilt aber nicht für das *Ergebnis* — das Ergebnis wird aufgeschlüsselt.
  **Speicherung: nur flüchtig** (User-Entscheidung 2026-10-14) — die Aufschlüsselung lebt im Speicher
  für den Lauf/die Sitzung, `folders.toml` wird **nicht** erweitert (keine Schemaänderung); nach einem
  Neustart steht dort weiterhin nur die Gesamtzahl. **Ort (User-Entscheidung 2026-10-14):** die
  **Fußzeile aggregiert** über alle Ziele **und** die **einzelne Ordner-Zeile** zeigt ihre eigenen drei
  Zahlen; das Datei-Detail bleibt wie heute (beide Skip-Fälle teilen sich dort einen Zustand).
- **Live-Status der Ordner-Zeile (Welle 3.3, 2026-10-16):** ob eine Zeile „Upload läuft" oder „Fehler"
  zeigt, wird **live** abgeleitet — läuft für dieses Ziel gerade ein Lauf, gilt „läuft"; der
  **persistierte** Ausgang zählt erst **nach** `finalize`. Umsetzung: die Live-Prüfungen hängen an
  **`folder_total` UND `s.active`** (zwei Stellen in `src/backup.rs`); fehlt das `s.active`, kippt die
  Anzeige im Frame zwischen Lauf-Ende und `finalize` kurz auf „Fehler". Anlass: eine Zeile stand auf
  „Fehler", während der Upload noch lief.

## Scope-Grenzen (In/Out) — User-Wunsch 2026-10-13

Bewusste, absichtliche Abgrenzung. Die vollständige Feature-für-Feature-Liste mit Beleg-Keys/-Symbolen
steht im Ledger (`AGENTS.todo.md` → „Scope-Inventar: vergessene Features & bewusste In/Out-Grenzen").

- **Out (bewusst, nicht MVP):** bidirektionale Synchronisation, Downloads/Restore, Datei-Management
  (Albums/Favoriten/Papierkorb/Teilen), Apple-Photos-/iPhoto-/Aperture-Import, Autoplay/Media-Player,
  Upsell-/Trial-/Overquota-/Outage-Banner, Auto-Update, Proxy- und „ausgeblendete Dateien"-Einstellungen,
  Crash-Relaunch-LaunchAgent, iOS-Dedupe-Prompt.
- **MVP-Lücke geschlossen (Korrektur 2026-10-14):** der interaktive Dateinamens-Konflikt-Dialog ist
  **implementiert** (`81cd277`, verifiziert in `a3a3625`): bei 409 fragt `Ask` pro Datei
  (Ignorieren / Als neue Datei speichern / Ersetzen) mit „Nicht noch einmal fragen" als run-weitem
  Override und 60-s-Timeout ⇒ Ignore. Die frühere Notiz „die Option ‚Nachfragen' ist ohne ihn
  wirkungslos" ist damit überholt.
- **Für ein öffentliches Release relevant:** About / Third-Party-Lizenzen (Hinweispflicht).
- **Concurrency-Cap „8":** kein technisches Limit, sondern die Obergrenze der kunden-/server-gelieferten
  Einstellung (`CustomerSettingsMaxSupportedUploadQueues = 8`); der **Geräte-Default** ist kleiner
  (`UploadMaxQueues = 4`, `MaxNumberOfQueuesPerProcessorCore = 1.0` ⇒ praktisch `min(4, Cores)`).
  Eine sachliche Begründung der 8 ist im Original **nicht** belegt.
- „Nicht umsetzen" heißt hier: dokumentiert und bewusst draußen — nicht „später automatisch".

## Plattform / Portierung (User-Entscheidung 2026-10-13)

- **Ziel: voll cross-platform.** macOS (Apple Silicon), Windows und Linux sind gleichwertige
  Ziele. Kehrtwende vom früheren „Native macOS"-Ziel.
- **Neue Features über cross-platform Crates** (nicht über macOS-APIs): Menüleiste/Tray →
  `tray-icon`, Benachrichtigungen → `notify-rust`, Autostart → `auto-launch`. Alle drei
  funktionieren auf macOS/Windows/Linux.
- **Bewusst macOS-spezifisch bleiben** (mit `#[cfg(target_os = "macos")]` bzw. No-op-Fallback auf
  anderen OS): das native App-Menü (`src/app_menu.rs`, damit ⌘X/⌘C/⌘V im WebView gehen — auf
  Windows/Linux erledigt das die WebView selbst) und die Main-Queue-Scheduling der Cookie-Probe in
  `src/login.rs` (der Run-Loop-Reentry ist ein macOS-Problem; andere OS proben direkt).
- Das eingebettete Login-WebView bleibt ein **`wry`-Kind-View** — `wry` ist cross-platform
  (macOS WKWebView / Windows WebView2 / Linux WebKitGTK) und wird über `raw-window-handle` an das
  eframe-Fenster gehängt; keine Änderung an der Grundarchitektur nötig.
- **Signierung ist nur auf macOS relevant.** Dort ist eine (selbstsignierte) Codesignatur nötig,
  damit Benachrichtigungen/Bundle-Identifier sauber funktionieren; Windows/Linux brauchen keine.
  Das macOS-`.app`-Bundling + Signatur wird als eigenes Paket geliefert.
- **Verifikationsgrenze:** Auf diesem Rechner ist nur der **macOS-Build lauffähig und testbar**.
  Windows/Linux werden durch `cfg`-Guards und cross-platform Crates abgedeckt und — soweit
  toolchainseitig möglich — per Cross-Compile-Check (`cargo check --target …`) belegt, **nicht**
  durch echte Läufe. Das ist eine bekannte, akzeptierte Einschränkung.
- **Verworfen:** „selbstsigniertes `.app` nur für macOS, sonst nichts" als Antwort auf den
  Benachrichtigungs-/Menüleisten-Wunsch (war die erste Teilantwort des Users, ersetzt durch
  „voll cross-platform").

## Modul-Struktur (User-Entscheidung 2026-10-13)

`src/main.rs` ist ein Monolith (~1780 Zeilen: State + Rendering + Upload-Worker + Formular +
Icons). Das ist der Engpass des Ports: fast jedes Paket ändert dieselbe Datei, also läuft alles
seriell. Deshalb wird **reiner Refactor ohne Verhaltensänderung** in Module gesplittet, danach
trifft jedes Paket möglichst eine Datei und Subagenten können parallel arbeiten.

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

## Internationalisierung (i18n) — User-Entscheidung 2026-10-13

- **Jetzt einführen, minimal:** direkt **nach REF-2** (dann sind die UI-Strings frisch pro Feature-Modul
  sortiert) eine zentrale String-Schicht `src/i18n.rs` mit `Lang`-Enum und einer Tabelle. Zuerst
  **de + en** vollständig; jede weitere Sprache ist danach **additiv** (kein Umbau).
- **Sprachwahl:** Default = OS-Locale, **Override in den Einstellungen**; im `Config` persistiert
  (`language`, serde-Default aus der OS-Locale, damit alte `config.toml` weiter lädt).
- **Umfang mittelfristig: lateinische Sprachen** (de, en, fr, it, es, pt). **Kein CJK** (egui's
  Default-Font ist Latin-only → bräuchte einen Noto-CJK-Font, gebündelt ~16 MB oder pro OS geladen)
  und **kein RTL** (egui hat kein Bidi-Layout; Arabisch/Hebräisch nur in der Webview, nicht in
  unserem Chrome).
- **Webview-Sprache:** **Marktplatz-Default belassen** — Amazon entscheidet die Seitensprache aus
  TLD + Cookies/`Accept-Language`. Wir erzwingen **nichts** (`lc-`-Cookie/`language`-Parameter wären
  unbelegt und könnten die Login-Seite verändern). Folge: deutsches UI + `amazon.com` ⇒ Loginseite
  englisch — bewusst akzeptiert.
- **Regel:** Kein hartkodierter UI-Text außerhalb von `src/i18n.rs`. Neue Strings kommen direkt
  zweisprachig dazu. Die Marktplatz-Ländernamen in `countries.rs` sind ebenfalls UI-Text (→ i18n).

## Dateinamen-Kodierung: NFC (User-Entscheidung 2026-10-14)

- **Der Upload-Name wird auf NFC normalisiert — und der Anzeigetext ebenso.** Belegter Anlass: der
  Server normalisiert **nicht**. Eine Abfrage in NFD findet nur die NFD-Datei, eine in NFC nur die
  NFC-Datei (`27_ÖFB-Reisinger_01.dng`: NFD-Treffer, NFC `count=0`). macOS liefert **NFD**, Windows
  **NFC** ⇒ derselbe Dateiname ist auf den beiden Plattformen ein **anderer String**.
- **Warum das zählt (belegt):** die Namensbehandlung ist **exakt/ordinal** — das Original sucht den
  Zielknoten per `GetNodeByNameAndParentIfExists` (exaktes `name = ?`), und unser Konfliktweg
  (`Ask`/`SaveAsNew`/`Replace`) hängt ebenso am Namen. Ohne Normalisierung findet „Ersetzen" auf dem
  Mac die von Windows (NFC) angelegte Datei **nicht** und legt eine zweite an — dieselbe Aktion
  verhält sich also je Plattform verschieden. Dazu die Anzeige: macOS-Namen (NFD) rendern in der
  eingebetteten Schrift als `O□FB`.
- **Was NFC *nicht* verhindert (Korrektur 2026-10-14):** ein zweiter Upload **desselben Inhalts**
  wird schon **serverseitig** abgewiesen — live belegt: `409` mit
  `errorCode = "DuplicatesConflictError"`, die Message nennt den `FILE_MD5` und
  `errorDetails.conflictNodeIds`. Ein früherer Schluss, im Konto lägen „beide Formen derselben
  Datei", war **falsch**: `27_ÖFB-Reisinger_01.jpg` existiert dort zweimal, aber als **verschiedene
  Fotos** (NFD 28.09.2026, 6.274.270 B, MD5 `1d16ed8f…` = unsere Datei; NFC 28.03.2026, 8.911.096 B,
  MD5 `c5f0f37e…` = anderes Bild mit gleichem Namen). Namensgleichheit aus zwei Shootings — **kein**
  Duplikat derselben Datei.
- NFC ist die gemeinsame Form: Windows und die Web-Seite sind NFC, und das Original sendet auf
  Windows ebenfalls NFC (auf macOS roh NFD — es normalisiert gar nicht, belegt in
  `docs/SPEC_FILENAME_ENCODING.md`).
- **Unverändert bleiben: der lokale Pfad und der MD5-Index.** Die Dedup-Schlüssel sind Pfade, die
  dürfen sich nicht verschieben; die Normalisierung betrifft **nur** den Namen, der an den Server
  geht, und den Anzeigetext.
- Damit ist die frühere Entscheidung „`unicode-normalization` als Abhängigkeit, NFC **nur** für den
  Anzeigetext" (2026-10-14) **erweitert** — sie war zum Zeitpunkt der neuen Frage noch nicht
  umgesetzt, und sie deckte den Upload-Namen nicht ab.
- **Verworfen:** NFD senden (macht Windows zum Sonderfall, der die Doppelungen erzeugt); nichts tun
  (die Doppelungen bleiben).

## Duplikate & Dateikonflikte: alle Varianten wie das Original (User-Entscheidung 2026-10-14)

Recon: `docs/SPEC_DUPLICATES.md` (Mechanismen, Erkennungsweg, 409, Gap-Tabelle) und
`docs/SPEC_CONFLICT_OPTIONS.md` (die vollständige Varianten-Liste mit Abnahmekriterium). Kernbefund:
das Original hat **zwei orthogonale** Mechanismen — **Inhalts-Dedupe** (MD5, **über Pfade hinweg**,
`GetNodeByMd5`) und **Namenskonflikt** (exakter Name + Parent, **pro Ziel**, global gibt es nichts).

**User-Entscheidung: alles umsetzen, was das Original kann.** Offene Punkte stehen als Checkliste im
Ledger (`AGENTS.todo.md` → „Duplikate & Dateikonflikte"); die vier belegten Lücken sind
Inhalts-Dedupe über Pfade, das Trennen der 409-Ursache (`MD5_DUPLICATE` vs. `NameAlreadyExists`),
das Persistieren von „Nicht noch einmal fragen" **pro Ziel** und das Reparieren von „Ersetzen":
`conflictResolution=OVERWRITE` wird vom Cookie-Host **nicht akzeptiert** (`POST` ⇒ 404, leerer
Body — live belegt), der Weg des Originals ist `PUT /v2/upload/<nodeId>?conflictResolution=NONE`
— **aber nur mit den Query-Parametern des POST** (`name`, `fileSize`, `kind`, `parentNodeId`; live
belegt 2026-10-15: ohne `fileSize` ⇒ **400 `MissingFileSize`**, mit dem POST-Satz gültig). Der `PUT`
unterliegt außerdem der **Inhalts-Dedupe**: exakt dieselben Bytes ⇒ 409 statt 200 — deshalb wird ein
Inhaltsduplikat **nie** per PUT „ersetzt", sondern bleibt „bereits im Konto".
**Verworfen:** beim rein pfadbasierten Index bleiben (er erkennt den NFD/NFC-Zwilling nicht).

## Arbeitsablauf (Build-Verify, seit 2026-10-13)

- **Strikt (User-Regel 2026-10-15):** Jede TODO-Umsetzung läuft **ausschließlich** über den
  Build-Verify-Loop — der Orchestrator editiert **keine** `src/`-Datei, auch nicht „mal eben".
  Anlass: am 2026-10-15 wurden drei `src/`-Dateien (`settings.rs`, `login.rs`, `i18n.rs`) direkt
  vom Hauptagenten editiert, statt sie zu delegieren — die Änderungen wurden zurückgenommen und
  über den Loop neu umgesetzt.
  Je Task: **(1)** Umsetzer-Subagent (frische Session) mit Ziel-Dateien + vollständiger
  Spezifikation; **(2)** `cargo build` grün, Ausgabe zitieren; **(3)** **unabhängiger**
  Verifier-Subagent (andere Session) prüft die **Substanz** — Diff gegen Auftrag, Invarianten,
  Blast Radius; **(4)** Commit **nach jeder** Verify-Runde, unabhängig vom Verdikt. Bei
  „CHANGES REQUIRED": fixen → erneut verifizieren → `git commit --amend`. Der Orchestrator darf
  **lesen/untersuchen** (Code, Logs, Original-Binary) und schreibt nur `AGENTS.md`,
  `AGENTS.todo.md`, `AGENTS.skills.md`.
- Der Hauptagent ist **Orchestrator**: er selbst schreibt nur `AGENTS.md`, `AGENTS.todo.md` und `AGENTS.skills.md`. Alle anderen Dateien bearbeiten Subagenten.
- **Ledger-Hygiene (User-Entscheidung 2026-10-15):** `AGENTS.todo.md` ist das Protokoll der **offenen**
  Arbeit, nicht die Geschichtsschreibung. Sobald ein Eintrag umgesetzt **und** unabhängig verifiziert
  ist (Verifier `APPROVE` + Commit), wird er **gelöscht** — nicht als „✅ erledigt" stehen gelassen;
  die Historie lebt in den Commits. **Vor dem Löschen** wird geprüft, ob der Eintrag Wissen trägt,
  das **nur dort** steht: solches Wissen wandert **zuerst** in die passende Datei unter `docs/`
  (Clean-Room-Spec zum Thema, `docs/PROTOCOL.md` für Endpunkte, `docs/features/LIVE_ACCEPTANCE.md`
  für Live-Befunde, `docs/DEV_NOTES.md` für Fallstricke unserer Codebase), erst dann wird gelöscht.
  **Niemals gelöscht:** offene Punkte, bewusst Zurückgestelltes und der ganze Abschnitt „Verworfen"
  (damit keine verworfenen Ansätze erneut gebaut werden).
- **Umsetzer:** `general`-Subagent mit Modell **`opencode-go/longcat-2.5-preview-free`**
  (User-Entscheidung 2026-10-16: „bitte mit longcat 2.5 preview als subagenten modell nehmen";
  $0/free, 1M ctx, vision-fähig). Modell-Geschichte dieses Projekts:
  `step-5-preview-free` → `deepseek-v4.1-flash` (2026-10-14) → `step-5-preview-free` (2026-10-15)
  → `mimo-v2.6-flash-free` (2026-10-15) → `deepseek-v4.1-flash-free` (2026-10-16, $0-Zwilling)
  → `opencode-go/deepseek-v4.1-flash` (2026-10-16, paid) → **`opencode-go/longcat-2.5-preview-free`**
  (2026-10-16). Bekommt Ziel-Dateien und die vollständige Spezifikation, frische Session pro Task.
  **Kein `#variant`-Suffix.** Der Twin `opencode/longcat-2.5-preview-free` bleibt ungenutzt
  (Namespace-Regel: alles auf `opencode-go/`); `opencode-go/longcat-2.0` ist **nicht** gemeint.
  **Kein** Wechsel ohne neue User-Entscheidung.
- **Verifier:** ein **anderer** `general`-Subagent mit **derselben** Vorgabe
  (`opencode-go/longcat-2.5-preview-free`), frische Session pro Runde; prüft die Substanz (Diff gegen
  Auftrag, Invarianten, Blast Radius), nicht die Gates.
  **Live-Werte bleiben beim Hauptagent:** Briefings an Subagenten enthalten **keine** Live-Werte
  (Cookies, Account-Parameter, Probe-Payloads) — Live-Probes gegen das Amazon-Konto laufen
  ausschließlich über den Hauptagent. Das war ursprünglich die Folge des Gratis-Modells
  (Trainings-Vorbehalt) und bleibt als **praktische Regel** bestehen — bei einem $0-Modell
  (LongCat ist free) gilt sie doppelt.
  **Stand 2026-10-16:** Subagenten laufen auf einem **$0-Modell** (LongCat 2.5 Preview Free) —
  die Zwischenphase auf dem bezahlten `deepseek-v4.1-flash` („dasselbe Modell wie die
  Hauptagenten") ist damit wieder **aufgehoben**. `step-5-preview-free` ist **tot** (vom Anbieter
  2026-10-15 entfernt) und darf nicht zurückkommen.
- Reihenfolge je Task: `cargo build` (grün, Ausgabe zitieren) → Verify → **immer committen**; bei „CHANGES REQUIRED": fixen → erneut verifizieren → `git commit --amend`.
- **Provider-Fehler („Endpoint is unavailable", User-Regel 2026-10-16):** die Subagent-Sessions
  brechen gelegentlich ab (nach Erfahrung **etwa die Hälfte** der Läufe), **die Arbeit überlebt** auf
  der Platte. Dann **nicht** lange diagnostizieren, sondern **„weiter"**: die Session fortsetzen
  bzw. eine kurze Folge-Session auf den vorhandenen Diff ansetzen (Ziel-Dateien, „arbeite darauf
  auf", grün bauen, Bericht). Kein Zurücksetzen, keine Neu-Erzählung des Auftrags.
- **CI/Release (User-Entscheidung 2026-10-13, hebt „Kein Push, kein CI" auf):** Ziel ist ein
  öffentliches GitHub-Repo (bei öffentlichen Repos sind die macOS/Windows/Linux-Runner kostenlos).
  - **Push/PR gegen `main`:** nur **Linux** — `cargo build` + `cargo test` als Pflicht-Gate; die
    anderen OS werden dort **nicht** gebaut.
  - **Release:** **Git-Tag `v*`** → CI-Matrix-Build **Windows + Linux**; alle Beine müssen grün sein
    (`needs:`), sonst kein Release → GitHub Release mit den Artefakten. **macOS wird NICHT im CI
    gebaut** (User-Entscheidung 2026-10-13: „nur lokal macOS bauen ist ok") — lokal über
    `packaging/make-app.sh` (arm64) und bei Bedarf manuell ans Release hängen.
  - **macOS-Signierung:** lokal `packaging/make-app.sh` (ad-hoc per Default, `CODESIGN_IDENTITY`
    für eine eigene Identität). Für ein Release an Dritte bräuchte es eine **„Developer ID
    Application"**-Identität aus dem **Apple Developer Program** (99 $/Jahr) + **Notarisierung**
    (`notarytool`) + Stapling — nur das akzeptieren fremde Macs. **Selbstsigniert/ad-hoc sind nur
    lokal vertraut**; für heruntergeladene Builds gilt der dokumentierte Gatekeeper-Workaround
    (Rechtsklick→Öffnen bzw. `xattr -dr com.apple.quarantine`, siehe `docs/RELEASE.md`).
  - Kein `codesign --deep` (von Apple veraltet/abgeraten). `packaging/make-app.sh` nimmt die Identität
    per `CODESIGN_IDENTITY`.
  - Secrets nur in `main`/Tag-Läufen (fork-PRs erhalten keine Secrets). Windows: Authenticode optional
    (sonst SmartScreen-Hinweis); Linux: keine Signatur. Details: `docs/RELEASE.md`.
- **Spec-first pro Feature (User-Entscheidung 2026-10-13):** Vor jeder Implementierung wird das
  Original-Binary (`/Applications/Amazon Photos.app`: .NET-DLL + Ressourcen, nur lesend) zum
  jeweiligen Feature untersucht — **Status 2026-10-15: es liegt wieder unter `/Applications` und ist
  vollständig lesbar** (`CFBundleShortVersionString = 11.3.0`, `CommitId = 1d7941b5`, 812 Einträge in
  `Contents/Resources`, `Contents/MonoBundle/Amazon Photos.dll` lesbar). Die frühere Warnung „liegt
  derzeit im Papierkorb" (2026-10-14) war **falsch** — wahr ist nur, dass `~/.Trash` per **TCC** nicht
  lesbar und leer ist. Vor jedem Recon also **prüfen**, ob das Bundle am dokumentierten Ort liegt,
  sonst laufen die Aufträge ins Leere.
  **Decompile läuft wieder (Befund + Fix 2026-10-16):** die frühere Notiz „.NET-Runtime fehlt" ist
  **veraltet** — `dotnet` ist installiert (Homebrew, Runtime 10.0.12), `ilspycmd` (11.1.0.9782,
  ICSharpCode.Decompiler 11.1.0.9782) liegt in `~/.dotnet/tools`. Es scheiterte nur am
  `DOTNET_ROOT`: der arm64-App-Host sucht `DOTNET_ROOT_ARM64`. **Dafür gibt es jetzt einen Wrapper
  `ilspycmd` in `~/.local/bin`** (im PATH, funktioniert auch in **nicht-interaktiven** Shells, wo
  `~/.zshrc` nicht geladen wird — wichtig für Subagenten). Aufrufe:
  `ilspycmd -l c "<dll>"` (Klassen, 2486), `ilspycmd -t <Vollständiger.Typname> "<dll>"`
  (ein Typ), `ilspycmd -o <dir> "<dll>"` (alles). DLL:
  `/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll`.
  **REA ist für diese Assembly kein Ersatz:** `inspect_managed_artifact` liefert
  `metadata-not-admitted` („Managed metadata exceeds the 67108864 byte representation budget"),
  und `search_android_classes` ist der falsche Weg (jadx/APK).
  **Recon-Regel:** neue Specs **belegen am Code** (Typ/Methode) und markieren jede Aussage als „am
  Code geprüft" vs. „aus älteren Dumps"; jede Spec trägt einen Anhang „Wie diese Spec geprüft
  wurde" mit dem exakten Aufruf. Die Specs stützen sich daher auf die dekompilierte Assembly und
  die zuvor gesicherten Ressourcen-Dumps.
  Das Ergebnis wird als **Clean-room-Spec** in `docs/` festgehalten
  (beobachtetes Verhalten in eigenen Worten/Tabellen, keine verbatimen Code-Auszüge, keine
  Personendaten) — analog zu `docs/UI_FLOWS.md` / `docs/UI_STATES.md`. Erst danach Umsetzer-Briefing.
- **Build-Verify ist nur grobe Basis:** Bauen → prüfen → committen gilt; Förmlichkeit
  (Runden-Protokoll, Footer-Schreibweise) nach Augenmaß, kein Selbstzweck.

## OpenCode-MCP: rea-Server (2026-10-13, User-Entscheidung)

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
- Protokoll/Begründung: `AGENTS.todo.md` „OpenCode-MCP: rea-Server".

## Repo

- Project / git repo name: **Prime Photos** (directory `prime-photos`).
- Git repository initialized on main.
- Build: `cargo build` / `cargo build --release`.
- **Warnungs-Baseline: `0`** (User-Entscheidung 2026-10-15; das Aufräumpaket stellt sie her, siehe
  Ledger). Sechs der sieben damaligen Warnungen waren toter Code bzw. Kosmetik, eine war
  `float_literal_f32_fallback` in `src/icons.rs` — **future incompatible**, in einer künftigen
  Compiler-Version ein harter Fehler. Ab hier gilt: **jede neue Build-Warnung ist ein Rückschritt**
  und wird im selben Task behoben; nichts wird pauschal mit `#[allow]` zugedeckt.
  **Erzwungen (User-Entscheidung 2026-10-15, „cargo warnings sollen als error behandelt
  werden"):** `[lints.rust] warnings = "deny"` in `Cargo.toml` — ab sofort ist **jede** Warnung ein
  **harter Build-Fehler**, kein Hinweis mehr. `[lints]` greift nur für **unser** Paket (`src/`,
  `build.rs`, Tests), **nicht** für Dependencies (die laufen ohnehin mit `cap-lints`); Rust ≥ 1.74
  ist erfüllt (edition 2024). **Einzige Ausnahme — und sie ist begründet:** `folders::supported_extensions()`
  (`src/folders.rs:29-37`) ist bewusst **nicht** verdrahtet und trägt ein punktuelles
  `#[allow(dead_code)]`; das überschreibt das paketweite `deny` (empirisch belegt in Verify Runde 1
  zu `c019528`). Es ist der vorbereitete Haken für einen späteren Datei-Picker und wird von **keiner**
  Codestelle aufgerufen — die frühere Notiz „das Symbol existiert nicht mehr" (2026-10-15) war
  **falsch**, der Verifier hat es gefunden. Neue Karteileichen bleiben verboten: nur dieser eine,
  dokumentierte Haken ist erlaubt.
  **Verworfen:** `.cargo/config.toml` mit `rustflags = ["-Dwarnings"]` (würde **auch** Deps hart
  nehmen, deren Warnungen wir nicht beheben können — ein fremder Patch würde unseren Build rot
  machen) und die Erzwingung **nur** in der CI (es gibt bislang **kein** Workflow, und lokal bliebe
  es weich statt hart).
- **`cargo fmt --check` ist kein Gate (Befund 2026-10-15):** Das Repo ist **nicht** fmt-clean als
  Baseline — `cargo fmt --check` meldet Abweichungen in **unangetasteten** Dateien (`build.rs`,
  `src/app.rs`: Import-Reihenfolge und Zeilenumbrüche). Ein fmt-Diff ist damit **kein**
  Regressionssignal und darf einen Task nicht blockieren; Regressionssignal ist allein die
  **Warnungs-Baseline 0** (oben).
