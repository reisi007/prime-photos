# SPEC_BACKGROUND_SYNC — Background / automatic upload (Clean-Room Spec)

Behavioural specification of the **original** Amazon Photos desktop client's
*automatic / periodic backing-up*: what starts a scan, how the cadence is
configured, how the automatic upload reacts to filesystem changes, to a paused
backup and to the menu-bar background, and how all of that surfaces in the UI.

This document is **clean-room**: it records *observed behaviour* in our own words
and tables. It contains **no verbatim decompiled code** and **no personal data**.
German user-facing labels are quoted from the shipped
`de.lproj/Localizable.strings` (they are product strings, not code). Account, user,
device and path values are omitted.

Scope boundary: this doc owns the *scan/upload trigger and cadence* model.
Duplicates are owned by `SPEC_DEDUP.md`, folder scan/filter/persistence by
`SPEC_FOLDERS.md`, menu-bar presence + notifications by
`SPEC_NOTIFICATIONS.md`, the interval/filter UI by `UI_FLOWS.md` §4.5/§4.7
and `UI_STATES.md` §3/§4.

---

## 1. Sources (all read-only)

| Item | Value |
|---|---|
| Target | `/Applications/Amazon Photos.app` (never launched/modified) |
| Managed code | `Contents/MonoBundle/Amazon Photos.dll` via `ilspycmd` |
| App-wide tunables | `Contents/…/app-settings.json` (shipped defaults) and `~/Library/Application Support/Amazon Cloud Drive/app-settings.json` |
| Per-schedule state | `~/Library/Application Support/Amazon Cloud Drive/<account>-schedules.db` (table `schedules`) |
| German labels | `Contents/Resources/de.lproj/Localizable.strings` |
| Existing notes reused | `UI_FLOWS.md` §4.5–4.7, §9, §11; `UI_STATES.md` §3, §4.2, §4.4; `SPEC_FOLDERS.md` §3; `SPEC_NOTIFICATIONS.md` §2 |

**Confidence convention.** *observed* = read directly off one of the sources above.
*inferred* = reasoned from adjacent evidence. **UNKNOWN** = could not be determined
from the inspected sources. Per point, §9 lists belegt vs. UNKNOWN.

Relevant symbols (named only to describe behaviour): `BackupScannerWorker`,
`BackupScanRequest`, `BackupScanType`, `ScanRequestState`, `BackupWatcherWorker`,
`ObservableFileSystemWatcher`, `BackupFileChangeDetector`, `IntervalExpression`,
`IntervalOption`, `BackupItemInterval`, `Clock.IsNonBusinessHours`, `GetNextUploadTime`,
`TickTockTimer`, `RunLoopWorker`, `AppSettings`, `WorkerTask<TWorker>`.

---

## 2. The scan/upload pipeline (architecture, observed)

Two long-lived workers do the automatic work; both derive from `RunLoopWorker`
(the run loop is polled and self-healing: `RunLoopWorker.VerifyRunLoopIsRunning`
runs every `AppSettings.WorkerRunLoopCheckInSeconds`).

| Worker | Role | Run context |
|---|---|---|
| `BackupWatcherWorker` | Turns native filesystem events into change notifications; owns one watcher per schedule folder. | `[RunContext(Mode = AppMode.Backup, Frequency = RunFrequency.Always)]` — runs whenever the app is in Backup mode. |
| `BackupScannerWorker` | Owns the scan **request queue**, decides *when* a scan runs, and calls the enumeration (`QuickScan` / `DeepScan`). | no explicit `RunContext` (defaults) — started once from the startup list. |

Both are launched by the startup task list (`StartupTaskManager` contains
`WorkerTask<BackupWatcherWorker>` and `WorkerTask<BackupScannerWorker>`), i.e. at
app start, after the session exists — *observed*.

```
FSEvents  →  ObservableFileSystemWatcher  →  BackupWatcherWorker
              (per folder, latency 3 s)       ├─ file event → EnumerationService.ProcessFileEvent
                                              └─ folder event → debounce 5 s → scan request
periodic timer (24 h) ─┐
device arrival/removal ─┼→ BackupScannerWorker queue (PriorityQueue) → RunScan
UI actions (scan now…) ─┘                                             ├─ QuickScan  (incremental enumerate)
                                                                      └─ DeepScan   (full re-enumerate; non-business hours only)
```

The **AWS-side** "cold boot" (`GetChanges` → `cloud.db` node cache, surfaced as
"Konfiguration wird durchgeführt") is a *separate* mechanism from the local folder
scan; this doc does not claim they are coupled (*observed*: different services).

---

## 3. Scan triggers (observed)

`BackupScannerWorker` holds a `PriorityQueue<BackupScanRequest>` and dequeues a
request only when it is *due* (`HasRequestsToTrigger`). There are five sources:

| # | Trigger | Where | Flags |
|---|---|---|---|
| 1 | **Periodic check** | `BackupScannerWorker.RequestPeriodicScans`, driven by `Timer.TickEvery(BackupScannerCheckIntervalInHours)` | per schedule, only if overdue; `DeferUploads=true`, no ForceScan |
| 2 | **Filesystem event** in a watched folder | `BackupWatcherWorker.HandleFileSystemChange` → `EnumerationService.ProcessFileEvent` | file context |
| 3 | **External-drive arrival** | `BackupScannerWorker.HandleDeviceArrival` | `ForceScan = !IsPaused` |
| 4 | **UI / intent actions** | `ScanAndUploadNowScheduleIntentHandler`, `ResumeBackup(s)IntentHandler`, `CreateDefaultBackupActionIntentHandler`, `ConfigureBackupActionIntentHandler`, `ChangeBackupIntervalOptionIntentHandler` | mostly `ForceScan=true` |
| 5 | **Enumeration requeue** | `EnumerationService.OnNeedsRequeue` (cancelled/failed scan, photo-library, deep-scan MD5 mismatch) | re-queued with `StartDelay` |

### 3.1 The periodic check in detail (observed)

`RequestPeriodicScans` runs on the shared `TickTockTimer` — a global 3-second
heartbeat (`TickTockTimer` constructor) whose `TickEvery` fires when
`tick > lastRun + interval`. Because the first tick (~3 s after startup) already
exceeds the interval, **the periodic check runs about once at app start and then
every `BackupScannerCheckIntervalInHours` hours** — *observed for `TickEvery`; that
the startup tick therefore triggers the check is *inferred* (the arithmetic is
observed, the concrete first-scan consequence is not directly logged in a source
we read)*.

For every schedule it compares two timestamps:

| Condition | Action |
|---|---|
| `LastScanTime <= now − BackupScanIntervalInHours` | enqueue `BackupScanRequest(record)` (QuickScan, `DeferUploads=true`) |
| `LastDeepScanTime <= now − BackupDeepScanIntervalInHours` | enqueue `BackupScanRequest(record, DeepScan)` |

So the periodic check does **not** scan unconditionally: a schedule whose
`last_scan_time` is recent is left alone. This is the meaning of the name
`BackupScanIntervalInHours` — the *quick-scan staleness threshold*, distinct from
the user-facing "Änderungen hochladen" cadence (`UI_STATES.md` §4.2).

### 3.2 Deep scan only in "non-business hours" (observed)

A `DeepScan` request is only *due* when `Clock.IsNonBusinessHours()` is true in
addition to its due date. `Clock.IsNonBusinessHours` returns `true`
- on **Saturday/Sunday** (always), or
- on weekdays outside `[07:00, 19:00)` — the evening/morning window from
  `NonBusinessHoursOffsetInHours` (default **19**, clamped to `[17, 22]`; the class
  subtracts a 12-hour window, hence the lower bound 07:00).

### 3.3 Request ordering (observed)

`ScanRequestComparer` orders by `BackupScanType` (QuickScan before DeepScan), then
`DequeueDate`, then `LastScanTime`. A duplicate request (same record + same scan
type) is dropped ("AlreadyQueued").

### 3.4 Request pre-checks (observed)

`ProcessScanRequest` runs *before* enqueue and again before running:

- external drive changed → update its path; gone → `ScheduleError.ExternalDriveMissing`;
- missing destination node id → resolve/create via `BackupPathMapper`;
- destination node deleted → `ScheduleError.DestinationNodeNotAvailable`;
- local path missing → `ScheduleError.LocalFolderNotFound`.
On a terminal schedule error the watcher for that path is removed and the schedule's
uploads are suspended (`PauseSource.SystemPaused`).

### 3.5 `ForceScan` semantics (observed)

`RunScan` applies `ForceScan` before scanning:

1. if the schedule is paused, unpause it (`IsPaused = false`);
2. `ResumeUploadsForSchedule` (lift `ScheduledPaused` suspension);
3. `RetryErrored` for that schedule;
4. `RetryNonCompletedRecords`.

Timestamp writes: QuickScan sets `last_scan_time` **and** `last_enumeration_time`;
DeepScan sets `last_deep_scan_time`. Fields persist in `schedules` (`SPEC_FOLDERS.md`
§3.1).

---

## 4. Time & interval semantics

### 4.1 Two independent clocks (observed)

| Clock | Key (default) | Meaning |
|---|---|---|
| Periodic check cadence | `BackupScannerCheckIntervalInHours` (**24**) | how often the scanner re-evaluates overdue schedules |
| Quick-scan staleness | `BackupScanIntervalInHours` (**336** = 14 d) | a schedule is quick-scanned when its last scan is at least this old |
| Deep-scan staleness | `BackupDeepScanIntervalInHours` (**168** = 7 d) | full re-enumerate threshold (non-business hours only) |
| Scanner run-loop poll | `AutoSaveSchedulePollIntervalInSeconds` (**60**) | wait between empty queue polls (`BackupScannerWorker.PollInterval`) |
| Watcher run-loop poll | `BackupWatcherPollIntervalInSeconds` (**30**) | wait between empty queue polls (`BackupWatcherWorker.PollInterval`) |
| Reupdate/cloud clock | `ReupdateIntervalInHours` (**168**) | server-driven node re-update, separate task |

The shipped `app-settings.json` in the inspected install contains
`BackupScanIntervalInHours = 336` — *observed*. The other values above are the
**code defaults** in `AppSettings` (the DB-backed `AutoGet(default, key)`); whether
this install ever overrides them was not established.

### 4.2 The user-facing cadence — `IntervalOption` (observed)

The per-schedule cadence lives in `schedules.interval` as a cron-shaped
`IntervalExpression` (7 fields). The UI offers a closed set; the exact German labels
and their meanings are in `UI_STATES.md` §4.2. Behaviourally relevant here:

| Option | Trigger shape | Upload timing |
|---|---|---|
| `Immediate` ("Sofort") | `CreateSecondsTrigger(0, 0)` — every second, continuous | **no deferral** — uploads as soon as discovered |
| `FiveMinutes` … `SixHours` | fixed minute/hour steps | deferred to the next slot |
| `OneDay` / `OneWeek` | daily/weekly at a chosen time (weekday for weekly) | deferred to the next slot |
| `Never` | matches only a fixed past instant | effectively "no automatic upload" |

**Deferred uploads** (observed): `BackupScanRequest.DeferUploads` defaults to
`true`. When a QuickScan has `DeferUploads=true` and items are found, the scan
resolves a next-upload time: `GetUploadTimeForDeferredUploads`, or — if that is in
the past — `ScheduleRecordExtensions.GetNextUploadTime(now)`. `GetNextUploadTime`
returns **`null` for the `Immediate` expression** (→ upload now) and otherwise the
next cron occurrence after the record's creation time. Deferred records are saved
with `RetryReason.DeferredUploads`, i.e. they sit queued until their slot.

`ForceScan` (scan-now, resume, create, interval change) always uses
`DeferUploads=false`, which is why those paths upload immediately regardless of the
cadence.

### 4.3 Global default cadence (observed)

Two customer settings seed new backups: `DefaultOnboardingIntervalOption` and
`DefaultPostOnboardingIntervalOption` — both default **`Immediate`**. A global
"last chosen interval" is persisted as the customer-settings key `BackupItemInterval`
(default = the `Immediate` expression).

---

## 5. The "Neue Dateien automatisch hochladen" toggle (observed)

Strings (`de.lproj/Localizable.strings`):
- title `clouddrive_desktop_auto_upload_toggle_text` = "Neue Dateien automatisch hochladen";
- sub-text `…_auto_upload_toggle_subtitle_text` = "Zukünftige Dateien oder
  Dateiänderungen werden automatisch als neue Dateien in die Cloud hochgeladen";
- help tooltip `…_auto_upload_toggle_tooltip_text` = "Wenn „Neue Dateien automatisch
  hochladen" aktiviert ist, werden neue Fotos und Videos, die dem lokalen Ordner
  hinzugefügt wurden, sofort auf dein Amazon Photos-Konto hochgeladen.".

**It is not a separate flag** — it is a *view onto the interval option*:

| Control | Location (feature-flag layout) | On | Off |
|---|---|---|---|
| `AutoUploadToggle` (switch) | backup **details** header, only when `EnableCXUploadSimplification` (UI_FLOWS §4.7) | `IntervalOption.Immediate` | `IntervalOption.Never` |
| `BackupView_SettingsAutoUpload_Checkbox` | backup **configuration** window, CX layout | `Immediate` | `Never` |

- `AutoUploadToggle` shows an icon (`ImageType.Repeat`), the title, the switch and a
  help tooltip; it **starts disabled** (`Enabled = false`) and is enabled by the
  view model — *observed* in `AutoUploadToggle`.
- Flipping it publishes `ChangeBackupIntervalOptionIntent` (details) / is folded
  into the save action (config window). The row tooltip "Der automatische Upload ist
  aktiviert/deaktiviert" (`…_schedules_item_tooltip_autoupload_on` / `_off`) and the
  folder icon (`ClosedFolderAutoUpdate` vs `ClosedFolder`) track the same distinction.
- In the **non-CX** configuration layout the equivalent control is the interval
  selector "Änderungen hochladen:" (`clouddrive_desktop_backupconfiguration_selectintervaltitle`),
  plus the weekly "… aktiviert" / "um" pair — no separate auto-upload checkbox.

### 5.1 Changing the interval while items are queued (observed strings)

`ChangeBackupIntervalOptionIntentHandler` (execute): cancel the running enumeration
for that schedule, then `Interrupt<BackupUploaderWorker>(PauseSource.UserPaused)` to
suspend its uploads, write the new interval, unpause the record, save. Then:
- `Never` → persist a next-upload time, no scan;
- otherwise → `ResetDeferredUploadsForSchedule` and `RequestScan(DeferUploads=false, ForceScan=true)`.

The dialog strings exist for "you changed the plan while items are queued":
`clouddrive_desktop_backupconfiguration_backupintervalchanged_dialogtitle` =
"Elemente in der Warteschlange", `…_dialogmessage_confirmation` = "Möchten Sie die
Elemente in der Warteschlange jetzt oder später hochladen?",
`…_dialogmessage_nextuploadtime` = "Nach dem neuen Plan werden Elemente in der
Warteschlange um {0} auf {1} hochgeladen.",
buttons `…_dialogbutton_uploadimmediately` = "Jetzt hochladen" /
`…_dialogbutton_uploadlater` = "Später".

*(Which exact control shows this dialog in the CX layout — the selector or the
checkbox — was not traced; the strings and the handler are observed.)*

---

## 6. Filesystem watching (observed)

`BackupWatcherWorker` keeps a `Dictionary<path, (watcher, subscription)>`, one entry
per normalized schedule path (trailing slash stripped, `PathEx.NormalizePathWithoutTrailingSlash`).

- **Watcher implementation** — on macOS a native **`FSEventStream`** per folder
  (`ObservableFileSystemWatcher`): flags `IgnoreSelf | FileEvents`, a **3-second
  latency**, events grouped by path and throttled **100 ms**, scheduled on the main
  run loop, then started. (`ObservableFileSystemWatcherFactory.CreateWatcher` simply
  constructs it; a missing folder throws `FileNotFoundException`/`ArgumentException`.)
- **Registration** — `BackupWatcherWorker.Start` walks `ListSchedules()` and adds a
  watcher per folder, **skipping paused schedules** and handling disconnected drives
  and missing destination nodes first. A watcher creation failure raises
  `ScheduleError.LocalFolderNotFound` (`WatcherOnErrorSubject`).
- **Change filter** — `HandleFileSystemChange` only reacts to `Move`, `Rename`,
  `Create`, `Modify`; the target must still exist (`_fileSystem.Exists`); ignored
  paths (`_fileFilter.IsIgnored(..., allowEdits: true)`) are dropped.
- **Debounce** — folder events are grouped by path and throttled **5 seconds**
  (`Throttle(FiveSeconds)`), keeping only the last; a queued path suppresses any
  queued path it is a prefix of. The watcher's own run loop drains one queue entry
  per pass (poll interval `BackupWatcherPollIntervalInSeconds`, default 30 s).
- **Routing** — a *folder* event enqueues a folder scan; a *file* event inside a
  photo library goes through the debounce, otherwise it is published as a
  `BackupWatcherChange` that `BackupScannerWorker.HandleFileEvent` forwards to
  `EnumerationService.ProcessFileEvent` (an incremental per-file handling, no full
  scan).
- **Re-arm** — on watcher completion/error the subscription re-adds the watcher for
  the same path (self-healing).

**What "changed" means** is decided by `BackupFileChangeDetector.DetectChanges`:
`Size`, `ModifiedDate`, and a `SystemFileIdentifier` (device + file id) comparison.
Which of these count as a change is configured per scan type via
`QuickScanChangeIndicatorCombinations` / `DeepScanChangeIndicatorCombinations`
(app-settings; defaults exist as `Constants.Default*`). If neither list is
configured the detector returns `false` and emits the
`BackupFileChangeDetector.InvalidOrMissingIndicators` health metric.

---

## 7. Pause interaction (observed)

Two independent pause mechanisms (also in `UI_FLOWS.md` §11.4):

| Mechanism | Scope | Storage | Effect on background sync |
|---|---|---|---|
| "Alle Sicherungsvorgänge anhalten" (`PauseAllBackupsIntent`) | all schedules | `schedules.paused = 1` for all | pauses per-schedule; watchers for paused folders are not (re)created |
| per-backup pause ("Datensicherung anhalten", `PauseBackupItemIntent`) | one schedule | `schedules.paused = 1` | ditto |
| "Alle Aktivitäten anhalten" (global worker) | all workers | worker state machine | parks the uploader; menu-bar shows Paused |

- Pause/resume go through `IWorkersStateMachine.Interrupt<BackupUploaderWorker>(PauseSource…)`,
  so they are **not instantaneous**; while `Transitioning` the controls are disabled.
- Pausing writes `ScheduledPaused` as the suspension reason on already-queued upload
  records (`ScheduledReason` in `SPEC_DEDUP`/`SPEC_FOLDERS` terms) — those items stay
  queued and are cleared on resume.
- `ResumeAllBackupsIntentHandler` / `ResumeBackupItemIntentHandler` clear the pause
  flag and then `RequestScan(ForceScan = true)` per schedule, which also
  `RetryErrored`s.
- A paused schedule does **not** get a filesystem watcher at startup, and
  `ProcessScanRequest`/`HandleDeviceRemoval` remove the watcher and suspend uploads
  when the source disappears.

---

## 8. Background / menu-bar interplay (observed)

The client is a menu-bar app (`LSUIElement=true`, `ActivationPolicy=Accessory`,
`ApplicationShouldTerminateAfterLastWindowClosed=false`) — details in
`SPEC_NOTIFICATIONS.md` §2.1/§2.6. Consequence for background sync:

- Closing the main window does **not** stop the app; the workers and their watchers
  keep running in the background, and the only user-visible surface is the status
  item (`SPEC_NOTIFICATIONS.md` §2.4 icon/tooltip map: `Processing`, `Paused`,
  `Synced`, `Errored`, `NoNetwork`).
- The status popup (§9.1 in `UI_FLOWS.md`) mirrors the background state as three
  rows (Backup / Download / Sync) with a remaining/error stats suffix; the Backup
  row's call-to-action becomes "retry errored" when failures exist.
- There is **no separate background helper, LaunchAgent or XPC uploader** — the
  only LaunchAgent is a crash-recovery relauncher with `KeepAlive=true`
  (`SPEC_NOTIFICATIONS.md` §2.2).
- The original shows **no native "upload finished" toast**; completion/errors appear
  only via the status icon, tooltip and popup (`SPEC_NOTIFICATIONS.md` §3.2).

---

## 9. Belegt vs. UNKNOWN

**Belegt (observed from the DLL / shipped settings / strings):**

- Both workers and their `RunContext`/startup wiring (`WorkerTask<BackupWatcherWorker>`,
  `WorkerTask<BackupScannerWorker>`).
- All tunables and defaults: `BackupScannerCheckIntervalInHours=24`,
  `BackupScanIntervalInHours=336` (also present in the shipped `app-settings.json`),
  `BackupDeepScanIntervalInHours=168`, `AutoSaveSchedulePollIntervalInSeconds=60`,
  `BackupWatcherPollIntervalInSeconds=30`, `ReupdateIntervalInHours=168`,
  `NonBusinessHoursOffsetInHours=19`.
- `RequestPeriodicScans` threshold logic, `HasRequestsToTrigger` (QuickScan due by
  date; DeepScan additionally requires `IsNonBusinessHours`), request comparator and
  the "AlreadyQueued" drop.
- `IsNonBusinessHours` weekend + evening/morning window and the `[17,22]` clamp.
- `RunScan` timestamp writes and the four `ForceScan` side effects.
- `DeferUploads` default `true`; `GetNextUploadTime` → `null` for the `Immediate`
  expression; `RetryReason.DeferredUploads`.
- `ImmediateIntervalExpression = CreateSecondsTrigger(0, 0)`.
- FSEvents watcher: 3 s latency, `IgnoreSelf|FileEvents`, 100 ms per-path throttle,
  5 s folder debounce, watched change types, existence/ignore filters, re-arm.
- `BackupFileChangeDetector` change indicators and the missing-indicator metric.
- Auto-upload toggle ↔ interval mapping (Immediate/Never) in both the details view
  and the CX config window; the German strings for toggle, tooltip, subtitles and
  the "Elemente in der Warteschlange" dialog.
- `ChangeBackupIntervalOptionIntentHandler` behaviour and pause semantics.
- Device arrival/removal handling (watcher add/remove, `ExternalDriveMissing`,
  pause+suspend).

**Inferred (reasoned, not read off a line):**

- That the startup tick of the 24 h timer actually fires the periodic check right at
  app start (the `TickEvery` arithmetic is observed; the concrete first-scan effect
  is derived from it).
- That no other startup task additionally seeds scans for all schedules (only the
  five trigger sources in §3 were found; a hidden boot scan cannot be fully excluded).
- That scans are coupled to network availability only through the uploader/worker
  state machine, not through the scanner itself.

**UNKNOWN (explicitly not determined):**

- Whether a scan is *skipped or deferred* while offline — the scanner's own network
  guard was not found; the uploader/tooltips do react to "Kein Netzwerk".
- Whether sleep/wake or a dropped FSEvents stream triggers a catch-up scan (no wake
  handler was traced; only the re-arm-on-error path is observed).
- Behaviour on sign-out / account switch (are watchers and the periodic timer torn
  down?) — not traced.
- The exact trigger of the "Elemente in der Warteschlange" dialog in the CX layout
  (strings + handler observed; the precise control binding not).
- What the non-CX configuration window shows *and* whether its auto-upload
  checkbox is reachable (the CX variant is the observed one; the non-CX layout uses
  the interval selector).
- The concrete `QuickScanChangeIndicatorCombinations` /
  `DeepScanChangeIndicatorCombinations` values as resolved at runtime (defaults are
  `Constants.Default*`; the A/B treatment resolution was not traced).
- Whether the app scans on battery / is throttled by power state (nothing found).
- Non-macOS watcher backends (the observed watcher is macOS `FSEventStream`; the
  client is macOS-only, so other OSes are out of the inspected scope).
- The exact `RunContext` teardown semantics for `BackupScannerWorker` (default
  `Mode=All, Frequency=FirstTime`) vs. mode switches — attribute observed, re-run
  behaviour not.

---

## 10. Mapping to our app (plan only — do NOT implement here)

Our app (Rust/egui+eframe) today has **manual-only** backups: `src/folders.rs`
persists `BackupFolder { path, name, enabled, dedup, conflict_resolution,
total_files, uploaded_files, last_updated, status }` (`enabled` is stored but unused),
`src/upload.rs` runs `run_upload` only when the user triggers it, the global pause is
a `tokio::sync::watch::Sender<bool>` (`src/app.rs`), the tray (`src/tray.rs`) offers
"Fenster anzeigen / Anhalten|Fortsetzen / Beenden", close-to-tray already hides the
window while the app keeps running (`src/app.rs`), `src/notify.rs` sends a
run-finished notification behind `config.notifications`, and `src/dedup.rs` holds the
persistent path+MD5 index. `SPEC_FOLDERS.md` §2.6/§4.1 already records "no
background scheduler" as the gap. PORT-5 closes exactly that gap.

### 10.1 What PORT-5 concretely needs

| # | Need | Where (target file) | Concretely |
|---|---|---|---|
| 1 | **Per-backup auto-upload flag** | `src/folders.rs` (`BackupFolder`) + `src/backup_config.rs` | Add `auto_upload: bool` with `#[serde(default)]` (old `folders.toml` must still load). Wire the currently-disabled "Neue Dateien automatisch hochladen" field in the shared config form to it. This is the *minimal* cadence model: On ≈ the original's `Immediate`, Off ≈ `Never`. |
| 2 | **Filesystem watcher** | new module (e.g. `src/sync.rs`) | Use the cross-platform `notify` crate (FSEvents on macOS) to watch each enabled + `auto_upload` folder. Mirror the original's shape: per-path **debounce ~5 s**, react only to create/modify/rename/move, ignore paths that no longer exist. |
| 3 | **Scan on event** | `src/sync.rs` + existing `folders::scan_images` | On a debounced change, rescan the folder through `scan_images` and enqueue only files not already in `src/dedup.rs`. Never bypass the dedup pre-upload check. |
| 4 | **Periodic fallback re-scan** | `src/sync.rs` | FS events can be missed (sleep/wake, network volumes), so also run a timer in the tokio runtime (e.g. a "check interval" analogous to `BackupScannerCheckIntervalInHours`; start with a short poll). This is our stand-in for both the periodic check and the run-loop poll. |
| 5 | **Trigger set** | `src/app.rs` / `src/main.rs` | Start the scheduler after login (no scans while signed out), on app start, on FS event, on the periodic timer and on the existing manual start. `ForceScan`-style "scan now" == our current per-row "Hochladen". |
| 6 | **Background run via the tray** | `src/tray.rs` + `src/app.rs` | The scheduler must run in the `tokio::Runtime` independent of egui's repaint loop (our UI is on-demand). Close-to-tray already keeps the process alive; the tray already exposes the global pause. The original's menu-bar *icon states* (processing/paused/done/error) are a later nicety (PORT-3 already ships a static glyph). |
| 7 | **Pause / resume interaction** | `src/upload.rs` | Reuse the existing global `watch::Sender<bool>`: while paused, do **not** start a new background run, and the running `upload_worker` already parks (its pause race is `SPEC_FOLDERS.md` §2.7). Optionally add a per-backup pause later (the original has both; our MVP only needs the global one). |
| 8 | **Dedup interaction** | `src/dedup.rs` | A background scan must consult the same index as a manual run (same path + MD5 → skip) so idle re-scans upload nothing. No separate index. |
| 9 | **Untouched / explicitly out of scope for PORT-5** | — | Per-backup cloud destination node, content filter, deep cloud scan, external-drive AutoConnect, the menu-bar status popup, the full `IntervalOption` set (5-min … weekly). Start with the boolean auto-upload; an explicit interval enum can follow if the user wants it. |
| 10 | **Notifications** | `src/notify.rs` | The original sends **no** "backup finished" toast. We already do (behind `config.notifications`). Open decision: keep it for background runs too, or suppress auto-run notifications. |
| 11 | **i18n** | `src/i18n.rs` | Every new string (toggle label, sub-text, tooltip, status/log text) goes in de+en — no hardcoded UI text (`AGENTS.md` i18n rule). |

### 10.2 Deliberate divergences to record

- **No periodic "every 14 days" staleness threshold** as the primary driver. We use
  *event-driven + short periodic fallback*, because that is what makes "new files
  upload automatically" actually feel automatic. The original's 336 h/168 h numbers
  are its *safety-net re-scan* cadence, **not** its freshness mechanism — freshness
  comes from FSEvents.
- **Boolean toggle first, cron cadence later.** The original models cadence as a
  cron `IntervalExpression`; we start with `auto_upload: bool` (the user-visible
  toggle is exactly that) and only add an interval enum if needed.
- **Single global pause** for the MVP; the per-backup pause is a follow-up.
- **No crash-recovery LaunchAgent / no separate helper** — same decision as
  `SPEC_NOTIFICATIONS.md` §5.

---

## 11. Redaction note

No account identifiers, user names, device/serial values, absolute user paths or
verbatim decompiled code are reproduced. German user-facing labels are quoted from
the shipped `Localizable.strings`. The target app was only read (plist/strings/
read-only decompilation of the managed assembly and its shipped settings) and was
never launched, signed or modified. `SPEC_BACKGROUND_SYNC.md` is the only file
created or changed by this task.
