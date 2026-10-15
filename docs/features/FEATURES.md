# Features — what Prime Photos does

A prose reference of **our** implementation: what the app offers and how each
piece behaves. This file describes **Prime Photos**, not the original Amazon
Photos client.

Where behaviour was modelled on the original, the clean-room recon lives in
`docs/original/`; the wire protocol in `docs/protocol/`; the durable decisions
that govern all of this in `AGENTS.md`.

---

## 1. Sign in and stay signed in

Prime Photos signs in with **its own device flow** (PKCE), not by scraping a
browser session.

- The sign-in page opens **inside the app** in an embedded web view, under our
  own slim header (logo + "Prime Photos" + "Abbrechen"). While Amazon's page is
  open, the window title reads "Login – Prime Photos".
- A country/marketplace selector picks the TLD (`amazon.de`, `amazon.com`, …);
  on first run it is pre-filled from the system locale.
- Every time the sign-in view opens, its browsing data is **cleared first**.
  Without that wipe, Amazon's browser cookies survived a sign-out and let the
  next sign-in skip two-factor authentication.
- After a successful sign-in only the **refresh token** is stored — in the OS
  keychain (macOS Keychain / Windows Credential Manager / Linux Secret
  Service), under our own service name. The access token and the four session
  cookies live **in memory only**; nothing session-related is written to disk.
- The app **starts straight into the main UI** when a refresh token is stored.
  Access token and cookies are renewed in the background; the sign-in screen is
  only shown when there is no stored session.
- The access token is renewed in the background at 80 % of its remaining
  lifetime, and the web cookies are re-minted at start-up and after every
  successful renewal. While offline, nothing is given up: a connection error
  does not consume an attempt, and the app simply tries again on the next tick.
- An invalidated session clears the tokens and returns to the sign-in screen —
  no app restart required.
- Signing out deregisters the device with Amazon and deletes our session
  material.

## 2. Backup targets ("Sicherungsziele")

A backup target pairs a local folder with an upload policy.

- Source folder, name, and — per target — a content filter, an interval, a
  duplicate switch, and a name-conflict policy.
- Targets can be paused individually; the whole app can be paused globally.
- The dialog is shared between creating and editing a target.
- Fields whose backing service does not exist yet are shown **disabled** with a
  "Noch nicht verfügbar" hint, keeping the full original layout.

## 3. Upload

- Files are uploaded with the browser-cookie path to the user's zone endpoint.
- Only supported photo formats are accepted: jpg, jpeg, png, heic, heif, tif,
  tiff, gif, webp, bmp plus common RAW formats (cr2, cr3, arw, nef, dng, raf,
  rw2, orf, srw, pef, …). `.xmp` sidecars are explicitly rejected.
- All targets selected in one run share a single upload queue with a
  configurable concurrency (1–8). An optional "slow mode" forces a single
  connection for the current run only.
- Names are uploaded and displayed in **NFC** normalisation, because macOS
  hands out NFD while Windows and the web use NFC, and the server does not
  normalise. Without this, the same file would resolve differently depending on
  which machine uploaded it.
- Progress is aggregated across all targets, while each target row reports its
  own outcome. Results are split into **newly uploaded / already in the account
  / unchanged / errors** — "done" alone would hide the difference between a
  real upload and a skip.

## 4. Duplicates and name conflicts

Two orthogonal mechanisms, matching the original:

- **Content deduplication (MD5).** A file's identity is its content, tracked in
  a persistent local SQLite index. Same path + same MD5 → skipped as
  unchanged; same path + different MD5 → uploaded; content already known under
  a **different** path (a renamed or moved file) → skipped locally as "bereits
  im Konto" and recorded in the index so later scans take the fast path.
  Hashing is only done for files the stat check cannot resolve.
- **Name conflicts** (same name, different content) offer all four original
  policies: ask, ignore, save as a new file (server renames), or replace.
  "Ask" prompts per file with a "don't ask again for this folder" override and
  a 60-second timeout that falls back to ignoring.

The default for new targets is **replace**; a target created before that
decision keeps its stored policy. The interactive prompt is the safety net —
the local index is the primary defence, so the server conflict is rarely seen.

## 5. Change detection

- **OS file system events**, not polling: one watch per backup folder
  (FSEvents / ReadDirectoryChangesW / inotify).
- Per event only the affected file is examined — no full folder pass, no
  re-hashing of everything.
- A stat check (path + size + mtime) runs before hashing; only unknown or
  changed files are read and hashed.
- A safety net at start-up compares the folder's newest modification time with
  the one stored from the last run; if nothing is newer, the folder is skipped
  entirely — no hashing, no per-file queries.
- The event buffer is **unbounded**: events that arrive during a run or during
  a pause are parked and merged, never dropped, so camera imports of tens of
  thousands of files survive intact.

## 6. Drag & drop

A dropped selection is a **one-off upload**: no watch, no watermark, no
permanent backup. The queue is persisted in SQLite so a restart resumes it, and
it is deleted only after a completely finished run.

## 7. Pause

A global pause toggle ("Alle Sicherungsvorgänge anhalten" ↔ "fortsetzen") in
the backup header and in the tray menu. It is stored in the config, restored at
start-up, always switchable once a target exists or a run is live, and it also
blocks the event-driven detection.

## 8. Autostart

The "Nach dem Anmelden starten" login item is **on by default** at first use,
because without it no watcher runs after a reboot and change detection would
sleep. The OS state is the operative truth; the config only stores the intent,
so a hand-made entry is never fought.

## 9. Tray icon

The menu-bar icon appears **only while the user is signed in**. Without a
session there is no icon — and therefore also no close-to-tray behaviour:
closing the window quits the app.

- Glyphs follow the state: idle, active, paused, done, error.
- On macOS the glyph is a template image and follows the system appearance;
  elsewhere the icon is chosen by theme, falling back to the coloured logo
  when the theme is unknown.
- The menu offers "Fenster anzeigen", "Anhalten"/"Fortsetzen" and "Beenden",
  in the current UI language.

## 10. Settings

General settings (language, autostart, notifications), upload settings
(concurrency, slow mode, duplicate default, name-conflict default), and the
account block. Sign-in is automatic and is **not** a settings entry; signing
out lives in the sidebar account block.

## 11. Languages

German and English, with the OS locale as the default and an override in the
settings. The embedded sign-in page keeps the marketplace's own language —
Amazon decides it from the TLD and cookies, and forcing it would be unproven
and could alter the sign-in page.

## 12. Branding

Prime Photos is a third-party tool and never presents itself as an Amazon
app: no Amazon logos or wordmarks in our own chrome. Everything functionally
required stays — real hosts and URLs, header and cookie names, and the
"Mit Amazon anmelden" wording where a sign-in genuinely happens.

---

## Not part of the product

Bidirectional sync, downloads/restore, albums/favourites/trash/sharing,
Apple Photos / iPhoto / Aperture import, a media player, upsell/trial/quota
banners, auto-update, proxy and "hidden files" settings, and a crash-relaunch
launch agent are deliberately out of scope — see `AGENTS.md` →
"Scope-Grenzen".

The storage-usage bar is omitted for now: it only applies to Prime members and
would need a real quota fetch, so no placeholder is shown.
