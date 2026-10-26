# Status Window in Detail — Clean-Room Spec (reverse-engineered)

## 1. Purpose & scope

Behavioural specification of the **original** Amazon Photos desktop client's *status window in
detail* — the per-file table behind a selected backup, its columns, sorting/filtering/grouping and
relative-time rendering; the **drill-down details panel** and its row fields; the **error window**
(`failedsyncitemswindow_*`, the backup error window, `readdtoqueue`, `retryall`) with entry
conditions and dismissal; the **activity window** ("Alle Aktivitäten anhalten", `activitywindow_*`)
as a *global worker stop* — explicitly **not** the per-backup pause; and the **menu-bar status
popup** with its statistics suffix (`statuspopup_stats_*`, `contextmenu_*`).

Scope boundary: **progress texts** ("Upload-Vorgang läuft …", ETA, byte counters) are owned by
`SPEC_UPLOAD_PROGRESS.md`; the **backup-pause mechanism** (schedule flags, watcher removal,
`SPEC_BACKGROUND_SYNC.md` §7) is owned by `SPEC_BACKGROUND_SYNC.md`; the **status icon /
notification surfaces** are owned by `SPEC_NOTIFICATIONS.md`. This document owns the *table,
its model and its error surfaces*.

Confidence convention: **belegt** = read directly out of the reverse-engineered
`Amazon Photos.dll` (member/branch) or the shipped `de.lproj/Localizable.strings`; *inferred* =
reasoned from adjacent evidence, not read off a line; **UNKNOWN** = not determinable from the
inspected sources.

---

## 2. Sources & method (belegt)

- Tooling: `ilspycmd` (v11.1.0.9782) run with `PATH` extended and
  `DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`. A full type listing plus a full
  decompilation dump were produced, plus targeted `-t` decompilations of the classes below. No tool
  was installed for this task. → **belegt**
- Target, read-only and **never launched**:
  `/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll` and
  `/Applications/Amazon Photos.app/Contents/Resources/de.lproj/Localizable.strings`. → **belegt**
- Types read as evidence: `UI.Controls.TableControls.StatusTable.{BaseTransferItemsTableView\`1,
  BaseTransferItemsTableViewDelegate\`1, TransferItemStatusTableCell, IconWithLabelTableCell,
  TextLabelCell, TextLabelRowView, ErrorWindowIconLabelTableCell, DisplayItemsTableViewDataSource\`1}`,
  `UI.{RecordItem, TransferItem, UploadItem, TransferItemsViewModel, TransferItemsViewModelBase,
  TransferItemFilterExtensions}`, `UI.Helpers.TransferEnumExtensions`,
  `UI.DriveWindow.TabItemView.DriveBackup.Subviews.DetailsView.{BackupsDetailTableView\`1,
  ScheduledBackupDetailsView, OneTimeBackupDetailsView, BaseScheduledBackupDetailsView}`,
  `UI.DriveWindow.TabItemView.DriveBackup.{BackupHeaderView, BackupHeaderViewController}`,
  `UI.DriveWindow.DesktopBackupLegacyViews.{ScheduledBackupItemsViewModel,
  OneTimeBackupItemsViewModel}`, `UI.ErrorWindow.{ErrorViewController, ErrorView\`1,
  ErrorWindowService, BackupsErrorTableView\`1}`,
  `UI.DriveWindow.DriveBackupTab.ErrorWindowViewModel`, `UI.IntentHandlers.{ShowActivityWindowIntentHandler,
  ShowBackupErrorWindowIntentHandler, RetryErroredUploadsIntentHandler,
  RetryErroredDownloadsIntentHandler, RetryErroredUploadsIntentHandler}`,
  `UI.Intents.{ShowActivityWindowIntent, ShowBackupErrorWindowIntent, RetryErroredUploadsIntent,
  RetryErroredDownloadsIntent, RetryFailedSyncItemsIntent, ClearCompletedUploadsIntent,
  PauseAllBackupsIntent, ResumeAllBackupsIntent, PauseBackupItemIntent, ResumeBackupItemIntent}`,
  `UI.ActivityWindow.{ActivityWindowService, ActivityWindowViewController, ActivityViewController,
  ActivityViewModel, ActivityItemsTableViewController, ActivityToggleButtonView}`,
  `UI.StatusPopup.{StatusPopupWindowService, StatusPopupViewController, StatusPopupViewModel,
  StatsTableView, Views.BackupStateRowView}`, `UI.StatusPopup.{TransferStateRowViewModel\`1,
  BackupStateRowViewModel}`, `Infrastructure.State.Transfer.TransferStateExtensions`,
  `Infrastructure.State.Workers.WorkersStateMachine`, `UI.Commands.{PauseTransferActivityCommand,
  ResumeTransferActivityCommand, CancelManualUploadsCommand}`,
  `Services.ManualUpload.ManualUploadService`, `Services.AutoFixError.AutoFixErrorHelper`,
  `Helpers.DateTimeExtensions`. String references were confirmed by grepping the full decompilation
  for `Strings.<key>` call sites. → **belegt**
- Our own state was read from `src/` with grep and direct reads (`src/backup.rs`, `src/upload.rs`,
  `src/app.rs`, `src/i18n.rs`, `src/folders.rs`, `src/dedup.rs`, `src/config.rs`, `src/tray.rs`,
  `src/settings.rs`). → **belegt**

---

## 3. Mechanics, states, fields, persistence, UI texts

### 3.1 The item model shared by every status table (belegt)

Every status table (backup details, activity window, error window, download tab, sync tab) renders
**one** abstract row type, `UI.RecordItem`, with `UI.TransferItem` and its `UI.UploadItem` /
download specialisations on top. It is a *projection of a database row*, not a live object: it is
built from a single `IRecord` and then subscribes to a progress event to animate.

| Field | Source in the DB record | Meaning |
|---|---|---|
| `Id` | row id | primary key used by every command (`Retry`/`Dismiss`/`AddToQueue`) |
| `NodeId` / `ParentId` | cloud node ids | set once the node exists; drives "view on web" |
| `FriendlyName` | file name (last component) | the **Name** column |
| `FullPath` | absolute local path | row tooltip, progress-event matching |
| `LastModifiedDate` | **`record.StatusChanged`** | the "Letzte Aktualisierung" column and the age string |
| `SizeBytes` | `record.Size` | the **Größe** column and the byte totals |
| `Status` | `TransferStatus` | the filter/status vocabulary |
| `ErrorCode` | `TransferError` | short/long error text, auto-fix CTA |
| `SortDate` | `record.SortDate` | the sort tiebreaker |
| `TargetType` | `TargetType` | `Folder`/`File` — folder rows render no size |
| `Progress` / `RemainingTimeInSeconds` | `FileTransferProgressEvent` | per-file byte progress |
| `ErrorNeedsUserAttention` / `RetryNeedsUserAttention` | derived | "Achtung" override |
| `UserStatus` | derived enum | drives the status icon and `CanViewItem` |
| `StatusString` / `StatusToolTip` | abstract, per specialisation | status label / hover text |

Two derived fields matter for the status column: `CanViewItem` (a "view on web" link is offered when
the item is `Completed` and has a `NodeId`, or when it was `Skipped`) and `DisplayStatusString`,
which returns *"remaining-time (percent)"* while a byte progress is known. → **belegt**

### 3.2 Columns — the important detail: **there is no fixed 4-column set** (belegt)

There are **two** different backup-details table classes in this build, with **different columns**.
This is the single biggest trap in the topic, and it is why `UI_FLOWS.md` §4.7's four-column list
must be read as *one* of the two layouts:

| Table | Name | Last updated | Size | Status | Extra |
|---|---|---|---|---|---|
| `BackupsDetailTableView<T>` (CX layout, used by `ScheduledBackupDetailsView` **and** `OneTimeBackupDetailsView`) | 275 pt, min 275 | 100 pt, min 100 | **absent** | min 150, header carries a **down-caret icon** (the filter menu) | 3 columns |
| `BackupDetailsView` / `DetailTableView` (legacy layout, thumbnails) | 303 pt, min 275, header carries a **sort icon** | 95 pt, min 95 — title built by cutting `…_statuswindow_lastmodified` at the colon → **"Letzte Änderung"** | 87 pt, min 87 | min 150, header carries a **filter icon** | 4 columns |

Column headers in both: `…_statuswindow_filenamecolumntitle` ("Name"),
`…_statuswindow_lastmodifiedcolumntitle` ("Letzte Aktualisierung"),
`…_statuswindow_sizecolumntitle` ("Größe"), `…_statuswindow_statuscolumntitle` ("Status").
Resizing is allowed, **reordering is disabled**, row height 44 pt (error window: 75 pt), the last
column is stretched to fit, and collection changes are **sampled at 0.35 s** before the table
reloads — a deliberate anti-flicker measure, not a data delay. → **belegt**

So the answer to "which columns does the status window have" is: *the CX details view has three
(Name / Last updated / Status); the legacy details view has four and adds Size*. The "Größe"
column string is still referenced (by the legacy table, the activity window, the download and sync
tables), so it is **not** a dead key. → **belegt**

### 3.3 Sorting, filtering, grouping (belegt)

- **Sorting.** The generic view model sorts on every reload by a fixed precedence:
  `Status != Processing` first, then `Status == Errored`, then `Status == Skipped`, then
  `SortDate.UtcTicks` **descending**. `RecordItem.DefaultComparer` (used by the sorted collections
  in the one-time/activity models) buckets by a status sort key — Processing = 0, Errored = 1,
  anything else = 2, `>= Completed` = 3 — and then by `SortDate.UtcTicks` **ascending**.
  → **belegt**
- **Click-to-sort.** Only the **legacy** table is user-sortable: clicking the Name header sorts by
  `FriendlyName` and flips an ascending flag; the sort icon marks the column. The CX table has no
  sort action. → **belegt**
- **Filter.** Two independent mechanisms:
  1. the **generic** `SelectedFilter` (`All, Waiting, Processing, Completed, Canceled, Failed` —
     six values) on `TransferItemsViewModel`, which is a *view-model* property re-querying the DB
     and resetting the page to 1; it is what the download/sync tabs and the CSV-ish filter popups
     bind;
  2. the **CX backup details table's own** `NSMenu` with **seven** items — the six above **plus
     `Skipped`** — opened by clicking the *last* column header (`ClickedColumn == ColumnCount - 1`
     and `ClickedRow == -1`). Selecting an item radio-marks it and filters the already-loaded
     collection client-side through `_transferItemFilter.Includes(item.Status)`, executed **inside
     the view model's lock** so the paged view stays consistent. Default = `All`.
     → **belegt**
- **Grouping.** There is **no grouping** in either backup details table: the item list is flat.
  The *activity window* is the only place with a two-section grouping ("In Bearbeitung" /
  "Warten"), and that grouping is by status bucket, not by folder. Our port's queue panel is
  also a **single flat list**, ordered globally by file status (user decision 2026-10-26); the
  earlier per-folder grouping was our own design decision, **not** a port of anything in the
  original. → **belegt** (absence of grouping in the backup tables) / *inferred* (our own)
- **Pagination.** 1000 items per page, `SelectedPage` / `TotalPages` derived from the filtered
  count. → **belegt**
- **Empty state.** A special row view renders `TransferItemFilter.<x>.EnumToLocalizeEmptyDisplayItemName(Backup)`
  — separate string families for upload and download. For the backup error table, the empty text
  is the **failed** variant ("Keine fehlgeschlagenen Uploads. (Puh!)"). → **belegt**

### 3.4 Relative time — `datetimeago_*` (belegt)

`Helpers.DateTimeExtensions.ToAgoShortString(DateTimeOffset)` computes `UtcNow() - dt` and picks
exactly one branch, in this order:

| Age | Rendering |
|---|---|
| ≥ 365 days | absolute date, `"MMM d yyyy"` |
| > 7 days | absolute date, `"MMM d"` |
| exactly 1 day | `…_datetimeago_oneday` |
| 2–7 days | `…_datetimeago_manydays` with `{0}` = days |
| exactly 1 hour | `…_datetimeago_onehour` |
| 2–23 hours | `…_datetimeago_manyhours` with hours |
| exactly 1 minute | `…_datetimeago_onemin` |
| 2–59 minutes | `…_datetimeago_manymins` with minutes |
| exactly 1 second | `…_datetimeago_onesec` |
| 2–59 seconds | `…_datetimeago_manysecs` with seconds |
| otherwise (sub-second / future) | falls back to `…_datetimeago_onesec` |
| any exception | empty string |

Two consequences worth recording: the two absolute-date branches are **not** localised (an `MMM`
month abbreviation in the invariant culture, so a German UI still shows an English month name), and
the column value is derived from **`record.StatusChanged`**, not from the file's filesystem
timestamp — so "Letzte Aktualisierung" means "when this row last changed state", not "file
modified". Whether the age string is recomputed on a timer while the table is open is **UNKNOWN**
(the table reloads on sampled collection change and DB requery events; no per-cell timer was
found). → **belegt** (rendering + source field) / **UNKNOWN** (refresh cadence of a visible cell)

The reverse helper `ToRemainingShortString(TimeSpan)` renders a *future* date for the ETA text and
lives in `SPEC_UPLOAD_PROGRESS.md` territory. → **belegt** (existence)

### 3.5 The drill-down / details panel (belegt)

Selecting a backup row opens a details panel whose chrome is:

- **Auto-upload toggle** at the top (`…_auto_upload_toggle_text` + subtitle), owned by
  `SPEC_BACKGROUND_SYNC.md` §5. → **belegt**
- **Two display-mode buttons** in the details header: nested/drill-down
  (`…_backupdetailsview_drilldown`, tooltip "Verschachtelte Anzeige") and flat list
  (`…_backupdetailsview_flatlist`, "Flache Listenanzeige"). The mode is persisted in
  `CustomerSettings.BackupItemsDisplayMode` — a *customer* setting, not per backup. → **belegt**
- **Breadcrumb navigation stack** (a horizontal stack with a parent-nav stack view, right carets and
  an overflow button) — this is what makes the drill-down a *navigation* rather than a filter. In
  drill-down mode, only rows that are `AutoSaveNode` with `TargetType == Folder` are selectable;
  `ShouldSelectRow` returns false for everything else and false for **all** rows in flat mode.
  Clicking a selectable row executes the view model's `NavigationCommand`, which needs
  `!IsLoading`. → **belegt**
- **Footer**: a primary capsule button labelled `…_login_ui_retry_button` ("Wiederholen"),
  initially **disabled**; it is enabled only when the schedule's `FailedCount > 0`, and publishes
  `RetryErroredUploadsIntent(scheduleId)`. → **belegt**
- **Loading indicator** shown while `TransferItemsLoading`. → **belegt**
- **Error banner** between the header and the table: `…_errorbannerview_uploaderror` (singular) /
  `…_errorbannerview_uploaderrorplural` (plural) chosen by `ErrorCount <= 1`, with a hyperlink
  `…_errorbannerview_showerrorwindow` ("Fehler beim Hochladen anzeigen") that opens the error
  window. → **belegt**

**The row fields a user can actually reach per file** (all reached through the Name cell's hover
tooltip plus the Status cell):

| Surface | Content |
|---|---|
| Name cell tooltip | `FullPath`, then (files only) the size line `…_statuswindow_sizetooltipformat` + `FileSize`, then `…_statuswindow_lastmodified` with `ToAgoShortString` |
| Status cell | the status label, a status icon keyed on `UserStatus`, a hover tooltip (`StatusToolTip`), error colour when `UserStatus == Failed`; while a byte progress is known the label switches to `DisplayStatusString` |
| Status cell hyperlink | `…_statuswindow_viewonweb` ("Im Web anzeigen") when `CanViewItem`; for `Errored` + `NeedsPhotoLibraryPermissions` the link instead carries the error's short string and opens the system-preference pane |
| Status cell, second link | `…_statuswindow_readdtoqueue` ("Erneut zur Warteschlange hinzufügen") **only** when `Status == Skipped` |
| Error window row | `FriendlyName` + `StatusToolTip` as sub-text, icon, `("(size)")` appended, and a per-row CTA + dismiss button (see §3.6) |

The link action itself branches: `Skipped` → re-add the record to the queue;
`NeedsPhotoLibraryPermissions` → launch the privacy preference pane; otherwise → open the item in
the browser via `LaunchCloudDriveLinkIntent`. → **belegt**

### 3.6 The error window — conditions, contents, dismissal (belegt)

**Two entry paths, one window per schedule.**

1. `ShowBackupErrorWindowIntent(scheduleId)` — published by the details view's error-banner
   hyperlink and by `ShowBackupErrorWindowCommand` on both `ScheduledBackupItemsViewModel`
   (the schedule id) and `OneTimeBackupItemsViewModel` (**hard-coded** `"ManualUpload"`). The
   handler calls `IBackupErrorWindowService.Show(id)`. → **belegt**
2. There is a second, legacy intent (`RetryFailedSyncItemsIntent` + handler) and the
   `…_failedsyncitemswindow_*` string pair (`title`, `retry`); **neither is referenced by any
   managed code path** — grep finds only the string definitions. → *inferred* dead (the absence of
   a caller is **belegt**).

**Window mechanics.** `ErrorWindowService` is an `AsyncWindowServiceBase<string, ErrorWindowViewModel>`
that keeps a **dictionary keyed by `scheduleId`** → `NSWindowController`, so the same schedule can
never have two error windows and *different* schedules can. If a window for the id already exists it
is simply re-fronted (`OrderFrontRegardless` + `MakeKeyWindow`). Otherwise it publishes
`WindowOpenEvent`, builds the view model from a factory, and creates a
`DriveThemedWindowController` — titled, closable, resizable, **600 × 400**, centred, with a health
metric `WindowOpened.ErrorWindow` (pivot Backup). Close is observed via
`NSWindow.WillCloseNotification`, which disposes the controller and removes it from the dictionary. → **belegt**

**Contents.** Footer with `…_retryall` ("Alle erneut versuchen") as a primary capsule and
`…_close` ("Schließen") as tertiary, footer height 60. Body is a headerless table
(`HeaderView` is null) with a **single** `NameColumn` at 275 pt and 75 pt rows → so the error
window shows *names only*, no size/status/date columns. Empty state renders the **failed** empty
text. → **belegt**

**Per-row actions.** The error table's cell exposes a CTA button and a dismiss button:

- CTA text = `AutoFixErrorHelper.FixableErrors[errorCode]` when the error is auto-fixable
  (`CloudDriveOverQuota` → "Upgrade vornehmen"; `FileNotFound` / `FolderNotFound` → "Entfernen"),
  otherwise the generic `…_retry` ("Wiederholen"). → **belegt**
- Dismiss = `…_errorwindow_dismiss` ("Verwerfen"). → **belegt**

**What each command does** (`ErrorWindowViewModel`, constructed with the `scheduleId`):

| Command | Effect |
|---|---|
| `RetryErroredUploadsCommand` ("Alle erneut versuchen") | business metric + health metric, then publish `RetryErroredUploadsIntent(scheduleId)` → `ManualUploadService.RetryErrored(scheduleId)` |
| `RetryErroredRecordCommand(uploadId)` | on a background task: load the record; if `ContentType == Other` → **delete** the upload record (and, for a non-`ManualUpload` schedule, the matching `AutoSaveRecord` by local path) then reload; otherwise `RetryErroredRecord(record, now)` |
| `DismissRecordCommand(uploadId)` | metrics + `SkipUploadRecord(record, now)` + reload |
| `FixErrorCommand(uploadId)` | metrics + `IAutoFixErrorService.TryFixError(record)` + reload |

`RetryErroredRecord` is a targeted SQL update: `status = Waiting`, `status_changed = now`,
`retries = 0`, `retry_reason = 0`, `next_retry = null`, `error_code = 0`,
`retry_timer_start = null`, scoped to `status = Errored and schedule_id = ? and id = ?`. If the
record `IsAutoSave()` it also updates the `AutoSaveRecord` and publishes `BackupRequerySuggested`;
otherwise it publishes `TransferDatabaseRequerySuggestedEvent`. → **belegt**

**Retry-all semantics differ per schedule** and this is easy to get wrong.
`ManualUploadService.RetryErrored(scheduleId)` queries records with status
`Waiting | Processing | Retrying | Errored` **and `ContentType == Other`**, then **deletes** those
upload records by path — and, unless the schedule is `"ManualUpload"`, their `AutoSaveRecord`s too
— before calling `RetryErroredRecords(out statusChangeDate, scheduleId)`. Net effect: for a
**scheduled** backup, "retry all" is a *delete-and-re-enumerate* (the next scan re-creates the
rows); for the **one-time** queue there are no autosave rows to delete, so the same call simply
re-queues the errored rows and re-opens the manual-upload batch (`BatchId` new if
`!BatchInProgress`, else "batch resumed", `LastBatchCanceled = false`). → **belegt**

**Re-add to queue.** `…_statuswindow_readdtoqueue` resolves a record
(`GetUploadRecordByBackupId` for scheduled, `GetUploadRecordById` for one-time) and calls
`ResetSkippedUploadRecordToWaiting(record, DateTimeOffset.Now)`, with a
`ReAddSkippedRecordsToQueue` business metric. This is the **only** way back from a *dismissed*
(skipped) record — which is why `LinkTitle` shows the link only for `Status == Skipped`. → **belegt**

**Dismiss is not undo.** `DismissRecordCommand` → `SkipUploadRecord` moves the row out of the
errored set; the row leaves the error window immediately (the window reloads). It stays skipped in
the DB and only reappears via `readdtoqueue`. → **belegt**

**Title.** `ErrorWindowViewModel.CreateTitle()` formats `…_errorwindow_titleformat`
("Fehler bei {0}") with the schedule's `ScheduledBackupItem.Name`; for an empty/unresolvable
schedule id it falls back to `…_backup_onetimebackupitem_title` ("Einmalige Sicherung"). → **belegt**

**Auto-fix.** `AutoFixErrorHelper.FixableErrors` contains exactly three errors (over-quota,
file-not-found, folder-not-found). `TryFixError`'s internals were not read → **UNKNOWN**.

**Selection.** Rows in the error window are **not selectable** (`ShouldSelectRow` → false) — every
action is reached from the row's buttons. → **belegt**

### 3.7 "Verlauf löschen" / cancel — adjacent, briefly (belegt)

`ClearCompletedUploadsIntent` opens a modal confirmation (`…_statuswindow_clearcompletedconfirmationheader`
"Verlauf löschen?" + `…_statuswindow_uploads_clearcompletedconfirmationtextmessage` +
`…_statuswindow_clearcompletedbuttontitle` "Verlauf löschen" + `…_cancel` "Abbrechen") and on
confirm calls `ClearCompleted()`; the download twin uses the `downloads` confirmation text.
`CancelManualUploadsCommand` (wired from the one-time footer's cancel button and the tray/row
"Upload abbrechen" item) takes the worker state machine, the global backup transfer state, the
enumeration service, the event service, the upload DB and an **abort-multipart-upload service** as
its dependencies; its body was not read → *inferred* it aborts in-flight multipart uploads and
cancels pending manual uploads, but the details are **UNKNOWN**.

### 3.8 The activity window — "Alle Aktivitäten anhalten" is **not** here (belegt)

The activity window is an **inspection** surface, not a control surface. It has **no pause/stop
button at all**: its only controls are two collapsible section headers and the tables beneath them.

- Window: 550 × 391, closable-only style, titled `…_activitywindow_title`
  **"Upload-Vorgang läuft"**. Opened via `ShowActivityWindowIntent` →
  `ShowActivityWindowIntentHandler` → `ActivityWindowService.Show()` (registered as a singleton).
  → **belegt**
- **Three visual states** driven by `IsLoading` and `IsBackingUp`:
  loading → spinner only; backing up → the sections; neither → `NoActivityView`
  (`…_activitywindow_noactivity_header`, `…_noactivity_info`, `…_noactivity_viewuploadsbuttontitle`
  "Uploads im Internet anzeigen"). → **belegt**
- **Two collapsible sections** in a vertical stack: "In Bearbeitung"
  (`…_activitywindow_processingitems_title`) and "Warten"
  (`…_activitywindow_waitingitems_title`); each header is 41 pt with a hand-drawn caret that
  flips between expanded/collapsed, bold 12 pt when active and a hover-row background. Each header
  carries an info line `…_activitywindow_remaining_items_format` = "{0:N0} Elemente ({1:N0})
  verbleibend" (count + formatted byte size). → **belegt**
- **Table columns** (all three shared `statuswindow_*` keys): Name 270, **Größe 80**, Status 200;
  `StatusColumnPosition = 2`; row height 44; folder rows show no size. → **belegt**
- **Model.** `ActivityViewModel` loads **all** records whose status is
  `Waiting | Processing | Retrying | Completing` — with **no schedule filter and no content-type
  filter**, i.e. every backup *and* the one-time queue in one list. It keeps two sorted collections
  plus four counters (`WaitingItemsRemainingCount/SizeInBytes`,
  `ProcessingItemsRemainingCount/SizeInBytes`), moved between buckets by `UploadStatusEvent`;
  terminal statuses (`Completed`, `Canceled`, `Errored`, `Ignored`,
  `IgnoredDueToPotentialConflictWithiOSUploads`) remove the item; an unknown status throws
  `ArgumentOutOfRangeException`. It also re-loads on `TransferDatabaseRequerySuggestedEvent` and
  removes on `DeletedUploadRecordsEvent`. `IsBackingUp` = either counter > 0. Default
  `ActivityDisplayMode` = `Processing`. → **belegt**
- So the activity window is a **cross-backup, cross-queue** view of what is in flight — the only
  place where the user sees the one-time queue and every schedule together.

**Where "Alle Aktivitäten anhalten" actually lives.** The global stop is
`…_pause_allactivity` / `…_resume_allactivity`, rendered as **two hyperlink buttons in the main
window header** (identifiers `MainView_PauseAllActivity_Button` / `MainView_ResumeAllActivity_Button`,
placed left of the account button). They execute `PauseTransferActivityCommand` /
`ResumeTransferActivityCommand`, which run on a background task
`WorkersStateMachine.PauseAll(PauseSource.UserPaused)` / `ResumeAll(PauseSource.UserPaused)` and,
when the resulting `GlobalState` matches, show a **toast**:
`…_globaltoggle_all_activity_paused` ("Alle Aktivitäten wurden angehalten") /
`…_globaltoggle_all_activity_unpaused` ("Alle Aktivitäten wurden fortgesetzt"). The toast is shown
only when the main window `IsOpen`. → **belegt**

**`WorkersStateMachine` in one paragraph.** It keeps an `IObservableCollection<IWorkerState>`, one
per worker, plus a `GlobalState`. `PauseAll(source)` sets `GlobalState = Transitioning`, takes an
ops semaphore, and for every `PausableWorkerState` flips it to `Transitioning` and calls
`worker.Pause()` — skipping workers already flagged paused — then `Refresh(Paused,
PauseSource | source)` on success or the fallback state on failure; finally `GlobalState = Paused`
if all succeeded. `ResumeAll(source?)` only touches workers whose state has the `Paused` flag, and
a resume that succeeds clears the whole flag set. `StopAll(predicate)`,
`RefreshAll()`, `Restart`, `Interrupt` (pause → intermediate action → `worker.Interrupt` → restore
previous state) complete the surface. `PauseSource` is a **flags** enum: `None`, `ServicePaused`,
`NetworkPaused`, `SystemPaused = Service | Network`, `UserPaused`, plus the two combinations — so a
worker can be paused by the user *and* the system at once, and a filtered `ResumeAll(UserPaused)`
only clears the user bit. Metrics: `PauseAllInMilliseconds` / `ResumeAllInMilliseconds` with a
Success/Failure reliability pivot. Initial `GlobalState` is `Running`. → **belegt**

**The two pauses are different things, and the difference is load-bearing:**

| | Global stop ("Alle Aktivitäten anhalten") | Backup pause ("Alle Sicherungsvorgänge anhalten") |
|---|---|---|
| Target | every `IPausableWorker` in the state machine | the `BackupUploaderWorker` only |
| Mechanism | `PauseAll(UserPaused)` / `ResumeAll(UserPaused)` | `Interrupt<BackupUploaderWorker>(UserPaused, …)` + `UpdateSchedulesPauseState(pause)` |
| Persisted? | **no** — worker state only, in-memory | yes — `schedules.paused` rows (`SPEC_BACKGROUND_SYNC.md` §7) |
| Visible in | window header hyperlink; "ANGEHALTEN"/"Sicherung angehalten" row + popup states | backup header capsule + footer button, hidden when no scheduled items exist |
| Disabled while | `Transitioning` (controls disable on `!Running`) | same |

System-initiated pauses use the same machinery with `PauseSource.SystemPaused`:
`BackupScannerWorker.HandleDeviceRemoval` (external drive unplugged → schedules flagged
`ExternalDriveMissing`, watchers removed, uploader interrupted with
`UpdateSchedulePauseState(pause: true)`) and the `ProcessScanRequest` failure branch
(folder/node errors → `SuspendUploadsForSchedule(scheduleId)`). → **belegt**

### 3.9 The menu-bar status popup and its statistics (belegt)

- Opened by `StatusPopupWindowService.ShowRelativeTo(view)`: a transient `NSPopover`
  (`Behavior = Transient`, `Animates = false`) anchored to the status item, and — a detail worth
  keeping — the popover's window is **detached from the status item's window** and then made key,
  and the app is activated ignoring other apps. It publishes `PopupNotificationReadEvent` (twice)
  and reports `WindowOpened.StatusPopup` with a pivot `UnreadNotifications` /
  `NoUnreadNotifications`. The view model is created **lazily** and disposed with the popover on
  `popoverDidClose:`. All modal variants of `IWindowService` throw `NotImplementedException` — the
  popup is the *only* surface of this service. → **belegt**
- Layout: a 450 pt wide vertical stack — header 50, stats block (3 rows × 50),
  "Benachrichtigungen" header 25, notifications list, quota footer 65. → **belegt**
- **Stats block.** `StatsTableView` stacks `BackupStateRowView`, `DownloadStateRowView`,
  `SyncStateRowView` (each 50 pt) and refreshes on a `CombineLatest` over the backup stats'
  `InProgress`/`Queued`/`Suspended`/`Failed`/`Completed` and the sync stats' — the download row is
  constructed but its stats are not part of the observed set in this build → *inferred* the download
  row may be static. → **belegt** (construction, observation set) / *inferred*
- **Row model.** `TransferStateRowViewModel<T>` derives icon + text from the global transfer state
  and the cold-boot state, with this precedence: cold-boot in progress *and* completed → cold-boot
  text; completed without errors → completed icon + completed text; completed **with** errors →
  **error icon + completed text** (so "Datensicherung abgeschlossen" is shown with a red icon);
  pending → transferring icon + transferring text. `Completed()` means
  `InProgress + Queued + Suspended == 0`. A `null` cold-boot status, or one whose
  `ColdBootState == Unknown`, is treated as "boot in progress". → **belegt**
- **Backup row specifics.** Icons `BackupBlue` (transferring), `PauseGray` (paused),
  `CompleteGreen`, `Error`. Precedence: completed+clean → "Datensicherung abgeschlossen";
  completed+errors → error icon + same text; pending and cold-booting → "Warten"; pending →
  "Sicherung wird durchgeführt"; suspended and clean → pause icon + "Sicherung angehalten";
  suspended and errors → error icon + "Sicherung angehalten". Clicking the row opens the Backup tab
  (`ShowDriveWindowIntent(TabType, FromStatusPopup, TabFilter)`). → **belegt**
- **Call to action.** With `TotalCount == 0` the row shows `…_statuspopup_nobackups` ("Keine
  Datensicherungen gefunden") with a grey icon and the CTA `…_onboarding_startbackup`
  ("Datensicherung starten"), which publishes `ShowBackupConfigurationWindowIntent` with
  `Mode = Create, CanChangeSource = true, CanChangeDestination = true`. Otherwise: with failures
  and nothing in progress/queued, the CTA is "Wiederholen", which publishes
  `RetryErroredUploadsIntent(scheduleId)` for **every** schedule with `FailedCount != 0` **plus**
  `RetryErroredUploadsIntent("ManualUpload")` when the one-time state has failures. Details text
  colour is the faded colour, turning error-coloured when errors exist. → **belegt**
- **The statistics suffix.** `TransferStateExtensions.ToStatusPopupSummary` returns a single
  parenthesised suffix per row, chosen in this order:

| Condition | String | German |
|---|---|---|
| `FailedCount == 0` and `InProgress + Queued > 0` | `…_statuspopup_stats_remaining` | "({0:n0} verbleibend)" |
| `FailedCount == 1` and remaining > 0 | `…_statuspopup_stats_remaininganderror` | "({0:n0} verbleibende und {1:n0} Fehler)" |
| `FailedCount == 1`, nothing remaining | `…_statuspopup_stats_error` | "({0:n0} Fehler)" |
| `FailedCount > 1` and remaining > 0 | `…_statuspopup_stats_remaininganderrors` | "({0:n0} verbleibende und {1:n0} Fehler)" |
| `FailedCount > 1`, nothing remaining | `…_statuspopup_stats_errors` | "({0:n0} Fehler)" |
| nothing at all | *(empty string)* | — |

  "Remaining" is `InProgressCount + QueuedCount` — **suspended items are not counted**. Note there
  is **no** key literally named `statusall`: the "all filter" key is `…_statuswindow_filterall`
  ("Alle"). → **belegt**
- **Menu-bar hover text.** `…_contextmenu_hover_status_message` formats
  "Amazon Photos - {0}" with the current status message, `…_contextmenu_hover_no_status_message`
  is the bare "Amazon Photos", and the status message itself comes from the
  `…_contextmenu_message_*` family (`uploadpaused` "Angehalten", `uploadremaining`
  "Upload-Vorgang läuft ({0} Dateien verbleibend)", `uptodate` "Upload abgeschlossen", `nonetwork`,
  `overquota`, `unreadnotifications`, `getchanges` "Konfiguration wird durchgeführt") or
  `…_contextmenu_uploaderrored` "{0} Dateien konnten nicht hochgeladen werden".
  `…_contextmenu_statuswindow` is "Status anzeigen". → **belegt**
- **First close of the status window** raises a one-shot notification
  (`…_mac_notification_statuswindowclosedfirsttime_message` = "Du kannst die App jederzeit über
  dieses Symbol in der Menüleiste aufrufen."; the `_title` key is **not present** in the German
  strings file → the title falls back to the base localisation or a nib; *inferred*). → **belegt** /
  *inferred*

### 3.10 Persistence summary (belegt)

What is persisted for the surfaces in this document: the **details display mode**
(`CustomerSettings.BackupItemsDisplayMode`) and the per-row records themselves
(`uploads` / `AutoSaveRecord` with `status`, `status_changed`, `error_code`, `retry_reason`,
`next_retry`, `retry_timer_start`). **Not** persisted: the selected filter and page, the sort
direction, the error-window position, the activity window's collapsed sections, and the global
worker pause. → **belegt**

---

## 4. String evidence (keys → German)

Keys cited in this doc, verbatim from `de.lproj/Localizable.strings` (Product strings, not
code). `…` = `clouddrive_desktop_`. "Referenced" = a `Strings.<key>` call site exists in the
managed assembly. → **belegt**

Rows are ordered by surface: table chrome → status vocabulary → relative time → activity window →
failed-sync/error window → error banner + clear-history → status popup → menu bar → the two pauses →
details panel.

| Key (… = clouddrive_desktop_) | German | Referenced? |
|---|---|---|
| `clouddrive_desktop_statuswindow_filenamecolumntitle` | Name | yes |
| `clouddrive_desktop_statuswindow_sizecolumntitle` | Größe | yes |
| `clouddrive_desktop_statuswindow_lastmodifiedcolumntitle` | Letzte Aktualisierung | yes |
| `clouddrive_desktop_statuswindow_statuscolumntitle` | Status | yes |
| `clouddrive_desktop_statuswindow_statusall` | Alle | **no** |
| `clouddrive_desktop_statuswindow_filterall` | Alle | yes |
| `clouddrive_desktop_statuswindow_emptycolumnstring` | -- | yes |
| `clouddrive_desktop_statuswindow_lastmodified` | Letzte Änderung: {0} | yes |
| `clouddrive_desktop_statuswindow_sizetooltipformat` | Größe: {0} | yes |
| `clouddrive_desktop_statuswindow_viewonweb` | Im Web anzeigen | yes |
| `clouddrive_desktop_statuswindow_readdtoqueue` | Erneut zur Warteschlange hinzufügen | yes |
| `clouddrive_desktop_statuswindow_showstatus` | Status anzeigen: | yes |
| `clouddrive_desktop_statuswindow_statuswaiting` | Warten | yes |
| `clouddrive_desktop_statuswindow_statusprocessing` | In Bearbeitung | yes |
| `clouddrive_desktop_statuswindow_statuscompleting` | Wird überprüft | yes* |
| `clouddrive_desktop_statuswindow_statusretrying` | Wiederholen | yes* |
| `clouddrive_desktop_statuswindow_statuscompleted` | Abgeschlossen | yes |
| `clouddrive_desktop_statuswindow_statuscanceled` | Abgebrochen | yes |
| `clouddrive_desktop_statuswindow_statusfailed` | Fehlgeschlagen | yes |
| `clouddrive_desktop_statuswindow_statusskipped` | Verworfen | yes |
| `clouddrive_desktop_statuswindow_statussyncing` | Synchronisierung | yes |
| `clouddrive_desktop_statuswindow_statuswarning` | Achtung | yes |
| `clouddrive_desktop_statuswindow_errors` | Mit Fehlern abgeschlossen | yes |
| `clouddrive_desktop_statuswindow_statuscompleting_detail` | Wir überprüfen, ob Ihre Datei sicher in Amazon Photos gespeichert ist. Bei großen Dateien kann dies einige Zeit dauern. | yes |
| `clouddrive_desktop_statuswindow_statuswaiting_upload_tooltip` | Das Hochladen wird vorbereitet. Dieser Vorgang kann bis zu 10 Minuten dauern. | yes |
| `clouddrive_desktop_statuswindow_statuswaiting_download_tooltip` | Der Download wird vorbereitet. Dieser Vorgang kann bis zu 10 Minuten dauern. | yes |
| `clouddrive_desktop_datetimeago_onesec` | Vor 1 s | yes |
| `clouddrive_desktop_datetimeago_manysecs` | Vor {0} s | yes |
| `clouddrive_desktop_datetimeago_onemin` | Vor 1 Min. | yes |
| `clouddrive_desktop_datetimeago_manymins` | Vor {0} Min. | yes |
| `clouddrive_desktop_datetimeago_onehour` | Vor 1 Stunde | yes |
| `clouddrive_desktop_datetimeago_manyhours` | Vor {0} Stunden | yes |
| `clouddrive_desktop_datetimeago_oneday` | Vor 1 Tag | yes |
| `clouddrive_desktop_datetimeago_manydays` | Vor {0} Tagen | yes |
| `clouddrive_desktop_activitywindow_title` | Upload-Vorgang läuft | yes |
| `clouddrive_desktop_activitywindow_processingitems_title` | In Bearbeitung | yes |
| `clouddrive_desktop_activitywindow_waitingitems_title` | Warten | yes |
| `clouddrive_desktop_activitywindow_remaining_items_format` | {0:N0} Elemente ({1:N0}) verbleibend | yes |
| `clouddrive_desktop_activitywindow_noactivity_header` | Es ist keine aktuelle Aktivität zum Anzeigen vorhanden. | yes |
| `clouddrive_desktop_activitywindow_noactivity_info` | Sie können gesicherte Fotos im Internet anzeigen und Ihre Änderungen für Familienmitglieder und Freunde freigeben. | yes |
| `clouddrive_desktop_activitywindow_noactivity_viewuploadsbuttontitle` | Uploads im Internet anzeigen | yes |
| `clouddrive_desktop_failedsyncitemswindow_title` | Fehlgeschlagene Elemente | **no** |
| `clouddrive_desktop_failedsyncitemswindow_retry` | Wiederholen | **no** |
| `clouddrive_desktop_retryall` | Alle erneut versuchen | yes |
| `clouddrive_desktop_retry` | Wiederholen | yes |
| `clouddrive_desktop_errorwindow_titleformat` | Fehler bei {0} | yes |
| `clouddrive_desktop_errorwindow_dismiss` | Verwerfen | yes |
| `clouddrive_desktop_errorwindow_notfounderrorcta` | Entfernen | yes |
| `clouddrive_desktop_errorwindow_overquotaerrorcta` | Upgrade vornehmen | yes |
| `clouddrive_desktop_errorwindow_sizeformat` | ({0}) | **no** |
| `clouddrive_desktop_close` | Schließen | yes |
| `clouddrive_desktop_cancel` | Abbrechen | yes |
| `clouddrive_desktop_backup_onetimebackupitem_title` | Einmalige Sicherung | yes |
| `clouddrive_desktop_errorbannerview_uploaderror` | Bei Ihrem Upload ist ein Fehler aufgetreten. | yes |
| `clouddrive_desktop_errorbannerview_uploaderrorplural` | Bei Ihrem Upload sind Fehler aufgetreten. | yes |
| `clouddrive_desktop_errorbannerview_showerrorwindow` | Fehler beim Hochladen anzeigen | yes |
| `clouddrive_desktop_statuswindow_clearcompletedbuttontitle` | Verlauf löschen | yes |
| `clouddrive_desktop_statuswindow_clearcompletedconfirmationheader` | Verlauf löschen? | yes |
| `clouddrive_desktop_statuswindow_uploads_clearcompletedconfirmationtextmessage` | Dadurch wird die Liste der vergangenen Uploads gelöscht. | yes |
| `clouddrive_desktop_statuswindow_downloads_clearcompletedconfirmationtextmessage` | Dadurch wird die Liste der vergangenen Downloads gelöscht. | yes |
| `clouddrive_desktop_statuswindow_canceluploadbuttontitle` | Upload abbrechen | yes |
| `clouddrive_desktop_statuswindow_retryfailedbuttontitle` | Neuversuch fehlgeschlagen | yes |
| `clouddrive_desktop_statuspopup_stats_remaining` | ({0:n0} verbleibend) | yes |
| `clouddrive_desktop_statuspopup_stats_remaininganderror` | ({0:n0} verbleibende und {1:n0} Fehler) | yes |
| `clouddrive_desktop_statuspopup_stats_error` | ({0:n0} Fehler) | yes |
| `clouddrive_desktop_statuspopup_stats_remaininganderrors` | ({0:n0} verbleibende und {1:n0} Fehler) | yes |
| `clouddrive_desktop_statuspopup_stats_errors` | ({0:n0} Fehler) | yes |
| `clouddrive_desktop_statuspopup_backingup` | Sicherung wird durchgeführt | yes |
| `clouddrive_desktop_statuspopup_backupcomplete` | Datensicherung abgeschlossen | yes |
| `clouddrive_desktop_statuspopup_backuppaused` | Sicherung angehalten | yes |
| `clouddrive_desktop_statuspopup_nobackups` | Keine Datensicherungen gefunden | yes |
| `clouddrive_desktop_statuspopup_notifications` | Benachrichtigungen | yes |
| `clouddrive_desktop_statuspopup_nonotifications` | Keine neuen Benachrichtigungen | yes |
| `clouddrive_desktop_statuspopup_notification_dismiss` | Schließen | yes |
| `clouddrive_desktop_onboarding_startbackup` | Datensicherung starten | yes |
| `clouddrive_desktop_contextmenu_hover_status_message` | Amazon Photos - {0} | yes |
| `clouddrive_desktop_contextmenu_hover_no_status_message` | Amazon Photos | yes |
| `clouddrive_desktop_contextmenu_message_getchanges` | Konfiguration wird durchgeführt | yes |
| `clouddrive_desktop_contextmenu_message_nonetwork` | Netzwerk nicht verfügbar | yes |
| `clouddrive_desktop_contextmenu_message_unreadnotifications` | Ungelesene Benachrichtigungen | yes |
| `clouddrive_desktop_contextmenu_message_transfercomplete` | Abgeschlossen | yes |
| `clouddrive_desktop_contextmenu_statuswindow` | Status anzeigen | **no** |
| `clouddrive_desktop_contextmenu_message_uploadpaused` | Angehalten | **no** |
| `clouddrive_desktop_contextmenu_message_uploadremaining` | Upload-Vorgang läuft ({0} Dateien verbleibend) | **no** |
| `clouddrive_desktop_contextmenu_message_uptodate` | Upload abgeschlossen | **no** |
| `clouddrive_desktop_contextmenu_message_overquota` | Kein Speicherplatz mehr | **no** |
| `clouddrive_desktop_contextmenu_uploaderrored` | {0} Dateien konnten nicht hochgeladen werden | **no** |
| `clouddrive_desktop_backupscreen_pauseallbackups` | Alle Sicherungsvorgänge anhalten | yes |
| `clouddrive_desktop_backupscreen_resumeallbackups` | Alle Sicherungsvorgänge fortsetzen | yes |
| `clouddrive_desktop_statuswindow_scheduletab_pauseschedule` | Datensicherung anhalten | yes |
| `clouddrive_desktop_statuswindow_scheduletab_resumeschedule` | Datensicherung fortsetzen | yes |
| `clouddrive_desktop_statuswindow_scheduletab_paused` | ANGEHALTEN | yes |
| `clouddrive_desktop_pause_allactivity` | Alle Aktivitäten anhalten | yes |
| `clouddrive_desktop_resume_allactivity` | Alle Aktivitäten fortsetzen | yes |
| `clouddrive_desktop_globaltoggle_all_activity_paused` | Alle Aktivitäten wurden angehalten | yes |
| `clouddrive_desktop_globaltoggle_all_activity_unpaused` | Alle Aktivitäten wurden fortgesetzt | yes |
| `clouddrive_desktop_backupdetailsview_drilldown` | Verschachtelte Anzeige | yes |
| `clouddrive_desktop_backupdetailsview_flatlist` | Flache Listenanzeige | yes |
| `clouddrive_desktop_auto_upload_toggle_text` | Neue Dateien automatisch hochladen | yes |

`yes*` = resolved **indirectly**: the key is a literal in `TransferEnumExtensions.UploadStatusStringMap`
(`Dictionary<TransferStatus, string>`) and read back through `Strings.Localize(map[status])` in
`GetUploadString()` — a live code path, but not a direct `Strings.<key>` property read.

**What this table settles and what it contradicts.**

- **`statuswindow_statusall` exists.** §3.9's claim that "there is **no** key literally named
  `statusall`" is **wrong**: the key is present in `de.lproj` with the German value "Alle", but it
  has **no** `Strings.` call site and no map entry — it is a genuinely dead key, and the live
  "all filter" key remains `…_statuswindow_filterall`. → *inferred* dead (absence of a caller is
  **belegt**)
- **`failedsyncitemswindow_*` has exactly two keys**, `title` and `retry` — there is **no** third
  key for a dismiss/skip action in that family. §3.6's *inferred*-dead reading is therefore the
  whole story of this family, and both keys are unreferenced. → *inferred* dead
- **There is no `activitywindow_pauseall`.** The activity-window key family (7 keys) contains no
  pause/stop key at all; the global stop is `…_pause_allactivity` / `…_resume_allactivity`, the
  backup pause is `…_backupscreen_pauseallbackups` / `…_backupscreen_resumeallbackups` plus the
  per-row `…_statuswindow_scheduletab_pauseschedule` / `_resumeschedule`. The §3.8 separation is
  **confirmed** by the string families. → **belegt**
- **The whole menu-bar message family except four keys is unreferenced.** `getchanges`,
  `nonetwork`, `unreadnotifications` and `transfercomplete` are read from code; `uploadpaused`,
  `uploadremaining`, `uptodate`, `overquota`, `statuswindow` and `uploaderrored` are not — no
  `Strings.` caller survives in this build. Whether a nib/xib binds them was not checked
  (the strings file is the only localisation artefact inspected here) → **UNKNOWN**.
- **`errorwindow_sizeformat` ("({0})") is unreferenced**, yet §3.6 reports `("(size)")` appended in
  the error-window row. The CTA keys (`overquotaerrorcta`, `notfounderrorcta`) and `titleformat`
  *are* referenced, so the surfaced row is real but its size suffix comes from elsewhere → the
  format key itself is *inferred* dead, the behaviour **belegt**.

---

## 5. Gap analysis vs. our app (committed state)

Our state was read from `src/` (grep + direct reads), in particular `src/backup.rs` (queue panel,
row menu, pause toggle), `src/upload.rs` (`UploadShared`, `FileState`, `status_text`,
`eta_secs`), `src/folders.rs` (`FolderStatus`, `ConflictResolution`), `src/dedup.rs`,
`src/config.rs`, `src/tray.rs`, `src/i18n.rs` and `src/app.rs`. → **belegt**

| Capability | Original | Our app | File reference |
|---|---|---|---|
| Details/status table columns | Name + Letzte Aktualisierung + Status (+ Größe in the legacy layout), fixed min widths, resizable | **fixed four-column header** — Ordner · Name · Größe · Status, virtualised list via `ScrollArea::show_rows` (no pagination) | `src/backup.rs` (queue panel: column header + `show_rows`) |
| Per-file size column | `…_statuswindow_sizecolumntitle`, hidden for folder rows | **Größe column per row** — `crate::text::format_size(e.size)` | `src/backup.rs` (queue file row) |
| Per-file last-modified | column + tooltip from `record.StatusChanged` | **none** — only the folder-level `last_updated` timestamp string | `src/folders.rs` (`BackupFolder::last_updated`), rendered in `src/backup.rs` |
| Relative time (`datetimeago_*`) | 12-branch "Vor {n} …" renderer, absolute date beyond a week | **none** — absolute `HH:MM` from `chrono::Local` | `src/backup.rs` (`chrono::Local::now().format("…%H:%M")` in `finalize_upload` / `finalize_background_store`) |
| Row tooltip (path + size + age) | composed from three strings | **partial** — full file path on the name (always); full folder path on the Ordner cell only when one is resolvable (never for the one-time target or an unresolvable folder index); a `x von y` byte detail on the Status cell only while the row is `Uploading` with a known size (`size > 0`); no age | `src/backup.rs` (queue file row, `on_hover_text`) |
| Sorting | status precedence then `SortDate`, plus click-to-sort by name in the legacy table | **status buckets** — In Bearbeitung → Fehlgeschlagen → Warten → Abgeschlossen → Übersprungen, insertion order within a bucket (user decision 2026-10-26; **not** the original key: no `SortDate`, no click-to-sort) | `src/backup.rs` (`queue_layout`) |
| Filtering | 7-item menu (6 + Skipped) or a 6-value view-model filter, client-side under a lock | **none** | — |
| Empty-state text per filter | separate upload/download key families | **none** | — |
| Pagination | 1000 items/page | **none** — and needed: the queue panel caps rendering at **300 rows** (`if shown >= 300 break`) | `src/backup.rs` (queue panel) |
| Drill-down / breadcrumb | nested vs flat mode persisted in customer settings; folder rows navigate | **none** — the queue is always a single flat, status-sorted list with no folder groups (the Ordner column is per row only) | `src/backup.rs` (`queue_layout`) |
| Auto-upload toggle in details | toggle + subtitle in the details header | **n/a** — `config.background_sync` covers it | `src/settings.rs`, `src/config.rs` |
| Details footer "Wiederholen" | enabled only when `FailedCount > 0`, publishes `RetryErroredUploadsIntent` | **partial** — a "Wiederholen" row-menu item exists but always restarts a scan-based run; there is no per-schedule/folder *failed-record* set to retry from | `src/backup.rs` (`FolderAction::Retry` → `start_folder_upload`) |
| Error window | own 600 × 400 window, one per schedule, headerless name-only table, per-row CTA + dismiss, "Alle erneut versuchen" + "Schließen" | **none** — the only error surface is the per-file red state in the queue panel plus a log line | `src/backup.rs` (queue panel), `src/upload.rs` (`FileState::Failed`) |
| Dismiss (skip) a failed file | `SkipUploadRecord` → leaves the failed set, recoverable via `readdtoqueue` | **none** | — |
| Re-add skipped to queue | `ResetSkippedUploadRecordToWaiting` | **none** — a skipped file is never re-offered | `src/upload.rs` (dedup skip paths) |
| Dismiss vs re-undo | dismiss is one-way; only `readdtoqueue` returns a row | **n/a** | — |
| "Verlauf löschen" | modal confirmation + `ClearCompleted()` | **none** | — |
| "Upload abbrechen" (cancel) | `CancelManualUploadsCommand` incl. multipart abort | **none** — a run cannot be cancelled mid-flight, only paused | `src/upload.rs` (`paused` watch) |
| Activity window | 550 × 391 window, "In Bearbeitung" / "Warten" collapsible sections with count+size info, three states incl. no-activity, Name/Größe/Status columns, cross-schedule | **none** — the closest analogue is the in-page queue panel | `src/backup.rs` |
| Global stop ("Alle Aktivitäten anhalten") | header hyperlink + `WorkersStateMachine.PauseAll(UserPaused)` + toast; *not* persisted; pauses **all** workers | **partial and conflated** — our one toggle IS the backup pause: `self.paused` is persisted in `config.toml` and blocks the background sync, i.e. it behaves like the original's *backup* pause, while the original's global stop is in-memory and worker-wide | `src/app.rs` (`paused: watch::Sender<bool>`), `src/config.rs` (`paused`), `src/backup.rs` (pause toggle) |
| Worker `Transitioning` disable | pause/resume controls disabled while a transition is in flight | **none** — our pause is a `watch` flip, instantaneous | `src/upload.rs` |
| `PauseSource` flags (system vs user) | distinct reasons coexist; filtered resume clears one bit | **none** — a single global bool | `src/app.rs` |
| Per-backup pause row icon | "Datensicherung anhalten" / "… fortsetzen" per schedule | **partial** — a per-row ⏸/▶ button exists but flips `Folder::enabled` (the schedule on/off), not a pause flag, and is only enabled while a run is active | `src/backup.rs` (`FolderAction::ToggleEnabled`) |
| Status popup | 450 pt popover, 3 rows, per-row icon/text/details-colour/CTA, stats suffix, notifications, quota footer | **none** — we have no status popup at all | `src/tray.rs` (3 items only: show / pause / quit) |
| Stats suffix `statuspopup_stats_*` | 5-way singular/plural × remaining/errors choice | **none** — the tray shows a single "Anhalten"/"Fortsetzen" label and the in-window log shows "Upload läuft: {} · {} · {}" / "Letzter Upload: {} erfolgreich" | `src/app.rs` (bottom log panel), `src/tray.rs` |
| Menu-bar hover text | `contextmenu_hover_*` + `contextmenu_message_*` family | **none** | `src/tray.rs` |
| Row CTA "Sicherung starten" (no backups) | `statuspopup_nobackups` + `onboarding_startbackup` | **partial** — "Datensicherung hinzufügen …" opens the config form, but there is no zero-state row | `src/backup.rs` (header) |
| Status vocabulary | 11 transfer statuses mapped to strings, incl. "Zuvor hochgeladen", "Wird überprüft", "Verworfen", "Achtung" | 5 states (`Waiting`/`Uploading`/`Done`/`Failed`/`Skipped`) → "Warten"/"In Bearbeitung"/"Abgeschlossen"/"Fehlgeschlagen"/"Übersprungen" | `src/upload.rs` (`FileState`), `src/backup.rs` (state labels) |
| i18n layer | `Localizable.strings` | central de/en table — present, but the status strings are ad-hoc `tr()` calls scattered across `src/backup.rs` / `src/upload.rs` | `src/i18n.rs`, `src/backup.rs` |

---

## 6. UNKNOWN

- **Which layout a real user sees.** Both `BackupsDetailTableView` (3 columns) and the legacy
  `BackupDetailsView`/`DetailTableView` (4 columns + thumbnails) exist; the selector between them
  (a flag on `DriveWindowViewModel`, a customer setting or a nib choice) was not traced. The CX
  flag `EnableCXUploadSimplification` is known to gate the *drag-and-drop* branch, and the details
  views reference it only for background colour — so the switch is elsewhere. → **UNKNOWN**
- **Refresh cadence of a visible age cell.** The table reloads on sampled collection changes
  (0.35 s) and DB-requery events, but no per-cell timer was found, so whether "Vor 3 Min."
  self-updates while the row is on screen is → **UNKNOWN**
- **Whether `failedsyncitemswindow_*` is truly dead.** The two strings and the
  `RetryFailedSyncItemsIntent` + handler exist; no managed publisher of either was found.
  → *inferred* dead / possibly nib-only, but a reflection or cross-assembly caller is
  → **UNKNOWN**
- **Whether `ShowActivityWindowIntent` has any publisher.** Only the intent class and its handler
  exist in the assembly; no `Publish(new ShowActivityWindowIntent…)` call site was found. So how a
  user reaches the activity window is → **UNKNOWN** (*inferred*: it is opened from a window-control
  button whose binding lives outside the decompiled code, or it is unreachable in this build).
- **`IAutoFixErrorService.TryFixError` body** — the three auto-fixable errors are known; what the
  fix actually does (open storage management, delete the record) is → **UNKNOWN**
- **`CancelManualUploadsCommand` body** — the dependencies (incl. an abort-multipart-upload service)
  are **belegt**, the exact steps are → **UNKNOWN**
- **Persistence of the selected filter / page / sort direction** — nothing was found that stores
  them; *inferred* not persisted, but → **UNKNOWN**
- **Whether the error window is per-schedule or per-backup in practice** — the dictionary is keyed
  by the string `scheduleId`, and the one-time queue always passes the literal `"ManualUpload"`, so
  the *one-time* error window is a single shared window; whether the UI ever opens two error
  windows at once is → **UNKNOWN**
- **The download row's stats binding in the popup** — the download stats object is constructed but
  not part of the observed `CombineLatest`; whether the row refreshes through another path is
  → **UNKNOWN**
- **`RecordItem.GetWebLink()` branches** — a `TargetType` switch with `Folder`/`File` cases
  exists; the concrete URLs were not read, and neither matters for the port → **UNKNOWN**
- **Whether a paused (suspended) record is counted anywhere in the popup suffix** — by
  construction `ToStatusPopupSummary` uses `InProgress + Queued` only, so suspended items are
  invisible in the suffix; whether the row *text* ("Sicherung angehalten") is the only hint is
  → **belegt** for the suffix, **UNKNOWN** for intent.
- **The `…_mac_notification_statuswindowclosedfirsttime_title` text** — the `_message` key is
  present in `de.lproj`, the `_title` key is not; the rendered German title is → **UNKNOWN**
  (*inferred*: base-localisation or nib fallback).

---

## 7. Cross-references

- `UI_FLOWS.md` §4.7 (details view, nested vs flat, item-table columns), §9.1 (status popup),
  §11.2 (status precedence table), §11.4 (the two pause mechanisms), §11.5 (completion / retry).
- `UI_STATES.md` §3.1 (row model), §3.2 (status vocabulary), §3.3 (aggregated row state),
  §3.6 (details view: nested vs flat), §3.8 (DB fields behind the backup screen), §5.1 (status
  popup), §5.2 (activity window).
- `SPEC_UPLOAD_PROGRESS.md` — **progress texts, ETA, byte counters, "Upload-Vorgang läuft"**.
  Everything about *how fast* and *how far* belongs there, not here.
- `SPEC_BACKGROUND_SYNC.md` §7 — the **backup-pause** mechanism (schedule flags, forced scans
  on resume, watcher removal, `ProcessScanRequest`); this document only contrasts it with the
  global worker stop in §3.8.
- `SPEC_NOTIFICATIONS.md` §2.4 (status icon), §3.2 (event → notification map), §3.3 (the three
  notification surfaces) — the popup's notification list is specified there, not here.
- `SPEC_ONE_TIME_UPLOAD.md` §5.2/§5.3 — the one-time queue's row model and its
  `OneTimeBackupItemsViewModel` (the `"ManualUpload"` schedule id used by the error window).
- `SPEC_DEDUP.md` §1 (MD5 identity) and §4.1 (conflict policy) — why a record can end up
  skipped rather than failed.
- `SPEC_CHANGE_DETECTION.md` — the `max mtime` watermark decision; unrelated to the status
  table but shares the "suspended vs completed" vocabulary.
- Our code: `src/backup.rs` (`backup_ui`, `start_folder_upload`, `start_one_time_upload`,
  `finalize_upload`, queue panel, row menu), `src/upload.rs` (`UploadShared`, `FileState`,
  `status_text`, `eta_secs`, `upload_progress_text`), `src/folders.rs` (`FolderStatus`,
  `ConflictResolution`), `src/dedup.rs`, `src/config.rs` (`paused`), `src/tray.rs`,
  `src/app.rs` (`update`, pause watch, bottom log panel), `src/i18n.rs`.
