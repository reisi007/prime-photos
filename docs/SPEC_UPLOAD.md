# Spec: Photo Upload (cookie-based web flow)

Status: backfill spec for **already implemented** behaviour. Clean-room: own words and
tables only. Protocol/reference details from `docs/PROTOCOL.md`, `docs/openapi.yaml`,
the official (read-only) `/Applications/Amazon Photos.app`, and the committed
source state (`git show HEAD:src/prime_photos.rs`, upload-worker parts of
`git show HEAD:src/main.rs`). The working tree is dirty with an unrelated in-flight
Pause and menu fixes; **nothing in this spec is based on the working tree**.

---

## 1. Scope

This spec covers the **web/cookie upload path** actually used by this app:

- `POST https://content-<zone>.drive.amazonaws.com/v2/upload` (raw file bytes,
  cookie auth) and everything that wraps it: capture-date handling (server-side
  derivation, §2.4), duplicate (409) handling, album (VISUAL_COLLECTION)
  creation/membership, and the concurrent worker scheduling in the GUI.
- It does **not** cover the desktop client's device flow
  (`upload-photos.amazon.de/v2/upload` with `x-amz-access-token`), which is
  documented in `docs/PROTOCOL.md` §2.17 / §3 but **not used** by us.
- It does **not** cover download, folder reconciliation, or dedup (see separate
  backlog/spec items).

---

## 2. Original-side reference (what the endpoint is)

### 2.1 Endpoint

| Item | Value |
|---|---|
| Method | `POST` |
| Host pattern | `https://content-<zone>.drive.amazonaws.com` (zone from `Config.zone`, default `eu`) |
| Path | `/v2/upload` |
| Auth | Cookie set (`session-id`, `x-acb<CC>`, `at-acb<CC>`, `ubid-acb<CC>`) plus `x-amzn-SessionId` header (see §2.3) |
| Success | `201 Created` with the created file node (JSON) |
| Duplicate | `409 Conflict` with `conflictNodeIds` (see §2.5) |

Query parameters (our URL construction order in `PrimePhotos::upload_picture`):

| Param | Source / value | Notes |
|---|---|---|
| `conflictResolution` | `RENAME` (hard-coded in our URL) | Original captures use `NONE` (duplicate avoidance). See open point O-3. |
| `fileSize` | byte length of body | Must equal body length. |
| `name` | `Path::file_name` (file name only) | **Not percent-encoded by us** in the `Url::parse` path; the `url` crate may re-encode some characters (unverified behaviour). Original PROTOCOL notes `+` for spaces. |
| `parentNodeId` | cached Web folder id (or `NONE` in dry run) | See §4.4. |
| `contentDate` | **not sent** | The server derives the capture date from the file (§2.4); a client value would be taken over verbatim. |

openapi.yaml `/v2/upload` additionally enumerates `kind` (enum `[FILE]`) and notes
`contentDate` optional (otherwise derived from EXIF/file time) — we omit both and
let the server derive the capture date (§2.4); observation: the server still
returns a `FILE` node. **Unverified** whether omitting `kind` is always accepted.

### 2.2 Required request headers (our committed client)

`PrimePhotos::build_common_headers` plus per-upload inserts:

| Header | Value (committed) |
|---|---|
| `Content-Type` | `application/x-www-form-urlencoded` (inserted for the upload; despite raw body) |
| `Accept` | `application/json, text/plain, */*` |
| `Cookie` | `session-id=<sid>; x-acb<CC>=<x>; at-acb<CC>=<at>; ubid-acb<CC>=<ubid>` |
| `x-amzn-SessionId` | session id |
| `x-amzn-file-md5` | lowercase hex MD5 of the body |
| `User-Agent` | from `Config.user_agent` (default Safari string) |

Notably, compared with the PROTOCOL.md desktop upload, we do **not** send:
`Content-MD5` (base64), `Content-Disposition: ... filename=<md5>`,
`x-amzn-RequestId` / `x-amzn-Trace-Id`, `x-amz-access-token`,
`x-amz-clouddrive-appid`, `x-amzn-clouddrive-source`. Cookie auth replaces the
OAuth/token set. This is the intended web-flow difference.

### 2.3 Auth

- All identity comes from the persisted `Config` (`~/Library/Application Support/prime_photos/config.toml`): `country`, `zone`, `session_id`, `cookie_x_acb`, `cookie_at_acb`, `cookie_ubid_acb`.
- There is no token refresh in this path; `cookie_x_amz_access_token` is stored
  but **not used** by the committed web upload.
- HTTP 401 is detected via `PrimePhotos::check_auth_error` and logged with a manual
  cookie-renewal hint. No automatic refresh (backlog).

### 2.4 Capture date: derived by the server

We send **no** `contentDate` (user decision 2026-10-16, live-tested against the
real account on our cookie host `content-eu.drive.amazonaws.com/v2/upload`):

- A `POST` **without** `contentDate` answers `201`, and the response carries a
  `contentDate` the server derived from the file itself — live: `DateTimeOriginal`
  **plus** `SubSecTime` (`2026-10-13T19:47:36.123Z`), **not** the differing EXIF
  `ModifyDate` (`2026-10-20`) and **not** the file mtime.
- A `POST` **with** a client-side value is taken over **verbatim**
  (`…36.000Z`): a value we computed would override the server's correct derivation.
- The replace `PUT` (a 409 name conflict resolved in place with
  `PUT /v2/upload/<nodeId>`) likewise carries no `contentDate`; the node keeps the
  server-derived value (`…36.123Z`).

This matches the original client: clean-room recon of the bundle
(`docs/SPEC_CAPTURE_DATE.md` §1–§7) shows it **never** sets `contentDate` — the
field stays `null` and the query builder discards it; §10 of that document records
the delta to our earlier client-side derivation.

Earlier behaviour (removed 2026-10-16): our own EXIF read (`DateTimeDigitized` →
`DateTimeOriginal` → `DateTime`) with a file-time fallback — including the rule
that aborted the upload when the file was younger than 10 minutes — was sent as
`contentDate`. That whole derivation is gone; the server is the single source of
the capture date.

### 2.5 Response handling

Success (`2xx`): the body is parsed as `ResponseUpload`; we return `body.id`
(the new node id). `ResponseUpload` maps most fields of the PROTOCOL §2.17 sample
(id, name, kind, version, dates, parents, parent_map as `ResponseNode { FOLDER: Vec<String> }`,
status, restricted, protectedFolder, contentProperties, properties, transforms, keywords,
ownerId, subKinds).

Our implementation treats the response metadata as **optional** — only `id`
stays a required field, deliberately: there is no `#[serde(default)]` on the
`ResponseUpload` struct itself and no `Default` derive, either of which would
let an answer without an `id` parse into an empty node id and pass as a
successful upload. `id` missing therefore remains a parse error. The reason for
the optionality is a live observation (2026-10-15): a **successful** upload of
an image without EXIF (a PNG, `c2.png`) answers with a valid node JSON
(`id`, `name`, `kind`, `version`, …) whose `contentProperties.image` carries no
`resolutionUnit`, which the previous strict shape rejected with a JSON parse
error — a successful upload counted as an error, and no index row written, so
the file was re-read and re-hashed on every later run. This affects **any**
EXIF-less type `is_supported_image` allows — live observed with a PNG; `bmp`,
`gif`, `tif`/`tiff` sit in the same group, and even `webp` is no safe bet,
because it *can* carry EXIF. The list is deliberately **not** exhaustive and
must not be read as one; inside a JPEG/CR3-only folder the gap is invisible.
Read from the answer is only `id` anyway.

`409 Conflict`: parsed as `ResponseUploadFail`:

```text
ResponseUploadFail {
  message: String,
  error_code: String,       // "errorCode"
  time_stamp: String,       // "timeStamp"
  request_id: String,       // "requestId"
  error_details: {
    conflict_node_ids: Vec<String>   // "conflictNodeIds"
  }
}
```

We return `Ok(conflict_node_ids.first())` — i.e. a 409 is **treated as success
with the existing node's id**. This is the duplicate short-circuit. If the list
is empty, `first().unwrap()` panics (unhandled edge).

Any other non-success status **except 401**: 401 returns `Err` (§2.3);
otherwise the body is read as text, logged at `debug!`, then the process
**panics** (no HTTP error returned). This is a real implementation hazard,
not a designed retry.

Errors during `.send()` / body read are converted with `Err(e.to_string())`, which
**drops the reqwest source chain** (open point O-1).

### 2.6 Album handling

Triggered in `run_upload` only after all workers finish, and only when the album
name (trimmed) is non-empty.

1. Collect `shared.uploaded_ids` (all ids returned by `upload_picture`, i.e.
   successes **and** first conflict ids from 409s).
2. Fresh `PrimePhotos` instance.
3. `get_album(name)`:
   - `list_albums()`: `GET https://www.amazon.<cc>/drive/v1/search` with
     `?asset=ALL&filters=type%3A(ALBUMS)&limit=200&lowResThumbnail=true&searchContext=customer&sort=['createdDate DESC']&tempLink=false&resourceVersion=V2&ContentType=JSON&_=<epoch-ms>`.
   - Exact name match in the returned `data`; on match return that node.
   - Otherwise `create_album(name)`.
4. `create_album`:
   - `POST https://www.amazon.<cc>/drive/v1/nodes`
   - body `QueryNodeCreateNode { kind: "VISUAL_COLLECTION", name, resource_version: "V2", content_type: "JSON" }` (JSON-serialized)
   - headers from `build_common_headers` (JSON content type).
   - dry run returns `ResponseSearchData::empty()`.
5. `add_to_album(album_id, uploaded_ids)`:
   - `PATCH https://www.amazon.<cc>/drive/v1/nodes/{album_id}/children`
   - body `QueryNodeModify { op: "add", value: [ids…], resource_version: "V2", content_type: "JSON" }`
   - uses `build_common_headers` with `Content-Type` overridden to
     `application/x-www-form-urlencoded` (same quirk as upload).
   - dry run: `Ok(())`.

`VISUAL_COLLECTION` is the album kind in both the decompiled client's local DB
queries and openapi `kind` enum. We treat "album" and `VISUAL_COLLECTION` as
synonyms.

---

## 3. Our implementation (committed state = HEAD)

### 3.1 Module map

| File / symbol | Role |
|---|---|
| `src/prime_photos.rs` `PrimePhotos` | HTTP client wrapper; all endpoints. |
| `PrimePhotos::new(conf, dry_run)` | builds reqwest `Client` (pool 10 idle/host, 90 s idle timeout, 60 s tcp keepalive, 300 s total timeout). |
| `PrimePhotos::upload_picture(pic_path, contents: Option<&[u8]>, md5sum: Option<&str>)` | full single-file upload (§2.1–§2.5). |
| `PrimePhotos::get_album`, `create_album`, `add_to_album`, `list_albums` | album handling (§2.6). |
| `PrimePhotos::update_web_pictures_dir_node`, `find_root_node`, `find_children_nodes` | resolves the destination (Web) folder. |
| `src/main.rs` `run_upload(...)` | builds queue, spawns workers, post-run album add. |
| `src/main.rs` `upload_worker(...)` | per-worker loop, pause polling, rate limiting. |
| `src/main.rs` `UploadShared` | shared counters/log (`total`, `current`, `done`, `folder_total`, `folder_done`, `failed`, `folder_failed`, `uploaded_ids`, `active`, `log`). |
| `src/config.rs` `Config` | cookie/auth config. |
| `src/folders.rs` `is_supported_image` | extension filter (single source of truth); enforced before items reach the queue. |

### 3.2 Single-file sequence (`upload_picture`)

1. If `web_dir_node` is `None` and not dry run, resolve it via
   `update_web_pictures_dir_node` (root → Pictures → Web) and cache.
2. Compute `(contents, md5, size)` either from caller-provided slices/hash or
   by reading the file + `md5::compute` (hex string).
3. No capture date is computed; the request sends no `contentDate` — the server
   derives it (§2.4).
4. Choose `parent_node` = cached id (or `"NONE"` in dry run).
5. Build URL (§2.1) and headers (§2.2).
6. Dry run: return `Ok("")` with no request.
7. `POST` raw bytes; on transport error return `Err(e.to_string())`.
8. On 2xx: parse `ResponseUpload`, return `Ok(id)`.
9. On 409: parse `ResponseUploadFail`, return `Ok(first conflict id)`.
10. Other status: read body, log, `panic!()`.

### 3.3 Destination folder resolution (`update_web_pictures_dir_node`)

- `find_root_node()` → first node of `isRoot:true` (panics on request error;
  returns empty + error-log if data is empty).
- `find_children_nodes("kind:FOLDER+AND+status:(AVAILABLE*)+AND+name:\"Pictures\"", root_id)`
  → first child.
- Then same for `name:"Web"` under Pictures.
- Result cached in `web_dir_node: Option<ResponseSearchData>` and reused for
  every subsequent file in the same `PrimePhotos` instance.

`find_nodes_internal` uses `GET` with query params and `build_common_headers`.
Dry run returns empty data (so a dry-run `PrimePhotos` has no cached node and
uploads report parent `NONE`).

### 3.4 Worker scheduling (`run_upload`)

- Queue: `VecDeque<(usize, PathBuf)>` built from `items`.
- `workers = concurrency.max(1).min(count.max(1))` (at least one worker even
  for an empty queue, which exits immediately on `pop_front() == None`).
- Spawns `workers` tasks via `JoinSet`, each running `upload_worker`.
- Each worker:
  - parks while paused (400 ms `borrow_and_update` loop) and races each file's
    upload future against `pause_rx.changed()` (biased `select!`);
  - `pop_front` one item; if `None`, break;
  - stat size, set `shared.current`;
  - times the upload from `Instant::now()`;
  - calls `upload_picture(&path, None, None)`;
  - on `Ok(id)`: `shared.done += 1`, `folder_done[idx] += 1`, push id to
    `uploaded_ids`, log `OK    {path}`;
  - on `Err(e)`: `shared.failed += 1`, `folder_failed[idx] += 1`,
    log `FEHLER {path}: {e}`;
  - rate limit: if `limit_kbps > 0 && size > 0`, sleep for
    `size / (limit_kbps*1024) - elapsed` if positive.
- Joins all workers.
- If `album.trim()` non-empty **and** `!uploaded_ids.is_empty()`: new uploader,
  `get_album`, `add_to_album` with `uploaded_ids`; results ignored (`let _ =`).

### 3.5 Concurrency with paused flag

`paused` is a `tokio::sync::watch::Sender<bool>` shared between the GUI toggle and
workers (commit `4a47795`). The toggle flips it (`send_replace(!all_paused)`).
Each worker holds a `watch::Receiver` and races its per-file `upload_picture`
future against `pause_rx.changed()` (biased `select!`): on pause the in-flight
future is dropped (request cancelled) and the item is `push_front`-ed exactly once;
the per-file rate-limit sleep is raced the same way. Parked workers keep
`active == true`, so finalization still only runs once the queue drains.
Residuals F-2 (pacing can skip across pause/resume) and F-3 (abort discards
partial bytes; `RENAME` duplicate risk) are recorded in `AGENTS.todo.md`.

---

## 4. Behaviour mapping (original concept → our code)

| Concept | Our implementation |
|---|---|
| Upload raw bytes to content host | `PrimePhotos::upload_picture` → `POST {zone}…/v2/upload` |
| Parent folder node id | cached `web_dir_node` (Root/Pictures/Web) |
| MD5 of body | `md5::compute` hex, sent as `x-amzn-file-md5` |
| Capture/content date | derived by the server; no `contentDate` sent (§2.4) |
| Duplicate detection (server 409) | `ResponseUploadFail.error_details.conflict_node_ids`, returns first id |
| Album = visual collection | `kind: "VISUAL_COLLECTION"` node + `PATCH …/children` `op: add` |
| Concurrent workers + bandwidth cap | `run_upload` / `upload_worker` |
| Pause | `paused: watch::Sender<bool>`; in-flight future raced and dropped on pause (fixed `4a47795`) |

---

## 5. Known open points (recorded, not fixed here)

From `AGENTS.todo.md` (P1 upload items):

- **O-1** Full reqwest error chain is dropped (`Err(e.to_string())`) — root
  cause (DNS / reset / body stream / timeout) lost; no `{e:?}` / `source()` logging.
- **O-2** *(fixed in `4a47795`)* Pause now aborts in-flight work: workers race the
  upload future against the watch channel, drop it and requeue the item. Remaining
  residuals F-2/F-3 are tracked in `AGENTS.todo.md`.
- **O-3** `conflictResolution=RENAME` creates a copy on conflict; the original
  uses `NONE` (avoid duplicates).
- **O-4** Videos unsupported: only `is_supported_image` extensions pass; RAW list
  accepted, `.xmp` rejected. Video extensions and differing upload params unknown.
- **O-5** No retry/backoff for transient errors (network, 5xx, 429, 408).
- **O-6** Non-2xx/non-409 responses `panic!()` after reading the body instead
  of returning an error.
- **O-7** Empty `conflict_node_ids` on a 409 panics on `first().unwrap()`.
- **O-8** `name` is not URL-encoded in our URL (original PROTOCOL notes `+` for
  spaces); compatibility unverified.
- **O-9** Headers use `application/x-www-form-urlencoded` for the raw body;
  PROTOCOL desktop flow uses `application/octet-stream`. Unverified whether this
  matters to the server.
- **O-10** No auto-refresh of cookies on 401 (backlog).
- **O-11** `kind=FILE` query param omitted; unverified whether always accepted.

---

## 6. Unverifiable / unknown points

- Exact tag order the server applies when deriving `contentDate` from the file:
  live-proven for one sample to be `DateTimeOriginal` + `SubSecTime`, but the full
  precedence is not mapped. The original client never sets `contentDate` at all
  (`docs/SPEC_CAPTURE_DATE.md`).
- Whether the web host requires the omitted desktop headers (`Content-MD5`,
  `Content-Disposition`, trace ids, source id).
- Error response shapes for non-409 failures (none observed in captures; our
  `ResponseUploadFail` struct is modelled on the reference implementation /
  unverified shape; see F-7 in the verifier report).
- Large-file behaviour: captures only small files; multipart/ranged unknown.
- Whether `parentNodeId` resolution differs when the Web folder does not yet exist
  (we assume it exists after a first upload; otherwise `update_web_pictures_dir_node`
  returns empty data and panics on `.unwrap()`).
- Album list pagination (fixed `limit=200`); accounts with >200 albums unhandled.

---

## 7. Non-goals

- Implementing any of O-1..O-11 or unverified items.
- Changing the upload endpoint or auth scheme.
- Download flow, dedup persistence, video support.
- Test suite / CI (repo has none).

---

Spec owner: orchestrator backfill (2026-10-13). Write-only deliverable:
`docs/SPEC_UPLOAD.md`.
