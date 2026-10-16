# Drag & Drop / Einmalige Sicherung — Clean-Room Spec (reverse-engineered)

Behavioural specification of the **original** Amazon Photos desktop client's drag-and-drop
upload path and its **one-time backup** queue, plus the mapping onto this port. The original
behaviour below is **reverse-engineered from the shipped managed assembly**, not inferred from
string tables alone.

This document is **clean-room**: it records *observed behaviour* in our own words and tables.
It contains **no verbatim decompiled code** and **no personal data**. German user-facing labels
are quoted from the shipped `de.lproj/Localizable.strings` (product strings, not code); type and
member names are cited only to the extent needed to describe the mechanism.

**Confidence convention.**
- **belegt** = read directly out of the reverse-engineered `Amazon Photos.dll` (member/branch),
  the German strings file, the screenshots, or our committed code.
- *inferred* = reasoned from adjacent evidence, not read off a line.
- **UNKNOWN** = could not be determined from the inspected sources.

---

## 1. Purpose & scope

This doc owns the **drag-and-drop → one-time (ad-hoc) upload** feature: the drop entry points,
the option chain that decides what happens, the one-time queue row ("Einmalige Sicherung"), its
persistence, and the overlay/scanning/notice surfaces.

Not duplicated here: folder scan/filter + persistence (`SPEC_FOLDERS.md`); duplicate
identity and dedup eligibility (`SPEC_DEDUP.md`); scheduled cadence/watchers/pause
(`SPEC_BACKGROUND_SYNC.md`); progress UI (`SPEC_UPLOAD_PROGRESS.md`); notifications /
menu-bar presence (`SPEC_NOTIFICATIONS.md`).

---

## 2. Reverse-engineering method (belegt)

- Tooling: `ilspycmd` **is installed** at `~/.dotnet/tools/ilspycmd` (v11.1.0.9782), run with
  `PATH` extended and `DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`. No tool was
  installed for this task. → **belegt**
- Target (read-only, never launched): `/Applications/Amazon Photos.app` →
  `Contents/MonoBundle/Amazon Photos.dll` (managed assembly) and
  `Contents/Resources/de.lproj/Localizable.strings`. A full type dump plus the targeted
  classes below were read. → **belegt**
- Classes used as evidence: `Services.DragDrop.{DragDropHandler, BaseOption,
  ChooseDestinationOption, DefaultFolderBackupOption, DragDropOptionProvider}`,
  `Config.DragDropModule`, `UI.DragDrop.{DragDropViewModel, StaticDragDropOptionViewModel}`,
  `UI.DriveWindow.{DriveMainView, DragDrop.DragDropView, DragDrop.DragDropOverlayView,
  DragDrop.DragDropScanningOverlay, DragDrop.StaticDragDropOptionView}`,
  `Services.Enumeration.EnumerationService`, `Services.ManualUpload.{ManualUploadContext,
  BackupUploaderWorker}`, `Database.ManualUpload.{AutoSaveRecord, UploadRecord}`,
  `UI.DriveWindow.DesktopBackup.{OneTimeBackupItem, OneTimeBackupItemStatusViewModel}`,
  `UI.DriveWindow.DesktopBackupLegacyViews.OneTimeBackupItemsViewModel`,
  `Services.Events.Models.FilesDroppedEvent`, `Constants`, `HealthMetrics`,
  `ScheduleCreatedEventHandler`. → **belegt**

---

## 3. What happens on drop — the handler chain (Q1)

### 3.1 Drop entry points

Two `NSView`s implement `INSDraggingDestination` and are the only places that call
`IDragDropOption.OnDrop(paths)` (grep: exactly two `.OnDrop(` call sites). → **belegt**

| View | Role | Accepts |
|---|---|---|
| `UI.DriveWindow.DragDrop.DragDropView` | the **CX** in-panel drop area (`ShowDragDropOverlayView` builds it inside an overlay `NSView` over the backups table) | **folders only** — every dropped path must have `TargetType != File`; otherwise a warning dialog with `…_schedules_warningmessage_only_drop_folders` ("Nur Ordner in diesem Bereich löschen") is shown and `OnDrop` is **not** called |
| `UI.DriveWindow.DragDrop.StaticDragDropOptionView` | the option tile inside the **legacy** full-window overlay | **files and folders** (no filter) |

Both register `NSFilenamesType`, read the pasteboard `public.file-url` list, and call
`<option>.OnDrop(paths)`. `DriveMainView` (the window content view) handles `DraggingEntered`
only: it refuses the drag unless the selected screen is `Backup` and no scanning overlay or
folder picker is open, shows the overlay, and counts paths into the
`HealthMetrics.DragDrop.PathsCount` / `BusinessMetrics.DragDrop` metrics; `DraggingEnded`
removes the overlay. `DriveMainView` has **no** `PerformDragOperation` — the overlay child is
the actual drop target. → **belegt**

### 3.2 Option chain

`BaseOption : IDragDropOption` is abstract and defines the contract. `OnDrop(paths)` calls the
abstract `DoOnDrop(paths)` and then publishes `FilesDroppedEvent(count, MetricTagName)`. It
**swallows** `TaskCanceledException` ("drop operation was cancelled") and any other exception
("drop operation had an error") — a rejected/cancelled drop is not a user-facing failure. It
also provides shared helpers: `EnumerateOneTimeBackup(paths, destinationNodeId)`,
`EnumerateSearchForMediaFiles(paths)`, `SetSelectedScreen(screen, source)`,
`ShowDragDropScanningOverlay()` / `RemoveDragDropScanningOverlay()`. → **belegt**

Two concrete options exist (both registered via `DragDropModule` as `IDragDropOption`):

| Option | `MetricTagName` | What `DoOnDrop` does | Wired to a view? |
|---|---|---|---|
| `ChooseDestinationOption` | `"Selected"` | CX branch → **creates a scheduled backup** per folder (see §3.4); legacy branch → scanning overlay + a cloud-folder picker, then one-time enumeration | **yes** — both `DragDropView` and `DragDropOverlayView` bind it |
| `DefaultFolderBackupOption` | `"Default"` | one-time enumeration straight to the default backup destination (`GetOrCreateBackupDestination(Constants.DefaultDragDropDestination)`) | **no** — exposed as `DragDropViewModel.DefaultFolderOption`, but no view binds it |

`DragDropViewModel` constructs a `StaticDragDropOptionViewModel` for each concrete option and
exposes them as `DefaultFolderOption` / `ChooseFolderOption`; the drop views use
`ChooseFolderOption`. `DefaultFolderOption` has no consumer in this build → *inferred* dead
(the binding evidence is **belegt**). → **belegt** / *inferred*

### 3.3 `DragDropHandler.Dispatch` is present but unused

`DragDropHandler : IDragDropHandler` resolves a destination (its own private
`DefaultDragDropDestination = ["Pictures","Desktop"]`) via
`IBackupPathMapper.GetOrCreateBackupDestination(...)` and then calls
`IEnumerationService.EnumerateOneTimeBackupUploadPaths(paths, id)`. It is DI-registered
(`RegisterType<DragDropHandler>().As<IDragDropHandler>().SingleInstance()`), **but no code in
the assembly calls `Dispatch`/uses `IDragDropHandler`** (grep finds only the definition, the
interface and the registration). → *inferred* dead entry point in this build (the absence of a
caller is **belegt**). The live path is `BaseOption.OnDrop`. → **belegt** / *inferred*

### 3.4 The CX vs. legacy split (important)

`ChooseDestinationOption.DoOnDrop` branches on `DriveWindowViewModel.EnableCXUploadSimplification`
(a feature-flag treatment):

- **CX path (modern): create a real scheduled backup per folder.** Each dropped path is
  validated by `IAllowedScheduleFolderStrategy.GetScheduleFolderWarning`; valid paths get
  `HealthMetrics.CXSimplification.BackupCreatedUsingDragDrop` and
  `IBackupScannerService.CreateAndScanBackup(path)`. `BackupScannerWorker.CreateAndScanBackup`
  builds a `ScheduleRecord` with `Interval = DefaultPostOnboardingIntervalOption`, saves it via
  `ISchedulesManager.SaveScheduleRecord`, and enqueues a `BackupScanRequest`. Invalid paths are
  collected and shown in a modal titled `…_schedules_warningmessage_folders_not_backedup`
  ("Einige Ordner wurden nicht gesichert"), listing up to 10 names. → **belegt**
- **Legacy path: the one-time queue.** Show the scanning overlay; run
  `EnumerateSearchForMediaFiles(paths)` (return early if it finds no media); switch to the
  Backup screen (`DriveWindowLaunchSource.FromDragAndDrop`); remove the scanning overlay; open
  the cloud node picker (`ICloudNodeProvider.PickNode()`, empty selection → `TaskCanceledException`
  "No node was selected from the picker."); then `EnumerateOneTimeBackup(paths, nodeId)`. → **belegt**

So the *one-time connection row* is the **legacy** drop semantics; under the CX layout a drop
produces **scheduled** backups instead. This doc specifies the legacy one-time behaviour (what
our port targets) and records the CX divergence.

---

## 4. Options and the default destination (Q2)

- The option selection is not made by the user with a separate dialog: the **view that receives
  the drop determines the option**. The wired targets both use `ChooseDestinationOption`
  ("Ordner auswählen" / choose-folder). A default-folder option exists (`DefaultFolderBackupOption`)
  but is unbound (§3.2). → **belegt** / *inferred*
- **Default destination resolution** is by cloud **node id**, not a literal path:
  `IBackupPathMapper.GetOrCreateBackupDestination(<segments>)`. Observed segment defaults:
  - `DefaultFolderBackupOption` → `Constants.DefaultDragDropDestination` = `["Amazon Photos Desktop"]`
    (and `Constants.DefaultFolderDragDrop = "Amazon Photos Desktop"`). → **belegt**
  - `DragDropHandler` (unused) → its own private `["Pictures","Desktop"]`. → **belegt**
  - Apple Photos one-time import (`EnumerationService.OneTimeMediaImport`) →
    `["<pictures folder>","<desktop folder>","<media-import folder>","yyyy-MM-dd"]`. → **belegt**
- The user-visible **"Cloud-Pfad: /All/{0}"** (`…_dragdrop_defaultfolder_path`) and the
  companion text `…_dragdrop_defaultfolder` ("Laden Sie Elemente in einen Cloud-Standardordner
  hoch.") are **defined in the strings file but not referenced by the managed code** (grep: no
  `Strings.…` call site). → the literal `/All/{0}` label is a UI presentation of "root folder
  `Alle`", not a code constant; its render site is **UNKNOWN** (may live in a nib/legacy layout).
  → **belegt** (unreferenced) / **UNKNOWN** (render site)

---

## 5. How the one-time backup is modelled (Q3)

### 5.1 Enumeration

`EnumerationService.EnumerateOneTimeBackupUploadPaths(paths, destinationNodeId = null)` is
**void (fire-and-forget)**: it records the business metric `ManualUploadStart` (dimension
"NetworkUnavailable" when offline) and calls the private `EnumeratePaths`. → **belegt**

`EnumeratePaths` establishes a **batch** on `IManualUploadDatabase`: `BatchInProgress`,
`BatchId` (new GUID), `LastBatchStartDate`, `LastBatchCanceled=false`. It increments
`_manualEnumeratingCount`; on 0→1 it publishes `UploadEnumeratingStartEvent` and resets
`ManualUploadsIdleEvent`. For each dropped path it resolves an optional photo library and then
walks the path with the filter **`ContentFilter.Everything.ForMediaOnly()`** — i.e. **photos and
videos only** — persisting upload records. On 1→0 it publishes `UploadEnumeratingEndEvent` and
signals `ManualUploadsIdleEvent`. Health metrics: `OneTimeBackups.FilesEnumerated`,
`OneTimeBackups.EnumerationEmpty`, `OneTimeBackups.EnumerationTime` and
`Performance.FileSystemWalkingSpeed`. → **belegt**

`EnumerateSearchForMediaFiles(paths)` walks the paths and returns `true` only if some walked
file matches `ContentFilter.PhotosAndVideo` (the legacy pre-check). → **belegt**

### 5.2 One row, one transfer state

`UI.DriveWindow.DesktopBackup.OneTimeBackupItem : IBackupItem` is the single row:
`Name => …_backup_onetimebackupitem_title` (fixed "Einmalige Sicherung"),
`ScheduleId => "ManualUpload"` (hard-coded), `IsScheduledBackup => false`. It wraps **one**
`IBackupTransferState` (the whole queue) and a `OneTimeBackupItemStatusViewModel`. → **belegt**

`OneTimeBackupItemStatusViewModel : BackupItemStatusViewModel` subscribes to
`IWorkersStateMachine.GlobalState` and re-derives on change; it requires the state to be a
`BackupTransferState` (otherwise throws "Incorrect transfer state type wired for one-time
backup item"). Its precedence: CompletedWithFailures/Failed → "{n} Fehler" (Error); paused
(global) → "ANGEHALTEN"; InProgress/Unknown + cold-boot → "Warten" (+10-min tooltip);
Completed → "Datensicherung abgeschlossen"; InProgress → "Upload-Vorgang läuft";
Preparing → "Konfiguration wird durchgeführt"; Unknown → empty. Only the **global** pause
applies (no per-backup flag). → **belegt**

The queue is rolled up as `AggregatedBackupTransferState.OneTimeBackups = 2` (with
`ScheduledAndOneTimeBackups = ScheduledBackups | OneTimeBackups`), which is what drives the
backup-screen's empty/list states. → **belegt**

### 5.3 Files, folders, count

- "How many items" is not fixed: the drop enumerates **recursively**; folders are walked, files
  are classified by the media-only filter. → **belegt**
- The started toast shows the number of **dropped top-level paths** (`paths.Count()`), not the
  number of files. → **belegt**
- The per-file list for the queue is a separate `OneTimeBackupItemsViewModel :
  TransferItemsViewModel` (legacy view) that filters every event/record by
  `ScheduleId == "ManualUpload"`; `GetRecords` calls
  `IManualUploadDatabase.GetAll(..., filterScheduleId:true, "ManualUpload",
  excludeDeferredUploads:true, excludeSkipped:true)`. It also exposes
  "show error window" (`ShowBackupErrorWindowIntent("ManualUpload")`) and
  "add skipped record back to queue" (`ResetSkippedUploadRecordToWaiting`). → **belegt**

### 5.4 Dedup

A one-time/manual upload is **`UploadType.Normal`** and is therefore **not** eligible for the
MD5 cross-account duplicate check: `EnumerationService.GetUploadRecord` starts at
`UploadType.Normal` and only upgrades to `PhotosImport` (photo library) or `Deduped`
(`record.DeduplicateOptions == PhotosAndVideos`, i.e. a *scheduled* dedupe backup); a one-time
drop has no `record`, so it stays `Normal`. The gate `UploadRecordExtensions.GetSuppressFileUpload`
returns `Deduplication` (suppress) whenever `UploadType` is neither `PhotosImport` nor `Deduped`.
`UploadType` = `Normal=0, PhotosImport=1, Deduped=2, Reupload=99`. → **belegt**

So the one-time queue uploads "as-is"; only name/parent identity can avoid a repeat. Our port
diverges deliberately here (§11.5).

---

## 6. Persistence and lifecycle (Q4)

- **Persisted, but not a schedule.** One-time records are stored in the shared upload DBs with
  `schedule_id = "ManualUpload"`; there is **no** `schedules` row and
  `OneTimeBackupItem.IsScheduledBackup` is `false`. `EnumerationService` writes upload records
  with `ScheduleId = record?.ScheduleId ?? "ManualUpload"`. → **belegt**
- **Record shape.** `Database.ManualUpload.AutoSaveRecord` (table `backup`) fields:
  `schedule_path`, `relative_path`, `node_id`, `local_name`, `cloud_name`,
  `relative_parent_path`, `file_id`, `modified_date`, `size`, `status`, `status_changed`,
  `error_code`, `target_type`, `version`, `node_parents`, `content_version`, `schedule_id`,
  `retry_reason`; unique index `UIX_autosave_record(schedule_path, relative_path, schedule_id)`.
  Per-file upload rows live in the `uploads` table. → **belegt**
- **Batch identity.** The enumeration groups a drop into a batch (`BatchId` GUID,
  `BatchInProgress`, `LastBatchStartDate`, `LastBatchCanceled`) on `IManualUploadDatabase`. → **belegt**
- **After completion.** Completed one-time entries are removed on demand:
  `IManualUploadDatabase.ClearCompleted(scheduleId = "ManualUpload")` runs
  `delete from uploads where status >= <finalized> and schedule_id = ?` and publishes
  `UploadDatabaseClearCompletedEvent`; driven by `ClearCompletedUploadsIntentHandler`
  (`ClearCompletedUploadsIntent`). A related `DeletePendingAndErroredUploadsForSchedule` clears
  pending + errored rows, and `CancelManualUploadsCommand` cancels in-flight manual uploads. → **belegt**
- **Across restart.** Because the queue is DB-backed and `OneTimeBackupItemsViewModel` re-queries
  on `TransferDatabaseRequerySuggestedEvent`, persisted rows survive a restart. → **belegt**.
  Whether an **interrupted enumeration** (the walk, not the rows) resumes after a restart is
  **UNKNOWN**. → **UNKNOWN**
- The same one-time pipeline is also used outside drag-drop: the macOS Finder Service / CLI start
  (`UploadArgStartupTask`, AppStartupFlags `AutoPlay`) and the Apple Photos import
  (`OneTimeMediaImport`) call `EnumerateOneTimeBackupUploadPaths` / `OneTimeMediaImport`. → **belegt**

---

## 7. Overlay states and view components (Q5)

| State | Component | Detail |
|---|---|---|
| **Idle (CX)** | `DragDropView` (inside the overlay shown by `DriveMainView.ShowDragDropOverlayView`) | dashed border (`DashedBorderBox`, radius 12, width 3, dash 5), a 56 pt circle, a 32 pt `ImageType.CloudUpload` icon, and the centred text `…_dragdrop_drop_files_text` = "Dateien zum Hochladen ablegen..." |
| **Idle (legacy, backup list)** | `BackupsQuickViewController` `_dragDropContainerView` | instruction text `…_backupscreen_dragdrop_message` = "Dateien per Drag & Drop beliebig verschieben zur einmaligen Sicherung" |
| **Drag entered** | `StaticDragDropOptionViewModel.Active = true` (set in `DraggingEntered`) | `DragDropView.UpdateActive` swaps the dashed border to the active colour and scales the icon to 1.2× over 0.25 s; `StaticDragDropOptionView.UpdateActive` swaps to the active image/border colour. Reset to `false` on `DraggingExited` / `DraggingEnded` |
| **Overlay (legacy)** | `DragDropOverlayView` | overlay background + bold 20 title `…_dragdrop_title` = "Fotos und Videos zum Hochladen per Drag & Drop verschieben", plus one `StaticDragDropOptionView(ChooseFolderOption, DragDropChooseFolder, DragDropChooseFolderActive, "Ordner auswählen")` |
| **Scanning (legacy only)** | `DragDropScanningOverlay` | overlay-scanning background, bold 20 `…_dragdrop_scanningfiles` = "Dateien in Bearbeitung ..." and a `ColoredProgressIndicator` spinner; shown by `DriveMainView.ShowDragDropScanningOverlay`, added below the header/above the footer |
| **Started notice** | `DriveMainView.ShowOrRefreshBackupStartedToastView` → `ToastView(ToastType.BackupStartedToast)` | shown only when the selected screen is not `Backup`; title `…_hometab_uploadstarted_messagetoast` = "Hochladen gestartet – {0}", message from the caller; auto-closes after 5 s |
| **Row done** | `OneTimeBackupItemStatusViewModel` | status derived from the aggregated transfer state (see §5.2) |

Evidence notes (**belegt**): the overlay state machine is exactly `DraggingEntered →
Active=true`, `DraggingExited/DraggingEnded → Active=false`, drop → `PerformDragOperation →
OnDrop`. `BaseOption.EnumerateOneTimeBackup` shows the started toast with
`…_hometab_uploadstarted_toast` = "Hochladen gestartet - {0:N0} Elemente"; the CX backup path
shows `…_hometab_uploadstarted_backupcreatedmessage` = "Siehe Registerkarte „Datensicherung“"
via `ScheduleCreatedEventHandler` on `ScheduleChangedEvent.Create`.

`UI.WindowService` exposes `ShowBackupStartedToast`, `ShowDragDropScanningOverlay` and
`RemoveDragDropScanningOverlay`; the scanning overlay is used **only** in the legacy
`ChooseDestinationOption` branch. → **belegt**

---

## 8. String evidence (keys → German)

Keys cited in this doc, verbatim from `de.lproj/Localizable.strings`. The right-hand column
says whether the managed code references the key (grep for `Strings.<key>`). → **belegt**

| Key (… = `clouddrive_desktop_`) | German | Referenced? |
|---|---|---|
| `…_dragdrop_title` | "Fotos und Videos zum Hochladen per Drag & Drop verschieben" | yes (`DragDropOverlayView`) |
| `…_dragdrop_drop_files_text` | "Dateien zum Hochladen ablegen..." | yes (`DragDropView`) |
| `…_dragdrop_choosefolder` | "Ordner auswählen" | yes (`DragDropOverlayView`) |
| `…_dragdrop_scanningfiles` | "Dateien in Bearbeitung ..." | yes (`DragDropScanningOverlay`) |
| `…_backupscreen_dragdrop_message` | "Dateien per Drag & Drop beliebig verschieben zur einmaligen Sicherung" | yes (`BackupsQuickViewController`) |
| `…_backup_onetimebackupitem_title` | "Einmalige Sicherung" | yes (`OneTimeBackupItem`, list row) |
| `…_autoconnectwindow_onetimebackup` | "Sicherung einmalig hochladen" | yes (AutoConnect) |
| `…_hometab_uploadstarted_toast` | "Hochladen gestartet - {0:N0} Elemente" | yes (`BaseOption`) |
| `…_hometab_uploadstarted_messagetoast` | "Hochladen gestartet – {0}" | yes (toast title) |
| `…_hometab_uploadstarted_backupcreatedmessage` | "Siehe Registerkarte „Datensicherung“" | yes (CX toast) |
| `…_schedules_warningmessage_only_drop_folders` | "Nur Ordner in diesem Bereich löschen" | yes (`DragDropView`) |
| `…_schedules_warningmessage_folders_not_backedup` | "Einige Ordner wurden nicht gesichert" | yes (CX warning) |
| `…_dragdrop_defaultfolder` | "Laden Sie Elemente in einen Cloud-Standardordner hoch." | **no** |
| `…_dragdrop_defaultfolder_path` | "Cloud-Pfad: /All/{0}" | **no** |
| `…_drivewindow_dragdropoverlay` | "FOTOS PER DRAG & DROP VERSCHIEBEN, UM SIE HOCHZULADEN" | **no** |
| `…_statuswindow_draganddropoverlay_message` | "Zur einmaligen Sicherung mit der Maus hierher ziehen" | **no** |
| `…_backupeducation_onetimedragdrop` | "Für einmalige Sicherungen ziehen Sie Dateien oder Ordner per Drag-and-Drop in diesen Bereich" | **no** |
| `…_statuswindow_browsehint` | "Oder ziehen Sie Dateien zum Hochladen in dieses Fenster" | **no** |

The "**no**" rows are defined but not referenced by the managed assembly — *inferred* dead or
used only by a nib / alternate layout. Note this corrects the earlier draft, which treated
`…_drivewindow_dragdropoverlay` / `…_statuswindow_draganddropoverlay_message` as the active
overlay text: the overlay actually shown renders `…_dragdrop_title`. → **belegt**

---

## 9. Gap analysis vs. our app (committed state)

| Capability | Original | Our app | File reference |
|---|---|---|---|
| Drag input | two `INSDraggingDestination` views | **none** — `update` never reads dropped files | `src/app.rs` (`fn update`) |
| Overlay / scanning UI | `DragDropView`, `DragDropOverlayView`, `DragDropScanningOverlay` | **none** | `src/app.rs`, `src/backup.rs` |
| One-time queue row | `OneTimeBackupItem` (fixed name, one transfer state) | **none** — list is only `self.store.folders` | `src/backup.rs` (`Row`, `backup_ui`) |
| Per-file input | files + folders (legacy) / folders only (CX) | **partly** — scan takes a *directory* | `src/folders.rs` (`scan_images`) |
| Upload run | shared transfer engine | `run_upload(items, …, folder_opts)` reusable | `src/upload.rs` |
| Dedup | manual = `UploadType.Normal` (no MD5 dedup) | path+MD5 index, gated by a per-target bool | `src/dedup.rs`, `src/upload.rs` |
| Conflict policy | per schedule | `ConflictResolution` per target | `src/folders.rs` |
| Destination/cloud path | cloud node id (default "/All") or picker | **none** — cookie upload into the account | `src/prime_photos.rs` |
| Persistence of one-time queue | `uploads`/`backup` rows, `schedule_id=ManualUpload` | **none**; ad-hoc queue should **not** persist | `src/folders.rs` (`FolderStore`) |
| i18n layer | `Localizable.strings` | central de/en table | `src/i18n.rs` |

---

## 10. Mapping / implementation approach (plan only)

### 10.1 Input (egui 0.31)

`egui`'s `RawInput` already carries OS drag-and-drop: `ctx.input(|i| i.raw.hovered_files)`
non-empty while a drag is over the window; `ctx.input(|i| i.raw.dropped_files)` (each with an
optional `path: Option<PathBuf>`) on drop. Handle it in `PrimePhotoApp::update` (`src/app.rs`)
and dispatch into a new `one_time_upload(paths)`. Keep it OS-neutral (macOS/Windows/Linux) —
no `#[cfg]`. → *inferred (egui API)*

### 10.2 Overlay states

Mirror the observed states: an **idle** hint in the backup list, a **drag** overlay (framed drop
zone, title + hint) while `hovered_files` is non-empty and the drop is allowed (signed in, not
already running), a **scanning** state while enumerating, and the row's transfer state when
uploading. Reuse the drawn-icon style in `src/icons.rs`.

### 10.3 Synthetic one-time row (no persistence)

Do **not** add a `BackupFolder` to `self.store` (that would persist an ad-hoc target into
`folders.toml`). Hold a run-scoped field on `PrimePhotoApp` (e.g. `one_time: Option<OneTimeUpload>`
with `name` fixed to "Einmalige Sicherung" plus `total/done/status`) and render it in
`backup_ui` (`src/backup.rs`) with the same row chrome / `status_text` mapping.

### 10.4 Reuse of scan + upload

For each dropped path: directory → `folders::scan_images(dir)`; file →
`folders::is_supported_image(path)` (`src/folders.rs`). Feed the collected paths into
`run_upload` as `items: Vec<(usize, PathBuf)>` using a reserved synthetic index and a matching
`folder_opts` entry — the whole queue/worker/pause pipeline is then reused
(`src/upload.rs`).

### 10.5 Dedup & conflict (deliberate divergence)

The original uploads a one-time drop as `UploadType.Normal` (no MD5 dedup, §5.4). Our port keeps
its persistent path+MD5 index and dedups a repeated drop (same path + same MD5 → skip) — safer,
and matches how we already treat every folder. Record as a **deliberate divergence**. Conflict
policy defaults to `ConflictResolution::Ignore` (`conflictResolution=NONE`).

### 10.6 Destination (post-MVP)

Our engine has no per-run destination-node concept, so the MVP drop uploads into the account
root **with no destination UI**; the choose-folder branch needs a cloud-folder picker that does
not exist yet. (The original resolves a cloud **node id** via `GetOrCreateBackupDestination`,
defaulting under root "Alle" — §4.)

### 10.7 Notifications

The `DragDropToastNotification` class has no observed publisher
(`SPEC_NOTIFICATIONS.md` §3.2), but the original **does** show an **in-window**
`ToastType.BackupStartedToast` on a drop (§7). For us: reflect the run via the overlay/row and
optionally the existing `src/notify.rs` run-finished toast (behind `config.notifications`).

### 10.8 i18n keys to add (`src/i18n.rs`, de + en)

| German (key) | English |
|---|---|
| `Fotos und Videos zum Hochladen per Drag & Drop verschieben` | `Drag & drop photos and videos to upload` |
| `Dateien zum Hochladen ablegen...` | `Drop files to upload...` |
| `Ordner auswählen` | `Choose folder` |
| `Dateien in Bearbeitung …` | `Processing files …` |
| `Einmalige Sicherung` | `One-time backup` |
| `Hochladen gestartet - {} Elemente` | `Upload started – {} items` |
| `Nur Ordner in diesem Bereich ablegen` (our own; the original's wording is misleading) | `Only drop folders into this area` |

---

## 11. UNKNOWN (Q6 — explicitly not determined)

- **Multi-drop coalescing** — whether several drop events during one scanning pass merge into
  one batch/row or queue separately. The batch model (`BatchInProgress`/`BatchId`) suggests a
  merge, but the merge itself was not traced. → **UNKNOWN**
- **Drop during an active upload** — whether the original appends to the running queue or
  rejects/starts a second run. → **UNKNOWN**
- **External volumes** — whether a drop from an external volume is accepted and whether the path
  is canonicalised / the volume tracked (`ExternalDriveMissing` exists for schedules, not
  observed for a one-time drop). → **UNKNOWN**
- **Size / amount limits** — whether the original caps a drop by file count or total size before
  accepting. None was found. → **UNKNOWN**
- **Ghost vs. copy/move semantics** — the pasteboard yields `public.file-url`; whether the drop
  is a copy or a move was not traced. → **UNKNOWN**
- **Render site of `…_dragdrop_defaultfolder[_path]`** — defined but unreferenced by the managed
  code; possibly rendered by a nib / alternate layout. → **UNKNOWN**
- **Whether `DefaultFolderBackupOption` is truly unreachable** vs. wired through a path outside
  the decompiled assembly (reflection/other dll). → **UNKNOWN** (*inferred* dead)
- **Whether `DragDropHandler.Dispatch` is invoked from outside the assembly** (e.g. a native
  callback). No managed caller exists. → **UNKNOWN** (*inferred* dead)
- **Resume of an interrupted enumeration after restart** — rows are persisted, the walk's
  resumption is not traced. → **UNKNOWN**
- **`Option.OnDrop` behaviour for an empty/degenerate path list** — not traced. → **UNKNOWN**
- **Which treatment this install receives for `EnableCXUploadSimplification`** (server-driven
  A/B), i.e. whether a real user sees the one-time path or the CX scheduled path. → **UNKNOWN**

---

## 12. Cross-references

- `UI_FLOWS.md` §4.4 ("Einmalig" vs "Plan"), §4.8 (drag-and-drop flow), §11.2 (status
  precedence; one-time row ignores the per-backup pause).
- `UI_STATES.md` §3.1 (row model; one-time name fixed), §3.8 (`uploads` row `ManualUpload`).
- `SPEC_FOLDERS.md` §2 (scan/filter contract), §3 (original schedule model).
- `SPEC_DEDUP.md` §1 (MD5 identity), §2.2 (`UploadType` eligibility), §4.1 (conflict).
- `SPEC_BACKGROUND_SYNC.md` (cadence/pause; a one-time drop is outside the scheduler).
- `SPEC_UPLOAD_PROGRESS.md` (progress UI — a one-time run reuses it).
- `SPEC_NOTIFICATIONS.md` §3.2 (`DragDropToastNotification` has no publisher).
- Our code: `src/app.rs` (`update`), `src/backup.rs` (`backup_ui`, `start_folder_upload`,
  `finalize_upload`), `src/folders.rs` (`scan_images`, `is_supported_image`,
  `ConflictResolution`), `src/upload.rs` (`run_upload`, `upload_worker`), `src/dedup.rs`,
  `src/i18n.rs`.
