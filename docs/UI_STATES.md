# Amazon Photos macOS Desktop App — UI State & Data Model

Companion to [`UI_FLOWS.md`](./UI_FLOWS.md). This file describes **what is shown when** on
each screen, the enumerations that drive the UI, the exact display strings, and the
persisted fields (local SQLite tables / JSON settings) that back each state.

Same conventions as `UI_FLOWS.md`: **observed** = from the decompiled assembly + the German
`Localizable.strings`; *inferred* = read off view-construction code and not confirmed at
runtime; all examples use placeholders (`<account-id>`, `<user-name>`, `<path>`).

Sources:

- `Amazon Photos.dll` (decompiled) — enums, view-models, formatters.
- `de.lproj/Localizable.strings` — the German display strings.
- `~/Library/Application Support/Amazon Cloud Drive/` — the persisted state.

---

## 1. The three-layer state model

The UI never reads the databases directly. It observes a chain of reactive state objects,
and the DB is only the persistence layer behind them. *observed*

```
SQLite tables  ──►  domain services  ──►  transfer-state objects  ──►  view-models  ──►  AppKit views
(backup/uploads)    (Schedules,           (AggregatedTransferState,    (BackupItemStatus   (labels, icons,
                     Enumeration)          AggregatedWorkerState,       ViewModel, …)       colours)
                                           ScheduleError, stats)
```

Key state objects:

| Object | Meaning |
|---|---|
| `AggregatedWorkerState` | Global worker machine: `Transitioning`, `Running`, `Paused`, `Stopped`, `ForceStopped` |
| `AggregatedTransferState` | Per-transfer roll-up: `Unknown`, `Preparing`, `InProgress`, `Completed`, `Failed`, `InProgressWithFailures`, `CompletedWithFailures` |
| `AggregatedBackupTransferState` | Which backup kinds exist: `NoBackups`, `ScheduledBackups`, `OneTimeBackups`, both |
| `ColdBootState` | Startup scan/bootstrap: `Unknown`, `InProgress`, `Complete` |
| `ScheduleError` | `None`, `LocalFolderNotFound`, `DestinationNodeNotAvailable`, `ExternalDriveMissing`, `NameConflict` |
| `ScanRequestState` | Per-schedule scan: `NotRequested`, `Queued`, `Dequeued` |
| `TransferStatus` | Per-item ordinal state (see §3.2) |

---

## 2. Main window chrome states

### 2.1 Nav menu

| Item | Label key → German | Visible when |
|---|---|---|
| Backup | `…_navigationMenu_backup` → "Datensicherung" | always |
| Download | `…_navigationMenu_download` → "Herunterladen" | always |
| Sync | `…_navigationMenu_sync` → "Synchronisieren" | `AppMode` has the `Sync` flag |
| Settings | `…_navigationMenu_setttings` → "Einstellungen" | always |

Each item has `Selected` (active tab) and `Collapsed` (icon-only) state. Collapse is
persisted per user as `ShouldCollapseNavigationMenu`. *observed*
`Startseite` ("home") and `Intelligenter Scan` exist as strings but have no menu item in
this build → dead/legacy. *inferred*

### 2.2 Footer

| Element | Shown when | Text |
|---|---|---|
| Storage block | always (in backup-visible layouts) | "Ihr Speicherplatz" (`…_youraccount_yourstoragetitle`), "{1} von {2} belegt" (`…_youraccount_usageformat`), upgrade link "Upgrade jetzt vornehmen" (`…_youraccount_upgradenow_buttontitle`) |
| Uploads summary | while backups are active | "Elemente werden gesichert, ({0:n0} verbleibend)" (`…_backup_remainingcount`); time-remaining via `…_backup_remainingtimeandbytes` / `…_hours_remaining_message` / `…_minutes_remaining_message` / `…_seconds_remaining_message` |
| Global backup pause button | only when total scheduled item count > 0 | "Alle Sicherungsvorgänge anhalten" / "… fortsetzen" (`…_backupscreen_pauseallbackups` / `…_resumeallbackups`) |

Pluralisation variants implemented in the app: `…_datetimleft_*` / `…_datetimeago_*`
(one/many × sec/min/hour/day), `…_singular_errorsformat` vs `…_plural_errorsformat`,
`…_displayitememptymessage_*` per tab × status. *observed*

---

## 3. Backup tab — per-screen data model

### 3.1 Row model

Each row (`ScheduledBackupItem` / `OneTimeBackupItem`) exposes:

| Field | Source | Notes |
|---|---|---|
| `Name` | last path component, or external-drive display name | One-time row is fixed to "Einmalige Sicherung" |
| `Path` | `schedules.local_path` | tooltip shows full path |
| `IsExternal` | `schedules.drive_identifier` non-empty | changes icon/label |
| `Interval` | `schedules.interval` (an `IntervalExpression`) | see §4.2 |
| `IsPhotosSelected` / `IsVideosSelected` | `schedules.content_filter` bitmask | see §4.1 |
| `AvoidDuplicates` | `schedules.dedupe_photos_videos` | see §4.1 |
| `ConflictResolution` | `schedules.conflict_resolution` | see §4.1 |
| `StatusViewModel.StatusText` | derived (see §3.2) | upper-cased |
| `StatusViewModel.StatusSubText` | "{n} Fehler" while failing | optional |
| `StatusViewModel.StatusAlertLevel` | derived | drives colour (§3.4) |
| `StatusViewModel.PauseButtonVisible` | worker running & schedule not paused | per-row pause icon |
| `StatusViewModel.ScanAndUploadNowItemEnabled` | scan not already queued | per-row menu |
| `LastUpdatedTextString` | `backup.last_scan_time` via `LastEnumerationTime` | see §3.3 |

### 3.2 Status vocabulary (strings → German)

`TransferStatus` ordinal values are the persistence layer (`uploads.status`,
`backup.status`, `downloads.status`). The UI maps them to these labels (*observed*):

| Status | German label | Key |
|---|---|---|
| Waiting | "Warten" | `…_statuswindow_statuswaiting` |
| Processing | "In Bearbeitung" | `…_statuswindow_statusprocessing` |
| Retrying | "Wiederholen" | `…_statuswindow_statusretrying` |
| Completing / MultipartUploadCompleting | "Wird überprüft" | `…_statuswindow_statuscompleting` |
| Completed | "Abgeschlossen" | `…_statuswindow_statuscompleted` |
| Canceled | "Abgebrochen" | `…_statuswindow_statuscanceled` |
| Errored | "Fehlgeschlagen" | `…_statuswindow_statusfailed` |
| Ignored | "Zuvor hochgeladen" | `…_upload_statuswindow_statusignored` |
| Skipped | "Verworfen" | `…_statuswindow_statusskipped` |
| Sync | "Synchronisierung" | `…_statuswindow_statussyncing` |
| Downloading | "Wird heruntergeladen" | `…_upload_statuswindow_statusdownloading` |
| Uploading | "Upload-Vorgang läuft" | `…_upload_statuswindow_statusuploading` |
| — | "Achtung" (warning) | `…_statuswindow_statuswarning` |
| — | "Alle" (filter) | `…_statuswindow_statusall` |

Waiting-state extra info: tooltips distinguish the direction —
"Das Hochladen wird vorbereitet. Dieser Vorgang kann bis zu 10 Minuten dauern."
(`…_statuswaiting_upload_tooltip`) vs the download equivalent
(`…_statuswaiting_download_tooltip`); the "completing" state explains
"Wir überprüfen, ob Ihre Datei sicher in Amazon Photos gespeichert ist. …"
(`…_statuswindow_statuscompleting_detail`). *observed*

### 3.3 Aggregated row state (scheduled & one-time)

See `UI_FLOWS.md` §11 for the full precedence table. The text constants are:

| Displayed text | Key |
|---|---|
| "Upload-Vorgang läuft" | `…_upload_statuswindow_statusuploading` |
| "Datensicherung abgeschlossen" | `…_statuspopup_backupcomplete` (row) |
| "ANGEHALTEN" | `…_statuswindow_scheduletab_paused` |
| "Warten" | `…_statuswindow_statuswaiting` |
| "Konfiguration wird durchgeführt" | `…_contextmenu_message_getchanges` |
| "{n} Fehler" | `…_singular_errorsformat` / `…_plural_errorsformat` |
| "{0:N0} abgeschlossen" | `…_backupitem_completedcount` |
| "{0:N0} verbleiben" / "… verbleibend" | `…_backupitem_remaincount` / `…_backupitem_remainingcount` |
| "{0:N0} fehlgeschlagen" | `…_backupitem_failedcount` |

**"Zuletzt aktualisiert"** (`…_scheduledbackupitem_lastupdatedtime` = "Zuletzt
aktualisiert: {0}"):
- non-CX variant: shown whenever `LastEnumerationTime != DateTimeOffset.MinValue`;
  tooltip = absolute timestamp; the relative string refreshes on a decaying interval
  (`>1h → 3600 s`, `>5min → 300 s`, `>1min → 60 s`, else `5 s`).
- CX-simplification variant: wording becomes **"Wurde aktualisiert: {0}"**
  (`…_scheduledbackupitem_updatedtime`) and the line is shown only when the last
  enumeration is **≥ 180 s old**.
- Relative-age wording comes from `…_datetimeago_*` ("Vor 1 Min.", "Vor {0} Stunden", …). *observed*

### 3.4 Status colours (alert level)

`BackupStatusAlertLevel` ∈ {`Normal`, `Info`, `Warning`, `Error`, `Success`} drives the row's
status colour. Mapping (*inferred* from the theme constants, alert level observed):

| Level | Used for | Colour family |
|---|---|---|
| `Success` | backup completed | green |
| `Info` | uploading, waiting, preparing | blue/accent |
| `Warning` | "Einige Ordner wurden nicht gesichert" | amber |
| `Error` | failures, schedule errors | red |
| `Normal` | paused / idle | secondary/grey |

The status-popup rows similarly switch between a *transferring* icon, a *completed* green
icon and an *error* red icon (`BackupBlue`, `DownloadBlue`, `SyncBlue`, `PauseGray`,
`CompleteGreen`, `Error`). *observed*

### 3.5 Per-item progress text

Inside a backup item's progress area (`ProgressInformationView`), the text is one of
(**observed**):

| Condition | Text |
|---|---|
| percentage = 100 | **"Vollständig"** (`clouddrive_desktop_complete`) |
| paused | **"Angehalten bei {0} %"** (`clouddrive_desktop_paused_on_text`) |
| otherwise | **"{0}% hochgeladen..."** (`clouddrive_desktop_percentuploadedformat` = "{0}% hochgeladen") |

Global/aggregate progress texts: "{0} von {1} Elementen hochgeladen ({2} verbleibend"
(`…_transferingmessage_upload_plural`), "… Element …" singular,
"{0} Elemente verbleibend ({1}), {2} fehlgeschlagen"
(`…_transferingmessage_plural_witherrors`), "Verarbeitung abgeschlossen"
(`…_transferingmessage_complete`), "Upload abgeschlossen"
(`…_transferingmessage_upload_completed`). *observed*

### 3.6 Details view: nested vs flat

`BackupItemsDisplayMode` ∈ {`List`, `DrillDown`}. *observed*

| Mode | Toggle tooltip | Navigation chrome |
|---|---|---|
| `DrillDown` | "Verschachtelte Anzeige" (`…_backupdetailsview_drilldown`) | breadcrumb stack; parent-nav caret; overflow button |
| `List` | "Flache Listenanzeige" (`…_backupdetailsview_flatlist`) | no breadcrumbs; flat rows |

Detail row height is 48 pt in the CX-simplification layout; rows show a folder/file icon
(folder icon differs for `IntervalOption.Immediate`), a name with a path tooltip
("Lokale Datei öffnen" / "Lokalen Ordner öffnen",
`…_tooltip_open_local_file` / `…_tooltip_open_local_folder`), a status column
(position index 2), a size column and a last-modified column. *observed / partly inferred*

Empty detail messages per filter (`…_displayitememptymessage_*`):
"Keine Dateien hochgeladen. (Fügen Sie neue Inhalte hinzu!)",
"Keine Fehlgeschlagenen Uploads. (Puh!)", "Alle Dateien hochgeladen. (Keine Sorge!)",
"Keine abgebrochenen Dateien. (Das ist eine Erleichterung!)",
"Keine verworfenen Dateien. (Puh!)", plus a generic
"Hier gibt es nichts zu sehen, es ist alles in Ordnung!" (`…_default`). *observed*

### 3.7 Backup screen states

| Screen state | Trigger | What is shown |
|---|---|---|
| Empty | no scheduled backups and no one-time queue | Empty-state panel: "Verlieren Sie nie wieder ein Foto oder eine Datei" + "Einen Ordner zur automatischen Datensicherung festlegen" + "Ordner auswählen …" / "oder" / "Hochladen..." |
| List | ≥ 1 backup | left list + right detail of the selection |
| Cold boot | `ColdBootState = Unknown/InProgress` | rows show "Warten" with the 10-minute tooltip |
| Global paused | `AggregatedWorkerState.Paused` | every row "ANGEHALTEN"; footer/header button shows "… fortsetzen" |
| Rendering time | while computing counts | "Berechnung läuft ..." (`…_calculating_count`), "Verbleibende Zeit wird berechnet" (`…_backupconfiguration_calculatingtime`), "Ihre Dateien werden vorbereitet..." (`…_statuswindow_calculating`), "Dateien verifizieren" (`…_verifying_files_message`) |

### 3.8 DB fields behind the backup screen

| UI element | Table / field |
|---|---|
| Backup list membership | `schedules` rows (`id`, `local_path`, `destination_node_id`, `interval`, `paused`, `content_filter`, `dedupe_photos_videos`, `conflict_resolution`, `drive_identifier`, `error_code`, `last_scan_time`, `last_enumeration_time`, `last_deep_scan_time`, `created_date`) |
| "Zuletzt aktualisiert" | `schedules.last_enumeration_time` (and `backup.last_scan_time`) |
| Row status | derived from `backup.status` / `uploads.status` aggregates; schedule errors from `schedules.error_code` |
| Per-file rows | `backup` (`schedule_path`, `relative_path`, `node_id`, `size`, `modified_date`, `status`, `status_changed`, `error_code`, `target_type`, `version`, `content_version`, `retry_reason`) and `uploads` (`path`, `status`, `retries`, `next_retry`, `retry_reason`, `suspended_reason`, `error_code`, `precomputed_md5`, `upload_type`, `is_expanded`, `is_visible`) |
| One-time queue | `uploads` rows with `schedule_id = 'ManualUpload'` |
| Multipart progress | `upload_parts` (`part_id`, `node_id`, `upload_id`, `size`, `offset`, `md5`, `status`) |
| Destination cache | `sync.nodes` / `-cloud.db.nodes` (`id`, `name`, `kind`, `status`, `content_*`, `hidden`) |

`uploads.is_expanded` / `is_visible` are the persisted form of the drill-down expansion
state. *observed*

---

## 4. Settings model (per-schedule + per-customer)

### 4.1 Per-schedule settings

Stored in `schedules.*`; exposed in the backup-configuration window. *observed*

| Setting | Enum / values | German UI |
|---|---|---|
| `content_filter` | `ContentFilter`: `None=0`, `Photos=2`, `Videos=4`, `Other=8`, `PhotosAndVideo=6`, `Everything=14` | "Alles" / "Fotos und Videos" / "Fotos" (`…_scheduleconfiguration_contenteverything` / `…_contentimagesandvideos` / `…_contentimages`) and checkboxes "Fotos sichern" / "Videos sichern" (`…_upload_settings_backup_photos_checkbox` / `…_backup_videos_checkbox`) |
| `dedupe_photos_videos` | `DeduplicateOptions`: `None=0`, `Photos=2`, `Videos=4`, `PhotosAndVideos=6` | "Duplikate vermeiden: Keine Fotos und Videos sichern, die sich bereits in meinem Konto befinden" (`…_scheduleconfiguration_dedupeoption`); shortened form "Duplikate überspringen" (`…_upload_settings_skip_duplicates_option_title`) |
| `conflict_resolution` | `ConflictResolution`: `None`, `Ignore`, `SaveAsNew`, `Replace` | radio group "Umgang mit Dateinamenkonflikten:" → "Jedes Mal nachfragen" / "Ignorieren" / "Als neue Dateien speichern" / "Ersetzen" (`…_scheduleconfiguration_filenameconflict_askmeeverytime` / `_ignore` / `_saveasnewfile` / `_replace`); alternate wording "Neue Datei automatisch umbenennen" / "Neue Datei ignorieren" / "Alte Datei ersetzen" (`…_upload_settings_file_name_conflicts_*`) |
| `interval` | `IntervalOption` (see §4.2) | "Änderungen hochladen:" |
| `paused` | 0/1 | per-row pause |
| auto-upload | derived from the interval being `Immediate` | "Neue Dateien automatisch hochladen" (`…_auto_upload_toggle_text`) |

**Constraint noticed** *observed*: choosing `ContentFilter.Everything` forces
`AvoidDuplicates = false` (dedupe is only meaningful for photos/videos). Since
2023-11-30 the app blocks non-media uploads and warns about it
(`…_backupconfiguration_blocknonmediaupload_text`).

### 4.2 Interval options

`IntervalOption` (schedule cadence) and their German labels (*observed*):

| Value | German | Key |
|---|---|---|
| `Immediate` | "Sofort" | `…_scheduleconfiguration_interval_option_continuous` |
| `FiveMinutes` | "Alle 5 Minuten" | `…_interval_option_five_minutes` |
| `TenMinutes` | "Alle 10 Minuten" | `…_interval_option_ten_minutes` |
| `ThirtyMinutes` | "Alle 30 Minuten" | `…_interval_option_thirty_minutes` |
| `OneHour` | "Jede Stunde" | `…_interval_option_one_hour` |
| `ThreeHours` | "Alle 3 Stunden" | `…_interval_option_three_hour` |
| `SixHours` | "Alle 6 Stunden" | `…_interval_option_six_hour` |
| `OneDay` | "Einmal pro Tag" | `…_interval_option_one_day` |
| `OneWeek` | "Einmal pro Woche" | `…_interval_option_one_week` (with day-of-week "aktiviert" + time "um") |
| `Never` | (no upload) | — |

The interval is persisted as an `IntervalExpression` string in `schedules.interval`; the
UI carries the extra `DayOfWeek` + `Time` for the weekly option. Changing an interval while
items are queued triggers the "Elemente in der Warteschlange" dialog (§ `UI_FLOWS.md` 4.5). *observed*

The app-settings key **`BackupScanIntervalInHours = 336`** (14 days) is the *deep* scan
cadence, distinct from the user-facing upload interval. *observed*

### 4.3 Bandwidth & concurrency (Preferences → Bandbreite)

All values are per-customer in the settings DB; the UI bounds come from app settings with
these defaults. *observed*

| Setting | Property | Bounds / default | German UI |
|---|---|---|---|
| Concurrent uploads | `UploadQueues` | min **1**, max **8**, default 8 (`CustomerSettingsMaxSupportedUploadQueues`) | slider "Gleichzeitige Uploads:" (`…_bandwidth_uploads_slider_title`) |
| Concurrent downloads | `DownloadQueues` | min **1**, max **8**, default 8 | slider "Gleichzeitige Downloads:" (`…_bandwidth_downloads_slider_title`) |
| Upload rate cap | `UploadTransferKiloBytesPerSecond` | min **1**, max **99999**, default **100** KB/s when enabled | radio "Nicht begrenzen" / "Begrenzen auf" + "KB/Sekunde" (`…_bandwidth_dont_limit_title` / `…_limit_to_title` / `…_bandwidth_rate`), label "Upload-Rate pro Datei" (`…_download_rate_title`/`…_upload_rate_title`) |
| Download rate cap | `DownloadTransferKiloBytesPerSecond` | same bounds | same radio group, "Download-Rate pro Datei" |

Semantics: a value of **0** means "unlimited" — the view-model derives the "Begrenzen auf"
state from `value != 0`, and when unlimited it displays the default (100 KB/s) in the input
without persisting it. Out-of-range input is clamped to [min, max]. *observed*

Observed live values in this install: `settings.json` has **`UploadQueues = 8`** and
**no throttle values** (so both transfers are unlimited). *observed*

### 4.4 General / Backup / Sync / Proxy preferences

| Setting | Property | Persisted in | German UI |
|---|---|---|---|
| Launch at login | `AutoStartEnabled` | OS login items (needs permission; "Sie haben keine Berechtigung …", `…_insufficientpermission_opensystempreferences`) | "Amazon Photos beim Starten öffnen" (`…_auto_start_application`) |
| Show notifications | `NotificationsEnabled` | customer settings | "Benachrichtigungen anzeigen" (`…_enable_notifications`) |
| Notification badges | `EnableNotificationBadging` | customer settings | "Aktivieren Sie Badges für Benachrichtigungen" (`…_enablenotificationbadges`) |
| Hidden files | `ExcludeHiddenFiles` | customer settings | "Keine ausgeblendeten Dateien hochladen" (`…_excludehiddenfiles`) / "Ausgeblendete Dateien hochladen" (`…_includehiddenfiles`) + tooltip (`…_uploadhiddenfiles_tooltip`) |
| AutoConnect | `EnableAutoConnect` | `settings.json` | "AutoConnect aktivieren" (`…_preferences_backup_enable_autoconnect`) + tooltip (`…_autoconnect_tooltip`) |
| Dedupe photos/videos | `EnableDeduplicationForPhotosAndVideos` | customer settings | "Duplikate überspringen" |
| iOS-duplicate filter | `FilterOutPotentialDuplicatesFromApplePhotosLibrary` | customer settings | "Ignorieren Sie Bilder, die möglicherweise Duplikate aus der Amazon Photos iOS-App sind." (`…_filter_potentialduplicates_title` / `…_description`) |
| Backup interval (global default) | `BackupItemInterval` | customer settings | "Änderungen hochladen:" |
| Content filter (global) | `ContentFilter` | customer settings | see §4.1 |
| Sync enabled | `AppMode` has `Sync` | `settings.json` `AppMode` | "Synchronisierung aktivieren/deaktivieren..." |
| Selective sync folders | sync folder nodes | cloud state | "Wähle Ordner in deinen Amazon Photos zum Synchronisieren aus:" (`…_preferences_selective_sync_title`) + "Ordner wählen..." (`…_preferences_selective_sync_button_text`) |
| New top-level cloud folders | `ExcludeNewCloudFolders` | customer settings | "Synchronisierung für neue Ordner auf oberster Ebene deaktivieren, die ich auf Amazon Photos erstelle" (`…_exclude_syncing_new_cloud_folders`) / inverse `…_sync_new_cloud_folders` |
| Proxy mode | `ProxySettingsType` | customer settings | radio: "Kein Proxy" / "Automatische Erkennung" / "Manuell" (`…_proxy_noproxy` / `_autodetect` / `_manual`) each with a description paragraph |
| Proxy server/port | `ProxyServer`, `ProxyPort` | customer settings | "Server" / "Port" (`…_proxy_serverlabel` / `…_proxy_portlabel`), enabled only for "Manuell" |
| Send logs | — | — | "Protokolle senden" (`…_send_logs_button_text`) |

`ProxySettingsType` enum: `NoProxy`, `AutoDetect`, `Manual`. *observed*. Selecting anything
other than `Manual` clears the server/port fields in the view-model. *observed*

Validation: an invalid proxy address shows "Ungültige Proxy-Adresse" /
"Die angegebene Proxy-Adresse enthält ungültige Zeichen. …"
(`…_proxy_warning_title` / `…_proxy_warning_text`). *observed*

The Apply/OK flow: `IsDirty` is set by any edit; closing with `IsDirty` warns about losing
changes (§ `UI_FLOWS.md` §7). Selective-sync changes that switch a folder **off** require an
extra confirmation before persisting. *observed*

### 4.5 Settings persistence locations

| File | Fields | Used for |
|---|---|---|
| `~/Library/Application Support/Amazon Cloud Drive/<account>-settings.json` | `AppMode`, `DeviceSourceId`, `OnboardingVersion`, `FtueVersion`, `EnableAutoConnect`, `UploadQueues`, `AccountFeatures.upsell`, `ReleaseNotesVersion`, `HasBackfilledHiddenNodes`, `LastSentBackupMetrics`, `DelighterTrackingTime` | per-account UI state |
| `~/Library/Application Support/Amazon Cloud Drive/app-settings.json` | `BackupScanIntervalInHours = 336`, buffers (`CloudDriveServiceUploadNetworkBuffer = 4096`, `…DownloadNetworkBuffer`, `FileStreamUploadBuffer = 65536`), retry/back-off, feature-flag treatment maps (`EnableCXUploadSimplification`, `EnableUploaderUpdate`, `EnablePhotosBffUrl`, `UpsellWindowEnabled`, `ShowNonMediaFilterCheckbox`, `PushNotificationsEnabled`, `FileConflictNotificationEnabled`, …) | app-wide tunables and the A/B treatment maps |
| Each `*.db` `settings` table | name/type/value rows | per-subsystem persistence (sync/backup/download/schedules) |
| `serial-number` | device serial (redacted, shown in About) | About window |

Feature-flag note: `EnableCXUploadSimplification` etc. are *treatment maps*; the app enables
a behaviour only when the resolved treatment is `"T1"` (e.g. the compact backup-row layout,
the "Wurde aktualisiert" wording, the simplified About view, the large-scale-event banner). *observed*

---

## 5. Other screens — state model

### 5.1 Status popup

| Row | Transferring text | Paused text | Completed text | Tab target |
|---|---|---|---|---|
| Backup | "Sicherung wird durchgeführt" (`…_statuspopup_backingup`) | "Sicherung angehalten" (`…_statuspopup_backuppaused`) | "Datensicherung abgeschlossen" (`…_backup_complete`) | Backup |
| Download | "Wird heruntergeladen" (`…_statuspopup_downloading`) | (none) | "Download abgeschlossen" (`…_statuspopup_downloadcomplete`) | Restore |
| Sync | "Wird synchronisiert" (`…_statuspopup_syncing`) | (shows completed) | "Synchronisierung abgeschlossen" (`…_statuspopup_synccomplete`) | Sync |

- Icon per row: transferring = coloured blue icon; completed-no-errors = green; completed-with-errors / failing = red; suspended-no-errors = grey pause.
- The Backup row's call to action is `StartBackup` **only when the total item count is 0**
  (label opens the backup-configuration window); otherwise it becomes `RetryErrored` when
  failures exist. Download/Sync rows only offer `RetryErrored`. *observed*
- Notifications are sorted by notification type and exclude story/this-day items from the
  list (they are handled separately). *observed*

### 5.2 Activity window

Title "Upload-Vorgang läuft" (`…_activitywindow_title`). Two modes (`ActivityDisplayMode`):
**Processing** ("In Bearbeitung") and **Waiting** ("Warten"). Header text
"{0:N0} Elemente ({1:N0}) verbleibend" (`…_activitywindow_remaining_items_format`).
Empty state: "Es ist keine aktuelle Aktivität zum Anzeigen vorhanden."
(`…_activitywindow_noactivity_header`) + "Sie können gesicherte Fotos im Internet anzeigen …"
(`…_activitywindow_noactivity_info`) + button "Uploads im Internet anzeigen"
(`…_activitywindow_noactivity_viewuploadsbuttontitle`). Has an activity/pause toggle button. *observed*

### 5.3 About window

Title "Über Amazon Photos" (`clouddrive_desktop_aboutwindow_title`). Contents: logo,
"Version: {0}" (`…_versionformat`), "Geräte-Seriennummer: {0}" (`…_deviceSerialNumberformat`,
redacted), links "Nutzungsbedingungen" / "Datenschutzerklärung" / "Drittanbieterlizenzen"
(`…_termsofuse` / `…_privacynotice` / `…_thirdpartylicenses`), the send-logs paragraph
(`…_send_logs_tooltip`) + "Protokolle senden" button (+ spinner), a copyright line
(`…_copyright`, "© 2012-{0} Amazon.com, Inc. …") and "Schließen" (`…_about_close`). The
simplified variant (`EnableAboutViewSimplification`) hides part of this. *observed*

### 5.4 Startup / splash

Greeting "Willkommen, {0}" (`…_startupwindow_greeting`, user name redacted) and
"App wird geladen" (`…_startupwindow_loadingmessage`) with a progress bar driven by the
startup-task manager (`Progress` 0→1). *observed*

### 5.5 AutoConnect prompt

Header "{0} verbunden" (`…_autoconnectwindow_header`, device name redacted), body
"Möchten Sie Ihre Fotos und Videos mit AutoConnect sichern?"
(`…_autoconnectwindow_bodybackup`) + "Bewahren Sie sie mit Amazon Photos sicher auf."
(`…_autoconnectwindow_bodysafephotos`), actions "Sicherung einmalig hochladen"
(`…_autoconnectwindow_onetimebackup`) and "Nicht erneut fragen"
(`…_autoconnectwindow_dontaskagain`). The window can open in the AutoConnect configuration
mode of the backup-configuration window. *observed*

### 5.6 Selective Sync window

Title "Selektive Synchronisierung" (`clouddrive_desktop_selectivesyncwindow_title`), header
"Wählen Sie die Ordner, die Sie synchronisieren möchten."
(`…_selectivesyncwindow_header`), info "Alle Dateien, die sich nicht in einem Ordner
befinden, werden synchronisiert." (`…_selectivesyncwindow_info`), actions "Alle auswählen"
(`…_selectall`), "Gesamte Auswahl aufheben" (`…_selectnone`), "Synchronisieren"
(`…_selectivesyncwindow_sync`), context menu "Ordner öffnen" / "Anzeigen auf Amazon..."
(`…_contextmenuviewlocally` / `_viewonweb`). Opened from Onboarding or Preferences
(`SelectiveSyncWindowSource`). *observed*

### 5.7 Error window

Title "Fehler bei {0}" (`…_errorwindow_titleformat`), per-item size prefix "({0})"
(`…_errorwindow_sizeformat`), actions "Verwerfen" (`…_errorwindow_dismiss`),
"Entfernen" (`…_errorwindow_notfounderrorcta`), "Upgrade vornehmen"
(`…_errorwindow_overquotaerrorcta`), "Wiederholen" (`clouddrive_desktop_retry`). Reached from
the error banner "Bei Ihrem Upload ist ein Fehler aufgetreten."
(`…_errorbannerview_uploaderror` / `…_uploaderrorplural`) and "Fehler beim Hochladen anzeigen"
(`…_errorbannerview_showerrorwindow`). The long/short error catalogue is keyed by
`…_statuswindow_errorlong_*` / `…_errorshort_*` (network, permissions, quota, path length,
file in use, missing data, multipart, …). *observed*

### 5.8 File-name conflict

Overlay title "{0} Dateinamenkonflikt in {1}" (`…_filenameconflict_overlayview_title`,
plural variant), body "Das Ziel hat {0} Datei mit demselben Namen. Möchtest du sie als neue
Datei speichern oder die Datei ersetzen?" (`…_overlayview_body`), buttons "Als neue Datei
speichern" / "Ersetzen" / "Ignorieren" (`…_saveasnew_button_text` / `_replace_button_text` /
`_ignore_button_text`), checkbox "Nicht noch einmal nach {0}-Ordner fragen"
(`…_donotaskagain_checkbox_text`). A macOS notification "Dateinamenkonflikte gefunden"
(`…_filenameconflict_toast_title`) can bring the user back ("Öffne die App, um Konflikte zu
lösen.", `…_filenameconflict_toast`). *observed*

### 5.9 Upsell

Modal title "Dir geht der Speicherplatz aus" (`…_upsell_modal_title`), body
"Dateien, die dein Speicherkontingent überschreiten, werden nicht hochgeladen."
(`…_upsell_modal_description`), buttons "Mehr Speicherplatz kaufen"
(`…_upsell_buy_button`) / "Nicht jetzt" (`…_upsell_cancel_button`). Inline variant
"Schalte mehr Speicherplatz frei!" (`…_upsell_title`) + description (`…_upsell_description`).
Quote-specific notifications: near quota (`…_statuspopup_notification_nearquota`), over quota
(`…_notification_overquota`), free-tier upgrade (`…_notification_overquota_freetier_*`),
unlimited-photos video cap (`…_notification_overquota_unlimitedphotos_*`). *observed*

---

## 6. Onboarding / FTUE — state & persistence

### 6.1 Legacy Onboarding

`OnboardingState` ∈ {`Education`, `SetupSync`, `SetupComplete`, `Completed`}. *observed*

| State | Screen | Primary / secondary actions |
|---|---|---|
| Education | "Zwei Möglichkeiten zur Nutzung von Amazon Photos" | "Weiter" / "Nicht jetzt" |
| SetupSync | "Ordner synchronisieren" | "Synchronisierung starten"; toggling the checkbox flips the `Sync` flag in `AppMode` |
| SetupComplete | "Einrichtung abgeschlossen" | "Weiter" → Completed |
| Completed | — | workflow ends |

Persistence on save: `OnboardingVersion = LatestOnboardingVersion (2)` and `AppMode`
(written to the per-account `settings.json`). *observed*

### 6.2 FTUE

`FtuePage` ∈ {`Welcome`, `ConfigureAnyFolders`, `BulkDownload`, `Upsell`};
`FtueWindowSource` ∈ {`FirstTimeOpened` → start at Welcome, `GetStarted` → start at
ConfigureAnyFolders, `Upsell` → start at Upsell}. *observed*

| Page | Primary | Secondary / link | Next |
|---|---|---|---|
| Welcome | "Weiter" | "Einrichtung überspringen"; link "Erfahre mehr über das Herunterladen von Fotoordnern" | → ConfigureAnyFolders (or → BulkDownload) |
| ConfigureAnyFolders | "Weiter" (no plan) or "Fertig" (has plan) | "Weitere Ordner hinzufügen" | → Upsell if no plan, else → close on Backup tab |
| BulkDownload | "Ordner herunterladen" | "Mit dem Upload fortfahren" | → folder picker, or → Welcome |
| Upsell | (storage webview) | — | — |

- "Upsell is shown" is decided by *account has no plan*; the FTUE waits up to 3 s for the
  account info to load before deciding. *observed*
- Folder-conflict guards reuse the same strings as the backup configuration
  (`clouddrive_desktop_ftue_child_folder_warning`, `…_parent_folder_warning_*`,
  `…_ftue_remove_folder_dialog_*`). *observed*
- Apple Photos: the Photos-library permission text is
  `clouddrive_desktop_mac_NSPhotoLibraryUsageDescription` and the dialog
  `clouddrive_desktop_photoslibrary_dialogtitle` ("Apple Fotobibliothek"); uploading
  Apple Photos / iPhoto / Aperture *libraries* is explicitly unsupported
  (`…_statuswindow_errorlongiphotonotsupported`, `…_mac_onboard_noautosave_select_disclaimer`). *observed*

---

## 7. Quick reference — enum values as persisted

| Enum | Values (numeric) | Stored in |
|---|---|---|
| `TransferStatus` | None=0, Waiting=1, Processing=2, Retrying=4, MultipartUploadCompleting=8, Completing=16, Completed=32, Canceled=256, Errored=512, Ignored=1024, Skipped=2048, IgnoredDueToPotentialConflictWithiOSUploads=4096 | `*.status` |
| `ContentFilter` | None=0, Photos=2, Videos=4, Other=8, PhotosAndVideo=6, Everything=14 | `schedules.content_filter` |
| `DeduplicateOptions` | None=0, Photos=2, Videos=4, PhotosAndVideos=6 | `schedules.dedupe_photos_videos` |
| `ConflictResolution` | None=0, Ignore=1, SaveAsNew=2, Replace=3 | `schedules.conflict_resolution` |
| `ScheduleError` | None, LocalFolderNotFound, DestinationNodeNotAvailable, ExternalDriveMissing, NameConflict | `schedules.error_code` |
| `AggregatedTransferState` | Unknown=0, Preparing=1, InProgress=4, Completed=8, Failed=16 (combos OR'd) | derived (not stored) |
| `AggregatedWorkerState` | Transitioning=0, Running=1, Paused=2, Stopped=4, ForceStopped=12 | derived (not stored) |
| `ProxySettingsType` | NoProxy=0, AutoDetect=1, Manual=2 | customer settings |
| `IntervalOption` | Immediate, FiveMinutes, TenMinutes, ThirtyMinutes, OneHour, ThreeHours, SixHours, OneDay, OneWeek, Never | `schedules.interval` (as expression string) |

---

## 8. Open questions / not determined

- **Exact status colours.** `BackupStatusAlertLevel` is observed, but the concrete RGB
  values come from a theme layer that was not fully traced; the *colour family* in §3.4 is
  *inferred*.
- **"Zuletzt aktualisiert" vs "Wurde aktualisiert".** Two wordings exist; the switch is a
  feature-flag treatment (`EnableCXUploadSimplification == "T1"`). Which treatment this
  install receives at runtime was not determined (the flag is server-driven).
- **Localisation of relative time.** The `…_datetimeago_*` strings are German; the actual
  formatter (`ToAgoShortString`) and its threshold boundaries (e.g. when "s" becomes "Min.")
  were not fully traced → *not determined*.
- **UploadQueues default.** Code default is 8; observed value in this install is also 8, but
  whether 8 is user-set or factory default was not established.
- **Throttle default (100 KB/s).** Used as the *displayed* value when unlimited; whether it
  is also enforced as a floor was not traced.
- **Notification list rendering.** The per-notification cell layout and ordering key
  (`INotification.Type`) values were not enumerated.
- **Exact `.nib` geometry.** As in `UI_FLOWS.md` §13 — all pixel/form-factor statements are
  *inferred*; `ibtool` was unavailable.
- **Storage-usage entitlement mapping.** Which of `limited_withplan`,
  `unlimitedphotos_withoutplan`, `unlimitedeverything` applies to a given account was not
  fully traced; note this repo deliberately omits a storage bar in its own app.
