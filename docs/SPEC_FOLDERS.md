# SPEC_FOLDERS — Folder Scan, Filter and Persistence (Clean-Room Spec)

Backfill specification for the **already implemented** folder scan / filter / persistence
behaviour, mapped against the official Amazon Photos desktop client's *backup-schedule*
model. Applies to committed state only (`git show HEAD:src/folders.rs`,
`HEAD:src/main.rs` scan/start/finalize sections); working-tree edits are ignored.
Extension *taxonomy* (which extensions fall into Photos / Videos / Other) is owned by
`docs/SPEC_DEDUP.md`; values are only summarised here (canonical owner is that doc).

Companion docs: `docs/UI_FLOWS.md` (§4.5–§4.7), `docs/UI_STATES.md` (§3.2–§3.5).

## 1. Scope and sources

**Our side (committed):** `src/folders.rs` (`is_supported_image`, `supported_extensions`,
`scan_images`, `FolderStatus`, `BackupFolder`, `FolderStore`, `store_path`) and the
folder-handling methods of `src/main.rs` (`add_backup_folder`, `start_folder_upload`,
`finalize_upload`, `apply_action`, `backup_ui`, `status_text`).

**Original side (read-only):** `/Applications/Amazon Photos.app` (`Amazon Photos.dll`,
.NET/Mono, version 11.3.0) via `ilspycmd`; local state under
`~/Library/Application Support/Amazon Cloud Drive` (SQLite `*-schedules.db`,
`*-uploads.db`, `*-cloud.db`, `*-settings.json`); German labels from
`de.lproj/Localizable.strings`.

**Confidence convention.** "Observed" = read directly from a source above.
"Inferred" = reasoned. Values that could not be determined are marked **unknown**.
No verbatim decompiled code is reproduced; identifiers are named only to the extent
needed to describe persisted state. Absolute file paths and account identifiers are
omitted.

## 2. Our implementation (committed state)

### 2.1 Extension filter (`is_supported_image`)

Single source of truth for "is this file a photo we may upload?" in `src/folders.rs`.
Commit `HEAD` has only a folder picker (`FileDialog::pick_folder()`); there is no
file picker, and `supported_extensions()` is currently unwired (see §7 Q1).

- Extension is taken from `Path::extension()`, lowercased. A file with **no
  extension returns `false`**.
- `.xmp` is **explicitly rejected** even though it is a common sidecar of RAW
  files: if the lowercased extension equals `"xmp"`, the result is `false`.
- Otherwise the extension must be a member of one of two hard-coded sets:

| Set | Extensions |
|---|---|
| Standard | `jpg jpeg png heic heif tif tiff gif webp bmp` |
| RAW | `cr2 cr3 arw nef dng raf rw2 orf srw pef 3fr iiq erf mos kdc dcr mrw raw` |

- `supported_extensions()` returns the concatenation of both sets (as `&'static str`)
  for use by native dialogs. It contains the same entries, **excluding** `xmp`.

Notes:

- Total supported extensions: 10 standard + 18 RAW = 28. Videos are **not** in either
  set, so `scan_images` silently skips them (see §4.1).
- No MIME lookup is performed; classification is purely by extension string.
- Case-insensitive by construction (lowercased before comparison).
- Compare with the original: Amazon's `SupportedMimeMapping` classifies by
  lower-cased filename **extension** loaded from an embedded JSON resource
  (`supported-content-types.json`), and `.xmp` is categorised there as
  `IMAGE_SIDECAR` / `VIDEO_SIDECAR` (sidecar group; JSON keys
  `image_sidecar`/`video_sidecar`). See §3.4 and `docs/SPEC_DEDUP.md` for the
  full taxonomy.

### 2.2 Recursive scan (`scan_images`)

```
scan_images(dir) -> Vec<PathBuf>
```

- Uses `walkdir::WalkDir::new(dir)` over the directory tree.
- Unreadable entries are dropped silently (`filter_map(Result::ok)`).
- Only file-type entries are kept (`e.file_type().is_file()`); directories and
  non-file entries are skipped, including symbolic links to files (the entry's file
  type is the link itself).
- Each candidate path is then filtered by `is_supported_image`; non-matching files
  are dropped.
- Returns paths in `WalkDir`'s traversal order; no sorting or de-duplication is
  applied.
- Empty result (no supported images) is a normal outcome, but `start_folder_upload`
  treats it as a user-facing log event ("Keine Fotos … gefunden") and aborts the
  run without changing folder state.

### 2.3 Persistence (`folders.toml`)

| Item | Value |
|---|---|
| Path | `dirs::config_dir()` (on macOS `~/Library/Application Support`) + `prime_photos/folders.toml`; falls back to `std::env::temp_dir()` when `config_dir()` is `None` |
| Format | TOML (pretty-printed via `toml::to_string_pretty`) |
| Load | `std::fs::read_to_string` then `toml::from_str`; on read/parse error, defaults to empty store |
| Save | creates parent dir if missing, then writes full file (all folders rewritten on every save) |
| Schema root | `FolderStore { folders: Vec<BackupFolder> }` |

`BackupFolder` fields (all with serde defaults where noted):

| Field | Type | Default | Meaning |
|---|---|---|---|
| `path` | `PathBuf` | — (required) | absolute folder path |
| `name` | `String` | `""` (then set in `BackupFolder::new` to last path component, fallback full path display) | display name |
| `enabled` | `bool` | `true` | stored flag; see §2.4 |
| `total_files` | `usize` | `0` | images found during last `start_folder_upload` |
| `uploaded_files` | `usize` | `0` | successful uploads from last finalized run |
| `last_updated` | `String` | `""` | formatted local timestamp of last finalization |
| `status` | `FolderStatus` | `Idle` | serialized as lowercase (`idle`, `scanning`, …) |

`FolderStore` helpers:

- `add(path)`: returns `false` (and does nothing) if a folder with an **equal
  path** already exists; otherwise pushes `BackupFolder::new(path)` and returns
  `true`.
- `remove(path)`: `retain` by path equality; returns whether the count changed.

### 2.4 Lifecycle (`FolderStatus`)

```
enum FolderStatus { Idle (default), Scanning, Uploading, Paused, Done, Error }
```

Observed assignments in committed code:

| Transition | Where | Notes |
|---|---|---|
| → `Uploading` | `start_folder_upload`, for each selected folder | `total_files` set to scan count; persisted |
| → `Done` | `finalize_upload`, if that folder had no recorded failures | `uploaded_files` = done count |
| → `Error` | `finalize_upload`, if recorded failures > 0 | idem |
| `Scanning`, `Paused`, `Idle` | (assigned nowhere in this flow) | enum + render branch exist; `Scanning` is never set in HEAD; `Paused` is never set per folder even though the global pause uses it in the UI text |

Rendered text (`status_text`):

| Status | Text | Colour |
|---|---|---|
| `Uploading` | `"{n}% hochgeladen"` | teal |
| `Done` | `"100% hochgeladen"` | green |
| `Paused` | `"Angehalten ({n}%)"` | amber |
| `Error` | `"Fehler"` | red |
| `Scanning` | `"Wird vorbereitet …"` | muted |
| `Idle` (total > 0) | `"{n}% hochgeladen"` | muted |
| `Idle` (total = 0) | empty | muted |

`percent()` = `uploaded_files / total_files`, clamped to `[0,1]`; `0` when
`total_files == 0`.

Progress displayed in `backup_ui` prefers the in-memory upload-state entries
`folder_total` / `folder_done` when present, else the persisted
`BackupFolder.total_files` / `uploaded_files` (unconditional `unwrap_or`, not
gated on `active`; the maps are not cleared after a run).

### 2.5 Per-folder progress accounting

- On "Datensicherung hinzufügen …": native `FileDialog::pick_folder()`. If chosen
  and `add` succeeds, the folder is saved, then **immediately** uploaded via
  `start_folder_upload([idx])` (new backups scan+upload on creation).
- `start_folder_upload(indices)`:
  - refuses to start if an upload is `active` or the user is not logged in;
  - for each index, runs `scan_images`, stores `per_folder[index] = count`, collects
    `(index, path)` items;
  - on empty item list, logs and returns **without** status or persistence changes;
  - sets shared upload state `active/total/done/failed` and `folder_total`;
    clears `folder_done`/`folder_failed`;
  - resets `paused = false`, `was_active = true`;
  - sets each folder's `status = Uploading`, `total_files = count`, saves.
- `finalize_upload()` runs once when the async upload transitions from active to
  inactive (`was_active && !active`). It reads `folder_done` / `folder_failed`,
  and for each folder **that appears in `folder_done`**:
  - `uploaded_files = folder_done[index]`
  - `last_updated = now` (format `"%d.%m.%Y %H:%M"`, local)
  - `status = Error` if `folder_failed[index] > 0`, else `Done`.
  - then saves the store.
- Folders that did not participate in the run keep their previous state.
  (Historical note: before commit `4a47795` a pause could delay finalization;
  in `HEAD` the pause fix keeps `active == true` while parked, so finalization
  runs only after the queue truly drains.)

### 2.6 Folder actions (UI)

Handled by `apply_action(FolderAction)`:

| Action | Behaviour |
|---|---|
| `Upload` / `Retry` | `start_folder_upload([i])` |
| `Remove` | removes from store, saves, logs "Datensicherung gestoppt: {name}" (no confirmation dialog in our build) |
| `OpenFinder` | runs `open -R <path>` |
| `ToggleEnabled` | flips `enabled`, saves |

`enabled` is persisted and shown as a row flag, but in the committed flow there is
**no background scheduler** acting on it; backups are only triggered by explicit
actions or initial creation. Auto-upload of new files is not implemented (P2).

### 2.7 Global pause

- A single `watch::Sender<bool>` (`tokio::sync::watch`) controls all in-flight
  runs (commit `4a47795`).
- The "Alle Sicherungsvorgänge anhalten/fortsetzen" button is **enabled only
  while `upload.active` and the store is non-empty**; it toggles via
  `send_replace`.
- Each `upload_worker` holds a `watch::Receiver` and races its per-file
  `upload_picture` future against `pause_rx.changed()` (biased `select!`). On
  pause the in-flight future is dropped (request cancelled) and the item is
  `push_front`-ed exactly once; the rate-limit pacing sleep is raced the same
  way. Parked workers keep `active == true`; resume is picked up within 400 ms.
  (Residual caveats F-2/F-3 in `AGENTS.todo.md`.)

## 3. Original backup-schedule model (observed)

### 3.1 Persistence (`schedules` table)

One SQLite database per account under `~/Library/Application Support/Amazon Cloud
Drive/<account>-schedules.db`, table `schedules` (unique index
`UIX_schedule_record(local_path, drive_identifier)`):

| Column | Type | Meaning |
|---|---|---|
| `id` | varchar (PK) | `ScheduleId` GUID |
| `created_date` | bigint | creation timestamp |
| `local_path` | varchar | source folder (unique per drive) |
| `destination_node_id` | varchar | backup destination node id in cloud |
| `drive_identifier` | varchar | cloud drive id (unique together with path) |
| `drive_name` | varchar | cloud drive display name |
| `interval` | varchar | cron-like `IntervalExpression` or null |
| `last_scan_time` | bigint | last scan timestamp |
| `last_enumeration_time` | bigint | last enumeration timestamp |
| `last_deep_scan_time` | bigint | last deep scan timestamp |
| `content_filter` | integer | `ContentFilter` bitmask |
| `error_code` | integer | `ScheduleError` |
| `paused` | integer | per-schedule pause flag (0/1) |
| `dedupe_photos_videos` | integer | `DeduplicateOptions` bitmask |
| `conflict_resolution` | integer | `ConflictResolution` enum |

A parallel `backup`/`uploads` table set plus `cloud.db` (node cache) is used for
reconciliation and transfer state; `sync.db` etc. exist for other domains. These are
not reproduced here — see `docs/UI_STATES.md` §3.8.

### 3.2 ScheduleRecord (service object)

`ScheduleRecord` implements `IMutableScheduleRecord / IScheduleRecord` and wraps the
row above. Behavioural points observed:

- `LocalPath` is immutable after creation in the UI (editable only on new backups;
  `CanChangeSource`).
- `DestinationNodeId` is resolved lazily via `LoadDestinationNode(backupPathMapper)`
  (creates the destination folder in the cloud if absent) — upload target creation
  side effect.
- `IsPaused` toggles per schedule.
- `Token` (a `CancellationToken`) is carried per record for cancellation.
- A 336-hour global knob (`BackupScanIntervalInHours`, default **336**) controls
  when a *deep scan* re-runs. `AppSettings.BackupScanIntervalInHours` reads
  `"BackupScanIntervalInHours"` from the app settings store, default `336`.

### 3.3 Interval model

Intervals are cron-shaped `IntervalExpression` strings with seven fields
(`second minute hour dayOfMonth month dayOfWeek year`), parsed by `TryParse`.
The UI, however, offers a closed option set (`IntervalOption`):

| UI label (German) | Option | Effective trigger |
|---|---|---|
| "Sofort" | `Immediate` | every second, offset 0 (continuous) |
| "Alle 5 Minuten" | `FiveMinutes` | every 5 min |
| "Alle 10 Minuten" | `TenMinutes` | every 10 min |
| "Alle 30 Minuten" | `ThirtyMinutes` | every 30 min |
| "Jede Stunde" | `OneHour` | every hour |
| "Alle 3 Stunden" | `ThreeHours` | every 3 h |
| "Alle 6 Stunden" | `SixHours` | every 6 h |
| "Einmal pro Tag" | `OneDay` | daily at a chosen time |
| "Einmal pro Woche" | `OneWeek` | weekly on a chosen day/time |
| "Never" | `Never` | not scheduled |

Defaults observed: a newly created backup gets `ContentFilter = PhotosAndVideo`
and interval `Immediate`; dedupe is enabled by default **only** when the content
filter is not `Everything` (see §3.4). The last option's displayed name in
German is not observed in `Localizable.strings` (the enum's hard-coded fallback
`"Never"` may be shown as-is).

### 3.4 Content filter, dedupe and conflict resolution

`ContentFilter` bitmask (observed values; canonical owner is `docs/SPEC_DEDUP.md`
§3.1 — values summarised here for context):

| Flag | Value | Meaning |
|---|---|---|
| `None` | 0 | |
| `Photos` | 2 | image types + image sidecars |
| `Videos` | 4 | video types + video sidecars |
| `Other` | 8 | everything else |
| `PhotosAndVideo` | 6 | Photos \| Videos |
| `Everything` | 14 | PhotosAndVideo \| Other |

Matching is performed by `ContentFilterExtensions.Matches(path)`, which delegates
to `SupportedMimeMapping` (`IsImageType`, `IsImageSidecar`, `IsVideoType`,
`IsVideoSidecar`). `.xmp` is therefore treated as a **sidecar of both image and
video** groups in the original, i.e. included when either Photos or Videos is
selected (see §4.1 and SPEC_DEDUP).

`DeduplicateOptions` mirrors the same two groups: `None=0, Photos=2, Videos=4,
PhotosAndVideos=6`.

`ConflictResolution` ∈ {`None`, `Ignore`, `SaveAsNew`, `Replace`} (desktop
string "Umgang mit Dateinamenkonflikten"). `ScheduleError` ∈ {`None`,
`LocalFolderNotFound`, `DestinationNodeNotAvailable`, `ExternalDriveMissing`,
`NameConflict`}; these drive row-level error presentation and retryability.

Default onboarding for a new backup: `ContentFilter = PhotosAndVideo`;
`DeduplicateOptions = PhotosAndVideos` if dedupe is checked, else `None` (and
always `None` when `Everything` is selected); interval option `Immediate`.

### 3.5 Paused state and scan timestamps

- Pause is **per schedule** (`paused` 0/1); additionally there is a **global**
  "Alle Sicherungsvorgänge anhalten/fortsetzen" command.
- Timestamps:
  - `last_scan_time` (date of last `backup.last_scan_time`, used for "Zuletzt
    aktualisiert" with a decaying refresh interval: >1 h → 3600 s, >5 min →
    300 s, >1 min → 60 s, else 5 s).
  - `last_enumeration_time` (drives "Zuletzt aktualisiert"/"Wurde aktualisiert"
    wording variants).
  - `last_deep_scan_time` (gated by `BackupScanIntervalInHours`).
- UI row-level aggregate states (label + colour) come from
  `BackupStatusAlertLevel` ∈ {`Normal`, `Info`, `Warning`, `Error`, `Success`}.
  Our simpler six-state enum does not reproduce these alert levels.

### 3.6 Folder-selection guards

Before a folder can be added as a backup, the original validates it via
`AllowedScheduleFolderStrategy.GetScheduleFolderWarning(localPath)`. Conflicts are
evaluated in this order:

1. **Drive root** — if `localPath` equals the path root of the system folder,
   warning `…_schedules_rootfolder_warningmessage`: *"Drive-Stammordner ist nicht
   für Datensicherung zulässig."*
2. **Overlapping existing backup** — if `localPath` overlaps any existing
   schedule (`PathEx.IsOverlappingFolder`), warning
   `…_schedules_overlappingfolder_warningmessage`: *"Dieser Ordner ist bereits
   in Ihrer bestehende Datensicherung enthalten"* (plural form exists for
   multi-selection).
3. **Sync folder** — only in sync mode: if `localPath` equals or is inside the
   sync folder (or vice versa), warning
   `…_schedules_syncfolder_warningmessage`: *"Ihr Synchronisierungsordner oder
   Inhalte in der Synchronisierung können nicht als Ordner zur Datensicherung
   hinzugefügt werden."*
4. **Restricted / system location** — if `localPath` is a Mac restricted path
   (`/Applications`, `/Developer`, `/Library`, `/Network`, `/System`, `/bin`,
   `/cores`, `/dev`, `/etc`, `/home`, `/net`, `/opt`, `/private`, `/sbin`,
   `/usr`, `/var`) **or** has the filesystem `System` attribute, warning
   `…_schedules_restrictedfolder_warningmessage`: *"Dieser Pfad ist
   eingeschränkt. Bitte wählen Sie einen anderen Ordner zur Datensicherung aus."*
   (plural form exists).

Additional warnings exist for FTUE child/parent folder relationships
(`…_ftue_child_folder_warning`, `…_ftue_parent_folder_warning_*`), but their
exact conditions are **unknown** from the decompiled snippets captured.

On save, further validation runs: missing local path ("Lokaler Pfad ist nicht
verfügbar"), missing destination ("Zielknoten ist nicht verfügbar"), name
conflict with a root folder ("Namenskonflikt mit Stammordner"), and duplicate
schedule for the same local path ("Datensicherung für lokalen Pfad existiert
bereits").

### 3.7 Auto-upload toggles and row actions (for context)

Observed, but out of MVP for our build unless noted otherwise:

- An "Auto-Upload" toggle per folder tooltip:
  "Der automatische Upload ist aktiviert/deaktiviert". *observed*
- Row/context actions: "Jetzt scannen und hochladen", pause/resume, retry failed,
  reveal in Finder, details, edit, "Datensicherung stoppen" (with confirmation
  dialog "Soll die Sicherung dieses Ordners gestoppt werden?" — previously
  uploaded content stays safe).
- "Zuletzt aktualisiert: {0}" line shown when `LastEnumerationTime !=
  DateTimeOffset.MinValue`.
- Interface toggles between nested and flat detail views.

## 4. Mapping: ours vs. original

| Dimension | Our build (committed) | Original (Amazon Photos 11.3) |
|---|---|---|
| Persistent unit | `BackupFolder` in `folders.toml` (TOML) | `ScheduleRecord` row in SQLite `schedules` |
| Identity | path equality only | `ScheduleId` GUID + unique `(local_path, drive_identifier)` |
| Source folder | chosen via `pick_folder`, no validation | `local_path` with guards (§3.6) + `CanChangeSource` |
| Upload target / destination | none per folder; global cookies | per-schedule `destination_node_id`, `drive_identifier`, `drive_name` |
| Cadence / interval | **none** — manual scans only | `interval` (`IntervalExpression`) + per-folder option |
| Content selection | implicit: all 28 supported extensions | `content_filter` bitmask (Photos/Videos/Other) |
| Dedup option | none persisted | `dedupe_photos_videos` bitmask |
| Name-conflict handling | not configurable per folder | `conflict_resolution` enum |
| Paused | global `watch` flag (`tokio::sync::watch`), not per folder | per-row `paused` + global command |
| Progress counter | `total_files` / `uploaded_files` (last run) | transfer state in worker + `backup`/`uploads` tables; "Zuletzt aktualisiert" |
| Error reporting | none per folder (run log only) | `ScheduleError` enum |
| Scan timing / history | none persisted | `last_scan_time`, `last_enumeration_time`, `last_deep_scan_time` |
| Background scheduler | none | `BackupScannerWorker` + `BackupScanIntervalInHours` (336 h) |
| Extension filter | extension-string sets, `.xmp` rejected | extension-based `SupportedMimeMapping` (embedded JSON); `.xmp` = image/video sidecar |
| Folder add guards | **none** (only exact-path dedup) | root / overlapping / sync / restricted / FTUE child-parent |
| Row statuses in UI | 6-valued `FolderStatus` | `BackupStatusAlertLevel` (5 levels) + richer counts |
| Auto-upload of new files | not implemented | supported via interval + watcher |

### 4.1 Deliberate divergences to record

- **`.xmp` handling.** Our build rejects all `.xmp` sidecars by name, whereas the
  original classifies them as sidecars of both image and video groups and therefore
  transfers them when the content filter includes Photos or Videos. Our rejection
  is intentional (documented in `AGENTS.md` and TODO).
- **Videos.** Not supported anywhere in our extension sets; original supports them
  via `Videos`/`Other` filter groups. Tracked as backlog ("Videos hochladen").
- **Manual-only cadence.** Our backups never run on a schedule; the user triggers
  each run. This diverges from the original's interval-driven worker.
- **No per-folder destination.** Our upload uses the global cookie account; the
  original creates/uses a per-backup destination node.
- **Row status set.** Our `FolderStatus::Scanning` exists but is never assigned in
  the committed flow; original has a separate "Warten"/preparing state.

## 5. Unverified: user-reported `.xmp` misclassification (2026-10-13)

User reported: "*.xmp is recognised as an image — wrong.*"

**Static check of committed state (`HEAD:src/folders.rs`, `HEAD:src/main.rs`):**
`is_supported_image` rejects `"xmp"` before consulting either extension set, and
the sole collection path `scan_images` applies that filter to every entry. The
folder picker is `FileDialog::pick_folder()` and performs no extension filtering.
`supported_extensions()` also excludes `xmp`.

**Result: no committed code path was found that would classify `.xmp` as a supported
image.** Because `is_supported_image` is the single source of truth referenced by
both the scan and the documented filter contract, the most likely explanations are
outside the read code — a binary built before the rejection, a `.xmp` file stored
as an upload record from an earlier version, or a UI count mismatch — but none of
these could be confirmed.

**Not performed (per task constraints):** no upload was attempted to reproduce, and
no runtime inspection of in-memory upload logs was carried out. This item stays an
open/unverified finding; it is tracked as its own ledger entry in `AGENTS.todo.md`
(".xmp-Befund klären"), not as P1 verification work.

## 6. Mapping of gaps vs. MVP scope

Relevant to `AGENTS.todo.md`:

- **P2 – backup not configurable.** Missing original window features: destination
  selector, content filter selector (Alles / Fotos und Videos / Fotos), interval
  selector (incl. "Sofort"), dedupe checkbox, conflict-resolution radio group,
  save/cancel. Our `BackupFolder`/`FolderStore` have no fields for any of these
  (see schema above); they must be added before feature parity.
- **Folder-selection guards** (§3.6) are absent; our `add` only dedups by exact
  path. Add root/overlapping/sync/restricted checks for safety, reusing the
  warning strings quoted there.
- **Automatic cadence / background scan** absent; tracked as backlog.
- **Per-folder enable/disable and auto-upload** stored but not acted on.
- **`enabled` flag** is persisted yet the flow starts uploads on creation
  regardless of it.
- Extension taxonomy is **not** ours to define here — see
  `docs/SPEC_DEDUP.md` (ContentFilter groups, sidecar lists, dedupe IDs).

## 7. Open questions

1. Whether `supported_extensions()` is actually wired into any native dialog in
   the committed build — `add_backup_folder` uses `pick_folder()`, so the filter
   list may be unused at runtime.
2. Whether `FolderStatus::Scanning` was ever assigned in an earlier commit; in
   `HEAD` it is defined and rendered only.
3. Whether finalization correctly marks `Paused` runs: `finalize_upload` derives
   `Done`/`Error` solely from `folder_done`/`folder_failed` and never sets
   `Paused`, so a manually paused run that later completes may be mis-labelled.
   Not resolvable from this spec.
4. Exact conditions for FTUE child/parent folder warnings (§3.6) — unknown.
5. Original's display name for `Never` (hard-coded `"Never"` vs. a German label)
   — no observed Localizable entry.
6. Whether videos would change the scan set; if video support lands, the
   extension filter and `ContentFilter` mapping must be updated in lockstep
   (`SPEC_DEDUP.md`).

## 8. Redaction

No account identifiers, user names, device identifiers, absolute file paths, or
verbatim decompiled snippets are reproduced. German UI labels are quoted verbatim
from `de.lproj/Localizable.strings`. SQLite-derived structural facts are
described; no personal row data is included.
