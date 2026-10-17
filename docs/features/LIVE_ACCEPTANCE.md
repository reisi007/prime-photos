# Live acceptance — device session path (2026-10-15)

Evidence that the rebuilt session flow (`../original/SPEC_LOGIN.md` § 3.8,
`../protocol/PROTOCOL.md` § 2.1) works against the real Amazon Photos service.

This file exists because `AGENTS.md` holds **rules only** — every "this was
proven live" statement moved out of it and lives here instead.

## What was proven

| # | Behaviour | Result |
|---|---|---|
| 1 | Sign in once through our own webview (device flow, PKCE) | accepted |
| 2 | Restart the app **without** signing in again → straight into the main UI | pass |
| 3 | Account name and profile picture appear in the sidebar | pass |
| 4 | Uploads run and finish at 100 %, folder reports "Alles aktuell" | pass |
| 5 | Restart resumes the previous run instead of starting over | pass |

The device path is therefore live-proven: Amazon accepts our client identity
(`app_name` / `app_version` / `device_type` per `AGENTS.md`, our own
`device_serial`).

## Session material at rest

- The **refresh token** lives in the macOS keychain under our own service and
  account — never under `com.amazon.clouddrive.mac` / `amzp-rt`.
- The **access token** and the four session cookies (`session-id`, `x-acb*`,
  `at-acb*`, `ubid-acb*`) live **in memory only**. The corresponding `Config`
  fields are `#[serde(skip)]`, so `config.toml` carries **no** session material.
  Verified: no cookie value appears in the config file.

This mirrors the original, which keeps the same material in
`NSHttpCookieStorage` rather than on disk.

## Log evidence

From `~/Library/Application Support/prime_photos/prime-photos.log`:

```
Gespeicherte Sitzung gefunden – Access-Token wird erneuert.
Access-Token aufgefrischt (3600 s gültig).
Web-Cookies erneuert.
```

That is the full start-gate sequence: keychain token found → access token
renewed → web cookies minted, all in the background while the main UI is
already usable.

## Live findings that changed the implementation

| Finding | Consequence |
|---|---|
| `expires_in` arrives from `/auth/register` **and** `/auth/token` as the JSON **string** `"3600"` | the parser accepts string **and** number in both places |
| An `openid.oa2.authorization_code` is **single-use** | the redemption path repeats **only** when the request never reached the server (network error), **never** after a server answer |
| `wry::WebView::cookies()` blocks the main thread up to 1 s and always times out when called from a `DispatchQueue::main` block | the cookie probe and the whole web/cookie fallback were removed; the device flow is the only sign-in path |
| Amazon's browser cookies survived a sign-out (`~/Library/HTTPStorages/prime-photos.binarycookies`) and let the next sign-in skip 2FA | opening the login webview now clears its browsing data first (`../original/SPEC_LOGIN.md` § 3.3) |
| The 10-minute EXIF guard rejects an EXIF-less file that is younger than 10 minutes **before** an upload URL is even built — live-reproduced on 2026-10-15 and **original parity** (`../original/SPEC_UPLOAD.md`), not an invention of ours | for live test runs, age the file first: `SetFile -d "01/01/2020 12:00:00" -m "01/01/2020 12:00:00"`. Without that, the run shows an error line in the UI log only and the row keeps `status = uploading`; a later event/watermark run picks the file up once it is old enough |
| A stray node is removed with `DELETE https://www.amazon.<cc>/drive/v1/nodes/<id>` ⇒ **202 `{}`**; `PUT …/drive/v1/trash/<id>` does **not** exist on the cookie host (⇒ 404) | test targets and leftover nodes are cleaned up with the DELETE (verbs collected in `../protocol/PROTOCOL.md` § 5) |

## Replace `PUT` accepted live (2026-10-16)

The `PUT /v2/upload/<nodeId>` replace path (Welle 2, item 2.2b) **is now
exercised against the live account** — deliberately, on throwaway test nodes
only, with a genuine name conflict, and cleaned up afterwards.

| Step | Call | Result |
|---|---|---|
| 1 | `POST …/v2/upload` a new file ⇒ node C (no `contentDate`) | **201**, response `contentDate = …T19:47:36.123Z` |
| 2 | `POST …/v2/upload` the **same name**, different bytes | **409** `NameAlreadyExists`; `errorDetails.conflictNodeIds[0]` names C |
| 3 | `PUT /v2/upload/<C>` with the POST's query set, **without** `contentDate` | **200**, **same** node id, content swapped (size + MD5 of the second file), node's `contentDate` **unchanged** `…36.123Z` |
| 4 | cleanup: `DELETE …/drive/v1/nodes/<id>` | **202**; a re-query over the folder children finds none of the test names |

So the `PUT` works **without** a `contentDate` and leaves the node's capture date
standing. The earlier query-parameter requirement stands as recorded
(`fileSize` missing ⇒ 400 `MissingFileSize`). Full write-up:
`../original/SPEC_CAPTURE_DATE.md` § 12.3; endpoint: `../protocol/PROTOCOL.md` § 2.17.

## Streamed upload body needs an explicit `Content-Length` (Runde D, 2026-10-16)

The per-file byte progress (Befund 11c) streams the POST body in chunks
(`src/prime_photos.rs::ChunkedBody`). Live-tested against the account, a
streamed request **without** a length header is rejected:

| Request form | Result |
|---|---|
| `POST https://content-eu.drive.amazonaws.com/v2/upload?…` with `Transfer-Encoding: chunked` (no `Content-Length`) | **HTTP 400**, **no** node created |
| The same upload with an explicit `Content-Length` | **HTTP 201** (node created) |

So the host requires `Content-Length`; it does **not** accept a chunked body.
A `reqwest::Body::wrap_stream(…)` has `BodyLength::Unknown`, and hyper only
avoids chunked encoding when a length header is present
(`hyper/src/proto/h1/role.rs`, `Some(BodyLength::Unknown)` ⇒ `Encoder::length`).
The streamed upload therefore sets the header itself
(`src/prime_photos.rs::upload_body`). Regression-guarded by
`streamed_upload_request_is_not_chunked_and_carries_content_length`, which
inspects the raw request head of the real builder against a loopback listener:
`Content-Length` present and naming the file size, `Transfer-Encoding` absent.

## Live acceptance of waves 5.21–5.23 (2026-10-17)

The installed bundle (version 0.1.0, `SMAppService` autostart, the `NSLocale`
language fix) was run by the user. Two of the three due checks passed; the third
is a bug the user wants recorded.

| # | Check | Result |
|---|---|---|
| 1 | **System language** — start the app from Finder / by double-click | **pass** — the app starts in **German** (system locale `de_AT`) |
| 2 | **Autostart** — toggle the login-item switch off and on once | **pass** — the entry appeared **immediately** under *System Settings → General → Login Items*, **no** extra approval was needed |
| 3 | **"Ordner nicht gefunden"** — a source folder on a removed drive | **label pass, resume bug** — the row correctly showed **"Ordner nicht gefunden"** in **AMBER** (not "ANGEHALTEN"), **but** the run did **not** resume on its own when the drive came back |

Point 3 is the bug: on drive return there is **no** automatic resume, and the row
state is inconsistent — one folder sat on "Warten", one showed no status at all.
The intended behaviour is the original's device-return path
(`../original/SPEC_MISSING_SOURCE.md` §3.3.3/§3.3.6 — `HandleDeviceArrival` /
`HandleDisconnectedDevice`: refresh the path, lift the pause,
`ResumeUploadsForSchedule`; `../original/SPEC_BACKGROUND_SYNC.md` §3
"external-drive arrival", `ForceScan = !IsPaused`). Open item: `AGENTS.todo.md`.

Not live-checked: whether `LSMinimumSystemVersion 11.0` versus autostart from
macOS 13 on plays a role.

