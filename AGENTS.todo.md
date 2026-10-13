# TODO / decisions ledger

## Implemented

- [x] Rust app boots natively on macOS ARM (egui GUI).
- [x] Theme modeled on Amazon Photos (white content, grey sidebar, teal accent, dark primary button).
- [x] Sidebar navigation: Datensicherung / Einstellungen.
- [x] Persistent backup folders (`folders.toml`), survive restarts.
- [x] Backup list with per-folder progress, status, ⋯ menu (jetzt scannen, Im Finder zeigen, Wiederholen, stoppen).
- [x] "Alle Sicherungsvorgänge anhalten/fortsetzen" (global pause).
- [x] Concurrent uploads (Workers + JoinSet), per-file rate limit (Bandbreite tab).
- [x] Login: automatic browser-cookie capture (Chrome DevTools Protocol) + manual paste fallback.
- [x] `.xmp` sidecars rejected; RAW formats supported (single source in `folders.rs`).
- [x] OpenAPI spec (`docs/openapi.yaml`) + protocol notes (`docs/PROTOCOL.md`).
- [x] `ilspycmd` installed for .NET decompilation; Screen-Recording grant works (self-screenshots).

## Open

- [ ] Decompile `Amazon Photos.dll` and answer: quota/usage shape, exact extension filter, watcher interval, concurrency defaults, conflict handling, video support.
- [ ] Real "Speichernutzung" bar — only relevant for Prime members; show nothing until we can detect the plan and fetch real quota (no placeholder).
- [ ] Menu-bar status item (like the original cloud dropdown: status, notifications, storage).
- [ ] Auto-upload watcher ("Neue Dateien automatisch hochladen", FSEvents) + upload interval setting.
- [ ] Videos upload (photos only so far).
- [ ] Upload catalog (md5 dedup) so restarts don't re-upload.
- [ ] "Details anzeigen" panel (nested/flat like the original).
- [ ] Login helper: currently CDP-based; verify against logged-in Chrome.
- [ ] Auto-refresh cookies on 401.
- [ ] Package as .app bundle.

## Verworfen

- mitmproxy live capture: kept only as a debugging tool; app uses the cookie upload path.
- Device registration / OAuth from scratch: replaced by browser-cookie auth.
- Standalone paste-only login: superseded by CDP browser login (paste stays as fallback).
