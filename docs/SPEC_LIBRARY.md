# Cloud Library & Downloads / Restore — Clean-Room Spec (reverse-engineered)

Behavioural specification of the **original** Amazon Photos desktop client's **download /
restore** feature (the "Herunterladen" tab) and of the **cloud-library / file-management**
capabilities that exist behind it in the shipped assembly. The behaviour below is
**reverse-engineered from the shipped managed assembly**, not inferred from string tables alone.

This document is **clean-room**: it records *observed behaviour* in our own words and tables.
It contains **no verbatim decompiled code** and **no personal data**. German user-facing labels
are quoted from the shipped `de.lproj/Localizable.strings` (product strings, not code); type and
member names are cited only to the extent needed to describe the mechanism.

**Confidence convention.**
- **belegt** = read directly out of the reverse-engineered `Amazon Photos.dll` (member/branch),
  the German strings file, or our committed code.
- *inferred* = reasoned from adjacent evidence, not read off a line.
- **UNKNOWN** = could not be determined from the inspected sources.

---

## 1. Purpose & scope

This doc owns two things that in the original belong to one navigation target:

1. **Download / restore (tab `Screen.Restore`)** — the entry points ("Ordner" vs. "Alben"), the
   cloud folder picker and album picker, the local destination selection, the enumeration of the
   selected cloud subtree into `download.db`, the download worker (concurrency, pause, cancel,
   retry, clear-history), the per-file semantics (already-present, conflict handling, live photos,
   size/space checks, parallel parts) and the queue UI incl. its empty states.
2. **Cloud-library / file management** — what the assembly *can* do to cloud nodes (rename, move,
   trash, restore-from-trash, delete, patch metadata) versus what it actually *exposes to a user*.
   This distinction is the central finding of §3.12.

Not duplicated here: upload / one-time backup (`docs/SPEC_UPLOAD.md`,
`docs/SPEC_ONE_TIME_UPLOAD.md`); folder scan, filter and schedule persistence
(`docs/SPEC_FOLDERS.md`); duplicate identity (`docs/SPEC_DEDUP.md`); bidirectional sync
(`docs/SPEC_SYNC.md` — **cross-reference only**, that doc owns the sync engine); progress/ETA UI
(`docs/SPEC_UPLOAD_PROGRESS.md`); notifications (`docs/SPEC_NOTIFICATIONS.md`).

---

## 2. Reverse-engineering method (belegt)

- Tooling: `ilspycmd` (v11.1.0.9782) at `~/.dotnet/tools/ilspycmd`, run with `PATH` extended and
  `DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`. No tool was installed for this
  task. A full type listing plus a full decompilation were produced (`ilspycmd -l c`,
  `ilspycmd <dll>`); targeted classes were dumped individually. → **belegt**
- Target (read-only, **never launched**):
  `/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll` (managed assembly,
  4.6 MB) and `/Applications/Amazon Photos.app/Contents/Resources/de.lproj/Localizable.strings`.
  → **belegt**
- Classes used as evidence: `UI.DriveWindow.DriveWindowViewModel`, `UI.MenuViewModel`,
  `UI.DriveWindow.RestoreTab.{RestoreTabViewModel, RestoreItemsViewModel}`,
  `UI.DriveWindow.TabItemView.Restore.{RestoreTabItemViewController, RestoreTransferItemsViewController,
  RestoreTransferItemsTableView, ChooseRestoreTypeViewController}`,
  `UI.DriveWindow.PickerViews.{FolderPickerViewController, AlbumPickerViewController,
  CreateCloudFolderViewController, FolderListTableCell, NonMediaInfoViewController}`,
  `UI.AlbumPicker.{AlbumPickerViewModel, AlbumMetadataViewModel}`, `UI.AlbumMetadata`,
  `UI.{CloudDriveDownloadViewModel, CloudDrivePickerViewModel, DownloadItem,
  TransferItemFilterExtensions, StatusPopup.DownloadStateRowViewModel}`,
  `UI.FtueWindow.Pages.BulkDownloadPage`,
  `Services.ManualDownload.{ManualDownloaderWorker, ManualDownloadCommand,
  DownloadFilesConflictResolver, DownloadDestinationMapper, DownloadStatusEvent}`,
  `Database.ManualDownload.{ManualDownloadDatabase, DownloadRecord, ManualDownloadDatabaseEnumerator}`,
  `Database.CloudDrive.{Node, NodeParentRecord, NodeCoverRecord, CloudDriveDatabase}`,
  `Services.CloudDrive.{CloudDriveService, NodeService, ParallelDownloadManager, DownloadInfo}`,
  `UI.Commands.CancelManualDownloadsCommand`,
  `UI.IntentHandlers.{ClearProcessedDownloadsIntentHandler, RetryErroredDownloadsIntentHandler}`,
  `UI.Helpers.TransferEnumExtensions`, `Constants`. → **belegt**
- Every German label below was matched against the strings file, and every key was grepped in the
  full dump for a `Strings.<key>` call site to determine whether the managed code references it.
  → **belegt**

---

## 3. Mechanics, states, fields, persistence, UI texts

### 3.1 Screen model and navigation

The main window is driven by a four-value `Screen` enum: `Backup`, `Restore`, `Sync`,
`FolderPicker`. `DriveWindowViewModel.SetSelectedScreen(screen)` maps the value to a
`TabScreenViewModel`; `Screen.FolderPicker` is not a real screen — it switches `ActiveTab` back to
`Restore` and only sets `RestoreTabViewModel.ShowFolderPicker = true`. `Screen.Restore` is the
**default** selected screen. → **belegt**

`MenuViewModel` builds the left navigation from `SelectableMenuItems = { BackupItem, DownloadItem }`
— exactly two entries: `BackupItem` (`…_navigationMenu_backup`, "Datensicherung") → `Screen.Backup`,
and `DownloadItem` (`…_navigationMenu_download`, "Herunterladen") → `Screen.Restore`. `SettingsItem`
(`…_navigationMenu_setttings`) is a command, and `SyncItem` (`…_navigationMenu_sync`) exists only in
Sync app mode. The two further navigation keys `…_navigationMenu_home` ("Startseite") and
`…_navigationMenu_smartscan` ("Intelligenter Scan") are **defined in the strings file but have no
`Strings.` call site** → *inferred* dead strings from an older/other layout; there is no home
screen and no "smart scan" screen in this build. The navigation collapse state initialises from
`ICustomerSettings.ShouldCollapseNavigationMenu ?? …DefaultShouldCollapseNavigationMenu` (false)
and is written back on every toggle. → **belegt** / *inferred*

### 3.2 Restore tab states

`RestoreTabViewModel : TabScreenViewModel` holds the tab's whole state. `RestoreTabItemViewController`
stacks a `ChooseRestoreTypeViewController` and a `RestoreTransferItemsViewController` as full-size
children and shows one at a time, driven by a two-valued `DriveWindowTabState`:

| State | Meaning | Component shown |
|---|---|---|
| `EmptyState` | nothing queued and nothing running | `ChooseRestoreTypeViewController` |
| `MainTab` | items queued or a transfer in progress | `RestoreTransferItemsViewController` |

`UpdateTabState()` derives it from `TransferInProgress || StatusMessageViewModel.TotalItemCount != 0`.
`ForceEmptyState` **overrides** the derived value back to `EmptyState`; it is set by the
"Download more…" button and cleared by the choose-type view's cancel button. → **belegt**

`RestoreItemsViewModel : TransferItemsViewModel` supplies the rows: `ViewModelAssociatedWith =>
Downloader`, `ViewLinkText => …_statuswindow_scheduletab_revealinfinder` ("Im Finder anzeigen"),
each row a `DownloadItem : TransferItem`. Rows come from `_db.GetAll(filter, null, null, false)`
with `SelectedPage` / `TotalPages` over a page size of **1000**. Loading is suspended while the tab
is not selected (`HandleSelectedScreen(isSelected)` sets `TransferItemsLoadSuspended = !isSelected`)
and resumes with `LoadRestoreItems()` when it becomes visible. Six filters are offered
(All, Waiting, Processing, Completed, Canceled, Failed). The row table has **three columns**
(Name, Size, Status; Status at position 2). → **belegt**

### 3.3 Empty state: "choose restore type"

`ChooseRestoreTypeViewController` always renders a 20 pt header and one big 96×96 button
("Ordner herunterladen", icon `folder-large-*`), plus a cancel button hidden unless `ForceEmptyState`.
A **second** big button ("Album herunterladen(s)", icon `album-large-*`) is added only when
`AlbumDownloadsEnabled` — that getter is `_environment.OSVersion >= Constants.ElCapitan`; below it
the header is replaced by the folder-only message and the album button is never created. Both
buttons push the respective picker over the whole tab; `ShowFolderPicker` is also reachable from
outside the tab via `SetSelectedScreen(Screen.FolderPicker)`. → **belegt**

### 3.4 Cloud folder picker (single-select tree)

`FolderPickerViewController : BasePickerViewController<CloudDrivePickerViewModel>` uses generic
picker chrome (title, primary action, cancel) plus a header refresh capsule and a footer that can
carry "create new folder" and the non-media filter. For the **download** use it is configured with
`CanCreateNewFolders = false` and `AllowFilterNonMedia = true`; the upload use of the same class
sets the opposite. → **belegt**

`LoadAsync(selectedNodeId = null)`: with no argument → `Refresh()`, optionally
`RefreshShowNonMediaFilterCheckbox()`, then `GetRootBranch()`, whose first entry becomes the
selection (the root "Alle" node); with an argument → `GetNodeBranch(nodeId)`. Both set
`SelectedNodeBranch`, reload the table and select the matching row. → **belegt**

Selection is **single selection**: `ShouldSelectRow` assigns `ViewModel.SelectedNodeId = _data[row].Node.NodeId`
and reloads the branch for that node — selecting a row both selects and drills into the folder.
There is no multi-select for the folder path. Row rendering: the root node is labelled
`…_clouddrivepicker_rootfoldername` ("Alle"), a node whose `Name` is null gets
`…_clouddrivepicker_nonamefolder` ("-kein-Name-"), everything else its local name; rows show their
`Depth` and, once computed, their aggregated size. → **belegt**

Size aggregation: each `PathNode` carries a `Depth`; `NodeService.CalculatePathNodeSize` runs in the
background and fills a `FolderStats` via a recursive CTE over `node_parents`
(sum of `content_size`, count of `nodes.id` for `kind in ('FILE','ASSET')` and
`status = 'AVAILABLE'`). It is recomputed for the whole visible branch after the first successful
`GetChanges` round (`ResizePathNodes()`), so sizes can arrive after the list is on screen. → **belegt**

Network freshness: `Refresh()` is a **3-second-capped** call — it races
`ICloudDriveChangesService.GetLatestChanges()` against a 3 s delay, only when the app is neither
cold-booting nor offline; `Refreshing` gates the primary button and a spinner. The picker's title
and destination strings are set by the caller, not the picker. → **belegt**

The non-media checkbox is hidden unless `IAppSettings.ShowNonMediaFilterCheckbox` (default
**false**) is set **and** the account has non-media files (`UsageByTypesInfo.HasNonMediaFiles`),
the latter checked by a second 3 s race around `IAccountService.UpdateUsageInfo()`. Toggling writes
`ViewModel.OnlyNonMedia` and emits seen/toggle-on/toggle-off metrics. → **belegt**

"Create new folder" (upload path only): `CreateCloudFolderViewController` validates the name —
empty → `…errornoinput`; longer than **280** chars → `…errornametoolong`; containing any of
`Constants.InvalidCloudDriveFolderNameCharacters` → `…errorinvalidcharacters`; server conflict
(`TransferError.CloudDriveNodeConflict`) → `…errornameconflict`; else `…errorgeneric`. On success
the folder is created and immediately marked **not selected for sync**
(`ISelectiveSyncService.UpdateNode(folderNode, selected: false, ExcludedFolderReason.ManualUpload)`),
then the picker reloads on the new node id. → **belegt**

### 3.5 Album picker (multi-select, search)

`AlbumPickerViewController : BasePickerViewController<AlbumPickerViewModel>` renders a horizontal
collection of album tiles (cover, title, "(n Elemente)" count, creation date) with a search bar in
the header (`…_restore_albumsearch`). → **belegt**

Selection is **multi-select**: the collection view sets `AllowsMultipleSelection = true`, and
`AlbumPickerViewModel.SelectedAlbums` — an observable collection — is filled by the delegate's
`ItemsSelected` and drained by `ItemsDeselected` (each also flips the tile's `IsSelected`).
`StartAlbumDownload` maps the selection straight to node ids via
`SelectedAlbums.Select(amvm => amvm.AlbumMetadata.NodeId)`. → **belegt**

Data source: `LoadAlbums()` → `INodeService.GetAllAlbumMetadata()`, which reads the **local
`cloud.db`** (`ListNodes("VISUAL_COLLECTION")`) and per album builds `AlbumMetadata`
(node id, name, direct child count, covers, creation date). The child count is database-first
(`GetDirectChildCount`) unless the paginated-children treatment is active, in which case it comes
from the network (`GetDirectChildCountFromNetwork`). Covers come from
`ThumbnailWorker.GetAlbumCovers(nodeId)` and refresh asynchronously: an `AlbumCoversReadyEvent`
stream is buffered 3 s / 10 events and written back into visible rows. → **belegt**

Sorting is by `DateCreated` **ascending**. `ApplyFilter` keeps a row when the title starts with the
trimmed filter text under current-culture ignore-case compare, or when any filter word
fuzzy-matches a title word — fuzzy matching only applies to title words of at least 3 characters
with a maximum edit score of 2. The empty states distinguish "no albums in the account"
(`…_restore_noalbumsfromservice`) from "no hits for this search"
(`…_restore_noalbumsfromsearch` + `…_retry`). → **belegt**

### 3.6 Starting a download: destination and guards

Both start paths funnel through `GetDownloadLocation()`, a modal folder dialog
(`…_statuswindow_selectdownloadlocation`, "Download-Ziel auswählen") via
`IDialogService.ShowFolderSelectionDialog`. The underlying `NSOpenPanel` allows directories only,
allows creating directories, and is opened with **`multiselect: false`** — a download has exactly
one destination. → **belegt**

`StartFolderDownload(vm, destinationPath)`:
- if the app is in Sync app mode and the destination is inside the sync folder
  (`PathEx.IsPathChildOf(destinationPath, syncFolder.Path)`) the start is **refused** after a warning
  dialog (`…_statuswindow_downloadnotallowedinsyncfolder{header,body}`) explaining that this can
  create duplicates in the account.
- otherwise the `DownloadStart` metric is emitted (tagged with the non-media checkbox dimension and
  page "Downloads") and `IManualDownloadService.DownloadNodes([nodeId], path)` — or
  `DownloadNonMediaNodes([nodeId], path)` when `OnlyNonMedia` is set — runs on a thread-pool task
  whose continuation only logs failures. → **belegt**

`StartAlbumDownload(...)` is the same minus the non-media branch: one `DownloadNodes` over **all**
selected album node ids, plus the `AlbumDownloadStart` metric. The album path has **no** sync-folder
guard. → **belegt**

Local shaping: `GetLocalPath(localPath)` combines the chosen folder with the constant
`…_download_foldername` ("Amazon Photos-Downloads"); an empty input falls back to
`IFileSystem.DesktopPath`. A download of album "X" into `~/Downloads` therefore lands in
`~/Downloads/Amazon Photos-Downloads/<album name>/…`. Downloads never write loose into the chosen
folder. → **belegt**

### 3.7 Enumeration: walking the cloud subtree into `download.db`

`ManualDownloaderWorker.DownloadNodes(nodeIds, localPath, nonMediaOnly)` is the whole restore
pipeline — a local walk that materialises records, not one HTTP call:

1. `TryCreateBatch()` — if no batch is in progress: `LastBatchStartDate = now`, a new GUID
   `BatchId = "n"`, `LastBatchCanceled = false`.
2. `_enumeratingCount` 0→1 publishes `DownloadEnumeratingStartEvent`, 1→0 the End variant.
   (`IsEnumerating` is what the tab uses for "TransferInProgress".)
3. Per requested node id: `INodeService.TryGetNode(nodeId, out node)` then
   `AddNodes([node], pathWithSuffix, 0, nonMediaOnly, token)`. Missing nodes are skipped silently.
4. `AddNodes` buckets by `node.Kind`: `FILE` → `AddFiles`, `FOLDER` and `VISUAL_COLLECTION` →
   `AddFolders`; nothing else is walked.
5. `_event.Set()` wakes the run loop. → **belegt**

`AddFiles` skips a node when `nonMediaOnly` is set and the file name is not classified as non-media
(`SupportedMimeMapping.IsOtherType`). `AddFolders` writes a `DownloadRecord` for the folder itself,
then enumerates children — paginated (`GetChildrenPaginated(..., "AVAILABLE",
NodeSource.NetworkIfNotInDatabase, nextToken)` in a loop until the token is empty) or not
(`GetChildren`), per `EnableGetChildrenPaginated`. Before recursing it calls
`_conflictResolver.ResolveChildrenNameConflicts(children, localPath, token)`, i.e. local name
de-duplication happens **at enumeration time, on the cloud nodes, before any record is written**.
Enumeration is cancellable via a swapped `CancellationTokenSource` checked before every insert.
→ **belegt**

`DownloadRecord` (table `downloads`) mirrors one restored item:
`Id` (autoincrement PK), `NodeId`, `NodeKind` (`"FILE"`→`TargetType.File`, `"FOLDER"`→`Folder`,
`"VISUAL_COLLECTION"`→`Album`, else `Unknown`), `Path` (local target, indexed `IX_path`), `Size`
(from `ContentProperties.Size`), `Md5` (from `ContentProperties.MD5`), `CreatedDate` /
`ModifiedDate` (`ClientProperties` first, then the node's own), `Status` (`TransferStatus`, indexed
`IX_status`), `StatusChanged`, `Added` (also the sort date), `Retries` / `NextRetry` / `RetryReason`
(default 0 / null), `ErrorCode` (default `None`), `ParentPath` (indexed `IX_downloads_parent_path`),
`IsExpanded` / `IsVisible` (default true). `ScheduleId` and `ParentId` are **always null** — a
download belongs to no schedule; `FriendlyName` is the file name of `Path`. → **belegt**

`download.db` is a **per-customer** file (`ICustomerInfo.GetCustomerDatabaseFileName("download.db")`
under the app's database storage path), schema version **3**. Migration: 1→2 added
`next_retry` / `retry_reason`; 2→3 added `parent_path` / `is_expanded` / `is_visible`, the
parent-path index, and a backfill that walks rows in 1000-row batches and reports
`Downloads.MigratedRows` / `Downloads.MigratedTime`. → **belegt**

### 3.8 Transfer loop: concurrency, pause, cancel, retry, clear

`ManualDownloaderWorker : RunLoopWorker` polls an `AutoResetEvent` every
`PollInterval = ManualDownloadPollIntervalInSeconds` (**default 15 s**) and returns immediately when
`_db.GetRemainingFilesAndFoldersCount() == 0`. → **belegt**

Concurrency: `ParallelAsync.ForEach(_db, MaxDegreeOfParallelism = GetQueues(), DownloadNode, …)`.
`GetQueues()` resolves to `ICustomerSettings.DownloadQueues` clamped by
`CustomerSettingsMaxSupportedDownloadQueues` / `…Min…` (**8 / 1**) unless
`IgnoreCustomerSettingsDownloadQueuesSetting` is set, then
`min(AppSettings.DownloadQueues, customerSettings.DownloadQueues)`; the app default is
`DownloadQueues = 4`, cached 5 s. The DB is enumerated through a `ManualDownloadDatabaseEnumerator`
that pulls 250 rows at a time and flips each row to `Processing` as it is handed out. During a pass
the worker holds a `PreventSleep("Downloading from Amazon Drive")` assertion. → **belegt**

Global pause is honoured twice: `Pause()` stops the run loop, waits for `_transferring` to clear,
then `ResetNonFinalizedToWaiting()`; and inside the per-file work `DownloadNode` throws
`OperationCanceledException` outright when the aggregated worker state is not `Running`
(`_globalState != AggregatedWorkerState.Running`) — a pause cancels the in-flight file rather than
letting it finish. `Interrupt()` = pause + relaunch. → **belegt**

Status statements in `ManualDownloadDatabase` (`FinalizedTransferStatusStartValue = Completed`, so
Ignored/Skipped count as finalized): `ResetNonFinalizedToWaiting` → `status = Waiting` for
`status < Completed`; `CancelPendingDownload` → `status = Canceled` for Processing or Retrying;
`ClearProcessed` → `delete from downloads where status >= Completed`; `RetryErroredRecords` →
`status = Waiting, retries = retries + 1, error_code = 0` (and resumes/starts a batch);
`UpdateStatus` refuses to write `Processing` or `Waiting` directly (use `SetProcessing`).
→ **belegt** for each statement

Run end (`finally`): the completed count folds into `BusinessMetrics.DownloadComplete` tagged with
the batch id; when the remaining count is zero **and** nothing is enumerating, `BatchId = null` is
written and the conflict resolver's cache is cleared; then
`DownloadStoppedEvent{RemainingCount, ErrorOccured, Canceled}` is published. `OperationCanceledException`
in `DownloadNode` → `ResetToWaiting`; any other exception → `UpdateStatus(Errored/…)` derived by
`ex.ToTransferStatus()` / `ToTransferError()`, plus `BusinessMetrics.DownloadFailure` with
`{error}|{nodeId}|{batchId}`. A 30-minute timer reports `Downloads.QueueCount` / `QueueBytes` /
`QueueEmpty`. → **belegt**

The three queue-footer actions:
- **"Download abbrechen"** → `CancelManualDownloadsCommand`: publish
  `DownloadCanceledEvent{RemainingBytes, RemainingCount}`, call `IManualDownloadService.Cancel()`
  and ask the workers state machine to interrupt the worker with `CancelPendingDownload()`; on
  success `LastBatchCanceled = true` and `BatchId = null`.
- **"Neuversuch fehlgeschlagen"** → `RetryErroredDownloadsIntent` → `RetryErrored()`; enabled while
  any row is `Errored`.
- **"Verlauf löschen"** → `ClearProcessedDownloadsIntentHandler` confirms with
  `…_clearcompletedconfirmationheader` + the **download-specific** body
  (`…_downloads_clearcompletedconfirmationtextmessage`, "Dadurch wird die Liste der vergangenen
  Downloads gelöscht.") and a positive/cancel pair, then `_db.ClearProcessed()`. The upload tab uses
  the sibling `…_uploads_…` string. → **belegt**

### 3.9 Per-record download semantics

`ManualDownloadCommand.Execute(record, ct)` switches on `record.NodeKind`: `FOLDER` |
`VISUAL_COLLECTION` → `DownloadFolder`; `FILE` → `DownloadFile`; anything else →
`TransferStatus.Errored`. → **belegt**

**Folders / albums.** The local directory is created if absent, then stamped with creation /
modified / last-access times from the record (timestamp failures are logged and ignored). If the
directory already exists the record is **Ignored**. A path at or over 260 characters surfaces as
`PathTooLongException`; if the directory still does not exist after creation the record goes to
`Retrying`. → **belegt**

**Files**, in order:
1. `DoesExactFileAlreadyExistOnDisk` — the file must exist **and** have the same length **and** the
   same MD5; then the record is `Ignored` and nothing is transferred. This is the "already present"
   rule, and it makes the second conflict phase (§3.10) coherent.
2. If the file exists with any other content → `FileConflictException`.
3. `IAppSettings.DownloadDelayInSeconds` (default **0**) may insert a randomised delay — a
   test/back-off hook, not a user feature.
4. `INodeService.GetNode(record.NodeId)`; if the node has no `ChildAssetTypeInfo` and is a
   live-photo candidate (`image/heic` or `image/jpeg`) it is re-fetched from the network.
5. **Live photos**: if the node has a `LIVE_VIDEO` asset child, the video is downloaded first into a
   sibling file `<local name>.<extension>`; a missing child raises `AssetNodeMissingException`. Both
   files are then moved into place — video first, and if the still fails the video is deleted again.
6. Otherwise one `DownloadFileCore` → `ParallelDownloadManager.Download(node, workingFile,
   destinationFile, ct)`.
7. `TryMoveTempFileCore` performs the hand-off: if the destination appeared meanwhile the temp file
   is deleted and `FileConflictException` is thrown; otherwise the temp file is time-stamped from
   the record and moved onto the destination. "File already exists" IO errors (HResult
   `0x80070050` / `0x800700B7`, or `AmazonNSErrorException` code 516) go to the post-download
   conflict check below. → **belegt**

Everything is downloaded into a **temporary file** first (`DisposableTemporaryFile`), never directly
onto the destination. `DownloadInfo` returns the destination file plus the MD5 and CRC32 computed
while streaming. → **belegt**

### 3.10 "Already present" and local conflict resolution (two phases)

`DownloadFilesConflictResolver` has two distinct entry points — the part of the download semantics
that is easiest to get wrong, so it is spelled out. → **belegt** for both

**Phase 1 — enumeration time (`ResolveChildrenNameConflicts`).** For one destination directory the
resolver lists existing local files once, then for every **FILE** child of a cloud folder: if an
existing file matches `^<basename>.*<extension>$` **and** its MD5 equals the cloud node's MD5, the
cloud node's name is **replaced by that existing file's name** — the record then points at the
already-present file and §3.9 step 1 marks it `Ignored` (the MD5 of a candidate is computed once and
cached). Otherwise the node gets a counter-suffixed name `"<basename> (n)<extension>"`, where `n` is
carried per source name and the result path is re-checked against the filesystem until free. Folders
are not touched here. Every rename increments `HealthMetrics.Downloads.RenamedDownload`. → **belegt**

**Phase 2 — move time (`ResolveConflictAtMoveTime`).** Used only when the move onto the destination
fails with "file exists". The existing file's MD5 is computed; if it equals the downloaded MD5 the
method returns the empty string, the caller deletes the temp file and reports success — **the
existing file wins and the download is discarded**. Otherwise a new `"<basename> (n)<extension>"`
is produced from a per-path counter cache initialised by scanning the directory for the highest
existing `(n)` suffix (ordinal-ignore-case), the record's `Path` is rewritten to it, and the temp
file is moved there. → **belegt**

**Gate.** Phase 2 sits behind the remote experiment `EnablePostDownloadFileConflictCheck`, whose
treatment must be exactly `"T1"`; in any other treatment a move-time conflict throws
`FileConflictException` after cleaning up the temp file. Phase 1 is therefore the **unconditional**
behaviour and phase 2 is **treatment-dependent**. `ClearCache()` runs when a batch finishes.
→ **belegt** / *inferred* (which treatment an install receives is not determinable from the binary)

**Local path shaping.** `DownloadDestinationMapper.GetLocalPathForNode(destinationPath, node)`
returns the destination unchanged for the root node, rejects any kind other than FOLDER / FILE /
VISUAL_COLLECTION, and otherwise runs the node name through `PathEx.GetLocalName`, so cloud names
are sanitised into legal local names and every sanitisation is logged. → **belegt**

### 3.11 Size / space limits, parallel parts, throttling, progress

`ParallelDownloadManager` per file:
- `ValidateAvailableSpace(size, tempPath)` and `(size, destinationPath)` before writing — insufficient
  free space raises `OutOfStorageException`. This is the only size limit; there is **no** per-file
  cloud ceiling here. Both the node's `ContentProperties.Size` and the actual stream length are
  checked.
- Part decision: `parts = 1` for a short stream, otherwise
  `min(ParallelDownloadMaxParts (4), max(1, length / ParallelDownloadMinPartSizeInBytes (262 144 000)))`
  — up to 4 roughly-250 MB parts. Each part is a separate ranged `DownloadFile(nodeId, byteFrom, byteTo)`
  request written to `"<workingFile>.part<n>"` and then merged; the single-part path writes straight
  into the working file.
- The transfer request asks for a `byteFrom` of **0** when the node has a size — the manual download
  always starts at byte 0. There is no resume of an interrupted *file*; resume exists only at record
  level (§3.13) and in the multipart upload path.
- Bandwidth: a `ThrottledStream` wraps the response with
  `max(CustomerSettings.DownloadTransferKiloBytesPerSecond,
  AppSettings.NetworkThrottlingMaxDownloadTransferKiloBytesPerSecond) * 1024` bytes/s, cached 10 s;
  0 = unlimited, and that is what Preferences → Bandwidth writes.
- Progress uses the generic `FileTransferStartedEvent` / `…ProgressEvent` / `…CompletedEvent` triple
  (here `FileDownloadStartedEvent`, `FileDownloadProgressEvent`, `FileDownloadCompletedEvent`) feeding
  the status popup's `DownloadStateRowViewModel`: transferring icon `ImageType.DownloadBlue`, texts
  `…_statuspopup_downloading` / `…_statuspopup_downloadcomplete`, cold-boot text
  `…_statuswindow_statuswaiting`, and a single call-to-action `RetryErrored` →
  `RetryErroredDownloadsIntent`. → **belegt** for each item

### 3.12 Cloud library / file management: API capability vs. UI exposure

The client's *user-visible* file management is limited to the pickers of §3.4–3.5 (choose a cloud
folder, create one, choose albums) and the download / clear / cancel / retry actions of §3.8. There
is **no** in-app browser of the cloud library, no rename/move/copy UI, no favourites toggle, no
trash view, no restore-from-trash view and no share sheet. The strings file agrees: the only
album/favourite/trash/share/delete-related keys are three permission errors and the four
`…_restore_album*` keys (§4). → **belegt**

What the service layer *can* do to cloud nodes (`CloudDriveService`, `ICloudDriveStorageService` /
`IFilesSyncStorageService`):

| Capability | Request built by the assembly | Managed callers found |
|---|---|---|
| Read node metadata | `GET <metadata>/nodes/{id}` | `NodeService`, sync engine |
| Create folder | `POST <metadata>/nodes` (body `CloudDriveFolderMetadata`) | `CreateCloudFolderViewController`, `BackupPathMapper.GetOrCreateBackupDestination`, sync |
| Children | `…/children`, incl. `nodes/{id}/children` and `nodes/{toParentId}/children` | `NodeService`, picker |
| **Rename** | `PATCH <metadata>/nodes/{id}` with a `nodeChanges` JSON body (`NullValueHandling.Ignore`, metric `Request.PatchNode`) | `FilesSyncStorageService.RenameNode` |
| **Move** | same `PATCH nodes/{id}` (parents change), or the `…/children` variant | `FilesSyncStorageService.MoveNode`, move/rename conflict rules |
| **Trash** | `PUT <metadata>/trash/{id}` (optionally `?recurse=true`), metric `Request.TrashNode` | `FilesSyncStorageService.TrashNode` (sync engine) |
| **Restore from trash** | `POST <metadata>/trash/{id}/restore` | **none in this assembly** — only interface declaration, implementation and retry wrapper |
| **Delete** | `DELETE <metadata>/nodes/{id}` (optionally `?recurse=true`) | purge rules; never the restore tab |
| Replace content | `PUT <content>/nodes/{id}/content` (+ `Content-Range` on resume) | upload path |
| Download content | `GET <content>/nodes/{id}/content` (ranged via `byteFrom`/`byteTo`) | `ParallelDownloadManager` |

The `trash/{id}` call is a **PUT** and `trash/{id}/restore` is a **POST**; both were read off the
request construction inside `CloudDriveService` (the async state machines for `TrashNode` /
`RestoreTrashedNode`). `?recurse=true` is appended by both `DeleteNode` and `TrashNode` when their
`recurse` argument is set (`TrashNode(nodeId, recurse: true)` is the storage-interface default).
→ **belegt**

Consequences:
- `RestoreTrashedNode` is *inferred* **dead** in this build: declared on `ICloudDriveStorageService`,
  implemented in `CloudDriveService`, wrapped in `RetriableCloudDriveService` — but no code path
  calls it (absence of a caller is **belegt**). "Papierkorb/Restore" of cloud nodes is **not** a
  desktop-client feature.
- `TrashNode` is reached only from the sync engine (a local delete mirroring a cloud trash,
  `MoveLocalFileToTrashOperation` / `TrashCloudNodeRule` / the "local delete + cloud trash"
  conflict rules) plus the mass-delete notifications — never from the restore/download tab.
  `PatchNode` likewise only from sync rename/move logic.
- Permission failures for exactly these capabilities are modelled:
  `INSUFFICIENT_PERMISSION_FOR_{MOVE, PURGE, SHARE, TRASH_RESTORE, UPDATE_CONTENT, UPDATE_METADATA,
  DOWNLOAD, ADD_CHILD}` → `TransferError.InsufficientPermissions…` (196, 199, 201, 3088, 3089, 200,
  3090), all of which `ToSyncStatus()` turns into `SyncStatus.Errored` under the short text
  "Berechtigungen verweigert". That the client *knows* about these permissions is evidence the
  **server** enforces them for this account class — not that the desktop app offers the actions.
  → **belegt** / *inferred* (interpretation)
- Albums are a node **kind** (`VISUAL_COLLECTION`) mapped to `TargetType.Album` (a flag set:
  Unknown/File/Folder/Album). The only album operations the client performs are read and download.
- Favourites: **no** `favorite*` key in `Localizable.strings` and no favourites code in the managed
  assembly; the web/search API our port already models exposes a `favorite` aggregation bucket, so
  favourites are a *cloud* concept nothing in the desktop client writes or reads. → *inferred*
- Sharing: only the `…_errorlong_InsufficientPermissions_ForShare` string. The class `SharedFileList`
  in the assembly is macOS Launch-Services login/favourites-items interop, unrelated to sharing cloud
  files. → **belegt**

### 3.13 `cloud.db` node cache, `/drive/v1/nodes`, `/drive/v1/changes`

The cloud tree the pickers and the enumeration walk over is a **local SQLite cache**
(`ICloudDriveDatabase`, per-customer `cloud.db`) with `nodes`, `node_parents`, `node_image`,
`node_video`, `node_covers`, `SettingRecord`. Two indexes matter:
`IX_md5 on nodes(content_md5)` and `ix_node_by_name_and_parent on nodes(id, name, status)`.
`NodeSource` (`Network` / `Database` / `NetworkIfNotInDatabase`) decides whether a read is served
from the cache first. → **belegt**

- Album cover backfill: `BackfillNodeCoversTask` finds
  `select * from nodes left join node_covers on node_covers.node_id = nodes.id where nodes.kind = 'VISUAL_COLLECTION' and node_covers.node_id is null`
  — albums are discovered **in the local cache** and missing covers are fetched afterwards;
  `AlbumCoversReadyEvent` then updates the visible tiles.
- Folder stats and the "has non-media files" question are answered from the cache
  (`content_type not like 'image/%' and content_type not like 'video/%'`); the picker's non-media
  *flag* instead comes from the account usage API.
- Freshness is one worker: `CloudDriveGetChangesWorker` `POST`s to `changes` with
  `{checkpoint, limit, chunkSize, maxNodes, includePurged}` (`limit` defaults to
  `CloudDriveServiceGetChangesLimitSize`), retries `CloudDriveServiceRetries` times with a linear
  back-off from `CloudDriveServiceMaxBackOffInSeconds`, keeps bounded last checkpoints
  (`CloudDriveGetChangesCheckpointMaxSizeInBytes`) and exposes `ChangesFound` / `NodesChanged` /
  `GetChangesStuck`. The picker refresh is exactly `GetLatestChanges()` raced against 3 s (§3.4), and
  its first completion triggers `ResizePathNodes()`.
- Node metadata is read through `CloudDriveService` (`GetNode`, `GetChildren`,
  `GetChildrenPaginated`, `GetNodeBranch`, `GetAllAlbumMetadata`); folder creation is
  `POST nodes`. Every call carries `x-client-id: AmazonPhotosDesktop` and a Minerva metric with an
  HTTP status dimension. → **belegt**

### 3.14 Restart / resume semantics

Records live in `download.db`, so the queue survives a restart. On worker `Start()`, `Interrupt()`
and `Pause()` the worker runs `ResetNonFinalizedToWaiting()`, turning everything below `Completed`
back into `Waiting` — including a partially transferred file. Because the file transfer itself
always starts at byte 0, restarting re-downloads an interrupted file from scratch into a fresh
temporary file. **Restart resumption exists at the queue level, not the file level.** The batch is
also durable (`BatchId`, `BatchInProgress`, `LastBatchStartDate`, `LastBatchCanceled` are settings
rows in the same DB file), so a restart mid-batch keeps the batch identity, and `LastBatchCanceled`
drives the `…_statuswindow_transferingmessage_canceled` summary and the
`TransferStatusMessageIcon.Canceled` icon. → **belegt**

### 3.15 Our `docs/PROTOCOL.md` records only what the upload flows exercised

`docs/PROTOCOL.md` documents **only `POST` and `GET`** requests, because the trace behind it was
taken exclusively around upload paths (sign-in, `/drive/v1/account/source`, `/auth/token`,
`/v2/upload`, plus the incidental `/drive/v1/changes` and `/drive/v1/nodes` calls). That is a
**limit of the recording, not the behaviour of the app**: the assembly demonstrably also issues
`PUT`, `PATCH` and `DELETE` (§3.12). Anything in `PROTOCOL.md` that reads like a complete method
inventory must be read together with this section. → **belegt**

---

## 4. String evidence (keys → German)

Keys cited in this doc, verbatim from `de.lproj/Localizable.strings`. The right-hand column says
whether the managed code references the key (grep for `Strings.<key>` in the full dump). → **belegt**

| Key (… = `clouddrive_desktop_`) | German | Referenced? |
|---|---|---|
| `…_navigationMenu_download` | "Herunterladen" | yes (`MenuViewModel`) |
| `…_navigationMenu_backup` | "Datensicherung" | yes |
| `…_navigationMenu_setttings` | "Einstellungen" | yes |
| `…_navigationMenu_sync` | "Synchronisieren" | yes |
| `…_navigationMenu_home` / `…_navigationMenu_smartscan` | "Startseite" / "Intelligenter Scan" | **no** |
| `…_contextmenu_opendownloadstab` | "Herunterladen" | yes |
| `…_contextmenu_openbackuptab` / `…_openuploadstab` / `…_clouddrivelink` | "Datensicherung" / "Uploads" / "Meine Amazon Photos" | **no** |
| `…_contextmenu_opensynctab` / `…_photoslink` | "Synchronisieren" / "Ihre Fotos" | yes |
| `…_restore_header` | "Wählen Sie Ordner oder Alben zum Herunterladen ohne Synchronisierung aus." | yes |
| `…_restore_downloadfolder` | "Ordner herunterladen" | yes |
| `…_restore_downloadalbum` | "Album herunterladen(s)" | yes |
| `…_restore_selectalbum` | "Album (Alben) zum Herunterladen auswählen" | yes (picker title) |
| `…_restore_albumsearch` | "Nach Album suchen" | yes |
| `…_restore_albumcountmultiformat` / `…singleformat` | "({0} Elemente)" / "({0} Element)" | yes |
| `…_restore_noalbumsfromservice` | "Wir konnten keine Alben in Ihrem Konto finden." | yes |
| `…_restore_noalbumsfromsearch` | "Es konnten keine Ergebnisse für „{0}“ gefunden werden" | yes |
| `…_restore_noalbumsfromsearch_retry` | "Versuchen Sie es mit einem allgemeineren Suchbegriff." | yes |
| `…_clouddrivepicker_download_selectfoldermessage` | "Wählen Sie Ordner zum Herunterladen ohne Synchronisierung aus." | yes (album-less OS header) |
| `…_clouddrivepicker_downloadbutton` | "Herunterladen nach..." | yes (primary action, both pickers) |
| `…_clouddrivepicker_rootfoldername` | "Alle" | yes (root row) |
| `…_clouddrivepicker_nonamefolder` | "-kein-Name-" | yes |
| `…_clouddrivepicker_refreshbutton` | "Ordner aktualisieren" | yes |
| `…_clouddrivepicker_calculating` | "Berechnung läuft ..." | yes |
| `…_clouddrivepicker_selectbutton` / `…cancelbutton` / `…selectlocationmessage` | "Auswählen" / "Abbrechen" / "Speicherort für Ihre Dateien auswählen" | yes |
| `…_clouddrivepicker_items_count` / `…nodesizeformat` | "{0} Elemente" / "({0}, {1})" | yes (row count / row size) |
| `…_clouddrivepicker_onlynonmedia` | "Nur Dateien herunterladen, die keine Fotos und keine Videos sind" | yes |
| `…_clouddrivepicker_onlynonmedia_info` | "Um Dateien ohne Medien herunterzuladen, muss mindestens ein Ordner ausgewählt werden. …" | yes (info popover) |
| `…_clouddrivepicker_newfolderbutton` | "Neuen Ordner erstellen" | yes |
| `…_clouddrivepicker_newfolderdialog_{title,message,createbutton}` | "Ordner erstellen" / "Ordnername eingeben" / "Erstellen" | yes |
| `…_clouddrivepicker_newfolderdialog_errornoinput` | "Bitte geben Sie einen Ordnernamen ein" | yes |
| `…_clouddrivepicker_newfolderdialog_errornametoolong` | "Ordnernamen dürfen bis zu 280 Zeichen enthalten. Bitte versuchen Sie es erneut." | yes |
| `…_clouddrivepicker_newfolderdialog_errorinvalidcharacters` | "Ordnernamen können keine Sonderzeichen enthalten." | yes |
| `…_clouddrivepicker_newfolderdialog_errornameconflict` | "Ein Ordner mit diesem Namen ist bereits vorhanden." | yes |
| `…_clouddrivepicker_newfolderdialog_errorgeneric` | "Es ist ein Fehler aufgetreten. Bitte versuchen Sie es erneut." | yes |
| `…_clouddrivepicker_uploadbutton` / `…one_item` | "Hochladen" / "{0} Element" | **no** |
| `…_clouddrivepicker_nonetwork` / `…refresh_nonetwork_{title,body}` | "Keine Internetverbindung. …" | **no** (offline copy unused) |
| `…_statuswindow_emptystate_download_folderdownloadheader` | "Ordner zum Herunterladen auswählen" | yes (folder picker title) |
| `…_statuswindow_emptystate_download_folderdownloadbutton` | "Ordner auswählen..." | **no** |
| `…_statuswindow_emptystate_downloadbuttontitle` | "Herunterladen..." | **no** |
| `…_statuswindow_emptystate_downloadmessage[_nosize]` | "Alle Ihre Amazon Photos herunterladen ({0})" / "… ohne Größe" | **no** |
| `…_statuswindow_selectdownloadlocation` | "Download-Ziel auswählen" | yes (`GetDownloadLocation`) |
| `…_statuswindow_downloads_destinationpicker` | "Wählen" | **no** |
| `…_statuswindow_downloads_clearcompletedconfirmationtextmessage` | "Dadurch wird die Liste der vergangenen Downloads gelöscht." | yes (download clear-history) |
| `…_statuswindow_clearcompletedconfirmationheader` / `…clearcompletedbuttontitle` | "Verlauf löschen?" / "Verlauf löschen" | yes |
| `…_statuswindow_retryfailedbuttontitle` | "Neuversuch fehlgeschlagen" | yes |
| `…_statuswindow_canceldownloadbuttontitle` | "Download abbrechen" | yes |
| `…_statuswindow_download_morefolders_buttontitle` | "Herunterladen..." | yes |
| `…_statuswindow_downloadnotallowedinsyncfolderheader` | "Bitte wählen Sie einen anderen Ordner aus." | yes |
| `…_statuswindow_downloadnotallowedinsyncfolderbody` | "Das Herunterladen von Inhalten in deinen Synchronisierungsordner kann dazu führen, dass Duplikate …" | yes |
| `…_statuswindow_scheduletab_revealinfinder` | "Im Finder anzeigen" | yes (row link text) |
| `…_statuswindow_transferingmessage_download` | "{0} von {1} Element heruntergeladen ({2} verbleibend" | yes (singular summary) |
| `…_statuswindow_transferingmessage_download_plural` | "{0} von {1} Elementen heruntergeladen ({2} verbleibend" | yes |
| `…_statuswindow_transferingmessage_download_completed` | "Download abgeschlossen" | yes |
| `…_statuswindow_transferingmessage_canceled` | "{0} von {1} Elementen storniert" | yes |
| `…_statuswindow_transferingmessage_failed` | "{0} fehlgeschlagen" | yes |
| `…_statuswindow_statuswaiting_download_tooltip` | (tooltip variant of "Warten") | yes |
| `…_statuswindow_calculating` | (calculating summary) | yes |
| `…_statuspopup_downloading` | "Wird heruntergeladen" | yes |
| `…_statuspopup_downloadcomplete` | "Download abgeschlossen" | yes |
| `…_statuspopup_downloadcomplete_witherrors` | "Download mit Fehlern abgeschlossen" | **no** |
| `…_upload_statuswindow_statusdownloading` | "Wird heruntergeladen" | yes (row status + filter label) |
| `…_download_statuswindow_statusignored` | "Zuvor heruntergeladen" | yes (download status map) |
| `…_download_statuswindow_statusskipped` | "Verworfen" | yes (download status map) |
| `…_statuswindow_status{waiting,processing,retrying,completed,canceled,failed,completing,warning}` | "Warten" / "In Bearbeitung" / "Wiederholen" / "Abgeschlossen" / "Abgebrochen" / "Fehlgeschlagen" / "Wird überprüft" / "Achtung" | yes (status maps) |
| `…_displayitememptymessage_download_all` | "Keine Dateien heruntergeladen. (Laden Sie neue Inhalte herunter!)" | yes (empty message, filter All) |
| `…_displayitememptymessage_download_waiting` | "Alle Dateien heruntergeladen. (Keine Sorge!)" | yes |
| `…_displayitememptymessage_download_processing` | "Alle Dateien heruntergeladen. (Es ist alles in Ordnung!)" | yes |
| `…_displayitememptymessage_download_completed` | "Keine Dateien heruntergeladen." | yes |
| `…_displayitememptymessage_download_cancelled` | "Keine abgebrochenen Dateien. (Das ist eine Erleichterung!)" | yes |
| `…_displayitememptymessage_download_failed` | "Keine fehlgeschlagenen Downloads. (Puh!)" | yes |
| `…_download_foldername` | "Amazon Photos-Downloads" | yes (local sub-folder) |
| `…_statuswindow_downloaderrorlong_local_filenotfound` | "Die Datei konnte nicht heruntergeladen werden, da das lokale Ordnerziel nicht gefunden werden konnte." | yes |
| `…_statuswindow_downloaderrorlong_local_foldernotfound` | "Der Ordner konnte nicht heruntergeladen werden, da das lokale Ordnerziel nicht gefunden werden konnte." | yes |
| `…_statuswindow_errorlong_InsufficientPermissions_For{Move,Share,TrashRestore}` | permission errors for cloud actions | yes (error-string maps) |
| `…_statuswindow_errorshort_InsufficientPermissions` | "Berechtigungen verweigert" | yes |
| `…_statuswindow_errorlong_cloud_nodenotfound` | (node not found) | yes |
| `…_statuswindow_errorlongoutofstorage` | "Download kann nicht abgeschlossen werden. Auf diesem Computer ist kein ausreichender Speicherplatz verfügbar." | yes |
| `…_preferences_bandwidth_downloads_title` / `…_download_rate_title` | "Downloads:" / "Download-Rate pro Datei" | yes |
| `…_ftuewindow_bulkdownload_header` / `…_download_folders` | "Deine Amazon Photos-Ordner herunterladen" / "Ordner herunterladen" | yes (FTUE page) |
| `…_ftuewindow_bulkdownload_{continue_to_upload,learn_more_about_amazon_drive}` / `…_welcomepage_bulkdownload` / `…_quicktour_backupandrestore` | "Mit dem Upload fortfahren" / "Erfahre mehr über das Herunterladen von Fotoordnern" / "Registerkarten für Datensicherung und Download" | yes |

The "**no**" rows are defined but not referenced by the managed assembly — *inferred* dead, or
rendered by a nib / alternate layout. The dead `…_emptystate_downloadmessage[_nosize]` pair is
notable: the assembly has **no** "download my whole library with one button" code path; only the
FTUE page points at the folder picker. → **belegt** (unreferenced) / *inferred* (dead)

There are **no** keys containing `favorite`, `trash`, `restore` (other than the permission error and
the restore-tab names), `share` (other than the permission error), `delete` (other than
notification mass-delete) or `copy`. The `album_*` / `trash*` / `restore*` / `share*` / `favorite*`
material exists almost entirely on the **server/API** side, not as desktop UI strings. → **belegt**

---

## 5. Gap analysis vs. our port (committed state)

Our repository state was read from `src/` (grep + file reads) on `main`; nothing here is a TODO.
**Downloads / restore / cloud file management are deliberately out of scope for this port** — this
is an uploader (user decision 2026-10-13: MVP = login, upload-target creation, upload flow, settings
dialog; the download flow is relevant but explicitly not part of the MVP). The table records the
distance; the scope call itself is ours, not a gap to close.

| Capability | Original | Our port | File reference |
|---|---|---|---|
| Download / restore tab | full `Screen.Restore` tab, two entry points | **none** — `Page` has only `Backup` and `Settings` | `src/app.rs` (`enum Page`) |
| Download entry UI | 2 big buttons + header, OS-dependent album button | **none** | `src/app.rs` (`sidebar`, `nav_item`) |
| Download placeholder notice | — | "Downloads" heading in settings: "not implemented yet" (de+en) | `src/settings.rs`, `src/i18n.rs` |
| Cloud folder picker | single-select tree, refresh, sizes, root "Alle" | **none** — no cloud tree at all | — |
| Cloud folder creation | validation + sync-deselect | **none** | — |
| Album picker | multi-select grid, fuzzy search, covers, count | **none** | — |
| Album data source | `cloud.db` `VISUAL_COLLECTION` + covers + child count | album API used only to *find or create* the upload target | `src/prime_photos.rs` (`get_album`, `create_album`, `list_albums`) |
| Album node model | `kind = VISUAL_COLLECTION` → `TargetType.Album` | we already POST `kind: "VISUAL_COLLECTION"` when creating | `src/prime_photos.rs` (`create_album`) |
| Destination selection | modal folder dialog, single, `download_foldername` sub-folder | **none** — uploads go to the account root | `src/prime_photos.rs` |
| Sync-folder guard | refuses a destination inside the sync folder | not applicable (no sync, no download) | — |
| Download queue persistence | `download.db`, `downloads`, batch id, retries | **none**; our only DB is the MD5 dedup index | `src/dedup.rs` |
| Download concurrency | `DownloadQueues` default 4, clamp 1..8, cached 5 s | **upload-only** slider 1..8, default 3 | `src/config.rs`, `src/upload.rs` |
| Download bandwidth cap | per-file throttled stream, 0 = off | upload-only kbps cap (per-file, after upload) | `src/config.rs`, `src/upload.rs` |
| Pause / cancel / retry / clear-history | all four, confirm dialog for clear | uploads have pause + per-file conflict dialog; no clear-history | `src/config.rs` (`paused`), `src/upload.rs` (`resolve_conflict`) |
| "Already present" rule | same length **and** same MD5 → `Ignored` | equivalent on the upload side (path + MD5 → skip) | `src/dedup.rs`, `src/upload.rs` |
| Local name conflict | counter `name (n).ext` + an MD5-equal reuse path | cloud-side `conflictResolution` — `NONE` (`Ask`/`Ignore`/`Replace`) or `RENAME` (`SaveAsNew`); `OVERWRITE` is never sent (our host answers it with 404), `Replace` overwrites the node the 409 names via `PUT /v2/upload/<nodeId>` | `src/folders.rs`, `src/urls.rs`, `src/prime_photos.rs` |
| Temp file + move | download to temp, then move with timestamp copy | upload streams directly; no local write path | `src/upload.rs` |
| Live photos | `LIVE_VIDEO` asset child as sibling file | **none** | — |
| Multi-part download | up to 4 parts × ~250 MB, ranged requests | **none** | — |
| Free-space pre-check | before temp file and before destination | **none** (we only write config / folders.toml) | — |
| Progress / ETA | per-file + aggregate, cold-boot aware | upload progress bar + ETA, aggregated over runs | `src/upload.rs` (`upload_progress_text`, `eta_suffix`) |
| Status popup row | `DownloadStateRowViewModel` (`ImageType.DownloadBlue`) | tray exists; no status-popup detail | `src/tray.rs`, `src/notify.rs` |
| Empty-state messages | six `displayitememptymessage_download_*` | **none** | — |
| Row "reveal in Finder" | `…_scheduletab_revealinfinder` → `ViewOnFileSystemIntent` | **none** (nothing local to reveal) | — |
| Rename / move cloud node | `PATCH nodes/{id}` with `nodeChanges` | **none** | — |
| Trash cloud node | `PUT trash/{id}[?recurse=true]` | **none** | — |
| Restore from trash | `POST trash/{id}/restore` — implemented, **never called** | **none** | — |
| Delete node | `DELETE nodes/{id}[?recurse=true]` | **none** | — |
| Share cloud files | only a permission error string | **none** | — |
| Favourites | not present in the desktop client at all | `favorite` exists only as a search-aggregation DTO | `src/prime_photos.rs` (`ResponseAggregations`) |
| `cloud.db` node cache | full local mirror + `changes` checkpoint worker | **none** — stateless HTTP client | `src/prime_photos.rs` |
| i18n | `Localizable.strings` | central de/en table | `src/i18n.rs` |
| `PROTOCOL.md` method coverage | PUT/PATCH/DELETE exist in the app (§3.12) | our `PROTOCOL.md` documents **only POST and GET** — the trace only covered upload flows | `docs/PROTOCOL.md` |

---

## 6. UNKNOWN

- **Which remote treatments an install receives.** `EnablePostDownloadFileConflictCheck`,
  `EnableGetChildrenPaginated` and `EnableCXUploadSimplification` are weblab-gated; the binary shows
  the branch, never the assignment. Whether phase-2 conflict resolution and paginated children are
  live for a real user is **UNKNOWN**.
- **What `…_download_foldername` collides with.** The sub-folder is always appended and its
  timestamps are taken from the record. Whether a pre-existing "Amazon Photos-Downloads" with
  foreign content is reused or rejected is **UNKNOWN**.
- **Free space on network volumes / external drives** — checked for temp and destination, but
  behaviour when the destination volume disappears mid-run is **UNKNOWN**.
- **Multi-part tuning** — the effective part count depends on
  `ParallelDownloadMinPartSizeInBytes` / `…MaxParts` (defaults 262 144 000 / 4, read from the
  settings store); whether a customer-settings override exists for them is **UNKNOWN**.
- **"Copy" semantics.** No copy/move-download action exists in the client; whether the server
  supports a server-side copy for a download-into-the-same-account flow is **UNKNOWN**.
- **Favourites and sharing.** Nothing in the desktop client reads or writes them; the exact server
  capabilities behind `INSUFFICIENT_PERMISSION_FOR_SHARE` / `…_FOR_TRASH_RESTORE` are **UNKNOWN**
  from this binary alone.
- **Whether `RestoreTrashedNode` is reachable from outside the assembly** (native callback,
  reflection). No managed caller exists → *inferred* dead, **UNKNOWN** in the strict sense.
- **Whether the album picker ever loads non-`VISUAL_COLLECTION` nodes** — the query is kind-filtered
  in the cache, so *inferred* no, but the server-side definition of an album is **UNKNOWN**.
- **Album download into an existing album folder.** Phase 1 reuse is by MD5 of *files* inside a
  directory; albums are folders, so same-named albums are disambiguated by the counter suffix.
  Whether a same-named album is meant to merge is **UNKNOWN**.
- **The unreferenced picker strings and tray keys** read as dead, but their actual render site
  (nib vs. other bundle) is **UNKNOWN**.
- **Whether the download tab is offered when the account has only non-media files**, and how
  `OnlyNonMedia` interacts with album downloads (the album start path passes no non-media flag) —
  **UNKNOWN**.
- **Cold-boot gating of the picker refresh** — `Refresh()` is skipped while cold-booting; whether
  the picker then shows a stale tree or the (unreferenced) offline copy is **UNKNOWN**.

---

## 7. Cross-references

- `docs/UI_FLOWS.md` §3 (main-window layout and nav menu), §5 (Download tab `Screen.Restore`:
  purpose, entry points, controls), §8.2 (FTUE bulk-download page), §12 (dialogs).
- `docs/UI_STATES.md` §1 (three-layer state model), §3.2 (status vocabulary), §4.3 (bandwidth &
  concurrency, incl. the symmetric 8/1 bounds and the `DownloadMaxQueues = 4` device default),
  §5.1 (status popup rows).
- `docs/SPEC_UPLOAD.md`, `docs/SPEC_ONE_TIME_UPLOAD.md` — upload and one-time queue; the shared
  `TransferDatabase` / worker / status-row patterns are the mirror image of the ones here.
- `docs/SPEC_SYNC.md` — owns the sync engine, the only managed caller of `TrashNode`, `PatchNode`
  (rename/move) and the purge/delete rules, plus the mass-delete notifications. Cross-reference
  only; nothing here re-specifies sync.
- `docs/SPEC_FOLDERS.md` — folder scan/filter, schedule persistence, destination resolution.
- `docs/SPEC_DEDUP.md` — MD5 identity; the original's download "already present" rule (§3.9 step 1)
  is the same length+MD5 idea applied on the way *down*.
- `docs/SPEC_BACKGROUND_SYNC.md` — cadence/watchers; the download worker is a separate poll loop
  (`ManualDownloadPollIntervalInSeconds`) and not part of the scheduler.
- `docs/PROTOCOL.md` — read together with §3.15 (POST/GET only; PUT/PATCH/DELETE exist in the app).
- `docs/openapi.yaml` — the device/desktop REST surface incl. `/drive/v1/changes`.
- Our code: `src/app.rs` (`enum Page`, `sidebar`, `nav_item`), `src/settings.rs`
  (`bandwidth_settings`, download placeholder), `src/i18n.rs`, `src/prime_photos.rs`
  (`get_album`, `create_album`, `list_albums`, `search`, `ResponseAggregations`), `src/dedup.rs`,
  `src/folders.rs`, `src/config.rs`, `src/upload.rs`, `src/tray.rs`, `src/notify.rs`.

