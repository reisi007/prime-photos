# Prime Photo Uploader

Native macOS (Apple Silicon) Rust app (egui) for uploading photos to Amazon Photos.

## Source of truth

- GUI: `src/main.rs` (egui/eframe)
- Upload engine: `src/amzn_photo.rs` (cookie-based Amazon Photos web API)
- Config: `src/config.rs`, stored at `~/Library/Application Support/amzn_photos_uploader/config.toml`

## Decisions

- GUI framework: egui/eframe (native ARM, single binary).
- Auth: browser cookie-based (session-id + x-acb/at-acb/ubid-acb), obtained via `config_setup.py`; no standalone OAuth.
- Upload endpoint: `https://content-<zone>.drive.amazonaws.com/v2/upload?...` (zone defaults `eu`).
- Supported photos: jpg, jpeg, png, heic, heif, tif, tiff, gif, webp, bmp plus common RAW (cr2, cr3, arw, nef, dng, raf, rw2, orf, srw, pef, and others). `.xmp` sidecars are explicitly rejected.
- File picker filters and folder scans both enforce `is_supported_image`.
- Uses reference implementation from entourloop/amazon-photo-uploader (MIT) for API details.

## Repo

- Git repository initialized on main.
- Build: `cargo build` / `cargo build --release`.
