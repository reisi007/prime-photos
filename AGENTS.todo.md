# TODO / decisions ledger

## Implemented

- [x] Rust app boots natively on macOS ARM (egui GUI).
- [x] Config load/save through GUI and cookie-based auth.
- [x] Folder + file selection with extension allow-list.
- [x] Reject `.xmp` files as images (sidecar filter).
- [x] Upload selected images to Amazon Photos.
- [x] Album creation/lookup and add-to-album after upload.
- [x] Progress + log UI.

## Open

- [ ] Call `renew_auth_token` on 401 to auto-refresh expired `cookie_x_amz_access_token`.
- [ ] Better error reporting for failed uploads (currently panics on unknown status).
- [ ] Add cancellation for in-flight uploads.
- [ ] Persist upload catalog (md5 dedup) like CLI.
- [ ] Configure user agent/country via GUI validation.
- [ ] Add HEIC EXIF orientation handling if needed.
- [ ] Package .app bundle for end users.

## Verworfen

- mitmproxy live capture approach: unstable for uploads; Amazon Photos endpoints reverse-engineered via docs + existing Rust reference instead.
- Device registration / OAuth from scratch: replaced by browser cookie auth.
