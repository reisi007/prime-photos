# Bidirectional Synchronization — Clean-Room Spec (reverse-engineered)

Behavioural specification of the **original** Amazon Photos desktop client's
**bidirectional sync** product core: the sync folder, selective sync, the engine and its
queue, change detection, the rule-based resolver, error/retry handling, the mass-delete
notification and the sync status surfaces — plus the mapping onto this port. The
behaviour below is **reverse-engineered from the shipped managed assembly**, not inferred
from string tables alone.

This document is **clean-room**: it records *observed behaviour* in our own words and
tables. It contains **no verbatim decompiled code** and **no personal data**. German
user-facing labels are quoted from the shipped `de.lproj/Localizable.strings` (product
strings, not code); type and member names are cited only as far as needed to describe the
mechanism. Nothing here presents this port as an official Amazon application.

**Confidence convention.**
- **belegt** = read directly out of the reverse-engineered `Amazon Photos.dll`
  (member/branch), the German strings file, or our committed code.
- *inferred* = reasoned from adjacent evidence, not read off a line.
- **UNKNOWN** = could not be determined from the inspected sources.

---

## 1. Purpose & scope

This doc owns the **two-way mirror** between one local folder and the cloud account: what
enables/disables sync, what the sync folder is and how it is validated, which cloud
folders are selected for download (selective sync), how local and cloud changes are
detected, how the engine turns them into operations, how those operations are
queued/retried, how failures surface, and which notification/progress surfaces exist.

Not duplicated here: folder scan/filter for *backups* (`docs/SPEC_FOLDERS.md`),
cadence/watchers/pause of *scheduled backups* (`docs/SPEC_BACKGROUND_SYNC.md`), dedup
identity (`docs/SPEC_DEDUP.md`), the one-time/drag-drop queue
(`docs/SPEC_ONE_TIME_UPLOAD.md`), progress/ETA UI (`docs/SPEC_UPLOAD_PROGRESS.md`),
notifications and menu-bar presence (`docs/SPEC_NOTIFICATIONS.md`). Downloads as a
*user-initiated file operation* (Restore tab, trash, file management) belong to
`docs/SPEC_LIBRARY.md`; this doc covers downloads **as a sync direction** only.

Sync is a **legacy** product core in this build: the original deprecates it and migrates
users to one-way backup (§3.10). Our port implements the backup direction only, so sync is
documented as the parity reference — not as an MVP target (§5).

---

## 2. Sources & method (belegt)

| Item | Value |
|---|---|
| Target | `/Applications/Amazon Photos.app` (never launched, read-only) |
| Managed code | `Contents/MonoBundle/Amazon Photos.dll` — full type list (2486 types), full decompiled dump, plus targeted per-type decompiles |
| German labels | `Contents/Resources/de.lproj/Localizable.strings` (+ `InfoPlist.strings`, `ServicesMenu.strings`) |
| Tooling | `ilspycmd` via `PATH`/`DOTNET_ROOT`; already installed, nothing installed for this task |

Evidence classes read (named only to describe behaviour): the `Services.FilesSync`
namespace (`SyncWorker`, `SyncContext`, `SyncWatcherWorker`, `SyncReconciliationWorker`,
`FileScannerService`, `SyncResolver.Resolver`, `SelectiveSyncService`, `SelectiveSyncNode`,
`SyncPathMapper`, `Model.*`, `Events.*`, `Operations.*` — 43 types — and `SyncRules.*` — 33
local + 31 cloud + 23 conflict rules); `Database.CloudDrive.SelectiveSyncNodeRecordMapper`;
the five sync `StartupTasks` plus `ExcludeCloudFoldersInRootTask`; `AppModeHandler`,
`BaseAppSyncFolder`/`AppSyncFolder`; the `EnableSync`/`DisableSync`/`RetryFailedSyncItems`
intent handlers; `UI.SelectiveSync.*`,
`UI.SyncWindow.SyncStatusWindow.SyncStatusViewController`,
`UI.DriveWindow.SyncTab.{SyncItemsViewModel, SyncStatusTabViewModel}`,
`UI.StatusPopup.SyncStateRowViewModel`, `SyncMassDeletesNotificationHandler`,
`SyncToBackupMigrationHelper`, the two sync-migration banners, `TransferEnumExtensions`; and
the enums `SyncStatus`, `CloudStatus`, `SyncTransferType`, `TransferStatus`, `TransferError`,
`RetryReason`, `NodeSelectionState`, `ExcludedFolderReason`, `ToggleSyncSource`,
`OperationStatus`, `AppMode`, `AppStartupFlags`.

Enum values, default settings and column names were read directly; call sites were
confirmed by searching the full dump (which string keys are actually referenced, who calls
whom). → **belegt**

---

## 3. Mechanics, states, fields, persistence, UI text, events

### 3.1 Architecture — three sync-mode workers plus a resolver

All sync work runs in three long-lived workers, each tagged
`[RunContext(Mode = AppMode.Sync, Frequency = RunFrequency.Always)]`. They are exactly the
workers `AppModeHandler` stops when sync is disabled (§3.2). → **belegt**

| Worker | Role | Trigger |
|---|---|---|
| `SyncWorker` | the **engine**: run loop, builds a `SyncContext`, runs operations in parallel | `AutoResetEvent` + poll `SyncPollIntervalInSeconds` (**60 s**) |
| `SyncWatcherWorker` | **change detection**: local FS watcher, cloud `/changes`, two scanners | event-driven; re-arms its watcher on completion/error |
| `SyncReconciliationWorker` | **reconciliation scans**: cloud rescan, folder rescan, folder-status refresh | timers (§3.7) + explicit requests |

`SyncWatcherWorker` never transfers data: it resolves a change into an `Operation` via
`IResolver` and pushes it into `OperationFound` (a `ParallelSubject`), which `SyncWorker`
subscribes to in its constructor — operations can arrive before or while the engine loops.
The engine is **database-driven**: `SyncContext` pulls batches of `FilesSyncRecord` rows out
of `sync.db`; an operation that cannot run leaves its record in `Waiting` for a later pass.
→ **belegt**

### 3.2 The sync folder, and enabling/disabling sync

The sync folder is one local directory bound to one cloud root node and one customer.
`BaseAppSyncFolder.Create(path)` writes the extended attribute
`com.amazon.drive.sync.root` with the customer id (a mismatching marker makes the folder
invalid), refuses restricted system paths (`FileSystem.RestrictedUserPaths`), then writes a
root `FilesSyncRecord` (`IsRoot = true`, `NodeId` = cloud root node, status `Synced` with
reported status `Processing`, `CloudStatus = Synced`) together with the root `Node` into
`sync.db` — deleting a differing existing root record first. On success it sets
`customerSettings.SyncPath`, applies a branded icon (`AD_Folder_Yosemite.icns` via
`NSWorkspace.SetIconforFile`) and, only when the folder was newly created, adds it to the
Finder favourites; a business metric `SyncFolderCreated` carries `success=<bool>`.
`DisableSync` reverses the binding: it deletes the "Amazon Drive" Finder shortcut, drops
the folder from favourites and removes the branded icon. Path mapping is
`SyncPathMapper.BasePath => _syncFolder.Path`, with a node's local path built from its cloud
parent chain; walking back reads the parent's metadata then its `files` record and throws
`ParentPathNotSyncedException` when the parent is unknown. A chosen folder must pass
`ValidateDrive` (allowed location) and `ValidateCustomer` (belongs to this customer) and
must not overlap an existing **backup** schedule (`…_onboarding_invalidsyncfolder_*`,
`…_onboarding_syncfolder_overlappingwithautosave_*`); the reverse holds too — a backup
folder cannot be created inside the sync folder (`…_schedules_syncfolder_warningmessage`).
→ **belegt**

**Enabling** cannot happen inside the running process, because `sync.db` must be created fresh.
`EnableSyncIntentHandler` confirms (an interlock allows only one flow at a time; `ToggleSyncSource`
has `FromPreferences` — dialog inside the preferences window —, `FromMenuBar` — plain dialog — and
`FromSyncFolderChangeEvent`), then `AppModeHandler.EnableSync()`: (1) if sync mode is off, **delete
`sync.db`** (timed, logged), then (2) `IAppRestart.Restart($"--enable={AppMode.Sync}")`. On the next
start `StartupContext.EnablingSync` is true — that is what `DeleteSyncDatabaseTask` checks so it does
**not** wipe the fresh database again, and what makes the onboarding sync page appear already
enabled. → **belegt**

**Disabling.** `DisableSyncIntentHandler` early-returns unless sync mode is on: `FromPreferences`
shows a modal `…_disablesyncconfirmation` inside the preferences window; `FromSyncFolderChangeEvent`
asks **nothing** — the warning `…_syncfolder_changetitle` / `…_syncfolder_changedetail` is shown and
treated as confirmed. `AppModeHandler.DisableSync()` clears the `AppMode.Sync` flag, removes
shortcut/favourite/icon, and stops **every worker whose `RunContextAttribute.Mode == AppMode.Sync`**
via `IWorkersStateMachine.StopAll(GetWorkersToStop)` with a 500 ms wait — `SyncWorker`,
`SyncWatcherWorker` and `SyncReconciliationWorker` together. Local and cloud files are left
untouched. → **belegt**

Sync is also switched off **automatically** in three places: `SyncWorker.RunLoop` publishes
`DisableSyncIntent(FromSyncFolderChangeEvent)` on every poll while
`Directory.Exists(SyncPath)` is false; `SyncWatcherWorker.OnSyncFolderChanged` does the same
when the watched folder stops being `ExistsAndValid()`; and `EnsureSyncFolderIsValidTask`
(§3.9) shows the warning and calls `DisableSync()`. → **belegt**

### 3.3 The engine and its queue

`SyncWorker` (`ISyncEngine`, `IPausableWorker`). `Start()` resets all non-finalized records to
`Waiting`, signals the event and sets `customerSettings.PreviouslyEnabledSync = true` (this flag
keeps the sync tab visible for users who once had sync). `Stop()` / `Pause()` stop the run loop,
**wait until `IsSyncing` is false** (poll 200 ms), then reset non-finalized records to `Waiting`
again — a pause never leaves records stuck in `Processing`. `Resume()` = `Start()`; `Interrupt()` =
stop + reset + start; `GetQueues()` = `AppSettings.SyncMaxQueues` (default **20**), cached 5 s.
Loop body: sync folder gone → disable intent (§3.2); wait for event or poll;
`GetTransferRemainingStats()`; **if nothing remains, loop again immediately** (the engine is
idle-cheap); otherwise take a `PreventSleep("Syncing with Amazon Drive")` assertion, resolve a fresh
`ISyncContext`, set `IsSyncing = true`, publish `SyncStartEvent`, reset non-finalized to `Waiting`,
and run `ParallelAsync.ForEach(context.GetRunnableOperations(), MaxDegreeOfParallelism = GetQueues(),
Run, UnhandledException)`. In `finally`: `IsSyncing = false`, reset non-finalized to `Waiting`,
publish `SyncStoppedEvent { ErrorOccured, Canceled }`. → **belegt**

`SyncContext` scheduling: → **belegt**

- six in-memory queues — `completing`, `delete`, `folder`, `nonBlocking`, `upload`, `download`
  (capacity 50 each);
- `LoadQueue(...)` pulls at most **50** records per queue per DB round with
  `GetAll(retriableBefore: now, filter, count: 50, orderBy)`, ordered `local_path asc` — except
  `delete`, which is `local_path desc` so children are deleted before their parent folder; each
  queue has its own **5 s** re-request gate (`NextDatabaseRequest`), so a hot queue cannot starve
  the database;
- `LoadQueues()` runs up to two passes and only while
  `Stats.InProgressCount + Stats.QueuedCount > 0`; if all queues come back empty while work is
  pending it retries with `onlyChildrenOfSynced: false` (a safety pass that ignores the "parent
  must be synced" filter);
- `TryMoveNext` round-robins over the transfer types `NonBlocking`, `Upload`, `Download` (values
  1/512/1024) at most three times per call, then resolves the record via
  `IResolver.GetOperation(...)` and marks it `SetProcessing`;
- **separate slot budgets:** `CanUpload`/`CanDownload` compare live counters against
  `customerSettings.UploadQueues`/`DownloadQueues`, re-read whenever `ICustomerSettings.Changed`
  fires; the outer `MaxDegreeOfParallelism` caps the total;
- **in-flight conflict detection:** `TrySetRunning` refuses an operation that
  `IsInConflictWithRunningOperation` reports (an identical operation already running, or any
  running operation whose `IsConflictingOperation` says so); with `cancelAndRescanOnConflict`
  (used by `SyncWorker.OnOperationFound` for *new* operations) the running operation is asked to
  `AddConflict` and, if `ShouldCancelBecauseOfOperation`, is **cancelled** so the newer change is
  persisted and re-scanned;
- **stale-queue invalidation:** `db.ParentPathChanged` buffered 5 s removes queued records that
  are children of a moved/renamed parent (`RemovePathFromCache`); `Reset()` cancels and disposes
  all running operations and clears counters and queues.

`SyncWorker.OnOperationFound(op)` — the path for watcher-produced operations: (1)
`_resolver.IsResolved(op)` (a `DoNothingOperation` or `SetStatusToSyncedOperation`) →
complete it immediately and return; (2) conflict with a running operation → **drop** it
(cancel-and-rescan); (3) `IRequiresAsyncPersist` → `PersistAsync()`, else `op.Persist()`;
(4) persist succeeded → `StartSync()` (wake the engine); (5) `UnauthorizedAccessException`
→ record `Errored` with `TransferError.UnauthorizedAccess` / `RetryReason.Error`,
`UniqueConstraintViolationException` → `op.HandleUniqueConstraintViolation` (else a crash
metric), anything else → crash metric `CrashOperation.Found`. `SyncWorker.Run(runnable,
token)`: runs the operation; when the returned `OperationStatus` has `DidNotRun` or
`Failed`, the record goes back to `Waiting` via `SetWaiting`. → **belegt**

### 3.4 The resolver — rules decide the operation

`Resolver.GetOperation(cloudNode, lastSyncNode, fileRecord, syncEvent, token)`: (1) first
matching **local rule** (33 registered types), (2) first matching **cloud rule** (31
types), (3) first matching **conflict rule** (23 types); (4) the operation comes from the
conflict rule, else the local rule, else the cloud rule; (5) if the event source is
`EventSource.Database` and the resulting operation's `TransferType` differs from the
record's, it is replaced by a `FixTransferTypeOperation` carrying the intended type; (6) if
still nothing matched, the `DoNothingRule` is used. A rule whose `IsMatch` throws is
swallowed into a crash metric (`CrashOperation.Rule`) and the search continues — one broken
rule cannot wedge the engine. Rule families: local-change (`UploadFileRule`,
`Update/Move/RenameCloudFileRule`, `TrashCloudNodeRule`, `CreateCloudFolderRule`),
cloud-change (`DownloadFileRule`, `Update/Move/RenameLocalFileRule`, `CreateLocalFolderRule`,
`DeleteLocalSyncedFile/FolderRule`), selection (`SyncUnselectedFolderRule`,
`UnselectFolderOperation`, `IgnoredRecordRule`, `DeleteLocalUnselectedSynced…`),
housekeeping (`ResetRecordRule`, `DeleteErroredRecordRule`, `ReupdateRule`, `DelinkRecordRule`,
`UpdateFileSystemInfoRule`, `FixTransferTypeRule`), error (`SetErroredWhenPathConflictsWith
ProtectedFolder…`, `…IllegalCharacters`, `…NodeCollidesWithLocalPath`, `…HasIOError`) and
conflict rules that arbitrate when both sides changed the same item
(`RenameLocalAndRenameCloudConflictRule`, `MoveLocalAndMoveCloudConflictRule`,
`UpdateBothOnLocalAndCloudConflictRule`, `DeleteLocalFileAndTrashCloudNode…`,
`UploadTrashedInCloudAndUpdateCloudFile…`, `AutoPurgedNodeConflictRule`, …). → **belegt**

Notable concrete behaviour: `BlockUploadNonMediaFileRule` — a local **file** whose name maps
to a non-media MIME type (`SupportedMimeMapping.IsOtherType`) becomes a
`SetStatusToSyncedOperation`: the record is marked synced and **never uploaded**. Only local
files are affected; folders and delete/download transfer types are exempt, and it is
skipped when the cloud node is newer than the last-synced node. → **belegt**

### 3.5 The record model (`files`) and the status vocabulary

`FilesSyncRecord` (table `files`, schema version **17**) — one row per synced item.
Columns: `id`, `uuid` (unique), `node_id` (unique), `root`, `local_path` (unique),
`display_path`, `local_name`, `local_parent_path`, `created_date`, `modified_date`, `size`,
`md5`, `crc`, `transfer_type`, `target_type`, `cloud_status`, `status`, `reported_status`,
`status_changed`, `added`, `changed`, `retries`, `next_retry`, `retry_reason`,
`retry_timer_start`, `error_code`, `local_id`, `local_id_created`, `last_synced`, `cud`.
→ **belegt**

Semantics worth naming: `display_path` is used for cloud-only rows (cold boot) and
**encodes the directory separator as `_:/:_`** (`ColdBootPathSeperator`);
`retry_*` carry the backoff bookkeeping; `local_id`/`local_id_created` detect local-only
edits; `last_synced` drives first scans; `cud` is obsolete, superseded by
`RetryReason.FallbackToCreateDelete` in 5.7.1. Constructor defaults: `Retries = 0`,
`Status = Waiting`, `StatusChanged = MinValue`, `ErrorCode = None`, `TransferType =
NonBlocking`, `CloudStatus = Unsynced`, `Added`/`Changed` = now. There is **no `schedule_id`
and no `parent_id`** — sync records are not schedules (both return `null`). → **belegt**

Status enums: → **belegt**

- `SyncStatus`: `None=0, Waiting=1, Processing=2, Completing=16, Synced=32, Errored=512,
  Ignored=1024`.
- `CloudStatus`: `None=0, Unsynced=1, ColdBoot=2, Synced=32, NotSelected=1024`, plus
  `NotSelectedAndUnsynced`, `NotSelectedAndSynced`.
- `SyncTransferType` `[Flags]`: `Unknown=0, NonBlocking=1, Delete=2, Folder=4, Upload=512,
  Download=1024`.
- `TransferStatus` (UI view): `None, Waiting, Processing, Retrying,
  MultipartUploadCompleting, Completing, Completed, Canceled, Errored, Ignored, Skipped,
  IgnoredDueToPotentialConflictWithiOSUploads`.
- `ToTransferStatus(status, retries)`: `Waiting` with `retries > 1` becomes `Retrying`, so a
  re-queued record reads "Wiederholen" rather than "Warten".
- `ReportedStatus` is a **separate column for folders only** (for files it is just
  `Status`). `UpdateFolderStatuses()` / `BulkUpdateFolderReportedStatus()` /
  `ComputeBubbledStatus()` bubble child states up so a folder shows the aggregate of its
  content.
- **Global stats → tab counters.** `FilesSyncDatabase.GlobalStatsQuery` aggregates over
  `files where root <> 1`: `InProgress = Processing|Completing`, `Queued = Waiting`,
  `Failed = Errored`, `Completed = Synced`, `Suspended = Ignored`. Those five numbers drive
  `IGlobalSyncTransferState.Stats` and therefore the sync tab, the status-popup row and the
  remaining-item count.

**Row vocabulary.** `TransferEnumExtensions` maps a `TransferStatus` to a label key:
`None/Waiting → statuswaiting` ("Warten"), `Processing → statusprocessing` ("In
Bearbeitung"), `Retrying → statusretrying` ("Wiederholen"),
`Completing/MultipartUploadCompleting → statuscompleting` ("Wird überprüft"), `Completed →
statuscompleted` ("Abgeschlossen"), `Canceled → statuscanceled` ("Abgebrochen"), `Errored →
statusfailed` ("Fehlgeschlagen"), `Ignored`/`IgnoredDueToPotentialConflictWithiOSUploads`/
`Skipped` → the download-side `…_statusignored`/`…_statusskipped` keys. Two parallel error
maps (`ShortErrorStringMap`, `LongErrorStringMap`) map `TransferError` to a short and a long
message key — e.g. `OutOfStorage → …errorshortoutofstorage` / `…errorlongoutofstorage`,
`UnauthorizedAccess → …errorshortpermissions`, `FileConflict → …errorlong_local_fileconflict`,
`NodeNotFound → …errorlong_cloud_nodenotfound`, `ProtectedFolderConflict →
…_syncwindow_cantsyncprotectedfolder`. `FixableErrors` additionally offers a call-to-action
key for `CloudDriveOverQuota`, `FileNotFound`, `FolderNotFound`. → **belegt**

### 3.6 Operations, errors, retries and conflicts

`Operation` is abstract with the lifecycle `cancel? → CouldNotGetSlot (no upload or download
slot) → CouldNotSetRunning (conflict with a running operation) → InternalRun() → Completed |
Suspended | Failed… | DidNotRun`. `OperationStatus` is a flag-style enum whose failure members include `DidNotRun`,
`ContextCanceled`, `CouldNotRun`, `CouldNotSetRunning`, `CouldNotGetSlot`, `Failed`,
`FailedForUnknownReason` and `CompletedWithProblems`. Operations come in three shapes — `LocalOperation`, `CloudOperation`, plain
`Operation` — plus housekeeping operations (`SetStatusToWaiting/Synced/Errored`,
`ResetRecord`, `DelinkRecord`, `DeleteRecord`, `IgnoredRecord`, `FixTransferType`,
`Reupdate`, `UpdateFileSystemInfo`, `ClearMetadata`) and transfer families for upload
(`UploadOperation`/`UploadFileOperation`), download (`DownloadOperation`/
`DownloadFileOperation`) and create/delete/move/rename/update on both sides, plus
`SyncUnselectedFolder`, `UnselectFolder`, `FolderScan` and `MissingFileScan`.
→ **belegt**

**Retry / backoff.** `FilesSyncDatabase.UpdateStatus(record, Waiting|Completing, …)`:
`retries` is incremented only when an `errorCode` is present, otherwise reset to 0; the
strategy is chosen by family (plain error → `_errorBackoffStrategy`; `Completing` +
`RetryReason.PendingNodeCompleting` → `_pendingNodeNeedsCompletionBackoffStrategy`; otherwise
`_waitForResumeCompletionBackoffStrategy`); defaults are
`SyncEngineOperationMinRetriesBeforeBackOff = 5`, `SyncEngineOperationMinBackOffInSeconds =
1`, `SyncEngineOperationMaxBackOffInSeconds = 21600` (6 hours); `next_retry = now + backoff`,
and `SyncContext.LoadQueue` passes `retriableBefore: now`, so a record in backoff is **not**
handed out early. `RetryReason.IsNetworkRelated()` is true for `NetworkError`,
`PreviouslyPaused`, `NoResumeState`, `RequestTimeOut` — the cases where a retry means "the
network was the problem". → **belegt**

**Bulk status resets** used by pause/stop/start and by the UI: `ResetNonFinalizedToWaiting()`
(everything not finalized goes back to `Waiting`; called on engine start, stop, pause,
interrupt and after each parallel pass), `ResetErroredToWaiting(errorCode?)` (the **"retry all
failed"** action), `SetWaiting`, `SetProcessing` (Processing may only be set through
`SetProcessing`, never through `UpdateStatus`). The root record may never leave `Synced`:
`UpdateStatus` forces it back and logs an `Assume.ShouldNotHappen`. → **belegt**

**Failure UI.** Failed records (`SyncStatus.Errored`) are the `Failed` filter of the sync tab
and open the shared error window (`UI.ErrorWindow.ErrorWindowService` / `ErrorViewController`,
used for backups and sync). Sync-specific entry points: `RetryFailedSyncItemsIntent` →
`RetryFailedSyncItemsIntentHandler` → `_syncDatabase.ResetErroredToWaiting()` (what `…_retryall`
"Alle erneut versuchen" triggers and what the status-popup row's `CallToAction.RetryErrored`
publishes); `…_statuswindow_readdtoqueue` ("Erneut zur Warteschlange hinzufügen") as the
per-item "add back to queue" action rendered by two table-cell classes; and
`…_syncwindow_cantsyncfailedcount` ("{0} Dateien können nicht synchronisiert werden") as the
count line when items cannot be synced. → **belegt**

**Conflict resolution.** There is **no interactive conflict dialog for sync**. Conflicts are
resolved by the 23 conflict rules (§3.4) and by cancelling a conflicting in-flight operation
(§3.3). The only user-visible "conflict" surfaces are the protected-folder and
illegal-character error texts (`…_syncwindow_cantsyncprotectedfolder`,
`…_statuswindow_errorlong_pathinvalidcharacters` — the latter **not referenced** by the
managed code, §4) and the download guard `…_statuswindow_downloadnotallowedinsyncfolder*`,
which stops a user from downloading *into* the sync folder because that would create
duplicates on the next upload. → **belegt** / *inferred* (the "no dialog" claim rests on the
absence of a sync conflict-dialog call site)

### 3.7 Change detection (watchers and scanners)

`SyncWatcherWorker.Start()` wires six subscriptions: → **belegt**

| Source | Handler | Notes |
|---|---|---|
| `IAppSyncFolder.Changed` | `OnSyncFolderChanged` | folder no longer `ExistsAndValid()` → publish `DisableSyncIntent(FromSyncFolderChangeEvent)` |
| `IObservableFileSystemWatcher` on the sync folder | `WatcherOnPathChanged` | created by `CreateWatcher(syncFolder.Path)`; **re-subscribes itself on completion and on error** |
| `ICloudDriveChangesService.NodesChanged` (`/drive/v1/changes`) | `WatcherOnNodesChanged` | only `kind` FILE/FOLDER and `status != "PENDING"` |
| `ICloudDriveScannerService.NodesChanged` (reconciliation) | `ScannerOnNodesChanged` | **folders are processed before files** — parents before children |
| `IFolderScannerService.Changed` | `ScannerOnPathChanged` | folder-scan output (`PathChange`) |
| `IFileScannerService.Changed` (missing-file scan) | `ScannerOnPathChanged` | same handler |

`WatcherOnPathChanged` filters: an `Unknown` target type is ignored unless the change is
`Delete` or `Rename`; for **folders**, a `Create`/`Change` event is suppressed when the new
name equals the name already stored for that path (duplicate-notification guard). Survivors
call `ResetRetriesForPath(path)` then `HandleFileSystemChange`. `HandleFileSystemChange` then: skips the sync folder root itself; reads the on-disk metadata
(`IFilesSyncMetadata.Get`) and drops the event when it was written **< 500 ms ago** and has a
uuid but no node id ("probably a UUID write", i.e. our own download just landed); resolves the
record by `NodeId`, else `Uuid`, else local path, creating a new record from the file system if
unknown **and** the path exists; sets `CloudStatus = Unsynced` (or adds `NotSelected` for a
`NotSelected` change type) for any change other than `Delete`; and hands cloud node plus
last-synced node to the resolver. `PublishChangeEvent` debounces: if the record's
`StatusChanged` is **younger than 1 second**, the event is dropped. → **belegt**

`SyncReconciliationWorker` (also `ICloudDriveScannerService` + `IFolderScannerService`) runs
on timers: `SyncReconciliationPollIntervalInSeconds` 30 s (run-loop poll; processes queued
scan requests once due), `CloudDriveScannerRescanIntervalInSeconds` 3600 s (cloud rescan),
`FolderScannerRescanIntervalInSeconds` 86400 s (interval-based folder rescan),
`FolderStatusUpdateIntervalInSeconds` 30 s (root-folder status refresh; also on
`SyncStatusEvent`, sampled 3 s). Requests are **merged**, not duplicated: a second
`CloudScanRequest` folds into the pending one (`ForceGetChanges |= …`,
`CheckForNewFolders |= …`), and a `FolderScanRequest` covered by a pending scan (or a child of
one with `ScanSubFolders`) merges with logical OR over its flags (`StartDelay` takes the
minimum). `ScanCloud` optionally calls `GetLatestChanges()` first and then
`GetModifiedNodesAndPopulateNewDownloads(cloudDb, checkForNewFolders)`. → **belegt**

`ScanFolder(request, …)` is the **authoritative local rescan**: it compares the file system
with the database for one path and emits `PathChange` events.
`HasFileOrFolderChanged` decides `Create` (no record at all), `Rename` (same parent path,
different name), `Move` (different local path), and — for files only — **"changed" when
`Size` differs or `ModifiedDate` is not equal to the second** (`EqualToSeconds`); folders have
no content comparison. If the record has **no `LastSynced`** (first time the path is seen),
every child on disk and every database record under the path is published as `Create`, and the
local/DB sets are cleared so nothing else is compared. Children are enumerated one level at a
time, optionally recursing when `ScanSubFolders` is set; enumeration honours
`customerSettings.ExcludeHiddenFilesAndFolders`. A folder that is neither a folder nor a
missing file (`TargetTypeUnknownReason.NotFound`) aborts the scan. `FileScannerService`
(`MissingFileScanOperation`) is a separate `RunLoopWorker` whose `Changed` stream feeds the
same handler — it exists to find files that vanished without an event. → **belegt**

### 3.8 Selective sync — which cloud folders come down

Selective sync is an **exclusion list**, not a selection list: a folder is *not selected*
when it or any ancestor appears in `excluded_folders`. `ExcludedFolderRecord` (table
`excluded_folders`): `node_id` (primary key), `name`, `path` (indexed), `reason`
(`ExcludedFolderReason` = `None`, `ProtectedFolder`, `ManualUpload`, `NewCloudFolder`).
→ **belegt**

`SelectiveSyncService`: `IsNodeSelected(node)` — the root is always selected; otherwise the node
is unselected when it or any parent is excluded; otherwise it asks the cloud cache
`ICloudDriveDatabase.ContainsNode(nodeId, excludedIds)`; results are cached in an in-memory blob
cache for **30 s** and invalidated by `ExcludedFolderChanged` / node changes.
`IsLocalPathSelected(path)` walks the path up to the sync-folder base path and returns false as
soon as any segment is excluded — the local-side gate used by the sync rules.
`UpdateNode(node, selected, reason)` under a single-flight semaphore deletes the node's own
exclusion row **and** every exclusion row whose path is under it; when unselecting it writes one
row with `Reason` (default `ProtectedFolder` when the node is protected, else `None`) and stamps
`ExcludedFoldersLastModified = UtcNow`; `UpdateNodes` loops over it. `CleanUpPurgedNodes` deletes
exclusion rows for cloud folders that became `PURGED` or `TRASH`. → **belegt**

**Tree source for the picker.** `GetNodeSelectionSettings()` attaches the cloud DB and the
sync DB to one SQLite connection and runs a recursive CTE over `nodes`/`node_parents` minus
`excluded_folders`, returning only `FOLDER` nodes that are not protected, are `AVAILABLE`,
and whose parent is a folder. Rows are mapped by `SelectiveSyncNodeRecordMapper` into
`SelectiveSyncNode` (a `Node` plus `IsSelected` and `ParentNodeId`) — the mapper's record class
adds the `selected` and `node_parent_id` columns on top of the node record schema.
→ **belegt**

**UI.** `SelectiveSyncWindowViewModel` loads that tree, waits for a cold boot and for a
pending `/changes` pass to finish first, builds `FolderTreeItem` nodes with a tri-state
`NodeSelectionState` (`NotSelected`, `Selected`, `ChildNotSelected`), offers select-all /
select-none, and computes a `SelectiveSyncChanges` diff with `SwitchedOffCount` and
`ChangedFolders`. On *persist*: if `SwitchedOffCount > 0` the user must confirm
`…_selectivesyncwindow_confirmationbody` ("Wenn du die Synchronisierung für einen Ordner
deaktivierst, wird der Ordner von deinem Computer entfernt, bleibt aber in Amazon Photos
gespeichert."); only then is each changed folder written through `UpdateNode` and a
`SyncSettingsChangedEvent` published. During onboarding the tree is shown pre-selected.
→ **belegt**

**New top-level cloud folders.** `ExcludeCloudFoldersInRootTask` →
`SelectiveSyncService.ExcludeCloudFoldersInRoot(cloudDb)` only acts when
`customerSettings.ExcludeNewCloudFolders` is set (preferences toggle
`…_preferences_exclude_syncing_new_cloud_folders`). It marks **top-level folders created
after `ExcludedFoldersLastModified`** with reason `NewCloudFolder`, using a slightly different
query before vs. after the sync cold boot (after cold boot it also skips folders that already
have a `files` record). Purged/trashed exclusion rows are cleaned up in the same call.
→ **belegt**

### 3.9 Persistence, startup and cold boot

Sync owns **one SQLite database per customer**, `sync.db`, at
`DatabaseStoragePath/<customer>-sync.db` (`customerInfo.GetCustomerDatabaseFileName("sync.db")`).
Its tables: `files` (`FilesSyncRecord`), `nodes` (`Node`), `node_parents`, `settings`
(`SettingRecord`) and `excluded_folders` (`ExcludedFolderRecord`); schema version
`FilesSyncDatabase.LatestDatabaseVersion = 17`. The cloud node cache lives in a **separate**
`cloud.db` (`CloudDriveDatabase`); the two are attached together (`BaseDatabase.Attach`, aliases
`clouddb`/`syncdb`) for the selective-sync tree and for `ExcludeCloudFoldersInRoot`. DB-resident
settings written through `SaveSetting` include `LastRemainingFilesReported`, `FoldersColdBootTime`,
`SyncColdBootTime`, `ExcludedFoldersLastModified`; observable subjects `ExcludedFolderChanged`,
`StatusChanged`, `ParentPathChanged` let the service layer react to writes. → **belegt**

Per-customer JSON settings that sync reads: `SyncPath` (obsolete in favour of
`IAppSyncFolder.Path`), `PreviouslyEnabledSync`, `HasSyncBeenMigratedToBackup`,
`ExcludeNewCloudFolders`, `UploadQueues`, `DownloadQueues`,
`UploadTransferKiloBytesPerSecond`, `DownloadTransferKiloBytesPerSecond`,
`ExcludeHiddenFilesAndFolders`, `AppMode`. → **belegt**

`AppSettings` (server-tunable; values below are the compiled defaults):
`SyncPollIntervalInSeconds` 60, `SyncMaxQueues` 20, `SyncEngineQueueTimeoutInSeconds` **120**,
`SyncItemsListSizeLimit` 100, `SyncReconciliationPollIntervalInSeconds` 30,
`CloudDriveScannerRescanIntervalInSeconds` 3600, `FolderScannerRescanIntervalInSeconds` 86400,
`FolderStatusUpdateIntervalInSeconds` 30, `SyncEngineOperationMinRetriesBeforeBackOff` 5,
`SyncEngineOperationMinBackOffInSeconds` 1, `SyncEngineOperationMaxBackOffInSeconds` 21600,
`MassDeletesNotificationWindowInSeconds` 30, `MassDeletesThresholdForCurrentWindow` **200**,
`WaitTimeBetweenMassDeletesNotificationsInSeconds` **3600**, `CanEnableSync` **false**
(server-gated). → **belegt**

**`SyncEngineQueueTimeoutInSeconds` (120 s)** is used exactly once, in
`SyncWorker.MonitorTransfer()`: a merged stream of `FileTransferProgressEvent` and
`operationFinished` is given `Timeout(120 s)`; a `TimeoutException` records
`HealthMetrics.Timeout.SyncQueueTimeout` and `BusinessMetrics.SyncQueueTimeout`. It is a
**health signal that the queue stopped making progress** — not a per-file transfer timeout and
not a cancellation; nothing is cancelled when it fires. → **belegt**

**Startup order** (`StartupTaskManager.Tasks`, 67 entries; the sync-relevant slice):
`UpdateWeblabCacheStartupTask` → **`EnsureSyncFolderIsValidTask`** → **`DeleteSyncDatabaseTask`**
→ `InitializeDatabasesTask` → `IsColdBootTask` → `ColdBootFoldersTask` →
**`EnsureRootNodeIsInCloudAndSyncDatabasesTask`** → **`ExcludeCloudFoldersInRootTask`** →
`StartAppStateMachineTask` → `EventHandlerWorker` → `EnableBackupModeTask` → onboarding →
`WorkerWarmupTask<SyncWorker>` → `CloudDriveGetChangesWorker` → `FileScannerService` → … →
**`ShowSyncFolderStartupTask`** → `ShowDriveWindowStartupTask` → `ShowReleaseNotesWindowTask`.
→ **belegt**

| Task | Behaviour |
|---|---|
| `EnsureSyncFolderIsValidTask` | returns immediately when `sync.db` does not exist; otherwise folder valid → `args.SyncFolderExistsAndIsValid = true` and, when `AppMode == None`, add the `Sync` flag (an existing setup is restored after a mode reset); folder missing **and** sync mode on → warning + `DisableSync()` |
| `DeleteSyncDatabaseTask` | no-ops when the customer saw the login window *and* the folder is valid, or when `args.EnablingSync`, or when the app is not in sync mode and not enabling it; otherwise **deletes `sync.db` and restarts the app** (`IAppRestart.Restart()`) — a corrupt or abandoned sync state is wiped, not repaired |
| `EnsureRootNodeIsInCloudAndSyncDatabasesTask` | `ListNodes("isRoot:true")` → saves the root node into both the cloud and the sync database; on a failed/empty response it logs "cannot proceed without root node" and returns (sync simply does not start) |
| `ShowSyncFolderStartupTask` | should-run: signed in **and** onboarding complete **and** `args.ShowSyncFolder`; runs `OpenSyncFolderCommand` when the customer saw the onboarding window, or when the startup source is `AppStartupFlags.Desktop` and the folder is valid (a relaunch reveals the folder in Finder) |
| `ShowSyncStatusWindowStartupTask` | should-run: signed in **and** onboarding complete **and** `args.Reopen` **and** `args.ShowSyncStatusWindow`; shows the drive window with `InitialScreen = Screen.Sync`, `LaunchSource = DriveWindowLaunchSource.Unknown`; also listed in `ExternalStartupTaskManager` (with `UploadArgStartupTask` and `ShowSyncFolderStartupTask`) |

`ExcludeCloudFoldersInRootTask` runs early (right after the root-node task) so the exclusion
  list is consistent before any sync operation starts.

**Cold boot.** `IsColdBootTask` / `ColdBootFoldersTask` populate the **cloud** node cache
(`cloud.db`) by walking folders; the sync tab shows `…_contextmenu_message_getchanges`
("Konfiguration wird durchgeführt") while `appStatus.IsColdBooting`.
`FileColdBootCompletedEvent` is the trigger for the sync item list to load from the database for
the first time (`SyncItemsViewModel.InitializeTransferItemsFromDatabase` bails unless
`ColdBootState == Complete`). `SelectiveSyncWindowViewModel.Refresh` explicitly **waits** for the
folder cold boot and for a running `/changes` pass before it reads the tree — the
selective-sync picker is only trustworthy after both finished.
`SyncSettingsCalledBeforeColdbootingException` rejects sync settings changes arriving before the
cold boot. → **belegt**

### 3.10 Sync UI surfaces, mass-delete notification, deprecation

**Sync tab** (`Screen.Sync`, `SyncStatusViewController` inside a `DriveTabItemView`): a **32 pt
status icon** (`sync-icon-blue32` while syncing, `complete-icon-green32`, `pause-icon-gray32`,
`error-icon32`) plus a bold 14 pt detail label; a "Status anzeigen:" label with a popup button
offering exactly two filters — **Syncing** and **Failed** (`AvailableFilters`); the table
(`SyncTransferItemsTableView`) with Name / Size / Status / remaining-time columns
(`…_syncingwindow_*`), waiting rows showing "In Warteschlange"; a spinner while
`TransferItemsLoading`; and the tab state is `EmptyState` unless `AppMode` has the `Sync` flag.
→ **belegt**

`SyncStatusTabViewModel` derives the visible state (`RemainingItems = Stats.InProgressCount +
Stats.QueuedCount`, `ErroredCount = Stats.FailedCount`, `ShouldShowError = ErroredCount > 0`):
→ **belegt**

| State | Condition | Label |
|---|---|---|
| Configuring | `appStatus.IsColdBooting` | `…_contextmenu_message_getchanges` ("Konfiguration wird durchgeführt") |
| Syncing | `IsSyncing` and not paused/pausing/resuming/configuring | `…_statuswindow_statusprocessing` ("In Bearbeitung"), or `…_syncwindow_syncingitems[_plural]` with the remaining count as `N0` |
| Pausing | worker `Transitioning` from `Running` | `…_syncwindow_pausesync` ("Synchronisierung anhalten") |
| Paused | worker `Paused` | `…_syncwindow_syncpaused` ("Angehalten") |
| Resuming | worker `Transitioning` from `Paused` | (falls through to the app-status refresh) |
| Synced | nothing syncing, nothing remaining, no rows | `…_contextmenu_message_transfercomplete` ("Abgeschlossen") |
| Error (Failed filter) | `ErroredCount > 0` | `…_syncwindow_failedtosync` ("Synchronisieren von {0} Element(en) fehlgeschlagen") |
| Special | no network / service unavailable / over quota | `…_syncwindow_nonetwork`, `…_syncwindow_serviceunavailable`, `…_syncwindow_specialmessage_overquota`, `…_…_nointernetconnection`, `…_…_overquotaunlimitedphotos`, `…_…_overquotamanagestorage` |

**Sync item list** (`SyncItemsViewModel`): rows load from `sync.db` with `count =
SyncItemsListSizeLimit` (**100**) ordered `local_path asc`, filtered to
`Upload|Download|Folder` transfer types for the Syncing filter (errored items load with
`includeRetriableErrors`) and sorted processing → completing → waiting. The list is
**suspended and cleared while the tab is not selected** (`SyncItemsLoadSuspended`,
`HandleSelectedScreen`), refreshed on `FileColdBootCompletedEvent` / filter change /
end of suspension, and throttled to one database request per **10 s**
(`_databaseRequestInterval`). Progress comes from `FileTransferProgressEvent`, row updates
from `ISyncEvent`s. → **belegt**

**Status popup row** (`SyncStateRowViewModel`, a `TransferStateRowViewModel<IGlobalSyncTransferState>`):
the transferring icon is `ImageType.SyncBlue` (`sync-icon-blue24` — the only place that image is
used), the texts are `…_statuspopup_syncing` ("Wird synchronisiert") and
`…_statuspopup_synccomplete` ("Synchronisierung abgeschlossen"), the cold-booting text is shared
(`…_statuswindow_statuswaiting`), and its action is `RetryErrored` → `RetryFailedSyncItemsIntent`.
→ **belegt**

**Navigation / menu.** The nav item "Synchronisieren" (`…_navigationMenu_sync`, icon
`NavMenuItemSync`) switches to `Screen.Sync`; the header button uses `…_contextmenu_opensynctab`
("Synchronisieren"); the menu-bar toggle uses `…_enablesync` / `…_disablesync`.
`…_contextmenu_enablesynctab` and `…_statuswindow_synctabtitle` are **not referenced**. The
**preferences → Sync tab** (`…_preferences_sync_title` "Synchronisieren") carries the
enable/disable entry, the selective-sync picker (`…_preferences_selective_sync_title` "Wähle
Ordner in deinen Amazon Photos zum Synchronisieren aus:" + button "Ordner wählen...") and the
new-top-level-folder toggle (`…_preferences_exclude_syncing_new_cloud_folders`). Whether the tab
is offered at all is `ShowSyncTab = (CanEnableSync || AppMode has Sync || PreviouslyEnabledSync)
&& !HasSyncBeenMigratedToBackup`; `PersistSyncSettings()` is gated on `IsSyncEnabled` and runs
the confirmation flow of §3.8. `…_preferences_sync_new_cloud_folders` is **not referenced**.
→ **belegt**

**Mass-delete notification.** `SyncMassDeletesNotificationHandler` subscribes to two events and
applies the same sliding-window rule to both: `SyncLocalFileDeletedEvent(count)` (local
files/folders were deleted because their cloud nodes were trashed) and
`SyncCloudNodeTrashedEvent(count)` (cloud nodes were trashed, so local files/folders were
removed). Both pass through `PublishAfterExceedingThresholdInSlidingTimeBuffer(count, window,
threshold)` with `MassDeletesNotificationWindowInSeconds` = **30 s** and
`MassDeletesThresholdForCurrentWindow` = **200**: a notification fires when **more than 200
deletions accumulate inside a 30-second sliding window**. Folder deletes contribute their
descendant count (`GetDescendantNodeIdCount`), so deleting one folder with 300 children crosses
the threshold; single-file deletes contribute 1. On trigger: one `SyncDeletesNotification` with
title `…_notification_syncmassdeletes_title` ("Neue Nachricht von Amazon Photos"), message
`…_notification_syncmassdeletes_shortmessage` ("Eine große Anzahl von Dateien wurde gelöscht.")
and button `…_notification_syncmassdeletes_button` ("Weitere Informationen"), plus
`BusinessMetrics.MassDelete` / `HealthMetrics.MassDelete.{Local,Cloud}`. Each direction then has
a cooldown of `WaitTimeBetweenMassDeletesNotificationsInSeconds` = **3600 s** before it can
notify again. `…_notification_syncmassdeletes_message` ("Du hast eine große Anzahl von Dateien
aus Amazon Photos gelöscht") is defined but **not referenced**. → **belegt**

**Deprecation and migration to backup (why sync is legacy here).** `SyncDeprecationBannerViewModel`
shows `…_syncdeprecationbanner_header` ("Wichtige Aktualisierung") and
`…_syncdeprecationbanner_description` ("Amazon Photos wird Sync nicht mehr unterstützen. Ab dem
29.02.2024 …") with a "learn more" hyperlink, gated on a remote-experiment treatment.
`SyncToBackupMigrationBannerViewModel` is shown when the treatment is `T1`, sync mode is on and
`!HasSyncBeenMigratedToBackup`; its intent leads to `SyncToBackupMigrationHelper`, which turns the
**sync folder into a scheduled one-way backup** (`PrepareBackup(syncPath)` with
`DestinationNodeId = cloud root` and `ContentFilter.PhotosAndVideo`), saves the schedule, disables
sync, publishes `ScanAndUploadNowScheduleIntent`, shows the Backup screen with
`DriveWindowLaunchSource.FromSyncToBackupMigration`, and sets `PreviouslyEnabledSync = false`,
`HasSyncBeenMigratedToBackup = true`, hiding the sync banner and nav item. If sync was already
inactive it just disables it and reports success. The onboarding page mirrors the direction:
`…_onboarding_education_sync_info` ("Beim Löschen von Inhalten in der Cloud werden diese lokal
gelöscht und umgekehrt"), `…_onboarding_education_sync_subtitle` ("… werden überall gespiegelt")
and `…_onboarding_setupsync_moreinfo` ("Beim Löschen von Inhalten online werden diese auch lokal
gelöscht"). → **belegt**

**Sync-relevant events.** `SyncStartEvent`, `SyncStoppedEvent { ErrorOccured, Canceled }` (engine
lifecycle), `FolderStatusBulkUpdateEvent`, `SyncStatusEvent` (row/status churn; sampled into the
folder-status timer), `RootFolderStatusEvent`, `SyncLocalFileDeletedEvent`,
`SyncCloudNodeTrashedEvent`, `FileColdBootCompletedEvent`, `SyncSettingsChangedEvent`
(selective-sync changes persisted), plus the intents `EnableSyncIntent` / `DisableSyncIntent`
(`ToggleSyncSource`), `ShowSelectiveSyncWindowIntent`,
`SelectiveSyncWindowToggleFolderStateIntent`, `CloseSelectiveSyncWindowIntent`,
`ChooseSyncFolderIntent`, `RetryFailedSyncItemsIntent`, `SyncToBackupMigrationIntent`,
`ForceSyncToBackupMigrationIntent`. → **belegt**

---

## 4. String evidence (keys → German)

Keys cited in this doc, verbatim from `de.lproj/Localizable.strings`. The right-hand column
says whether the **managed assembly references** the key (a `Strings.<key>` call site outside
the `Strings` accessor class); keys reached only through a literal string inside a dictionary
are marked *(literal)*. Long German texts shortened with "…". → **belegt**

### 4.1 Enable / disable / navigation / folder binding

| Key (… = `clouddrive_desktop_`) | German | Referenced? |
|---|---|---|
| `…_enablesync` | "Synchronisierung aktivieren..." | yes (menu-bar item) |
| `…_enablesyncconfirmation` | "Möchten Sie die Synchronisierung wirklich aktivieren?" | yes (`EnableSyncIntentHandler`) |
| `…_enablesyncconfirmationextrainfo` | "Hierfür muss die Anwendung neu gestartet werden" | yes (`EnableSyncIntentHandler`) |
| `…_disablesync` | "Synchronisierung deaktivieren..." | yes (menu-bar item) |
| `…_disablesyncconfirmation` | "Möchten Sie die Synchronisierung wirklich deaktivieren?" | yes (`DisableSyncIntentHandler`) |
| `…_disablesync_explanation` | "Wenn die Synchronisierung deaktiviert ist, werden Änderungen in deinem Synchronisierungsordner nicht mehr … kopiert. …" | **no** |
| `…_navigationMenu_sync` | "Synchronisieren" | yes (nav item → `Screen.Sync`) |
| `…_contextmenu_opensynctab` | "Synchronisieren" | yes (header button) |
| `…_contextmenu_enablesynctab` | "Synchronisierung aktivieren" | **no** |
| `…_statuswindow_synctabtitle` | "Synchronisieren" | **no** |
| `…_syncfolder_info` | "Dein Amazon Photos-Ordner enthält Dateien und Ordner, die du mit Amazon synchronisierst." | **no** |
| `…_syncfolder_changetitle` | "Synchronisierung deaktiviert" | yes (`EnsureSyncFolderIsValidTask`, `DisableSyncIntentHandler`) |
| `…_syncfolder_changedetail` | "Die Synchronisierung wurde deaktiviert, da wir festgestellt haben, dass Ihr Synchronisierungsordner verschoben oder gelöscht wurde. …" | yes (same two sites) |
| `…_schedules_syncfolder_warningmessage` | "Ihr Synchronisierungsordner oder Inhalte in der Synchronisierung können nicht als Ordner zur Datensicherung hinzugefügt werden." | yes (3 sites) |
| `…_onboarding_invalidsyncfolder_header` / `…_message` | "Bitte wählen Sie einen anderen Ordner aus." / "Es können nur Ordner in Ihrem Benutzerverzeichnis als Synchronisationsordner festgelegt werden" | yes / **no** |
| `…_onboarding_syncfolder_changeexisting_header` / `…_warningmessage` | "Synchronisierungsordner aktivieren" / "Dieser Ordner ist bereits mit Ihrem Konto verknüpft. Möchten Sie wirklich in einen neuen Ordner neu anfangen?" | yes (both) |
| `…_onboarding_syncfolder_overlappingwithautosave_header` / `…_warningmessage` | "Bitte wählen Sie einen anderen Ordner aus." / "Dieser Ordner ist bereits zur Datensicherung ausgewählt und kann nicht zur Synchronisierung verwendet werden. …" | yes (both) |

### 4.2 Status / progress / errors

| Key | German | Referenced? |
|---|---|---|
| `…_statuswindow_statussyncing` | "Synchronisierung" | yes *(literal)* — filter label |
| `…_statuswindow_showstatus` | "Status anzeigen:" | yes (`SyncStatusViewController`) |
| `…_statuspopup_syncing` | "Wird synchronisiert" | yes (`SyncStateRowViewModel`) |
| `…_statuspopup_synccomplete` | "Synchronisierung abgeschlossen" | yes (`SyncStateRowViewModel`) |
| `…_syncwindow_pausesync` | "Synchronisierung anhalten" | yes (pausing state) |
| `…_syncwindow_resumesync` | "Synchronisierung fortsetzen" | yes (resume toggle) |
| `…_syncwindow_syncpaused` | "Angehalten" | yes (paused state) |
| `…_syncwindow_syncingitems` / `…_syncingitems_plural` | "Synchronisierung läuft ({0} Element verbleibend)" / "… ({0} Elemente verbleibend)" | yes (both; singular / plural branch) |
| `…_syncwindow_failedtosync` | "Synchronisieren von {0} Element(en) fehlgeschlagen" | yes (Failed filter header) |
| `…_syncwindow_cantsyncfailedcount` | "{0} Dateien können nicht synchronisiert werden" | **no** |
| `…_syncwindow_cantsyncprotectedfolder` | "Der Ordner kann nicht synchronisiert werden. Bitte benennen Sie den Ordner zum Starten der Synchronisierung um." | yes *(literal, `LongErrorStringMap`)* |
| `…_syncwindow_settings` / `…_foldertooltip` / `…_recentchanged` | "Einstellungen" / "Synchronisationsordner öffnen" / "Kürzlich geändert" | **no** (all three) |
| `…_syncwindow_nonetwork` | "Kein Netzwerk" | yes (status notification) |
| `…_syncwindow_serviceunavailable` | "Verbindung kann nicht hergestellt werden." | yes (status notification) |
| `…_syncwindow_specialmessage_nointernetconnection` / `…_overquota` / `…_overquotamanagestorage` / `…_overquotaunlimitedphotos` | "Keine Internetverbindung." / "Der Speicher Ihres Kontos ist aufgebraucht." / "Speicher verwalten" / "Video- und Dateispeicherplatz ist aufgebraucht. Fotos können weiterhin synchronisiert werden." | **no** (all four) |
| `…_statuswindow_statuswaiting` / `…_statusprocessing` / `…_statusretrying` / `…_statuscompleting` / `…_statuscompleted` / `…_statuscanceled` / `…_statusfailed` | "Warten" / "In Bearbeitung" / "Wiederholen" / "Wird überprüft" / "Abgeschlossen" / "Abgebrochen" / "Fehlgeschlagen" | yes *(literal, `SyncStatusStringMap`)* (all seven) |
| `…_contextmenu_message_getchanges` | "Konfiguration wird durchgeführt" | yes (configuring state) |
| `…_contextmenu_message_transfercomplete` | "Abgeschlossen" | yes (synced state) |
| `…_statuswindow_downloadnotallowedinsyncfolderheader` / `…_body` | "Bitte wählen Sie einen anderen Ordner aus." / "Das Herunterladen von Inhalten in deinen Synchronisierungsordner kann dazu führen, dass Duplikate … erstellt werden. …" | yes (both) |
| `…_statuswindow_errorlong_pathinvalidcharacters` | "Bitte geben Sie dem Element auf der Amazon Photos-Website einen neuen Namen, um es zu synchronisieren." | **no** |
| `…_failedsyncitemswindow_title` / `…_retry` | "Fehlgeschlagene Elemente" / "Wiederholen" | **no** (both) |
| `…_retryall` | "Alle erneut versuchen" | yes (error-table button) |
| `…_statuswindow_readdtoqueue` | "Erneut zur Warteschlange hinzufügen" | yes (per-row action, 2 sites) |
| `…_displayitememptymessage_sync_all` / `_waiting` / `_processing` / `_completed` / `_failed` / `_cancelled` | "Keine Dateien synchronisiert. (Fügen Sie neue Inhalte hinzu!)", "Alle Dateien synchronisiert. (Keine Sorge!)", "Alle Dateien synchronisiert. (Es ist alles in Ordnung!)", "Keine Dateien synchronisiert.", "Keine Synchronisierung fehlgeschlagen. (Puh!)", "Keine abgebrochenen Dateien. (Das ist eine Erleichterung!)" | yes *(literal)* (all six) |
| `…_syncingwindow_title` / `…_filenamecolumntitle` / `…_sizecolumntitle` / `…_statuscolumntitle` / `…_timeremainingcolumntitle` / `…_waitingtimeremaingtext` | "Amazon Photos" / "Name" / "Größe" / "Status" / "Verbleibende Zeit" / "In Warteschlange" | **no** (all six) |
| `…_signout_message` | "Der Inhalt bleibt auf diesem Computer und wird nicht mehr mit Amazon synchronisiert." | yes (sign-out dialog) |

### 4.3 Selective sync, preferences, notifications, deprecation, onboarding

| Key | German | Referenced? |
|---|---|---|
| `…_selectivesyncwindow_title` | "Selektive Synchronisierung" | yes |
| `…_selectivesyncwindow_header` | "Wählen Sie die Ordner, die Sie synchronisieren möchten." | yes |
| `…_selectivesyncwindow_info` | "Alle Dateien, die sich nicht in einem Ordner befinden, werden synchronisiert." | yes |
| `…_selectivesyncwindow_selectall` / `…_selectnone` / `…_sync` | "Alle auswählen" / "Gesamte Auswahl aufheben" / "Synchronisieren" | yes (all three) |
| `…_selectivesyncwindow_confirmationbody` | "Wenn du die Synchronisierung für einen Ordner deaktivierst, wird der Ordner von deinem Computer entfernt, bleibt aber in Amazon Photos gespeichert." | yes (`PersistSyncSettings`) |
| `…_selectivesyncwindow_confirmationheader` | "Änderungen bestätigen" | **no** (the body key serves as both header and body at the call site) |
| `…_selectivesyncwindow_contextmenuviewlocally` / `…_contextmenuviewonweb` | "Ordner öffnen" / "Anzeigen auf Amazon..." | **no** (both) |
| `…_preferences_sync_title` | "Synchronisieren" | yes (preferences tab) |
| `…_preferences_selective_sync_title` / `…_button_text` | "Wähle Ordner in deinen Amazon Photos zum Synchronisieren aus:" / "Ordner wählen..." | yes (both) |
| `…_preferences_sync_new_cloud_folders` | "Automatisch neue Ordner mit Amazon Photos synchronisieren" | **no** |
| `…_preferences_exclude_syncing_new_cloud_folders` | "Synchronisierung für neue Ordner auf oberster Ebene deaktivieren, die ich auf Amazon Photos erstelle" | yes |
| `…_notification_syncmassdeletes_title` | "Neue Nachricht von Amazon Photos" | yes |
| `…_notification_syncmassdeletes_shortmessage` | "Eine große Anzahl von Dateien wurde gelöscht." | yes |
| `…_notification_syncmassdeletes_button` | "Weitere Informationen" | **no** |
| `…_notification_syncmassdeletes_message` | "Du hast eine große Anzahl von Dateien aus Amazon Photos gelöscht" | **no** |
| `…_notification_syncupdate_title` / `…_message` / `…_button` / `…_otherbutton` | "Update verfügbar" / "Eine neue Version von Amazon Photos ist verfügbar." / "Aktualisieren" / "Nicht jetzt" | **no** (all four) |
| `…_syncdeprecationbanner_header` / `…_description` | "Wichtige Aktualisierung" / "Amazon Photos wird Sync nicht mehr unterstützen. Ab dem 29.02.2024 kannst du die Funktion nicht mehr nutzen, wenn du Sync ausschaltest." | yes (both) |
| `…_onboarding_startsync` | "Synchronisierung starten" | yes |
| `…_onboarding_setupsync_title` / `…_subtitle` | "Ordner synchronisieren" / "Mit der spiegelähnlichen Funktionalität der Synchronisierung können Sie Dateien in Ihrem Synchronisierungsordner bearbeiten …" | yes (both) |
| `…_onboarding_setupsync_choose_sync_folder_title` / `…_choose_amazondrive_folders_title` / `…_choose_amazondrive_folders_button` / `…_otheritem_title` | "Lokaler Synchronisierungsordner" / "Amazon Photos-Ordner:" / "Ordner wählen..." / "Sonstiges..." | yes (all four) |
| `…_onboarding_education_sync_title` / `…_subtitle` / `…_info`; `…_setupsync_moreinfo`; `…_setupsync_enablesync_checkbox_title`; `…_setupsync_choose_sync_folder_button`; `…_setupsync_learnmorelink`; `…_customizesync_header` / `…_info_1`; `…_startsyncing_header`; `…_allmode_setupcomplete_opensyncfolder`; `…_opensyncfoldericon_info`; `…_welcome_info` | "Ordner synchronisieren" / "Nehmen Sie Ihre aktualisierten Dateien dorthin mit, wo Sie sie brauchen. …" / "Beim Löschen von Inhalten in der Cloud werden diese lokal gelöscht und umgekehrt" / "Beim Löschen von Inhalten online werden diese auch lokal gelöscht" / "Synchronisation für Amazon Photos aktivieren" / "Ordner auswählen..." / "Weitere Informationen" / "Ordner von Amazon Photos synchronisieren" / "Wenn bereits Ordner in Ihrem Amazon Photos-Konto vorhanden sind, können Sie diese mit Ihrem Computer synchronisieren. …" / "Klicken Sie auf das Symbol zum Überprüfen des Fortschritts und mehr" / "Ihren Synchronisierungsordner öffnen" / "Ihren Synchronisierungsordner öffnen" / "Alles, was du zu diesem Ordner hinzufügst, wird mit deinen Amazon Photos synchronisiert." | **no** (all twelve) |

Three observations. (1) The **onboarding education page** family (`…_onboarding_education_sync_*`,
`…_onboarding_welcome_info`, `…_onboarding_setupsync_moreinfo`,
`…_onboarding_setupsync_enablesync_checkbox_title`) and the whole `…_syncingwindow_*`
column-title family are **defined but not referenced** by the managed assembly — *inferred*
they are rendered by a nib/storyboard or belong to a retired onboarding variant, exactly like
the unreferenced drag-drop keys in `SPEC_ONE_TIME_UPLOAD.md` §8. (2) The two keys the task
named explicitly — `…_failedsyncitemswindow_title` and `…_failedsyncitemswindow_retry` — are
**not referenced** either; the failed-items window that *is* built reuses the shared
`UI.ErrorWindow` service plus the generic `…_retryall` / `…_statuswindow_readdtoqueue`
actions. The "Fehlgeschlagene Elemente" window described in `UI_FLOWS.md` §6 is therefore
*inferred* to be that shared window with a sync-specific title coming from elsewhere (nib or
the call site), not from these two keys. (3) `…_selectivesyncwindow_confirmationheader` is
unused because the call site passes the *body* key twice. → **belegt** (absence) / *inferred*
(explanations)

---

## 5. Gap analysis vs. our port (committed state)

Our committed code was read for this table (`src/`, 20 modules, ~6.2k lines; CodeGraph was
not used). The short version: **our port has no bidirectional sync at all** — it is a one-way
uploader. The only places the word "sync" appears are `background_sync` (a periodic *re-scan
and upload* loop) and Rust's `std`/`tokio::sync` primitives. → **belegt**

| Capability | Original | Our port | File reference |
|---|---|---|---|
| Sync folder (one bound local dir, xattr marker, branded icon, Finder favourite) | `AppSyncFolder` + `com.amazon.drive.sync.root` xattr, `SyncPath` in customer settings | **none** — no sync-folder concept; backup folders are a plain list of scan roots | `src/folders.rs` (`FolderStore`, `BackupFolder`) |
| Enable/disable sync with confirmation + app restart | `EnableSyncIntentHandler` → delete `sync.db` → `Restart("--enable=Sync")` | **none** — no enable/disable flow, no restart hook | — |
| Sync-mode workers started/stopped by mode | `[RunContext(AppMode.Sync)]` × 3, stopped by `AppModeHandler.DisableSync` | **none** — one upload worker, no mode concept | `src/upload.rs` (`upload_worker`) |
| Sync engine + queue | `SyncWorker` + `SyncContext`: 6 queues, batch 50, 5 s re-request gate, round-robin over NonBlocking/Upload/Download, `SyncMaxQueues` 20, **separate** upload/download slot budgets | **partly (upload only)** — a single global queue with one `concurrency` (1..8) pool, no per-direction budgets, no DB-backed queue | `src/upload.rs` (`UploadShared`, `run_upload`), `src/config.rs` (`concurrency`) |
| Engine poll / idle behaviour | `SyncPollIntervalInSeconds` 60 s; engine sleeps when 0 remaining | **partly** — background re-scan loop ticks (first 15 min, then 90 min per `SPEC_BACKGROUND_SYNC.md` §6) | `src/backup.rs` (`background_sync_loop`, `background_sync_tick`) |
| `SyncEngineQueueTimeoutInSeconds` (120 s) progress watchdog | health/business metric on queue stall, no cancellation | **none** — no stall watchdog | — |
| Change detection | OS watcher on the sync folder (self re-arming), cloud `/changes`, folder rescan 24 h, missing-file scan, cloud rescan 1 h | **planned, not implemented** — decision 2026-10-14 mandates OS filesystem events (no polling) + `max mtime` watermark; no fs-watcher crate in `Cargo.toml` yet (`walkdir` for scans, `notify-rust` for OS notifications only) | `Cargo.toml`, `src/folders.rs` (`scan_images`) |
| Local change test | `Size` differs or `ModifiedDate` not equal to the second | **partly** — `stat_matches`/`should_skip` compare `(path, size, mtime)` before hashing (decision 2026-10-14) | `src/dedup.rs` (`stat_matches`, `should_skip`) |
| Selective sync (per-cloud-folder exclusion list, tri-state tree, confirmation) | `excluded_folders` + `SelectiveSyncService` + recursive CTE over cloud/sync DBs | **none** — no cloud-side folder inventory is kept; destinations are not modelled | `src/prime_photos.rs` (cookie upload into the account, no node model) |
| Downloads (sync direction) | `DownloadOperation`, `DownloadSlot`, `DownloadQueues` | **none** — settings state "Downloads sind in diesem Port noch nicht implementiert." | `src/settings.rs`, `src/i18n.rs` |
| Deletes / trash / moves / renames (sync direction) | 43 operation types incl. `TrashCloudNodeOperation`, `DeleteLocalFile…`, `Move…`, `Rename…` | **none** — upload-only; no local deletion, no cloud move/rename | `src/prime_photos.rs` (`upload_*`) |
| Non-media files in the sync folder | blocked by `BlockUploadNonMediaFileRule` (marked synced, never uploaded) | **n/a but parallel** — we only *scan* supported images in the first place | `src/folders.rs` (`is_supported_image`, `supported_extensions`) |
| Retry / backoff | `retries`, `next_retry`, min 5 retries, 1 s → 6 h, `RetryReason` (18 values) | **none** — a failed item is reported once; the run ends | `src/upload.rs` (`log`, status) |
| Conflict rules (both sides changed) | 23 `IRuleConflictRule` types | **none** — our conflict handling is the server's `conflictResolution` for uploads only (`NONE`/`RENAME`/`OVERWRITE`) | `src/folders.rs` (`ConflictResolution`), `src/upload.rs` (`resolve_conflict`) |
| Error window / retry-all / re-add-to-queue | shared `ErrorWindow` + `ResetErroredToWaiting` + per-row action | **none** — errors surface as a count/text in the run result | `src/upload.rs` (`status_text`), `src/backup.rs` |
| Sync status vocabulary | `SyncStatus` (7) / `SyncTransferType` (6) / `CloudStatus` (6) / `TransferStatus` (12) + label maps | **much smaller** — `FolderStatus` (idle/running/done/error/paused) | `src/folders.rs` (`FolderStatus`), `src/upload.rs` (`status_text`) |
| Progress surfaces | sync tab (2 filters, 100-row cap, 4 columns), status popup row with `ImageType.SyncBlue`, per-folder bubbled status | **partly** — one aggregated progress display over all uploads (decision 2026-10-14), plus tray + `run_finished` toast | `src/upload.rs` (`upload_progress_text`, `eta_secs`), `src/backup.rs`, `src/notify.rs`, `src/tray.rs` |
| Mass-delete notification | >200 deletions in a 30 s sliding window, 1 h cooldown per direction | **none** — no delete path, no such notification | — |
| Pause/resume | per-worker `IPausableWorker` + global pause; records reset to `Waiting` so nothing sticks in `Processing` | **yes (global)** — `paused` persisted in `config.toml`, blocks background sync and queued runs | `src/config.rs` (`paused`), `src/backup.rs`, `src/settings.rs` |
| Persistence | `sync.db` v17 (`files`, `nodes`, `node_parents`, `settings`, `excluded_folders`) | **different** — one `uploads` table (path/md5/size/mtime) in one SQLite next to the MD5 index; the ad-hoc drag-drop queue is persisted temporarily (decision 2026-10-14) | `src/dedup.rs` |
| Sync → backup migration | `SyncToBackupMigrationHelper` (sync folder becomes a scheduled `PhotosAndVideo` backup, sync disabled, nav item hidden) | **n/a** — we are already backup-only; the "migrated" state is our default | — |
| i18n layer | `Localizable.strings` (~120 sync keys, many unreferenced) | central de/en table (`tr()`) with one "Hintergrund-Synchronisierung" entry | `src/i18n.rs` |

Two notes. (a) The **queue** row: the original's queue is a *persistent, priority-shaped,
per-direction* queue; ours is a single `concurrency`-bounded worker pool over one in-memory item
list — sufficient for upload-only, and the reason the "8" cap in `AGENTS.md` is a settings cap
rather than a structural one. (b) The **change-detection** row: the decision of 2026-10-14 (OS
events, no polling, `max mtime` watermark) is deliberately **not** a port of the original's design
— the original keeps a 24 h folder rescan and a 1 h cloud rescan as safety nets, which our
decision discards in favour of the watermark. → **belegt** (our side) / *inferred* (consequences)

---

## 6. UNKNOWN (explicitly not determined)

- **Can sync be enabled for a new account in this build?** `CanEnableSync` defaults to **false**
  and is server-gated, and a deprecation banner exists. Whether a fresh account can still enable
  sync (vs. only users with `PreviouslyEnabledSync` or an active `AppMode.Sync`) is server state
  we cannot read. → **UNKNOWN**
- **`app-settings.json` shipped values.** All numbers in §3.9 are the compiled `AutoGet`
  defaults; no `app-settings.json` was located in the inspected resources, so the server-tuned
  values (queues, timeouts, thresholds) may differ. → **UNKNOWN**
- **Cold-boot resume.** Whether an interrupted folder cold boot continues or restarts on
  relaunch. → **UNKNOWN**
- **`…_syncingwindow_*` and `…_failedsyncitemswindow_*` render sites.** Both families are
  unreferenced; whether a nib/storyboard renders them (and thus whether the sync table's column
  titles and the failed-items window title are localised at all) cannot be determined from the
  managed assembly. → **UNKNOWN** (*inferred* nib)
- **Root-folder row inclusion.** `GlobalStatsQuery` excludes `root <> 1`, but whether the root
  record also appears as a row in the sync list is not traced. → **UNKNOWN**
- **Full index set of `sync.db` v17.** `IX_uuid`, `IX_node_id`, `IX_file_path`, `IX_parent_path`,
  `IX_cloud_status`, `IX_status`, `IX_target_type`, `IX_path` were read from attributes; the
  complete index list and what each migration added was not reconstructed. → **UNKNOWN**
- **Concrete error→reason mapping per rule.** The rules and enums are read; which
  `TransferError`/`RetryReason` pair each rule produces was not traced rule by rule.
  → **UNKNOWN**
- **`ExcludeHiddenFilesAndFolders` default** and the **sync folder's default path** — the setting
  is read but its default was not read; `SyncPath` is written from the user's choice and no
  default constant was found. → **UNKNOWN**
- **Rate limiting for sync.** `UploadTransferKiloBytesPerSecond` /
  `DownloadTransferKiloBytesPerSecond` exist in customer settings; whether the sync operations
  honour them, and where, was not traced. → **UNKNOWN**
- **Interaction with the 8-queue cap.** `SyncMaxQueues` defaults to 20 while the *upload* settings
  cap is `CustomerSettingsMaxSupportedUploadQueues = 8` (`AGENTS.md`). Whether the sync engine is
  ever clamped to that cap was not determined. → **UNKNOWN**
- **Whether unselecting a parent folder cascades.** `UpdateNode` deletes the node's own exclusion
  row plus rows whose *path* is under it; whether an unselected parent implicitly blocks
  already-synced children is *inferred* yes (the `…UnselectNewCloudFolders…` /
  `DeleteLocalUnselectedSynced…` rules exist) but the exact cascade was not traced.
  → **UNKNOWN** (*inferred*)
- **`Interrupt()` callers.** `SyncWorker.Interrupt` stops the run loop, resets records and starts
  again; who calls it (settings change? network change?) was not traced. → **UNKNOWN**
- **Sync revival without a restart, and the `Transitioning` state.** After `DisableSync` the
  workers are stopped and only `EnableSync` (which restarts the app) brings them back; whether any
  other path re-arms them is *inferred* no. Likewise, how a pause of the shared
  `AggregatedWorkerState` propagates into the sync worker's `Transitioning` state was not traced.
  → **UNKNOWN** (*inferred*)

---

## 7. Cross-references

- `docs/UI_FLOWS.md` §6 (Sync tab `Screen.Sync` — legacy), §7 (preferences window, Sync tab),
  §11.2 (status precedence), §11.4 (global pause toggle), §12 (dialogs).
- `docs/UI_STATES.md` (transfer-item row model, status popup rows).
- `docs/SPEC_BACKGROUND_SYNC.md` — owns scan/cadence/watchers/pause for *scheduled backups*.
  Sync reuses the same watcher patterns (`ObservableFileSystemWatcher`, `RunLoopWorker`) but
  with its own workers and settings; `BackupScanIntervalInHours = 336` is a backup setting,
  not a sync setting.
- `docs/SPEC_FOLDERS.md` — folder scan/filter contract (`is_supported_image`, `ContentFilter`),
  and the mutual exclusion between sync folders and backup folders
  (`…_schedules_syncfolder_warningmessage`).
- `docs/SPEC_DEDUP.md` — MD5 identity. Sync stores `md5`/`crc` on the record but its *change*
  test is size+mtime (§3.7), not a re-hash.
- `docs/SPEC_ONE_TIME_UPLOAD.md` — the one-time queue is a separate pipeline
  (`schedule_id = "ManualUpload"`), not part of sync.
- `docs/SPEC_LIBRARY.md` — downloads as a user-initiated file operation, trash, album/file
  management; this doc only covers downloads as a sync direction (`DownloadOperation`,
  `…_statuswindow_downloadnotallowedinsyncfolder*`).
- `docs/SPEC_NOTIFICATIONS.md` — `SyncDeletesNotification` and the status-popup notification
  message (`…_syncwindow_nonetwork`, `…_syncwindow_serviceunavailable`).
- Our code: `src/folders.rs` (`FolderStore`, `is_supported_image`, `ConflictResolution`,
  `scan_images`), `src/upload.rs` (`UploadShared`, `upload_worker`, `run_upload`,
  `status_text`), `src/dedup.rs` (`DedupIndex`, `stat_matches`, `should_skip`,
  `record_upload`), `src/config.rs` (`Config`, `paused`, `background_sync`, `concurrency`),
  `src/backup.rs` (`background_sync_loop`, `background_sync_tick`), `src/notify.rs`
  (`run_finished`), `src/settings.rs`, `src/i18n.rs`, `src/prime_photos.rs` (cookie upload
  path).
