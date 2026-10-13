# TODO / decisions ledger

## Implemented

- [x] Rust app boots natively on macOS ARM (egui GUI).
- [x] Theme modeled on Amazon Photos (white content, grey sidebar, teal accent, dark primary button).
- [x] Sidebar navigation: Datensicherung / Einstellungen.
- [x] Persistent backup folders (`folders.toml`), survive restarts.
- [x] Backup list with per-folder progress, status, ⋯ menu (jetzt scannen, Im Finder zeigen, Wiederholen, stoppen).
- [x] "Alle Sicherungsvorgänge anhalten/fortsetzen" (global pause).
- [x] Concurrent uploads (Workers + JoinSet), per-file rate limit (Bandbreite tab).
- [x] Login: embedded WKWebView **in the main window** (wry child view of the egui window); cookies captured via an async main-queue probe; manual cookie entry removed. Window title "Login – Prime Photo Uploader", slim branded header above the page (documented deviation from the original).
- [x] Country / marketplace selector before login (default from system locale on first run).
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
- [ ] **Abgleich eines bestehenden Ordners:** beim Hinzufügen/Scannen prüfen, welche Dateien schon (per md5) in Amazon Photos liegen, "X% hochgeladen" korrekt anzeigen und nur Fehlendes hochladen.
- [ ] "Details anzeigen" panel (nested/flat like the original).
- [ ] Auto-refresh cookies on 401.
- [ ] Package as .app bundle.

## Verworfen

- Separates Login-Fenster (`ppu-login` Helper-Prozess mit eigener `tao`-Eventloop): wirkte ungewohnt und passte nicht zum Original, das im Hauptfenster anmeldet. Ersetzt durch die eingebettete Child-WebView im Hauptfenster.
- Synchrone Cookie-Abfrage in `update` (wry `WebView::cookies`): Panic durch winit-Reentrancy (dreht die NSRunLoop). Ersetzt durch `dispatch_async` auf der Main-Queue.
- mitmproxy live capture: kept only as a debugging tool; app uses the cookie upload path.
- Device registration / OAuth from scratch: replaced by browser-cookie auth.
- Standalone paste-only login: superseded by CDP browser login (paste stays as fallback).
