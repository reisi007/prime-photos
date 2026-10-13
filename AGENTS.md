# Prime Photo Uploader

Native macOS (Apple Silicon) Rust app (egui) for uploading photos to Amazon Photos.

## Source of truth

- GUI: `src/main.rs` (egui/eframe), theme in `src/theme.rs`
- Upload engine: `src/amzn_photo.rs` (cookie-based Amazon Photos web API)
- Persistent backup folders: `src/folders.rs` -> `~/Library/Application Support/amzn_photos_uploader/folders.toml`
- Login: `src/login_helper.rs` (Chrome DevTools-Protocol cookie capture) + paste fallback
- Config: `src/config.rs`, stored at `~/Library/Application Support/amzn_photos_uploader/config.toml`
- Protocol notes: `docs/PROTOCOL.md`

## Decisions

- GUI framework: egui/eframe (native ARM, single binary).
- Auth (GUI): browser-cookie login via CDP login helper (no manual cookie entry); a country/marketplace selector picks the Amazon TLD before login (default from system locale on first run).
- Auth (upload engine): browser cookies (session-id + x-acb/at-acb/ubid-acb) -> `content-<zone>.drive.amazonaws.com/v2/upload`.
- Flow note: this is the **web** flow. The desktop client uses a different **device** flow (`amzn_photos_desktop_*` signin -> `/auth/register` -> refresh token -> `/auth/token` -> `upload-photos.amazon.de/v2/upload`). Documented, not implemented.
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
