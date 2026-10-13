# Prime Photo Uploader

Native macOS (Apple Silicon) Rust app (egui) for uploading photos to Amazon Photos.

## Source of truth

- GUI: `src/main.rs` (egui/eframe), theme in `src/theme.rs`
- Upload engine: `src/amzn_photo.rs` (cookie-based Amazon Photos web API)
- Persistent backup folders: `src/folders.rs` -> `~/Library/Application Support/amzn_photos_uploader/folders.toml`
- Login: `src/login.rs` (embedded wry WKWebView as a child view of the main window; cookies captured with an asynchronous main-queue probe)
- Config: `src/config.rs`, stored at `~/Library/Application Support/amzn_photos_uploader/config.toml`
- Protocol notes: `docs/PROTOCOL.md`

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
- File picker filters and folder scans both enforce `is_supported_image` (single source in `folders.rs`).
- Uses reference implementation from entourloop/amazon-photo-uploader (MIT) for API details.
- GUI language: German (matches the original Amazon Photos UI).
- "Speichernutzung"/storage bar is intentionally omitted for now: it only applies to Prime members and needs a real quota fetch; no placeholder is shown.

## Repo

- Git repository initialized on main.
- Build: `cargo build` / `cargo build --release`.
