# SPEC_CHANGE_DETECTION — Detecting new / modified / removed files (Clean-Room Spec)

Behavioural spec of the **original** Amazon Photos desktop client's *change detection*:
what counts as a change (stat vs. identity), which scan type applies which rules, what
the filesystem watcher does per platform, what triggers a scan — and, the point that was
open on our side — how the client behaves towards **locked / still-being-written files**.

Clean-room: own words and tables, **no verbatim decompiled code**, no personal data.
German UI labels are quoted from the shipped `de.lproj/Localizable.strings` (product
strings, not code). Type/member names are cited only to describe a mechanism.

**Convention:** **belegt** = read off the assembly/strings/our code · *inferred* = reasoned
from adjacent evidence · **UNKNOWN** = not determinable from these sources.

---

## 1. Purpose & scope

Owns "how does the client notice that something in a backup folder changed — and is that
something a *complete* file?".

| # | Question | § |
|---|---|---|
| 1 | What counts as a change (size, mtime, device+inode)? | 3.1–3.2 |
| 2 | QuickScan vs DeepScan indicator sets, `InvalidOrMissingIndicators` | 3.3 |
| 3 | Watcher mechanics (per platform) | 4 |
| 4 | File locks, half-written files, retries | 5 |
| 5 | Scan triggers | 3.5 |
| 6 | Deletion / rename detection and reaction | 3.4.2 |

**Not owned here:** scan/upload *cadence*, `ForceScan` side effects, pause interaction
(`SPEC_BACKGROUND_SYNC.md` §3, §6, §7) · dedup identity and conflict policy
(`SPEC_DEDUP.md` §1, §2.2, §4.1) · our folder scan/filter/persistence and extension
taxonomy (`SPEC_FOLDERS.md` §2, §4.1) · cloud-side reconciliation
(`SPEC_BACKGROUND_SYNC.md` §2, `UI_STATES.md` §3.8) · drag&drop/one-time path, which has
neither watcher nor scan (`SPEC_ONE_TIME_UPLOAD.md`) · upload transport and progress
(`SPEC_UPLOAD.md`, `SPEC_UPLOAD_PROGRESS.md`) · notifications
(`SPEC_NOTIFICATIONS.md`).

---

## 2. Sources & method

| Item | Value |
|---|---|
| Target (read-only, never launched) | `/Applications/Amazon Photos.app` |
| Managed code | `Contents/MonoBundle/Amazon Photos.dll` (4.6 MB), `ilspycmd` full dump + targeted grep |
| German labels | `Contents/Resources/de.lproj/Localizable.strings` |
| Our side (committed) | `src/folders.rs`, `src/dedup.rs`, `src/upload.rs`, `src/backup.rs`, `Cargo.toml` |

Type list first (`ilspycmd -l c`) to locate candidates, then full decompilation, then
`rg` for the named members, the `FileShare`/`IOException`/lock vocabulary and the
strings. Nothing below is inferred from strings alone unless marked. Every symbol is
named in the section where its behaviour is described.

**Platform of the artifact:** macOS `.app` (MonoBundle, `MacSystemFileIdentifier` via
`NSFileManager`). The DI container registers exactly **one**
`IObservableFileSystemWatcherFactory` → `ObservableFileSystemWatcherFactory` and exactly
**one** `ISystemFileIdentifier` → `MacSystemFileIdentifier`; no `System.IO.FileSystemWatcher`
implementation, no `Windows*`/`Linux*` variant. → **belegt.** *Inferred:* the *product*
has other builds (the strings file carries Windows-only keys such as
`…_schedules_tab_showinexplorer` and `…_editbutton_windows_gesture` "Strg+E"), but that
code is not in this assembly → **UNKNOWN** (§4.5, §8).

---

## 3. Change indicators & scan types

### 3.1 Indicator vocabulary and `DetectChanges`

`ChangeIndicator` is a `[Flags]` enum → **belegt**: `None` (0), `Size` (1), `ModifiedDate`
(2), `DeviceNumber` (4), `FileNumber` (8), `SystemFileIdentifier` (12 =
`DeviceNumber | FileNumber`, *inferred* "different file at the same path").
`SetFlagsToList` expands a flag set into individual flags for metric dimensions →
**belegt**.

`BackupFileChangeDetector` implements `IFileChangeDetector<IAutoSaveRecord>`; its
collaborators are `IAppSettings` and `ISystemFileIdentifier` (both null-guarded). →
**belegt**

`DetectChanges(record, fso, SystemFileIdentifier systemFileIdentifier = null)` →
**belegt**:

| # | Condition | Result |
|---|---|---|
| 0 | `record.TargetType == TargetType.Folder` | `None` — folders are never "modified" |
| 1 | `record.Size != fso.Size` | `Size` |
| 2 | `record.ModifiedDate != fso.ModifiedDate` | `ModifiedDate` |
| 3 | identifier = parameter ?? `GetFileIdFromPath(targetType, path)` | resolved (argument wins) |
| 4 | either identifier `null` | returns flags so far — **no** identifier flag |
| 5 | device **and** file id differ | `SystemFileIdentifier` |
| 6 | only device differs | `DeviceNumber` |
| 7 | only file id differs | `FileNumber` |

Two properties, **belegt**: comparison is **exact equality** on the persisted `long`
size / `DateTimeOffset` mtime (no tolerance), and an unresolvable identifier degrades to
a stat-only answer instead of throwing.

`SystemFileIdentifier` is a value object: `TryParse` splits `"<device>_<file>"` on `_`
and fails with an error log ("Failed to parse fileId. Either DeviceIdentifier or
FileIdentifier is missing") unless exactly two parts result; `Equals` compares both
parts; `GetHashCode` mixes them with prime 8191. → **belegt**

### 3.2 Identifier resolution (macOS)

`MacSystemFileIdentifier.GetFileIdFromPath(targetType, path)` → **belegt**:
`NSFileManager.DefaultManager.GetAttributes(path)`; if the attributes are `null` or lack
`SystemNumber`/`SystemFileNumber` it logs an error and returns
`(string.Empty, path.ToHashString())` — *inferred*: a path-hash fallback so the identifier
never vanishes; otherwise `(SystemNumber, SystemFileNumber)` — *inferred*: `st_dev` +
`st_ino`. *Inferred* consequence of the fallback: in the degraded state a **replaced**
file at the same path yields the *same* identifier, so identifier-based changes go
unnoticed, while `Size`/`ModifiedDate` still fire and the change is normally still
caught.

### 3.3 Which indicator sets are "a change" — per scan type

`HasChanged(ChangeIndicator changes, BackupScanType backupScanType)` → **belegt**:

| Case | Behaviour |
|---|---|
| both combination lists configured | QuickScan → `changes.HasFlag(x)` for any `x` in the quick list; DeepScan → same against the deep list |
| either list null/empty | `AddHealthMetric(HealthMetrics.BackupFileChangeDetector.InvalidOrMissingIndicators, 1)` and **return `false`** |
| unknown scan type | `ArgumentOutOfRangeException` |

A mis-configuration therefore **does not fail loudly** — it silently disables change
detection for every file and raises one health metric. → **belegt** Both lists are
`AppSettings`-backed (`_db.AutoGet(Constants.Default…, "<key>")`). → **belegt**

**Defaults from `Constants` → belegt:**

| Scan type | Combinations |
|---|---|
| **QuickScan** | `Size\|ModifiedDate` · `Size\|DeviceNumber` · `ModifiedDate\|FileNumber` · `SystemFileIdentifier` |
| **DeepScan** | `Size` · `ModifiedDate` |

As a rule → *inferred*: **QuickScan requires a combination** — a stat change *or* an inode
change counts, but a *single* stat flag alone (only `Size`, or only `ModifiedDate`) is
**not** actionable by default; a lone mtime bump must be corroborated by a size or inode
change. **DeepScan is looser** (any single stat difference) and additionally re-hashes and
compares against the cloud node (§3.4.3). Whether this install overrides the keys at
runtime → **UNKNOWN** (same caveat as `SPEC_BACKGROUND_SYNC.md` §9).

### 3.4 The two scan paths

#### 3.4.1 QuickScan — walk everything, compare per file

`EnumerationService.QuickScan(record, nextUploadTime)` → **belegt**: marks the schedule
as enumering (an overlapping scan for the same schedule logs "Overlapping scan running on
the same schedule … Cancelling all other scans..." and resets that record's token) ·
`Walk(rootPath, …, identifyDeletions: true)` — a **full recursive walk** · then
`IdentifyRecordsChangedInCloud(...)` (skipped while cold-booting: "App is still
coldbooting. Skip looking for changes made in cloud…"). `BackupScannerWorker.RunScan`
sets `LastScanTime` **and** `LastEnumerationTime` afterwards.

The walker (`FileSystemWalker.WalkAsync`) is a BFS over a `Queue<string>`; per directory
it catches `SecurityException | UnauthorizedAccessException | IOException` into a
`FileOrFolderAccessException` carried on the walked object (the walk **continues**), so
**an unreadable directory does not abort a scan**. `RunDatabaseOperations` then splits the
batch → **belegt**:

| Function | Set | Effect |
|---|---|---|
| `ApplyFilters` | error-free, valid for the content filter | drops errored/unknown/filtered entries |
| `IdentifyCreates` | walked **minus** persisted | new `AutoSaveRecord(uploadRecord, localPath, systemFileIdentifier)` |
| `IdentifyModifications` | walked **∩** persisted | the change detector runs here |
| `IdentifyOutOfSyncRecords` | persisted | autosave record synced from its linked upload record (metric `QuickScanOutOfSyncContent`) |
| `IdentifyDeletions` | persisted **minus** walked | §3.4.2 (run on a background task) |

`IdentifyModifications`, per file, resolves the identifier (`GetFileIdFromPath`), runs
`DetectChanges(persisted, fso, fileId)` and then `HasChanged(changes, QuickScan)` →
**belegt**:

| Outcome | What is written |
|---|---|
| **actionable** | new `UploadRecord`; persisted record updated — `NodeId` **cleared** when the identifier changed (`fileId.ToString() != value.FileId`), else kept; `FileId`, `ModifiedDate`, `Size`, `Status`, `StatusChanged` updated; `AutoSaveModified` funnel metric if it was already finalized. *Inferred* for the `NodeId` reset: a **replaced** file at a known path forgets the old cloud node so the pre-upload check cannot mistake the new file for the old one. |
| **observed but not actionable** | only `Size`, `ModifiedDate`, `FileId` refreshed — **no re-upload**; metric `QuickScanChangesIgnored` |
| **no indicator** | nothing; metric `QuickScanNoChanges` |

#### 3.4.2 Deletions — DB-side, never event-side

Deletions are derived from walked-set vs persisted-set, never from a `Delete` event
(§4.3 drops those) → **belegt**:

- `IdentifyDeletions` (after a walk): `dbKeys.Except(walked)`, excluding photo-library
  content that still exists; survivors go to `DeleteUploadRecordsForPaths` +
  `DeleteAutoSaveRecords`. Metrics `FileAbandoned` / `UploadAbandoned`.
- `IdentifyAndSaveErrors`: a `FileOrFolderNotFoundAndTypeUnknownException` for a persisted
  path deletes the autosave record and its upload records.
- `HandleUploadException` on a missing file (`IsFileOrFolderMissingError` → `IO`,
  `FileNotFound`, `FolderNotFound`, `DriveNotFound`,
  `PhotoLibraryEditedContentNotFound`): volume detached → `ScheduleError.ExternalDriveMissing`;
  otherwise, for a scheduled backup, the autosave record and the upload record are
  **deleted** and the method returns (no error status at all).
- `DeepScan.HandleDeletes` does the same from the DB side, warning "DeepScan detected {n}
  records to be deleted but only removed {m}" on mismatch.

Consequences, *inferred*: a vanished local file is treated as "this record is void", and
the **cloud copy is never deleted** — no inspected path purges the remote node. **UNKNOWN**
whether another subsystem reacts to a local deletion.

#### 3.4.3 DeepScan — walk the DB, re-verify with MD5

`EnumerationService.DeepScan(record)` → **belegt**: `GetRecordsForScheduleId` → per
record `GetPath(normalized)` → missing ⇒ `HandleDeletes`; present ⇒ `HandleModifications`,
which **skips** records with `Status < TransferStatus.Completed` or an empty `NodeId`,
then `DetectChanges(record, fso)` and `HasChanged(changes, BackupScanType.DeepScan)`;
actionable ⇒ `DetectMd5Changes`.

`DetectMd5Changes` → **belegt**: batches of `DeepScanMd5CheckBatchSize` (default **10**)
with `DeepScanMd5CheckBatchDelayInSeconds` (default **60**) between them; per file:

| Check | Result |
|---|---|
| `node.ContentProperties.Size != fso.Size` | size mismatch → re-recorded for re-upload (no hashing) |
| node has no MD5 | warn "DeepScan Ignoring … \| No Md5 value in Content Properties" → skip |
| local hash fails | warn "DeepScan Ignoring … \| Failed to compute MD5" → skip |
| local hash == node MD5 | debug "MD5 matches node MD5" → skip |
| mismatch | re-record with `precomputedMd5` → re-upload; metric `DeepScanMd5Mismatch` |

Metrics `DeepScanNoChanges` / `DeepScanChangesDetected` / `DeepScanChangesIgnored` /
`DeepScanMd5Mismatch`; the method swallows exceptions ("DeepScan failed to detect MD5
changes"). A cancelled, failed or overlapping deep scan is re-queued with
`StartDelay = DeepScanRequeueDelay` (static **30 min**). → **belegt**

Rule, *inferred*: **DeepScan is the only path that trusts content over metadata**;
QuickScan never opens a file to decide, it only re-stats.

#### 3.4.4 `ProcessFileEvent` — the per-file event path

`EnumerationService.ProcessFileEvent(path, targetType, record)` (fire-and-forget
`async void`) → **belegt**:

It resolves the photo library and returns immediately if the path is neither valid for the
schedule's content filter nor a photo-library path; otherwise it marks the schedule as
enumerating (`LastEnumerationTime = now`, save, start events) — unless a scan for the same
schedule is already running, in which case it logs "Overlapping scan running on the same
schedule … Ignoring this file-event and requesting a quick-scan on the schedule…", resets
cancellation, re-queues a plain `BackupScanRequest` and stops. Otherwise it walks **the
single path** (not the folder) and runs `RunDatabaseOperations` with
`ContentFilter.ForMediaOnly()`, emitting `BackupScanStarted` (pivot `ScanSource.FileEvent`),
`BackupScanEmpty`, `FilesScanned` and `Performance.FileSystemWalkingSpeed`; an
`OperationCanceledException` is a warning, any other exception logs "Unable to process
File event for path:{path}".

Design point, *inferred*: a file event does **not** re-walk the tree; it re-stats and
re-evaluates exactly the one path the OS named.

`IdentifyRecordsChangedInCloud` (cloud-side changes) resets `NodeId`, `NodeParents` and
`Version` and re-queues via `GetUploadRecord`; a missing local path trips
`Assume.ShouldNotHappen("Should not find cloud changes for deleted files", …)` →
**belegt**.

*Inferred* summary of the three paths: QuickScan re-stats every file in the tree and never
opens one; a single file event re-stats exactly one path; only DeepScan reads content, and
only for files whose stat already changed.

### 3.5 Scan triggers (summary; details in `SPEC_BACKGROUND_SYNC.md` §3)

| # | Trigger | ForceScan | Mark |
|---|---|---|---|
| 1 | App-start tick of the 24 h periodic check | no | *inferred* (arithmetic observed, first-scan consequence derived) |
| 2 | File event in a watched folder | n/a → straight to `ProcessFileEvent` | **belegt** |
| 3 | Folder event (debounced 5 s) | via the queued scan request | **belegt** |
| 4 | `HandleDeviceArrival` | `ForceScan = !IsPaused` | **belegt** |
| 5 | `HandleDeviceRemoval` | — (watcher removed, schedule paused) | **belegt** |
| 6 | Periodic staleness (336 h quick / 168 h deep) | no | **belegt** |
| 7 | UI actions (scan-now, resume, create, interval change) | yes, `DeferUploads=false` | **belegt** |
| 8 | Enumeration re-queue (cancelled/failed scan, DeepScan MD5 mismatch) | with `StartDelay` | **belegt** |

---

## 4. Watcher mechanics

### 4.1 The watcher object

`ObservableFileSystemWatcher` is the *only* `IObservableFileSystemWatcher` implementation;
its constructor takes **one folder path**. → **belegt**

| Element | Value / behaviour |
|---|---|
| backing primitive | `FSEventStream` over a single-element path array — **one folder per watcher** |
| latency | 3 s |
| flags | `IgnoreSelf \| FileEvents` |
| run loop | `ScheduleWithRunLoop(CFRunLoop.Main, …)` then `Start()` — the **main run loop**, not a private queue |
| fan-out | `ParallelSubject<FileSystemEvent>` capacity 50 |
| grouping/throttle | `SelectMany(Events)` → **group by path** → each group `Throttle`d **100 ms** → `Merge()` |
| metrics | `FileSystemEvent.BeforeEventGrouping` (per group) / `AfterEventGrouping` |
| error path | subscription error handler writes a crash log under `Crash.Transfer`; `Errored` is `Observable.Never` |
| disposal | `Stop()`, `Invalidate()`, `Dispose()` stream, subscription and subject |

The 100 ms per-path throttle is the **coalescing** mechanism: a burst for the same path
collapses to the last one in that window; with the 3 s latency this keeps a
camera-import folder from producing thousands of notifications. → **belegt** (values) /
*inferred* (purpose)

`GetFileSystemEvent` maps a raw `FSEvent` onto a `FileSystemEvent` (`FullPath`,
`Timestamp = UtcNow`, `EventSource = FileSystemWatcher`), logs every event verbosely and
emits `FileSystemFileEvent.ChangeType` / `FileSystemFolderEvent.ChangeType` metrics →
**belegt**. Flag mapping: `ItemIsDir` / `ItemIsFile` set only `TargetType` (Folder / File)
and no change type · `ItemRemoved` → `Delete` · `ItemCreated` → `Create` ·
`ItemRenamed` with the path gone → `Delete` · `ItemRenamed` plus `ItemFinderInfoMod` or
`ItemXattrMod` → `Modify` · `ItemRenamed` otherwise → `Rename` ·
`ItemModified` / `ItemInodeMetaMod` → `Modify`.

*Inferred:* the rename branch resolves the macOS "rename" event — which also fires for
moves and Finder-info changes — by re-checking existence and the Finder-info/xattr flags.
`ChangeType.Move` is *accepted* by the consumer (§4.3) but **never produced** here →
*inferred* a dead value on macOS.

`ObservableFileSystemWatcherFactory.CreateWatcher(path)` is a null-guarded constructor
wrapper — no caching, no platform switch. → **belegt**

### 4.2 `BackupWatcherWorker` — ownership & lifecycle

`BackupWatcherWorker : RunLoopWorker` carries `[RunContext(Mode = AppMode.Backup,
Frequency = RunFrequency.Always)]` → **belegt** (architecture: `SPEC_BACKGROUND_SYNC.md` §2).

State → **belegt**: `Dictionary<path, (watcher, subscription)>` — **one watcher plus one
composite subscription per normalized path** · a `Queue<FolderEventRequest>` for folder
events · a `Subject<FolderEventRequest>` debounce channel for photo-library events · an
`AutoResetEvent` (initial state set) that wakes the run loop · two `SubjectLite` outward
channels (`BackupChanged`, `WatcherOnError`) · an `IFileSystemFilter` · and
`PollInterval = BackupWatcherPollIntervalInSeconds`.

Lifecycle → **belegt**:

- **`Start`** iterates `ListSchedules()`; per schedule: skip if `IsPaused` (log "Skip add
  FileSystemWatcher for Schedule: … because it is Paused"); `ErrorCode ==
  ExternalDriveMissing` → `HandleDisconnectedDevice`; a set `DestinationNodeId` →
  `await HandleMissingDestinationNode` (a deleted node saves the record with
  `DestinationNodeNotAvailable`); then `AddFileSystemWatcherForPath(LocalPath)`.
- **`Stop`** disposes every watcher and subscription and clears the dictionary.
- **`AddFileSystemWatcherForPath(path)`** normalizes the path, returns early if the key
  exists, creates the watcher and subscribes three things: `Changed →
  HandleFileSystemChange`; the **completion** callback → re-add the same path (re-arm);
  `Errored` → the same. A `FileNotFoundException` / `ArgumentException` from the factory
  instead publishes `BackupWatcherError { LocalFolderNotFound }` and logs "Failed to setup
  watcher on path=…".
- **`RemoveFileSystemWatcherForPath(path)`** disposes and removes the entry.

Re-arm is **unconditional** — on completion *or* error the watcher for the same
normalized path is simply recreated, with **no backoff and no attempt counter** →
**belegt**; the implied risk is *inferred* (a permanently broken folder re-arms in a tight
loop, mitigated only by the failure here being a missing path and by the worker's crash
metric `WorkerCrash.ScheduleWatcher`).

### 4.3 Event handling

`HandleFileSystemChange(fileEvent, backupPath)` → **belegt**:

1. **Type gate**: only `Move`, `Rename`, `Create`, `Modify` continue; `Delete`,
   `Conflict`, `Ignored`, `NotSelected`, `Unknown`, `None` return immediately.
2. **Existence gate**: `if (!_fileSystem.Exists(fileEvent.FullPath)) return;`. An
   `IOException` here is caught, logged "File event ignored: {fileEvent}" and dropped.
3. **Ignore filter**: `_fileFilter.IsIgnored(path, allowEdits: true)` → drop.
4. **Schedule lookup**: `GetScheduleRecordByPath(backupPath)`; null → drop. For a
   **folder** event, drop if an autosave record for that path already exists.
5. **Routing**: *folder* → enqueue `FolderEventRequest(path, record)`, guarded so an
   already-queued prefix suppresses it, then wake the loop; *photo-library path* (or
   inside one) → the debounce subject; *anything else* → publish
   `BackupWatcherChange { TargetType = File }`, which `BackupScannerWorker.HandleFileEvent`
   forwards to `EnumerationService.ProcessFileEvent`.

The **folder debounce** is a second, layered mechanism → **belegt**:
`GroupByUntil(e => e.Path, …, grp => grp.Throttle(FiveSeconds))` with `FiveSeconds = 5 s`,
`SelectMany(x => x.LastAsync())`; each surviving request is enqueued unless an
already-queued path is a prefix of it (log "Enqueing scan for {request}").

Asymmetry, *inferred*: **file** events get only the 100 ms in-watcher throttle; **folder**
and photo-library events get the 5 s group-throttle.

**Run loop** → **belegt**: while not cancelled, if the folder queue is empty it waits on
`_event.WaitOne(PollInterval)` — parking up to `BackupWatcherPollIntervalInSeconds`
(default **30**) between checks; otherwise it dequeues **one** `FolderEventRequest` and
publishes `BackupWatcherChange { TargetType = Folder, … }`, which `BackupScannerWorker`
turns into a normal scan request. A `BackupWatcherError` becomes
`SaveScheduleRecord(record, errorCode)` → the schedule shows `LocalFolderNotFound`.

`BackupScannerWorker` also owns the hardware watcher: `HandleDeviceArrival` re-adds a
watcher per affected schedule and requests a scan with `ForceScan = !IsPaused`;
`HandleDeviceRemoval` expires the drive details, saves each affected schedule with
`ExternalDriveMissing`, removes its watcher, publishes a `ScheduleChangedEvent(Modify)`
and then, via `Interrupt<BackupUploaderWorker>(PauseSource.SystemPaused, …)`, pauses
those schedules. → **belegt**

### 4.4 What the watcher deliberately ignores

`FileFilter.IsIgnored(path, attributes, allowEdits)` → **belegt** — ignored when:
empty/whitespace path · `FileAttributes.System` (log "ignoring system file") ·
`FileAttributes.Temporary` (log "ignoring temporary file") · `FileAttributes.ReparsePoint`
(log "ignoring symbolic links" — *inferred* symlinks are not followed) · junk file
(regex `^.*\.(lnk)$` or `^.*/(\.ds_store|\.localized|icon\r)$`) · temporary-looking name
(regex `~(\$(.*)|(.*)(\.tmp))$`) · hidden (`IsHidden`, gate `ExcludeHiddenFilesAndFolders`
**default `true`**, forced on when `ignoreCustomerSettings`) · photo-library junk
(`IsPhotosLibraryJunk`). The watcher passes `allowEdits: true` → *inferred*: an edited
photo-library file must still be seen even though the library itself is otherwise junk.

### 4.5 Other platforms

**UNKNOWN.** The inspected bundle is macOS-native with exactly one watcher and one
identifier implementation. Windows/Linux behaviour of this machinery cannot be determined
from these sources.

---

## 5. File locks & half-written files

The section our port needs most. Short answer up front, *inferred*: **the original does
not pre-check whether a file is locked, does not wait for a copy to finish, and does not
verify size stability before reading.** It detects the *consequences* of a half-written
file — zero size, or a size that changes — and reacts with a **retry**, not a wait.

### 5.1 How files are opened

`Infrastructure.FileSystem` → **belegt**:

| Member | Share mode | Buffer / options |
|---|---|---|
| `OpenRead(path)` | `FileShare.ReadWrite \| FileShare.Delete` | `FileStreamUploadBuffer` (default **65536**), `Asynchronous \| SequentialScan` |
| `UploadStream(path)` | same | same |
| download writer | `FileShare.None` | `FileStreamDownloadBuffer` (default 65536) |
| `HasIOError(path, out TransferError)` | opens with `ReadWrite \| Delete` and **immediately closes it** | — |

`HasIOError` checks `IsPathTooLong` first (→ `PathTooLong`), then tries the open; an
`IOException` becomes `ToTransferError()`, any other exception returns `false`
("no IO error"). → **belegt**

*Inferred*: reads are deliberately **tolerant** — the client opens a file another process
holds, even one being appended to. On macOS (no mandatory locking, no Windows sharing
semantics) this essentially always succeeds, which is exactly why a lock probe would be
pointless there. The permissive `FileShare` choice is the *only* lock-related decision.

### 5.2 Where a lock check exists — and where it does not

- `HasIOError` has exactly **one** caller in the assembly:
  `SetErroredWhenHasIOErrorRule` (`Services.FilesSync.SyncRules.LocalRules`) — the
  **Sync** feature, not Backup. → **belegt**
- No `HasIOError` call in `EnumerationService`, `BackupFileChangeDetector`,
  `FileSystemWalker`, `FileFilter`, `ManualUploadService` or `CloudDriveStorageService`.
  No `FileShare.None` probe, no `IsLocked`/`FileInUse` pre-check, no "wait until the file
  is closed" loop anywhere in the backup/enumeration code. → **belegt** (absence)

→ **Before reading or hashing there is no lock or access check at all.** The file is
opened, and whatever happens happens. → **belegt** / *inferred*

### 5.3 What happens when the read fails anyway

All **belegt**:

- **`FileSystemWalker`** (during a walk): `SecurityException` /
  `UnauthorizedAccessException` / `IOException` per directory → `FileOrFolderAccessException`
  carried on the walked object, walk **continues**, aggregate logged once ("Not all files
  and folders were successfully discovered under …"). Those objects become **errored**
  upload/autosave records via `IdentifyAndSaveErrors` (metrics `EnumerationAccessError` /
  `EnumerationNotFound`), or — for `…NotFoundAndTypeUnknown` — their records are **deleted**.
- **`GetHashOrNull`** (DeepScan MD5): `OpenRead` + `HashStream.ComputeHash`;
  `OperationCanceledException` is rethrown, **any** other exception is swallowed → `null`
  → the caller warns "DeepScan Ignoring … | Failed to compute MD5" and **skips the file**.
  No retry, no re-queue.
- **`HandleUploadException`**: `errorCode = e.ToTransferError()`,
  `status = errorCode.ToTransferStatus()`, `retryReason = e.ToRetryReason(errorCode)` →
  `UpdateStatus` (§5.5).
- **`ProcessFileEvent`**: generic `catch (Exception)` → warning "Unable to process File
  event for path:{path}"; the event is dropped.
- **`DeepScan` overall**: `catch (Exception)` → "Failed to run DeepScan on schedule: …" +
  `RequestCancellation()`.

`IOException` → `TransferError` (`InternalToTransferError`) → **belegt**: `FileLocking`
(NSError) and errno **32, 33** → `FileInUse`; errno 19 → `Security`; 5, 1920 →
`UnauthorizedAccess`; 2, 124 → `FileNotFound`; 3 → `FolderNotFound`; 39, 112 →
`OutOfStorage`; 206 → `PathTooLong`; 21, 6, 25 → `DriveNotFound`; 57, 222 → `IO`; 53, 64,
67, 1326 → `NetworkDriveNotFound`; anything else → `IO` + crash log. Server-side error
strings map the same way → **belegt**: `Md5MismatchError`, `FileSizeSmallerThanExpected`,
`FileSizeLargerThanExpected` → `FileInUse`; `ConcurrentContentEdit` →
`ConcurrentContentEdit`. These drive the German "Datei wird verwendet" pair (§6).

### 5.4 Size-change guards — the real half-written-file defence

Three guards, all in the upload path → **belegt**:

| # | Where | Condition | Effect |
|---|---|---|---|
| 1 | `ManualUploadService.UploadFile` **before** upload | `RetryReason != SizeChangedDuringUpload && fso.Size == 0` | throw `ZeroSizeFileShouldDebounceException` |
| 2 | `CloudDriveStorageService.UploadFile` **before** upload | remote experiment `EnableFileSizeCheckBeforeUpload` treatment == `"T1"` **and** `uploadRecord.Size != GetPath(path).Size` | throw `SizeChangedBeforeUploadException` |
| 3 | `ManualUploadService.UploadFile` **after** upload | `record.IsAutoSave() && record.Size != GetPath(record.Path).Size` | throw `SizeChangedDuringUploadException(node)` |

Details, all **belegt**:

- **Zero size is treated as "not done yet", not as an error** — the exception's own name
  says it *should debounce*. The retry mapping (`SizeChangedDuringUploadException` **or**
  `ZeroSizeFileShouldDebounceException` → `RetryReason.SizeChangedDuringUpload`) plus the
  `RetryReason != RetryReason.SizeChangedDuringUpload` guard in guard #1 mean: the first
  attempt is retried, and on a retry the zero-size guard is *skipped*, so a genuinely
  empty file is eventually uploaded instead of spinning forever. *Inferred* for the
  "eventually" (the cadence is the uploader's, §5.5).
- Guard #2 is **behind an A/B treatment**; whether this install receives `T1` is
  **UNKNOWN**, so the strongest pre-upload stat re-check is conditional.
- Guard #3 is scoped to autosave (scheduled backup) records — a manual/one-time upload
  that changed size mid-flight is **not** caught.
- `record.Size` at that point comes from the stream (`record.Size = stream.Length`, set
  while hashing) and `record.PrecomputedMd5` from the same stream — so record size and
  stream size can already diverge for a growing file, and only guards #1–#3 notice.

**There is no size-stability window.** No "stat, wait N seconds, stat again, upload only
if equal" logic exists in the dump; the only waits are the FSEvents 3 s latency, the
100 ms throttle, the 5 s folder debounce, the uploader backoff and the DeepScan 60 s batch
delay. → **belegt** (absence)

### 5.5 Retry & backoff — what actually happens to a locked file

- `TransferError.IsRetriableTransferError()` → **belegt**: `Unknown`, `NetworkError`,
  `IO`, `BadRequest`, `FileInUse`, `NoResumeState`, `SizeChanged`, `ParentNodeUnavailable`,
  `MultipartUploadUnknown`, `ConcurrentContentEdit`, `ObjectDisposed`,
  `SizeChangedBeforeUpload` are retriable.
- `TransferError.ToTransferStatus()` → **belegt**: retriable → `Retrying`; else `Errored`
  (except the multipart-in-progress cases).
- `ManualUploadDatabase.UpdateStatus` → **belegt**:
  - budget: `EnableUploaderUpdate ? ManualUploadMaxPeriodicRetries :
    ManualUploadMaxRetries` — the latter defaults to **5**;
  - the terminal condition lists the exceptions: `Retries >= max && status != Completed &&
    Ignored && IgnoredDueToPotentialConflictWithiOSUploads && errorCode != NetworkError &&
    SizeChangedBeforeUpload && retryReason != PendingNodeCompleting` → then `Errored`. So
    `SizeChangedBeforeUpload` is explicitly **exempt** and keeps retrying past the budget;
  - backoff `ExponentialBackoffStrategy(0 → 1 s → CloudDriveServiceMaxBackOffInSeconds)`,
    default cap **180 s**;
  - `Waiting` → `ResetToWaiting(record, now, errorCode, retryReason)`, else
    `UpdateStatus(...)`.

So a locked file ends up as **`FileInUse` → `Retrying` → retried with 1 s…180 s
exponential backoff → up to 5 attempts → `Errored`** plus the "Datei wird verwendet"
message. → **belegt** for every step, *inferred* for the summary sentence.

### 5.6 Summary — the open question, answered

| Question | Original's answer | Mark |
|---|---|---|
| Lock/access check **before** reading or hashing? | **No** — reads open with `FileShare.ReadWrite \| Delete`; the only `HasIOError` probe belongs to Sync | **belegt** |
| Wait for a copy/finish event? | **No wait loop** — the zero-size case throws and relies on the uploader's retry/backoff | **belegt** |
| Size-stability check? | Only three point-in-time comparisons (zero-size, pre-upload, post-upload); no stability window; guard #2 A/B-gated | **belegt** |
| File still growing at read time? | Read and hashed **as it is**; if the size changes between stat and upload, guard #2/#3 fires → retry; if not, the partial content is uploaded | *inferred* |
| Another app holds the file exclusively (Windows-style)? | On macOS the share mode still opens it; `FileLocking`/errno 32/33 → `FileInUse` → retry ≤5× → `Errored` | **belegt** |
| Read fails during a DeepScan hash? | Skipped for that pass ("Failed to compute MD5"), no error status, no immediate retry | **belegt** |
| Whole directory unreadable? | Walk continues; an errored record is persisted for that path (`EnumerationAccessError`) | **belegt** |

---

## 6. String evidence

Keys verbatim from `de.lproj/Localizable.strings`; "ref?" = the managed assembly calls
`Strings.<key>`. → **belegt** (quotes + reference state)

| Key (… = `clouddrive_desktop_`) | German | Ref? |
|---|---|---|
| `…_statuswindow_errorshortfileinuse` | "Datei wird verwendet" | yes |
| `…_statuswindow_errorlongfileinuse` | "Diese Datei wird momentan von einer anderen Anwendung verwendet. Bitte schließen Sie die Datei und versuchen Sie es erneut." | yes (`FileInUse`, `SizeChanged`, `SizeChangedBeforeUpload`, `ConcurrentContentEdit` all map here) |
| `…_statuswindow_errorshortio` | "Fehler beim Lesen" | yes |
| `…_statuswindow_errorlongio` | "Diese Datei konnte nicht gelesen werden." | yes (`IO`) |
| `…_statuswindow_errorshortunknown` | "Unbekannter Fehler" | yes |
| `…_statuswindow_errorlong_local_filenotfound` | "Die Datei, die Sie hochgeladen haben, wurde nicht gefunden. Wenn Sie die Datei verschoben oder umbenannt haben, laden Sie sie bitte erneut hoch." | yes |
| `…_statuswindow_errorlong_local_foldernotfound` | "Der Ordner, den Sie hochgeladen haben, wurde nicht gefunden. Wenn Sie den Ordner verschoben oder umbenannt haben, laden Sie ihn bitte erneut hoch." | yes |
| `…_statuswindow_errorlong_local_drivenotfound` | "Das externe Laufwerk, aus dem Sie Dateien hochgeladen haben, ist möglicherweise offline. Bitte stellen Sie die Verbindung wieder her und versuchen Sie erneut." | yes |
| `…_folder_not_present_warning` | "Dieser Ordner wurde nicht gefunden. Er wurde möglicherweise umbenannt, verschoben oder gelöscht." | yes |
| `…_preferences_excludehiddenfiles` | "Keine ausgeblendeten Dateien hochladen" | yes |
| `…_preferences_includehiddenfiles` | "Ausgeblendete Dateien hochladen" | yes |
| `…_preferences_uploadhiddenfiles` | "Hochladen von ausgeblendeten Dateien aktivieren" | yes |
| `…_preferences_uploadhiddenfiles_tooltip` | "Zu ausgeblendeten Dateien können vom Betriebssystem generierte Systemdateien zählen, die dazu beitragen, dass Ihr Computer reibungslos funktioniert, oder Dateien, die Sie selbst ausgeblendet haben." | yes |
| `…_statuswindow_scheduletab_scannowbutton` | "Jetzt scannen und hochladen" | yes (manual scan-now) |

Two readings, *inferred*:

- The original's own wording confirms §5: the answer to a busy file is *"Bitte schließen
  Sie die Datei und versuchen Sie es erneut"* — **tell the user to close it and retry**,
  which is exactly the retry-with-backoff policy.
- The hidden-files preference is a real, user-visible control (three keys) and the only
  content filter that is not an extension rule; it gates `IsHidden` in the event filter
  as well as in the walk.

There is **no** German string for "file is being copied", "waiting for the file to
finish", "size stable" or any lock-wait hint → *inferred* further support that no such
mechanism exists. → **belegt** (absence in the strings file)

---

## 7. Gap analysis vs. our port

Our committed state, read directly → **belegt**:

| Aspect | Our code | Location |
|---|---|---|
| Folder scan | `walkdir::WalkDir`, `filter_map(Result::ok)` — unreadable entries **silently dropped**; `file_type().is_file()`; then `is_supported_image` (extension sets, `.xmp` rejected) | `src/folders.rs` |
| Metadata | `note_size` reads `fs::metadata().len()` only for the ETA byte total; **no mtime collected** in the scan | `src/backup.rs` (`UploadPlan::note_size`) |
| Dedup identity | table `uploads(path, md5, size, mtime, uploaded_at)`; `should_skip(path, md5, …)` = path **+ MD5 only**; `size`/`mtime` stored but not part of the decision | `src/dedup.rs` |
| Fast path | `stat_matches(path, size, mtime)` — same path **and** size **and** mtime → skip without reading/hashing; `mtime == 0` never hits; MD5 remains the identity | `src/dedup.rs`, wired in `src/upload.rs` before `fs::read` |
| Change trigger | **polling**: `BG_SYNC_FIRST_TICK` 15 min, then `BG_SYNC_INTERVAL` 90 min, re-scanning **every enabled folder** per tick; no filesystem events | `src/backup.rs` (`background_sync_loop` / `background_sync_tick`) |
| OS watcher | **none** — no `notify`/FSEvents/inotify dependency | `Cargo.toml` |
| Granularity | the tick re-walks the folder and re-plans every file; only the *upload* stage consults the index | `src/backup.rs` (`UploadPlan::build` → `scan_images`) |
| Deletions | not modelled; nothing removes a path from the index | `src/dedup.rs` |
| Read errors | `fs::read` failure → `failed += 1`, row `FileState::Failed`, folder finalised `Error` | `src/upload.rs` |
| Retry | none — one attempt per run; a failure is final for that run | `src/upload.rs` |
| Pause | global `watch::Sender<bool>`; the background tick is skipped while paused; a running worker parks after the current file | `src/backup.rs`, `src/upload.rs` |

### 7.1 Indicator model

| Original | Us | Verdict |
|---|---|---|
| `Size`, `ModifiedDate`, `DeviceNumber`, `FileNumber`, `SystemFileIdentifier` as named flags, combined per scan type | `should_skip` = path + MD5; `stat_matches` = path + size + mtime | **equivalent in effect** — size+mtime is our two-flag combination, and we keep MD5 as identity, which is *stricter* than the original's stat-only decision. *inferred* |
| DeepScan MD5 re-verification against the cloud node | none (our MD5 is local-only) | simpler and safer: we hash whenever the stat does not match, so a same-size/same-mtime content change is our only blind spot — identical to the original's QuickScan blind spot |
| `InvalidOrMissingIndicators` health metric | none | cosmetic; not needed for a single scan type |

### 7.2 Watcher / trigger model

| Original | Us (today) | Our direction (`AGENTS.md`, 2026-10-14) |
|---|---|---|
| FSEvents per folder, 3 s latency, 100 ms per-path throttle, 5 s folder debounce, 30 s queue poll, re-arm on completion/error, paused folders get no watcher | 15/90 min polling over all enabled folders | OS filesystem events per watched folder via a cross-platform crate (FSEvents / `ReadDirectoryChangesW` / inotify); **the 15/90-min poll is discarded** as the primary path |
| file event → single-path handling; folder event → debounced scan request | tick → whole-folder re-scan | per event, check only the affected file ("file add / edit per event"), no full folder walk, no re-hash of everything |
| cold boot: one periodic check per 24 h (plus 336 h / 168 h staleness) | first background run after 15 min | one enumeration per folder at start/after login, decided by stat — **no timer** |
| safety net for missed events | none | optional, still open: a per-folder `max mtime` watermark in SQLite, compared at start; a metadata-only `walkdir` (no reads) that produces **no hash, no per-file DB query, no upload plan** when unchanged. *Inherent limit, same as the original:* a copy that preserves mtime does not raise the watermark |
| pause blocks events (paused schedules get no watcher) | the background tick is skipped while paused | pause blocks events too |

*Inferred:* the original has the same blind spot elsewhere — 3 s latency + 100 ms
throttle mean an event can be missed (sleep/wake, ejected volume), and its only fallback
is the 336 h / 168 h staleness scan.

### 7.3 File locks / half-written files — the main gap

| Original | Us | Assessment |
|---|---|---|
| open with `FileShare.ReadWrite \| Delete`; no pre-read lock check | `std::fs::read(&path)` — read-only open, no share semantics, no pre-check | *equivalent* on macOS (no mandatory locking); on Windows a file opened with `FILE_SHARE_NONE` by another app fails in both. *inferred* |
| zero size → debounce → retry with backoff | a 0-byte file is read, hashed to the empty MD5 and **uploaded** | **gap** — we upload the partial file; needs the zero-size guard |
| pre-upload stat re-compare (A/B-gated) | none | **gap** — a file that grew between plan-build and upload is uploaded as-is |
| post-upload stat re-compare (autosave only) | none | **gap**, lower priority |
| `FileInUse` → retry 1 s…180 s, ≤5 attempts, then `Errored` | read error → `failed += 1`, row `Failed`, folder `Error`; **no retry** | **gap** — one transient failure ends the file for that run and marks the folder failed |
| hashing failure in a deep scan → skip that pass, no error | `failed += 1` + folder `Error` | *divergence*: ours is louder, arguably right for a manual-run UI |
| unreadable directory → walk continues, per-path errored record | `filter_map(Result::ok)` → the whole subtree is silently invisible | **gap** — an unreadable subfolder looks like "nothing new" instead of "N errors" |
| `FileFilter` ignores system/temporary/reparse/junk/hidden | only `.xmp` + extension sets | **gap** (hidden files are the visible one: the original has a preference for it) |

*Inferred:* the zero-size guard is the most valuable single import from this spec — one
comparison removes the "upload a half-written file" failure mode that a camera-import
session produces routinely.

### 7.4 Deletions / renames

| Original | Us |
|---|---|
| deletions derived from walked-set vs persisted-set after every scan; local records deleted, **cloud untouched**; `FileAbandoned`/`UploadAbandoned` metrics; the watcher never reacts to `Delete` | nothing — a removed file simply stays in the index forever, which is *behaviourally identical for uploads* (the path is gone, so it is never re-uploaded) |
| rename → `ItemRenamed` resolved via existence + Finder-info/xattr flags → `Rename`/`Modify`/`Delete`; a still-existing, non-ignored rename goes to the per-file path → the new path is unknown → `IdentifyCreates` → new record; the old path is only cleaned up by the next walk's `IdentifyDeletions` | nothing — a rename looks like a new file on the next scan (the index is path-keyed, so the old entry is dead weight) |
| `Move` accepted by the consumer, never produced on macOS | — |

*Inferred:* for a **backup-only** client the original's deletion handling is mostly
bookkeeping; our "leave the stale row" is acceptable but should be recorded as a
deliberate divergence — the `uploads` table grows monotonically with renamed files.

---

## 8. UNKNOWN

1. **Windows / Linux watcher and identifier implementations** — only macOS exists in this
   assembly; the strings file hints a Windows build exists. **UNKNOWN.**
2. **Whether this install gets `EnableFileSizeCheckBeforeUpload = "T1"`** — remote
   experiment, not resolvable statically. **UNKNOWN.**
3. **Effective runtime values of `QuickScan…`/`DeepScan…`** — the `Constants.Default*`
   values are **belegt**; the installed overrides are **UNKNOWN** (same caveat as
   `SPEC_BACKGROUND_SYNC.md` §9).
4. **Sleep/wake and dropped-stream recovery** — re-arm on completion/error is **belegt**,
   but no wake handler was traced and what happens if FSEvents silently misses events is
   **UNKNOWN**.
5. **Whether the re-arm loop is rate-limited anywhere** — the unconditional re-arm is
   **belegt**; whether the worker framework throttles repeated restarts is **UNKNOWN**.
6. **Which retry budget applies to `FileInUse`** — `ManualUploadMaxRetries = 5` is
   **belegt**, but `EnableUploaderUpdate` decides between budgets and its value is
   **UNKNOWN**.
7. **End-to-end latency from "file closed" to "record queued"** for a burst — the
   100 ms / 5 s / 3 s values are **belegt**; the composed latency is **UNKNOWN** (not
   measurable without running the app, which we must not).
8. **Lock/wait logic in a non-inspected assembly** (e.g. a native helper) — absent here,
   → **belegt** absence; elsewhere **UNKNOWN**.
9. **Producers of `ChangeType.Move`** — accepted, never produced on macOS; **UNKNOWN**
   whether another platform's watcher emits it.
10. **Photo-library path rules** (`IsPhotoLibraryPath`, `IsPhotoLibraryContent`) — read
    only far enough to know such paths take the debounced branch. **UNKNOWN.**
11. **Whether manual/one-time uploads have a post-upload size guard elsewhere** — the code
    says autosave-only (**belegt**); an equivalent guard elsewhere was not traced.
    **UNKNOWN.**

---

## 9. Cross-references

- `docs/SPEC_BACKGROUND_SYNC.md` — §2 (worker architecture), §3 (scan triggers,
  `ForceScan`), §4 (intervals, `BackupScanIntervalInHours` = 336 h), §6 (filesystem
  watching — this doc supplies the change-detection half), §7 (pause interaction),
  §9 (belegt vs. UNKNOWN).
- `docs/SPEC_DEDUP.md` — §1 (MD5 identity), §2.2 (`UploadType` eligibility), §4.1
  (conflict resolution).
- `docs/SPEC_FOLDERS.md` — §2 (our scan/filter contract), §3.2 (`ScheduleRecord`,
  `last_scan_time` / `last_enumeration_time` / `last_deep_scan_time`), §4.1 (gap table).
- `docs/SPEC_ONE_TIME_UPLOAD.md` — §5.4 (`UploadType.Normal`; a one-time drop has neither
  watcher nor scan).
- `docs/SPEC_UPLOAD.md`, `docs/SPEC_UPLOAD_PROGRESS.md` (what happens after a change is
  classified), `docs/SPEC_NOTIFICATIONS.md` (status icon; no "backup finished" toast).
- `docs/UI_FLOWS.md` §4.5–4.7 (backup configuration, auto-upload toggle),
  `docs/UI_STATES.md` §3/§4 (row states).
- Our code: `src/folders.rs` (`scan_images`, `is_supported_image`), `src/dedup.rs`
  (`should_skip`, `stat_matches`, `record_upload`), `src/upload.rs` (stat short-circuit,
  read, MD5, one-attempt error handling), `src/backup.rs` (`UploadPlan::build`,
  `background_sync_loop`, `background_sync_tick`, `BG_SYNC_FIRST_TICK` /
  `BG_SYNC_INTERVAL`), `Cargo.toml` (no watcher dependency).
- `AGENTS.md` → "Erkennung neuer/geänderter Dateien (User-Entscheidung 2026-10-14)":
  OS events instead of polling, per-event file check, stat check before hash, cold-start
  enumeration, pause blocks events, optional `max mtime` watermark, 15/90-min poll
  discarded.

---

## 10. Redaction note

No account identifiers, user names, device or serial values, absolute user paths or
verbatim decompiled code are reproduced. German user-facing labels are quoted from the
shipped `Localizable.strings`. The target application was only read (plist/strings and
read-only decompilation of the managed assembly) and was never launched, signed or
modified. `docs/SPEC_CHANGE_DETECTION.md` is the only file created by this task.
