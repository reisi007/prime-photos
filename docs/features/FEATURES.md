# Features — what Prime Photos does

A prose reference of **our** implementation: what the app offers and how each
piece behaves. This file describes **Prime Photos**, not the original Amazon
Photos client.

Where behaviour was modelled on the original, the clean-room recon lives in
[`../original/`](../original/); the wire protocol in [`../protocol/`](../protocol/);
development knowledge in [`../dev/`](../dev/).

---

## Index

### Session
- [login.md](login.md) — Sign-in (device flow / PKCE, web view wipe, marketplace, start gate, login chrome)
- [session-refresh.md](session-refresh.md) — Token refresh (80 %, background worker, cookie re-minting)
- [sign-out.md](sign-out.md) — Sign-out + aborting a running upload

### Backup
- [backup-targets.md](backup-targets.md) — Backup targets (folder dialog, content filter)
- [destination.md](destination.md) — Upload destination / mirroring
- [change-detection.md](change-detection.md) — Change detection (events, stat check, watermark)
- [duplicates-conflicts.md](duplicates-conflicts.md) — Deduplication and name-conflict handling

### Upload
- [upload.md](upload.md) — Upload engine (wire parameters, capture date, NFC filenames)
- [queue-progress.md](queue-progress.md) — Upload queue, concurrency, progress/ETA, result breakdown
- [pause.md](pause.md) — Global pause
- [one-time-upload.md](one-time-upload.md) — Drag & drop / one-time queue

### Desktop integration
- [autostart.md](autostart.md) — Login item (default on)
- [tray.md](tray.md) — Tray icon / menu / states
- [notifications.md](notifications.md) — OS notifications
- [settings.md](settings.md) — Settings page

### UI
- [window-layout.md](window-layout.md) — Window layout (panels, heights, cards)
- [error-handling.md](error-handling.md) — Error messages in the UI

### App meta
- [languages.md](languages.md) — Internationalisation (i18n)
- [branding.md](branding.md) — Name / branding / logo assets
- [about-licenses.md](about-licenses.md) — About window / third-party licenses
- [platform.md](platform.md) — Cross-platform portability

### Scope
- [scope.md](scope.md) — Scope boundaries and roadmap

---

## Not part of the product

Bidirectional sync, downloads/restore, albums/favourites/trash/sharing,
Apple Photos / iPhoto / Aperture import, a media player, upsell/trial/quota
banners, auto-update, proxy and "hidden files" settings, and a crash-relaunch
launch agent are deliberately out of scope — see [scope.md](scope.md).

The storage-usage bar is omitted for now: it only applies to Prime members and
would need a real quota fetch, so no placeholder is shown.
