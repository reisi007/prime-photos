# Prime Photo Uploader

Native macOS (Apple Silicon) Rust app (egui) for uploading photos to Amazon Photos.

## Source of truth

- GUI: `src/main.rs` (egui/eframe), theme in `src/theme.rs`
- Upload engine: `src/amzn_photo.rs` (cookie-based Amazon Photos web API)
- Persistent backup folders: `src/folders.rs` -> `~/Library/Application Support/amzn_photos_uploader/folders.toml`
- Login: `src/login.rs` (embedded wry WKWebView as a child view of the main window; cookies captured with an asynchronous main-queue probe)
- Config: `src/config.rs`, stored at `~/Library/Application Support/amzn_photos_uploader/config.toml`
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
- **Subagenten führen nichts Neues ein (Projektregel, 2026-10-13):** Jeder Auftrag ist strikt auf die
  beauftragte Änderung begrenzt — keine zusätzlichen Features, Dateien, Abhängigkeiten, Refactorings
  oder „Verbesserungen" auf Verdacht. Unklarheiten werden zurückgemeldet, nicht eigenmächtig gelöst.

## Decisions

- GUI framework: egui/eframe (native ARM, single binary).
- Auth (GUI): embedded WKWebView login, **in the same window as the app** (like the original). egui's window hosts a wry `WKWebView` child view; no extra window, no external browser, no CDP. Amazon's sign-in page is loaded directly (`/ap/signin?…` with `assoc_handle=amzn_photos_web_<cc>`). Cookie capture must **not** run inside egui's `update`: wry's `WebView::cookies` is synchronous and spins the macOS run loop, which re-enters winit's event handler and panics — so probes are dispatched to the main queue and the result is picked up on the next frame. A country/marketplace selector picks the TLD (default from system locale on first run).
- Auth (login chrome): deliberate deviation from the measured original (`docs/UI_FLOWS.md` §2.1). While Amazon's page is open, a slim branded header (our upload logo + "Prime Photo Uploader" + "Abbrechen") sits above the webview and the window title reads "Login – Prime Photo Uploader". Reason: the app is a third-party tool, not Amazon's own, so its identity and a way to cancel must stay reachable. The measured window sizes stay the benchmark for the main app window.
- Auth (upload engine): browser cookies (session-id + x-acb/at-acb/ubid-acb) -> `content-<zone>.drive.amazonaws.com/v2/upload`.
- Flow note: this is the **web** flow (cookie-based). The desktop client's **device** flow (`amzn_photos_desktop_*` signin -> `/auth/register` -> refresh token -> `/auth/token` -> `upload-photos.amazon.de/v2/upload`) is documented in `docs/openapi.yaml` but not used.
- UI reference: `docs/UI_FLOWS.md` + `docs/UI_STATES.md` (clean-room spec from the decompiled app + resources). Use them when changing the UI.
- Folder reconciliation (observed in the client's local DBs under `~/Library/Application Support/Amazon Cloud Drive`): local SQLite state, not an HTTP call. `backup` table (schedule_path, relative_path, node_id, size, modified_date, status, last_scan_time) + `uploads` table (path, status, precomputed_md5, node_id) + `cloud.db` node cache, synced via `/drive/v1/changes` and `/drive/v1/nodes`. `BackupScanIntervalInHours = 336`.
- Desktop-client upload protocol (observed): `POST https://upload-photos.amazon.de/v2/upload?name=<urlenc>&fileSize=<n>&conflictResolution=NONE&kind=FILE&parentNodeId=<id>` with raw body, `x-amz-access-token`, `x-amzn-file-md5` (hex), `Content-MD5` (base64), `x-amz-clouddrive-appid`, `x-amzn-clouddrive-source`. Not used by the app yet (we use the cookie path); documented for parity.
- Desktop auth chain (observed): `POST api.amazon.com/auth/token` (`source_token_type=refresh_token` -> `access_token`) -> `GET drive.amazonaws.com/drive/v2/account/endpoint` -> `POST cdws.../drive/v1/account/source`.
- Supported photos: jpg, jpeg, png, heic, heif, tif, tiff, gif, webp, bmp plus common RAW (cr2, cr3, arw, nef, dng, raf, rw2, orf, srw, pef, and others). `.xmp` sidecars are explicitly rejected.
- Folder scans enforce `is_supported_image` (single source in `folders.rs`). There is **no file picker** in HEAD (only `FileDialog::pick_folder()`) and `supported_extensions()` is currently unwired.
- Uses reference implementation from entourloop/amazon-photo-uploader (MIT) for API details.
- GUI language: German (matches the original Amazon Photos UI).
- "Speichernutzung"/storage bar is intentionally omitted for now: it only applies to Prime members and needs a real quota fetch; no placeholder is shown.
- Account/avatar block in the sidebar: when signed in, show the Amazon profile picture (no API identified yet — placeholder until then) plus "Angemeldet als:" and "Abmelden"; when **not** signed in, an icon replaces the picture. The green/red status color coding is kept deliberately (user likes it).
- MVP scope (user decision 2026-10-13): login, upload-target creation ("Upload-Ziel erstellen": backup configuration with destination, content filter, interval, dedup, conflict handling), upload flow, settings dialog. The download flow is relevant but explicitly NOT part of the MVP.
- Dedup (user decision 2026-10-13): a **persistent local MD5 index in SQLite** (like the original) decides skips (same path + same MD5 → skip; same path + different MD5 → re-upload; new file → upload). Server 409 is only a safety net, treated as "Ignored/Zuvor hochgeladen", not an error (mirrors the original's client-side `PreUploadCheck`).
- Conflict handling (user decision 2026-10-13): all **four original options** per upload target (Nachfragen / Ignorieren / Als neue Datei speichern / Ersetzen), mapped to `conflictResolution` (`NONE`/`RENAME`/`OVERWRITE`). Replaces the current hard-coded `RENAME`.
- Backup configuration window (user decision 2026-10-13): build the full "Datensicherungseinstellungen" window now (destination, content filter, interval, dedup, conflict handling, save/cancel), and **share the form logic** with the create-backup flow (same dialog for create and edit) instead of a separate row-menu.

## Arbeitsablauf (Build-Verify, seit 2026-10-13)

- Der Hauptagent ist **Orchestrator**: er selbst schreibt nur `AGENTS.md`, `AGENTS.todo.md` und `AGENTS.skills.md`. Alle anderen Dateien bearbeiten Subagenten.
- **Umsetzer:** `general`-Subagent mit Modell `opencode-go/deepseek-v4.1-flash#high` (User-Entscheidung
  2026-10-13, ersetzt `opencode/cat-jam-free` — cat-jam-free lieferte „rate limit exceeded", außerdem
  fehlt für Modellwechsel in laufende Sessions die Reasoning-History) — bekommt Ziel-Dateien und die
  vollständige Spezifikation, frische Session pro Task.
- **Verifier:** ein **anderer** `general`-Subagent mit Modell `opencode-go/deepseek-v4.1-flash#high`, frische Session pro Runde; prüft die Substanz (Diff gegen Auftrag, Invarianten, Blast Radius), nicht die Gates.
- Reihenfolge je Task: `cargo build` (grün, Ausgabe zitieren) → Verify → **immer committen**; bei „CHANGES REQUIRED": fixen → erneut verifizieren → `git commit --amend`.
- **Kein Push, kein CI:** Das Repo hat kein Remote; Verifikation erfolgt manuell pro Auftrag, nicht per Test-Suite.
- **Spec-first pro Feature (User-Entscheidung 2026-10-13):** Vor jeder Implementierung wird das
  Original-Binary (`/Applications/Amazon Photos.app`: .NET-DLL + Ressourcen, nur lesend) zum
  jeweiligen Feature untersucht; das Ergebnis wird als **Clean-room-Spec** in `docs/` festgehalten
  (beobachtetes Verhalten in eigenen Worten/Tabellen, keine verbatimen Code-Auszüge, keine
  Personendaten) — analog zu `docs/UI_FLOWS.md` / `docs/UI_STATES.md`. Erst danach Umsetzer-Briefing.
- **Build-Verify ist nur grobe Basis:** Bauen → prüfen → committen gilt; Förmlichkeit
  (Runden-Protokoll, Footer-Schreibweise) nach Augenmaß, kein Selbstzweck.

## Repo

- Git repository initialized on main.
- Build: `cargo build` / `cargo build --release`.
