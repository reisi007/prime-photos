# Preferences / Settings / About / Auto-Update — Clean-Room Spec (reverse-engineered)

Behavioural specification of the **original** Amazon Photos desktop client's settings surface:
the preferences window (all tabs and switches), the persistence layer behind each key, the
auto-update mechanism, and the About / Privacy / Terms / Third-Party-Licenses window — plus the
mapping onto this port. The original behaviour below is **reverse-engineered from the shipped
managed assembly** (`Amazon Photos.dll`), the German `Localizable.strings`, and `Info.plist`,
not inferred from screenshots alone.

This document is **clean-room**: it records *observed behaviour* in our own words and tables.
It contains **no verbatim decompiled code** and **no personal data**. German user-facing labels
are quoted from the shipped `de.lproj/Localizable.strings` (product strings, not code); type and
member names are cited only to the extent needed to describe the mechanism.

**Confidence convention.**
- **belegt** = read directly out of the reverse-engineered assembly (member/branch), the German
  strings file, `Info.plist`, or our committed code.
- *inferred* = reasoned from adjacent evidence, not read off a line.
- **UNKNOWN** = could not be determined from the inspected sources.

---

## 1. Purpose & scope

This doc owns the **complete settings inventory** of the original client: every preferences
window tab, switch, slider and radio group; the default of each key; **which file/database each
key is persisted in**; the upload/network constants that bound those settings; the **auto-update**
mechanic and its trigger; and the **About window** with its legal links (Terms of Use, Privacy
Notice, Third-Party Licenses) and log-sending.

Split of responsibilities (not duplicated here):
- **Login / sign-out / cookie capture / de-registration** → `SPEC_LOGIN.md` (this doc only
  records that the general tab hosts the sign-out button and the autostart switch).
- **Backup cadence, watchers, pause, per-schedule records** → `SPEC_BACKGROUND_SYNC.md`
  (here only the *defaults stored in preferences*: `BackupItemInterval`, `ContentFilter`,
  dedupe default, and the `BackupScanIntervalInHours` constant).
- **Progress / transfer UI / states** → `SPEC_UPLOAD_PROGRESS.md`.
- **Dedup identity + eligibility** → `SPEC_DEDUP.md` (this doc covers the preferences
  switches `EnableDeduplicationForPhotosAndVideos` /
  `FilterOutPotentialDuplicatesFromApplePhotosLibrary` and their persistency).
- **Folder scan filter** → `SPEC_FOLDERS.md` (this doc covers the
  `ExcludeHiddenFilesAndFolders` switch that feeds it).
- **Notifications** → `SPEC_NOTIFICATIONS.md` (this doc covers the preferences switches
  `ShowNotifications` / `EnableNotificationBadging` and the update notifications' text).
- **One-time drop queue** → `SPEC_ONE_TIME_UPLOAD.md`.
- **Upload protocol/API** → `SPEC_UPLOAD.md`, `../protocol/PROTOCOL.md`.

Deliberately **out of scope** here: the sync tab's folder selection UI internals (the original's
selective-sync tree), the upsell/quota windows, onboarding, and the diagnostics window — none of
which exist in the port.

---

## 2. Sources & method (belegt)

- Tooling: `ilspycmd` (v11.1.0.9782) at `~/.dotnet/tools/ilspycmd`, run with `PATH` extended and
  `DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`. The target was read-only and
  **never launched**. → **belegt**
- Target (read-only): `/Applications/Amazon Photos.app` →
  `Contents/MonoBundle/Amazon Photos.dll` (managed assembly, app version 11.3.0 build 36 per
  `Info.plist`, bundle id `com.amazon.clouddrive.mac`), plus
  `Contents/Resources/de.lproj/Localizable.strings` and `Contents/Info.plist`
  (read via `plutil -p`). A full type list, a full decompilation dump (~175k lines), and targeted
  per-type dumps were produced locally under `/tmp`. → **belegt**
- Classes used as evidence: `UI.PreferencesWindow.{PreferencesViewModel, PreferencesViewController,
  PreferencesView, PreferencesWindowService, GeneralPreferencesTabView, BandwidthPreferencesTabView,
  ProxyPreferencesTabView, SyncPreferencesTabView, BackupPreferencesTabView}`,
  `UI.IntentHandlers.{PreferencesActionIntentHandler, ShowPreferencesIntentHandler}`,
  `UI.Commands.{CloseAboutWindowCommand, LaunchPreferencesWindowCommand}`,
  `UI.AboutWindow.{AboutWindowService, AboutViewController, AboutView, AboutViewSimplification,
  AboutViewModel}`, `StartupTasks.{AppSettingsWorker, AppUpdaterWorker}`, `Infrastructure.{AppSettings,
  CustomerSettings, SettingsDatabase, DeploymentSettings, S3UpdateSettings}`,
  `StartupTasks.AppStartup` (login-item handling), `Services.Settings.AppUpdateService`,
  `Services.PopupNotificationCenter.Update.UpdateAvailableNotification`,
  `Services.Events.EventHandlers.UpdateAvailableNotificationHandler`,
  `Services.MultipartUpload.MultipartUploadService`, `Constants`, `CloudDriveLink`. → **belegt**
- Our port's committed state was read from `src/config.rs`, `src/settings.rs`, `src/app.rs`,
  `src/upload.rs`, `src/backup.rs`, `src/platform.rs`, `src/i18n.rs`, `src/folders.rs`,
  `src/dedup.rs`, `src/tray.rs`, `src/login.rs`, and `Cargo.toml`. → **belegt**

---

## 3. Settings inventory — areas, switches, defaults, persistence

### 3.0 The three persistence layers (belegt)

The original keeps preferences in **three distinct stores**; knowing which is which is
prerequisite for every row below.

| Store | File | Written by | Contains | Hot-reload |
|---|---|---|---|---|
| **Customer settings** | `settings.json` under `~/Library/Application Support/Amazon Cloud Drive/` | `CustomerSettings` (a `ReloadSettingsOnChange` over `SettingsDatabase`) | every *user-editable* preference (§3.1–3.6) | yes — `FileSystemWatcher` (size/lastwrite) + a 3 s throttle, then a `Changed` observable; subscribers (queue workers, transfer engines) re-read live (**belegt**) |
| **App settings** (server-driven, *not* user-editable) | `app-settings.json`, same directory | `AppSettingsWorker` (fetches CloudFront `…/config/all.config`, `{OS}.config`, `{OS+version}.config`, `{appversion}.config`, `{appversion+DSN}.config`, merges them, refresh every `SettingsRefreshTimeInSeconds` = 3600 s) | all numeric bounds/limits: queue max/min, throttle min/max, byte thresholds, intervals, `DefaultAutoUpdateAfterTimeInSeconds` | yes — same watcher pattern (**belegt**) |
| **OS login items** | macOS login items (via `System Events` AppleScript) | `AppStartup` | only the autostart flag; **not** in `settings.json` | read back on demand (**belegt**) |

`SettingsDatabase` itself is a flat JSON key/value store (`Get<T>`/`Set<T>` with an in-memory
cache, whole-file rewrite on flush, case-insensitive key lookup). `CustomerSettings` accesses it
through `AutoGet`/`AutoSet` extension helpers whose key is the **property name** — i.e. the JSON
key equals the C# property name verbatim. → **belegt**

Two further stores exist but are outside this doc: the backup/upload SQLite DBs
(`Database.*`, documented in `SPEC_BACKGROUND_SYNC.md` §6 / `SPEC_UPLOAD_PROGRESS.md`)
and the cookies (→ `SPEC_LOGIN.md`). → **belegt**

### 3.1 General tab ("Allgemeines") — account block

| Control | Original key | Default | Persisted in | UI text (de) |
|---|---|---|---|---|
| "Signed in as" name/identifier (read-only labels) | — (from `ICustomerInfo`) | — | — | "Angemeldet als:" |
| Sign-out button | — | — | — (triggers `DeregisterUserIntent`) | "Abmelden" |
| "Manage storage" hyperlink | — | — | — (opens `CloudDriveLink.ManageStorage` = `/photos/storage`) | "Speicher verwalten" |
| Section header | — | — | — | "Systemeinstellungen" |

The account block is a read-only echo of name + email-or-mobile (`Kundenkennung` is shown in
parentheses); the only actions are sign-out and the storage-management web link. → **belegt**

**Divergence note for the port:** our app deliberately shows a slimmer account block ("Angemeldet
als:" + "Abmelden") and no storage link — consistent with the AGENTS.md decision that the storage
bar is omitted until a real quota fetch exists. → **belegt** (our `src/app.rs` sidebar)

### 3.2 General tab — "Systemeinstellungen" switches

| Switch (view label) | Backing setting | Default | Persisted in | Notes |
|---|---|---|---|---|
| "Amazon Photos beim Starten öffnen" (checkbox) | `AutoStartEnabled` ← `AppStartup.GetApplicationStartupStatus()` | off (login item absent) | **OS login items only** — *not* in `settings.json` | toggling shells out to `System Events` via AppleScript to add/remove the login item; failure opens a modal ("Die Option ist leider nicht verfügbar" + "Sie haben keine Berechtigung zum Ändern der Start-Einstellung. Gewähren Sie die Berechtigung in den Systemeinstellungen.") and offers a jump to the Automation system-preferences pane; the checkbox snaps back to the OS-reported state → **belegt** |
| "Ausgeblendete Dateien hochladen" (checkbox **positive**) / backing flag is **negative** | `ExcludeHiddenFiles` ← `customerSettings.ExcludeHiddenFilesAndFolders` | **`true`** (i.e. hidden files are **excluded** by default, checkbox unchecked) | `settings.json` key `ExcludeHiddenFilesAndFolders` | view binds the *inverse* (`IncludeHiddenFiles = !ExcludeHiddenFiles`); sub-label under the checkbox is `…_uploadhiddenfiles_tooltip`; a second, opposite-worded key `…_excludehiddenfiles` ("Keine ausgeblendeten Dateien hochladen") exists in the strings file but is **unreferenced** by code (§4) → **belegt** |
| "AutoConnect aktivieren" (checkbox + sub-label) | `EnableAutoConnect` | **`true`** | `settings.json` | external-device auto-connect for backup setup; our port has no equivalent concept → **belegt** / out of scope |
| "Aktivieren Sie Badges für Benachrichtigungen" (checkbox) | `EnableNotificationBadging` | **`true`** | `settings.json` | dock badge behaviour; only observable effect found is a business metric on change — no further consumer in this assembly (*inferred*: applied by macOS docking/notification plumbing) → **belegt** / *inferred* |
| "Ignorieren Sie Bilder, die möglicherweise Duplikate aus der Amazon Photos iOS-App sind." (checkbox + description) | `FilterOutPotentialDuplicatesFromApplePhotosLibrary` | **`false`** | `settings.json` | when switched **off while it was on**, the save handler re-queues iOS-deduped uploads (`RetryIOSDeduped`) → **belegt** |

The general tab has **no language selector** and **no notifications checkbox** in this build:
`ShowNotifications` exists as a customer setting (default `true`) but is shown only in an
unreferenced legacy key (`…_preferences_enable_notifications`, §4) — *inferred*: the
notifications master switch was dropped from the UI when the popup status/notification centre took
over (see `SPEC_NOTIFICATIONS.md`); the key is still written by the save path
(`_customerSettings.ShowNotifications = viewModel.NotificationsEnabled`) but no view binds it, so
it always saves its loaded value. → **belegt** (no `ShowNotifications` view binding exists) /
*inferred* (why)

**Language:** there is **no** in-app language preference in the original — the German labels are
the shipped `de.lproj` bundle chosen by the OS locale (`CultureInfo.CurrentUICulture` is used for
metric/number formatting only). Our port **deliberately adds** a language override
(AGENTS.md decision 2026-10-13, `src/settings.rs` "Sprache" combo) — a **deliberate divergence**,
recorded here so it is not mistaken for parity. → **belegt**

### 3.3 Bandwidth tab ("Bandbreite")

One tab holds **both directions**, each split into a rate cap and a concurrency slider:
→ **belegt**

| Group | Controls | Backing setting | Default | Persisted in | UI texts (de) |
|---|---|---|---|---|---|
| **Uploads** | radio pair "Nicht begrenzen" / "Begrenzen auf" + numeric field + "KB/Sekunde" unit | `UploadThrottlingEnabled`, `UploadTransferKilobytesPerSecond` ← `customerSettings.UploadTransferKiloBytesPerSecond` | **disabled / 0 = unlimited**; when the user enables throttling without a prior value the field is pre-filled with `CustomerSettingsDefaultBandwidthRateInKiloBytesPerSecondIfThrottlingIsEnabled` = **100** KB/s | `settings.json` key `UploadTransferKiloBytesPerSecond` | "Uploads:", "Upload-Rate pro Datei", "Nicht begrenzen", "Begrenzen auf", "KB/Sekunde" |
| | slider "Gleichzeitige Uploads:" with one tick label per integer | `UploadQueues` ← `customerSettings.UploadQueues` | computed, see §3.3.1 | `settings.json` key `UploadQueues` | "Gleichzeitige Uploads:" |
| **Downloads** | same radio pair + field | `DownloadThrottlingEnabled`, `DownloadTransferKilobytesPerSecond` ← `customerSettings.DownloadTransferKiloBytesPerSecond` | **0 = unlimited**; pre-fill 100 | `settings.json` | "Downloads:", "Download-Rate pro Datei", "Nicht begrenzen", "Begrenzen auf" |
| | slider | `DownloadQueues` ← `customerSettings.DownloadQueues` | computed | `settings.json` | "Gleichzeitige Downloads:" |

**Field sanitising and clamping.** Text fields strip every non-digit character as you type; on
focus-out (and on OK) the value is parsed and clamped into
`[CustomerSettingsMinSupportedBandwidthThrottleRateInKiloBytesPerSecond,
CustomerSettingsMaxSupportedBandwidthThrottleRateInKiloBytesPerSecond]` (defaults **1 .. 99999**
KB/s) via the view-model's `AdjustInput`, clamped in the *view-model on every change* (a reactive
subscription re-adjusts). The getter additionally clamps a persisted value on load and maps 0
→ unlimited. → **belegt**

**Semantics of 0:** the *setting* stores KB/s; **0 means "unlimited"** and is exactly how the
throttling-enabled radio ("Nicht begrenzen") is persisted (`… = enabled ? value : 0`). The
view-model derives `ThrottlingEnabled = value != 0` when loading. → **belegt**

**Where the cap is enforced (not a UI detail, but relevant for parity):** the value is a
**stream-level** cap: `Math.Max(customerRate, NetworkThrottlingMaxUpload…)*1024` bytes/s wraps
the upload stream of *each part* in a throttled stream (download path likewise), re-read every
10 s (cached) so a live settings change takes effect without restart. `0` disables the wrapper.
This is a **different mechanism** from our per-file pacing (see §5). → **belegt**

#### 3.3.1 Queue-count defaults and bounds (belegt)

| Constant | Default | Meaning |
|---|---|---|
| `UploadMaxQueues` (app settings) | **4** | device-default upload worker cap |
| `MaxNumberOfQueuesPerProcessorCore` | **1.0** | per-core factor |
| `UploadQueues` (app settings) | `max(1, min(4, ProcessorCount * 1.0))` | **effective default** → on an 8-core machine 4; on a 2-core machine 2 |
| `CustomerSettingsMinSupportedUploadQueues` | **1** | slider minimum |
| `CustomerSettingsMaxSupportedUploadQueues` | **8** | slider maximum (the "8" in our AGENTS.md is this *ceiling*, not a user default) |
| `IgnoreCustomerSettingsUploadQueuesSetting` | **false** | when `false` the customer value is used verbatim; when `true` the device default caps it (`min(app, customer)`) |
| `DownloadMaxQueues` / `DownloadQueues` | 4 / same formula | download side |
| `SyncMaxQueues` | 20 | sync engine (not in the UI) |
| `FileScannerMaxQueues` | 12 | folder scanner (not in the UI) |

The upload queue count is consumed by the manual-upload queue pump (5 s cache) and re-read on
`customerSettings.Changed`. → **belegt**

**Not a user setting:** `LowPriorityUploadDistributionFactor` (0.25) and the high/low priority
slot split that distributes queues between backup and manual uploads exist internally. → **belegt**

### 3.4 Proxy tab ("Proxy")

Three-state radio group — exactly the macOS convention, plus server/port fields that only
become editable in manual mode: → **belegt**

| Control | Backing setting | Default | Persisted in | UI texts (de) |
|---|---|---|---|---|
| "Kein Proxy" | `ProxySettingsType.NoProxy` | — | `settings.json` key `ProxySettingsType` (enum name as string) | "Kein Proxy", "Wir stellen eine direkte Internetverbindung her, die die Proxy-Informationen des Betriebssystems ignoriert." |
| "Automatische Erkennung" | `ProxySettingsType.AutoDetect` | **default** | same | "Automatische Erkennung", "Für Internetverbindungen benutzen wir die Proxy-Einstellungen des Betriebssystems." |
| "Manuell" | `ProxySettingsType.Manual` | — | same | "Manuell", "Sie definieren Ihre eigenen Proxy-Einstellungen. Bitte wenden Sie sich an Ihren Netzwerkmanager oder ISP, wenn Sie nicht wissen, wie die richtigen Einstellungen sind." |
| Server field | `ProxyServer` (`IpAddress`) | `""` | `settings.json` | "Server" |
| Port field | `ProxyPort` (`int?`) | `null` | `settings.json` | "Port" |
| Group label | — | — | — | "Proxy-Einstellungen:" |

Switching away from manual **clears** server and port in the view-model (reactive subscription),
and the fields are visually disabled (greyed text/background) and emptied when not editable. The
port field strips non-digits. On OK, manual mode is **validated first**: hostname valid **and**
port present and in range, otherwise a warning is shown ("Ungültige Proxy-Adresse" / "Die
angegebene Proxy-Adresse enthält ungültige Zeichen. Bitte geben Sie eine gültige Adresse ein.")
and the whole save is **aborted** (no partial write). → **belegt**

*Inferred:* the enum is presumably fed into the HTTP stack's proxy configuration; the consumer
was not traced. → **UNKNOWN** (§6)

### 3.5 Sync tab ("Synchronisieren") — conditional, out of scope

The tab exists only under a treatment flag: it is added when
`CanEnableSync || AppMode has Sync || PreviouslyEnabledSync` **and not**
`HasSyncBeenMigratedToBackup`. Contents: enable/disable-sync buttons, a selective-sync folder
picker ("Ordner wählen..."), and the `ExcludeNewCloudFolders` switch (default **`true`**:
"Synchronisierung für neue Ordner auf oberster Ebene deaktivieren, die ich auf Amazon Photos
erstelle"). Sync is explicitly **out of the port's MVP**; recorded here for completeness and
because `ExcludeNewCloudFolders` persists in `settings.json`. → **belegt** (tab logic) / out of
scope (per AGENTS.md)

### 3.6 Backup tab ("Datensicherung") — dead in this build

The tab class exists with exactly one control — a reset button ("Datensicherung zurücksetzen",
confirmation "Datensicherung zurücksetzen?") and its reset-confirmation strings — but
`Initialize()` **never adds it** to the tab control (grep: the constructor creates it,
`View.TabControl.Add` is called only for general/sync/bandwidth/proxy). → *inferred* dead (the
absence of the `Add` call is **belegt**). The per-backup interval/content-filter defaults that
this tab would own live in the backup *configuration* window instead (`SPEC_BACKGROUND_SYNC.md`),
and the preferences save path writes their defaults (§3.7). → **belegt** / *inferred*

### 3.7 Apply / save semantics (the OK path)

The window has **OK** and **Abbrechen**; OK's action is `ApplyAndClose`. The view-model tracks
dirtiness per property (`SetDirtyIfChanged`); saving is a five-part parallel write: → **belegt**

| Save step | Writes |
|---|---|
| `SaveAccountSettings` | `ShowNotifications`, `EnableNotificationBadging`, `ExcludeHiddenFilesAndFolders`, autostart via `AppStartup` (with failure dialog + system-preferences jump), `FilterOutPotentialDuplicatesFromApplePhotosLibrary` (re-queue side effect) |
| `SaveBandwidthSettings` | `Download/UploadTransferKiloBytesPerSecond` (0 = off), `Download/UploadQueues` |
| `SaveSyncSettings` | `ExcludeNewCloudFolders`, plus the selective-sync tree changes via a confirmation modal when folders were switched off |
| `SaveProxySettings` | `ProxySettingsType`, `ProxyServer`, `ProxyPort` |
| `SaveBackupSettings` | `BackupItemInterval` (stored as an **interval expression string**, default `Immediate`), `ContentFilter` (default `PhotosAndVideo`), `EnableAutoConnect`, `EnableDeduplicationForPhotosAndVideos` (default `false` — post-onboarding default; the onboarding default is `true`) |

While a save is in flight the OK button reads "Wird angewendet..." and all three footer buttons
disable. Cancel = `Close` **without** any write. Every change additionally emits business/health
metrics (e.g. `ExcludeHiddenFiles` On/Off, `StartOnStartup`, `UploadThread`, `ProxySettingsType`,
rate-limit selections). → **belegt**

Note the asymmetry: **autostart is applied immediately via the OS** (and on failure rolled back
in the UI), whereas everything else is only written on OK. → **belegt**

### 3.8 Upload / network constants that bound the settings (app settings, not user-editable)

All defaults below are `_db.AutoGet(default, "Key")` reads from `app-settings.json`; the key names
are the property names. None appears in the preferences UI except where §3.3 says so. → **belegt**

| Key | Default | Relevance |
|---|---|---|
| `UploadMaxQueues` | 4 | device cap for the queue slider's *default* (§3.3.1) |
| `MaxNumberOfQueuesPerProcessorCore` | 1.0 | same |
| `CustomerSettingsMax/MinSupportedUploadQueues` | 8 / 1 | slider bounds |
| `CustomerSettingsMax/MinSupportedDownloadQueues` | 8 / 1 | ditto, download |
| `CustomerSettingsDefaultBandwidthRate…IfThrottlingIsEnabled` | 100 | pre-fill when enabling throttling |
| `CustomerSettingsMin/MaxSupportedBandwidthThrottleRate…` | 1 / 99999 | clamp bounds (KB/s) |
| `NetworkThrottlingMaxUpload/DownloadTransferKiloBytesPerSecond` | 0 | server-imposed ceiling; the effective cap is `max(customer, this)` |
| `NetworkThrottlingMaxSleepTimeInSeconds` | 32 | throttled-stream internals |
| `CloudDriveServiceUploadMaxFileSizeInBytes` | 52,428,800,000 (~50 GB) | max upload size |
| `ResumableUploadsEnabled` | **true** | resumable/multipart path on |
| `ResumableUploadsMinSizeInBytes` | 52,428,800 (50 MB) | threshold from which a file goes resumable |
| `FileSizeThreshold` | 52,428,800 | same order of magnitude, metrics bucketing |
| `ParallelDownloadMinPartSizeInBytes` | 262,144,000 (250 MB) | download part size |
| `ParallelDownloadMaxParts` | 4 | max parallel download parts |
| `BackupScanIntervalInHours` | **336** (14 days) | full rescan cadence of a backup folder (`BackgroundSync` spec) |
| `BackupScannerCheckIntervalInHours` | 24 | backup scanner check cadence |
| `BackupDeepScanIntervalInHours` | 168 | deep rescan |
| `BackupWatcherPollIntervalInSeconds` | 30 | FSEvents poll |
| `ManualUploadMaxRetries` / `ManualUploadPollIntervalInSeconds` | 5 / 15 | manual queue retry policy |
| `ManualUploadQueueTimeoutInSeconds` | `uploadTimeout+30` = 150 | queue timeout |
| `UploadDelayInSeconds` | 0 | inter-upload delay |
| `SettingsRefreshTimeInSeconds` | 3600 | refresh cadence of **app-settings** and the update check |
| `DefaultAutoUpdateAfterTimeInSeconds` | **1800** (30 min) | auto-update grace period (§3.10) |
| `NotifyForUpdateInSeconds` | 12,960,000 (15 days) | minimum gap between "update available" notifications |
| `MaxNumberOfConcurrentNetworkConnections` | `max(2, cores*12)` | ServicePoint tuning at startup |
| `CloudDriveServiceUploadMinTransferBytesPerSecond` + `MaxConsecutiveSlowUploadEvents` | 500 / 5 | slow-transfer warning threshold (surfaced in the UI as "slow upload" notices, `SPEC_UPLOAD_PROGRESS.md`) |

**On "UploadMaxPartSize / chunk size":** there is **no** client-side part-size setting. The
multipart upload path is fully **server-dictated**: the initiate call returns `partSize`
(and `serverSidePartSize` on each part), and the client splits the file with exactly that value;
`UploadPartSize` exists only as a *query parameter* the client **echoes back**, and the progress
model derives remaining time from it. So the original has no user-visible Chunk-Größe control —
this doc records the term as **not applicable** rather than UNKNOWN. → **belegt**

### 3.9 Backup-related *defaults* stored in preferences (belegt)

| Key (settings.json) | Default | Notes |
|---|---|---|
| `BackupItemInterval` | interval expression `Immediate` | string form of an interval expression; the backup configuration window reads it as the default for new backups and writes the user's choice back here on OK |
| `ContentFilter` | `PhotosAndVideo` | enum `None=0, Photos=2, Videos=4, Other=8, PhotosAndVideo=6, Everything=14`; the backup window inherits it as the default content filter |
| `EnableDeduplicationForPhotosAndVideos` | `false` (post-onboarding) / `true` (onboarding default) | "avoid duplicates" default per new backup |
| `EnableAutoConnect` | `true` | general tab switch (§3.2) |
| `BackupItemsDisplayMode` | `DrillDown` | list presentation, not a preferences control |

### 3.10 Auto-update (belegt)

Mechanic, in order:

1. **Startup check, silent.** `AppUpdaterWorker.Start()` runs `CheckForUpdate()` once
   (`notify=false`) and then arms a timer that ticks every `SettingsRefreshTimeInSeconds`
   (default 1 h) into `CheckForUpdateAndNotify()` (`notify=true`). Network-gated: only when the
   network detector reports available. → **belegt**
2. **Variant resolution.** Five CloudFront `…/update/*.update` manifests are fetched in parallel
   (app-version, app-version+OS-model, app-version+OS-model+OS-version, app-version+device-serial)
   and the first non-null wins, with an OS-incompatibility special case; each becomes an
   `IUpdateSettings` (`S3UpdateSettings`) carrying version, `Arguments`, `DialUpValue`, and
   `AutoUpdateAfterTimeInSeconds` (which *delegates to the app setting*, default 1800 s).
   → **belegt**
3. **Eligibility gate.** `IsCustomerEligibleForUpdate` derives a number from the device serial
   id (`ToDialupValue`) and requires it **below** the manifest's `DialUpValue` (hard 100 in the
   deployment settings class) — i.e. a rollout-percentage/staged-rollout throttle. Ineligible →
   no update, a health metric only. → **belegt**
4. **Compatibility gate.** `ICompatibilityChecker.CheckCompatibility()` returns a reason; `None`
   → "compatible update" path; anything else (e.g. unsupported OS) → an
   `IncompatibleUpdateAvailableEvent` (separate dialog flow, `SPEC_NOTIFICATIONS.md`
   territory). The supported-OS floor is also in app settings
   (`MinimumSupportedMacVersion` = 10.10.0; the Info.plist declares `LSMinimumSystemVersion`
   10.15 for this build). → **belegt**
5. **Compatible update, notify=false (startup):** if the settings say "notify only for sync"
   (`NotifyUpdateForSync`) **or** the grace period is 0, the event is merely published; otherwise
   the worker **waits `AutoUpdateAfterTimeInSeconds`** and then updates the app — the startup
   check therefore auto-installs a pending update ~30 min after launch without asking.
   → **belegt**
6. **Compatible update, notify=true (hourly tick):** publishes `UpdateAvailableEvent`; the
   handler decides by window state: **drive window open** → a popup notification ("Es ist ein
   Update verfügbar", action "Jetzt aktualisieren") *and* an in-app toast titled
   "Aktualisierung in {0} Minuten" ("Uploads werden nach der Aktualisierung fortgesetzt.");
   dismissing the toast clears the pending update); **window closed** → update immediately in
   quiet mode. A `_isUpdating` latch prevents double-updates; a second notification is suppressed
   for `NotifyForUpdateInSeconds` (15 days) when the notify-for-sync treatment applies.
   → **belegt**
7. **Installing.** `AppUpdateService.UpdateApp` launches a **stub installer** zip from CloudFront
   (`…/mac/AmazonPhotosInstallerApp.zip`) with `--quiet` (default) or `--unattended` (user
   clicked "update now"), plus `--show-status-window` when the drive window is open, plus the
   manifest's own arguments. Failure → a modal ("Neue Version verfügbar" / "Eine neue Version von
   Amazon Photos wurde veröffentlicht. Klicken Sie auf "OK", um die Downloadseite zu öffnen.")
   whose OK opens the download page. → **belegt**

There is **no user-facing on/off switch for auto-update** anywhere in the preferences UI — the
only user controls are the notification actions, and the grace period/treatment are
server-driven. → **belegt**

### 3.11 About window (belegt)

Opened from the app/overflow menu ("Mehr dazu", identifier `ProfileContextMenu_About_Button`)
and from the macOS menu bar's first submenu; presented **as a sheet** of the drive window
(transparent titlebar, movable-by-background), 504×410 pt. Closed via the "Schließen" capsule
button or the window close. Two layouts are selected by a **weblab treatment**
(`EnableAboutViewSimplification` == `"T1"` → the simplified 624×572 view; otherwise the full
900×572 view). → **belegt**

Content (identical in both layouts):

| Element | Source | UI text (de) |
|---|---|---|
| App logo | asset `AmazonPhotosLogo` | — |
| Version | `aboutwindow_versionformat` + `IAppInfo.Version` | "Version: {0}" |
| Device serial number | `aboutwindow_deviceSerialNumberformat` + `IAppInfo.DeviceSerialId` | "Geräte-Seriennummer: {0}" |
| Send-logs box: description + capsule button + spinner | `send_logs_tooltip` / `send_logs_button_text` | "Helfen Sie uns dabei, Amazon Photos zu verbessern und Probleme zu behesen…" (verbatim: "Helfen Sie uns dabei, Amazon Photos zu verbessern und Probleme zu beheben, indem Sie uns Ihre Aktivitätsprotokolle senden."), button "Protokolle senden" |
| Terms of Use link | `CloudDriveLink.TermsOfUse` (signed-in) / `TermsOfUseFlag` (signed-out) | "Nutzungsbedingungen" |
| Privacy Notice link | `CloudDriveLink.PrivacyNotice` | "Datenschutzerklärung" |
| Third-Party Licenses link | `CloudDriveLink.ThirdPartyLicenses` | "Drittanbieterlizenzen" |
| Copyright line | `aboutwindow_copyright` with the current year | "© 2012-{0} Amazon.com, Inc. und Tochtergesellschaften. Alle Rechte vorbehalten." |
| Close | `about_close` | "Schließen" |

The links open **web URLs** built by a domain provider: `/photos/desktop/termsofuse`,
`/photos/desktop/privacy`, `/photos/desktop/thirdpartylicenses` (marketplace-prefixed), with the
signed-out ToS variant pointing at `amazon.com/cd/tou/flagpage`. Sending logs runs a command with
a spinner, then success/failure dialogs ("Protokolle gesendet" / "Protokolle können nicht gesendet
werden"). The window title string (`aboutwindow_title` = "Über Amazon Photos") is defined but
**unreferenced** (the titlebar is hidden). → **belegt**

**Hint-pflicht relevance for our port** (AGENTS.md marks About / Third-Party-Lizenzen as
release-relevant): the original ships three legal links plus a copyright line in About; a
third-party-licenses notice is the *only* one of these that is a hard obligation for us
(we bundle crates under MIT/Apache-2.0). → **belegt** (mechanism) — which exact notices we owe is
a legal question outside this doc. → **UNKNOWN**

---

## 4. String evidence (keys → German)

Keys cited in this doc, verbatim from `de.lproj/Localizable.strings`. `…` stands for the prefix
`clouddrive_desktop_`. "Referenced?" = whether the managed assembly has a `Strings.<key>` call
site (searched in the full decompilation dump). → **belegt**

| Key (… = `clouddrive_desktop_`) | German | Referenced? |
|---|---|---|
| `…preferences_window_title` | "Voreinstellungen" | yes (`PreferencesWindowService`) |
| `…preferences_okButton` | "OK" | yes (`PreferencesView`) |
| `…applying` | "Wird angewendet..." | yes (`PreferencesViewController.ToggleBusyState`) |
| `…cancel` | "Abbrechen" | yes (`PreferencesView`) |
| `…contextmenu_preferences` | "Voreinstellungen" | yes (app menu + menu bar, key equivalent ⌘,) |
| `…contextmenu_preferences_windows_gesture` | "Alt+P" | **no** (Windows-only string in a macOS bundle) |
| `…preferences_general_title` | "Allgemeines" | yes (tab label) |
| `…preferences_bandwidth_title` | "Bandbreite" | yes (tab label) |
| `…preferences_proxy_title` | "Proxy" | yes (tab label) |
| `…preferences_sync_title` | "Synchronisieren" | yes (tab label) |
| `…enablesync` / `…disablesync` | "Synchronisierung aktivieren..." / "…deaktivieren..." | yes (`SyncPreferencesTabView`) |
| `…preferences_backup_title` | "Datensicherung" | yes (`BackupPreferencesTabView`) |
| `…preferences_backup_reset_title` | "Datensicherungseinstellungen löschen:" | yes (`BackupPreferencesTabView`) |
| `…preferences_backup_reset_button_text` | "Datensicherung zurücksetzen" | yes (`BackupPreferencesTabView`) |
| `…preferences_backup_resetconfirmationheader` | "Datensicherung zurücksetzen?" | **no** |
| `…preferences_backup_resetconfirmationtextmessage` | "Sind Sie sicher, dass Sie alle Datensicherungseinstellungen löschen wollen?" | **no** |
| `…preferences_account_title` | "Konto" | yes (avatar tooltip only — not a tab) |
| `…preferences_advanced_title` | "Erweitert" | **no** |
| `…preferences_applyButton` | "Übernehmen" | **no** (only OK/Abbrechen exist in this build) |
| `…preferences_bandwidth_settings_title` | "Bandbreiten-Einstellungen" | **no** (tab uses `…bandwidth_title`) |
| `…preferences_diagnostics_section_title` | "Diagnosen:" | **no** (diagnostics window is a separate, unbundled surface) |
| `…preferences_signed_in_title` | "Angemeldet als:" | yes (`GeneralPreferencesTabView`) |
| `…preferences_signout` | "Abmelden" | yes (`GeneralPreferencesTabView`) |
| `…preferences_manageStorage` | "Speicher verwalten" | yes (`GeneralPreferencesTabView`) |
| `…preferences_system_settings` | "Systemeinstellungen" | yes (`GeneralPreferencesTabView`) |
| `…preferences_auto_start_application` | "Amazon Photos beim Starten öffnen" | yes (`GeneralPreferencesTabView`) |
| `…preferences_insufficientpermission_opensystempreferences` | "Sie haben keine Berechtigung zum Ändern der Start-Einstellung. Gewähren Sie die Berechtigung in den Systemeinstellungen." | yes (autostart-failure dialog) |
| `…preferences_option_notavailable` | "Die Option ist leider nicht verfügbar" | yes (same dialog) |
| `…preferences_insufficientpermission_runasadmin` | "…Wählen Sie „Als Administrator ausführen“…" | **no** (Windows path) |
| `…preferences_includehiddenfiles` | "Ausgeblendete Dateien hochladen" | yes (`GeneralPreferencesTabView` — the **positive** checkbox label) |
| `…preferences_excludehiddenfiles` | "Keine ausgeblendeten Dateien hochladen" | **no** (mirror key; the backing setting is the negative `ExcludeHiddenFilesAndFolders`) |
| `…preferences_uploadhiddenfiles_tooltip` | "Ausgeblendete Dateien werden nur bei lokaler Bearbeitung ignoriert…" (verbatim see strings file) | yes (sub-label under the checkbox) |
| `…preferences_backup_enable_autoconnect` | "AutoConnect aktivieren" | yes (`GeneralPreferencesTabView`) |
| `…preferences_autoconnect_tooltip` | "AutoConnect erkennt neue Geräte und ermöglicht Ihnen, eine Sicherung für Ihr Gerät einzurichten. Andernfalls werden externe Geräte immer ignoriert." | yes (sub-label) |
| `…preferences_enablenotificationbadges` | "Aktivieren Sie Badges für Benachrichtigungen" | yes (`GeneralPreferencesTabView`) |
| `…preferences_filter_potentialduplicates_title` | "Ignorieren Sie Bilder, die möglicherweise Duplikate aus der Amazon Photos iOS-App sind." | yes (checkbox label + tooltip) |
| `…preferences_filter_potentialduplicates_description` | "Vermeiden Sie es, dass aufgrund der Sicherung über die iOS-App für Amazon Photos mögliche Duplikate in der Apple Fotobibliothek erstellt werden." | yes (sub-label) |
| `…preferences_enable_notifications` | "Benachrichtigungen anzeigen" | **no** (legacy master switch; `ShowNotifications` is saved but no checkbox binds it) |
| `…preferences_bandwidth_uploads_title` | "Uploads:" | yes (`BandwidthPreferencesTabView`) |
| `…preferences_bandwidth_downloads_title` | "Downloads:" | yes |
| `…preferences_upload_rate_title` | "Upload-Rate pro Datei" | yes |
| `…preferences_download_rate_title` | "Download-Rate pro Datei" | yes |
| `…preferences_bandwidth_dont_limit_title` | "Nicht begrenzen" | yes (both radio groups) |
| `…preferences_bandwidth_limit_to_title` | "Begrenzen auf" | yes |
| `…preferences_bandwidth_rate` | "KB/Sekunde" | yes |
| `…preferences_bandwidth_uploads_slider_title` | "Gleichzeitige Uploads:" | yes |
| `…preferences_bandwidth_downloads_slider_title` | "Gleichzeitige Downloads:" | yes |
| `…preferences_proxy_proxysettingstype_label` | "Proxy-Einstellungen:" | **no** (only used by a legacy/nib layout; the code builds the group without it) |
| `…preferences_proxy_noproxy` | "Kein Proxy" | yes |
| `…preferences_proxy_noproxy_description` | "Wir stellen eine direkte Internetverbindung her, die die Proxy-Informationen des Betriebssystems ignoriert." | yes |
| `…preferences_proxy_autodetect` | "Automatische Erkennung" | yes |
| `…preferences_proxy_autodetect_description` | "Für Internetverbindungen benutzen wir die Proxy-Einstellungen des Betriebssystems." | yes |
| `…preferences_proxy_manual` | "Manuell" | yes |
| `…preferences_proxy_manual_description` | "Sie definieren Ihre eigenen Proxy-Einstellungen. Bitte wenden Sie sich an Ihren Netzwerkmanager oder ISP, wenn Sie nicht wissen, wie die richtigen Einstellungen sind." | yes |
| `…preferences_proxy_serverlabel` | "Server" | yes |
| `…preferences_proxy_portlabel` | "Port" | yes |
| `…preferences_proxy_warning_title` | "Ungültige Proxy-Adresse" | yes (validation dialog) |
| `…preferences_proxy_warning_text` | "Die angegebene Proxy-Adresse enthält ungültige Zeichen. Bitte geben Sie eine gültige Adresse ein." | yes |
| `…preferences_selective_sync_title` | "Wähle Ordner in deinen Amazon Photos zum Synchronisieren aus:" | yes (sync tab) |
| `…preferences_selective_sync_button_text` | "Ordner wählen..." | yes (sync tab) |
| `…preferences_exclude_syncing_new_cloud_folders` | "Synchronisierung für neue Ordner auf oberster Ebene deaktivieren, die ich auf Amazon Photos erstelle" | yes (sync tab) |
| `…preferences_sync_new_cloud_folders` | "Automatisch neue Ordner mit Amazon Photos synchronisieren" | **no** |
| `…aboutwindow_title` | "Über Amazon Photos" | **no** (titlebar hidden) |
| `…aboutwindow_versionformat` | "Version: {0}" | yes (both layouts) |
| `…aboutwindow_deviceSerialNumberformat` | "Geräte-Seriennummer: {0}" | yes (both layouts) |
| `…aboutwindow_termsofuse` | "Nutzungsbedingungen" | yes (both layouts) |
| `…aboutwindow_privacynotice` | "Datenschutzerklärung" | yes (both layouts) |
| `…aboutwindow_thirdpartylicenses` | "Drittanbieterlizenzen" | yes (both layouts) |
| `…aboutwindow_copyright` | "© 2012-{0} Amazon.com, Inc. und Tochtergesellschaften. Alle Rechte vorbehalten." | yes (`AboutViewModel`) |
| `…about_close` | "Schließen" | yes (both layouts) |
| `…send_logs_tooltip` | "Helfen Sie uns dabei, Amazon Photos zu verbessern und Probleme zu beheben, indem Sie uns Ihre Aktivitätsprotokolle senden." | yes (both layouts) |
| `…send_logs_button_text` | "Protokolle senden" | yes |
| `…send_logs_success_dialog_title` / `…_message` | "Protokolle gesendet" / "Vielen Dank, dass Sie uns bei der Fehlersuche geholfen haben." | yes (`SendLogsCommand`) |
| `…send_logs_failure_dialog_title` / `…_message` | "Protokolle können nicht gesendet werden" / "Bitte versuchen Sie es später erneut." | yes |
| `…preferences_send_logs_button_text` / `…preferences_send_logs_tooltip` | "Protokolle senden" / "Senden Sie uns Ihre Aktivitätsprotokolle…" | **no** (legacy preferences variant; About uses the `…send_logs_*` pair) |
| `…contextmenu_support_about` | "Mehr dazu" | yes (About menu item) |
| `…mac_quit` | "Amazon Photos verlassen" | yes (app menu) |
| `…notification_update_title` | "Aktualisierung in {0} Minuten" | yes (update toast, non-sync treatment) |
| `…notification_update_button` | "Jetzt aktualisieren" | yes |
| `…notification_update_message` | "Uploads werden nach der Aktualisierung fortgesetzt." | yes |
| `…notification_syncupdate_title` / `…_button` / `…_message` | "Update verfügbar" / "Aktualisieren" / "Eine neue Version von Amazon Photos ist verfügbar." | yes (sync-treatment variant of the same toast) |
| `…statuspopup_notification_updateavailable` / `…_updatenow` | "Es ist ein Update verfügbar" / "Jetzt aktualisieren" | yes (popup notification) |
| `…update_available_title` / `…_message` / `…_update​now` / `…_notnow` | "Update verfügbar" / "Es ist ein Update für Amazon Photos verfügbar. Möchten Sie die APP jetzt aktualisieren?" / "Update installieren" / "Vielleicht später" | **no** (legacy update dialog, superseded by the toast+popup flow) |
| `…stubupdater_notfound_title` | "Neue Version verfügbar" | yes (`AppUpdateService` failure dialog) |
| `…stubupdater_notfound_message` | "Eine neue Version von Amazon Photos wurde veröffentlicht. Klicken Sie auf \"OK\", um die Downloadseite zu öffnen." | yes |
| `…dotnet_update_information` | "Das ausstehende Anwendungs-Update erfordert eine neuere Version von Microsoft .NET…" | **no** (unreferenced; Mono/.NET-update path not wired in this build) |
| `…preferences_signout_dialog_title` | "Wollen Sie sich von diesem Gerät abmelden?" | **no** (sign-out confirmation lives in the deregistration flow, `SPEC_LOGIN.md`) |

Rows marked **no** are defined but not referenced by the managed assembly — *inferred* dead, or
rendered by a nib/legacy layout outside this assembly. Note the corrections this table forces:
the active bandwidth tab title is `…bandwidth_title` ("Bandbreite"), **not**
`…bandwidth_settings_title`; the active hidden-files label is `…includehiddenfiles`
("Ausgeblendete Dateien hochladen"), **not** `…excludehiddenfiles`; and the log-sending button
lives in About under `…send_logs_button_text`, **not** `…preferences_send_logs_*`. → **belegt**

---

## 5. Gap analysis vs. our port (committed state)

| Capability | Original | Our port | File reference |
|---|---|---|---|
| Preferences window shell | separate window, 700×600, 4 tabs + OK/Abbrechen, dirty tracking, "Wird angewendet..." busy state | a page inside the main window with 2 tabs ("Allgemeines"/"Bandbreite") and per-tab "Speichern" | `src/settings.rs` (`settings_ui`, `Page`/`SettingsTab` in `src/app.rs`) |
| Account block | name + identifier + "Abmelden" + "Speicher verwalten" | "Angemeldet als:" + "Abmelden" (no storage link — deliberate, no quota fetch) | `src/settings.rs` (`general_settings`), `src/app.rs` (sidebar) |
| Sign-out | `DeregisterUserIntent` (server-side device deregistration) | local logout (cookies cleared) | `src/login.rs` (`logout`) |
| Autostart / login item | `AppStartup` via `System Events` AppleScript; failure dialog + jump to System Settings | `auto-launch` crate, `MacOSLaunchMode::SMAppService` (macOS 13+; `SMAppService.mainApp.register()` → visible in the **Anmeldeobjekte**; below 13 `Err(UnsupportedOS)` ⇒ the existing "unsupported" path); a **deliberate deviation** from the original's `osascript`/System Events (`SPEC_OS_INTEGRATION.md` §3.6); the **intent** is persisted in `Config.autostart_enabled: Option<bool>` (`Some(true)`/`Some(false)`, `Some(false)` = veto) — the OS state stays the operative truth | `src/app.rs` (`build_autostart`, `Autostart`, `set_autostart`), `Cargo.toml` (`auto-launch = "0.6"`) |
| Hidden files | `ExcludeHiddenFilesAndFolders`, default **true**, `settings.json` | **none** — `scan_images` walks without a hidden-file filter | `src/folders.rs` (`scan_images`) |
| AutoConnect | switch + tooltip, default true | out of scope (no external-device concept) | — |
| Notification badging | switch, default true | **none** | — |
| Notifications master switch | `ShowNotifications` saved, no visible checkbox | **notifications bool** persisted in `Config` (`config.toml`, default true) — *stronger* than the original (the original's is effectively dead) | `src/config.rs` (`notifications`), `src/settings.rs` |
| Apple-Photos-iOS dedup filter | switch + re-queue side effect, default false | none (no iOS import path) | — |
| Language | **no** in-app setting (OS-locale bundle) | **deliberate divergence**: de/en combo, `Option<String>` in `Config`, default = OS locale | `src/config.rs` (`language`), `src/i18n.rs`, `src/settings.rs` ("Sprache") |
| Upload rate cap | radio pair + field, KB/s, clamp 1..99999, **stream-level** throttle (cached 10 s), default 100 on enable | radio pair + `DragValue`, KB/s, default 100 on enable; **per-file pacing** after upload (`limit_kbps` in `upload_worker`) | `src/settings.rs` (`bandwidth_settings`), `src/upload.rs` (~line 541), `src/backup.rs` (`limit`) |
| Upload concurrency | slider 1..8 (customer ceiling 8), default `min(4, cores)`, `settings.json` | `egui::Slider` 1..8, **hard-coded default 3**, **runtime-only** (not in `Config`) | `src/app.rs` (`concurrency: 3`), `src/settings.rs`, `src/upload.rs` (`workers = concurrency.max(1).min(count)`) |
| Download concurrency / rates | full download group on the same tab | placeholder text only ("Downloads sind in diesem Port noch nicht implementiert.") | `src/settings.rs` (`bandwidth_settings`) |
| Proxy | 3-state radio + server/port + validation + warning dialog | **none** | — |
| Sync tab | conditional tab, selective sync, `ExcludeNewCloudFolders` | out of scope (AGENTS.md: no bidirectional sync) | — |
| Backup tab | reset-all-backups button, **dead** (never added) | none; per-target edit exists instead | `src/backup_config.rs` |
| Per-backup defaults in prefs | `BackupItemInterval` (Immediate), `ContentFilter` (PhotosAndVideo), `EnableDeduplicationForPhotosAndVideos` (false) stored in `settings.json` | per-target fields live in `folders.toml` (`dedup`, conflict policy); **no interval/content-filter defaults** | `src/folders.rs` (`FolderStore`, `ConflictResolution`), `src/backup_config.rs` |
| Slow-Mode (temporary concurrency=1) | not present as such — the original has no "one file at a time" toggle; concurrency is a slider | `slow_mode` checkbox, **run-scoped, reset on restart** | `src/app.rs` (`slow_mode`), `src/backup.rs` (`if self.slow_mode { 1 }`) |
| Persistence format | `settings.json` (flat JSON, hot reload via `FileSystemWatcher`) + `app-settings.json` (server, 1 h refresh) + OS login items + SQLite | `config.toml` (TOML, explicit save) + `folders.toml` + SQLite (dedup index) | `src/config.rs`, `src/folders.rs`, `src/dedup.rs` |
| Live reload of settings | yes — `Changed` observable, subscribers re-read (queues, throttle) without restart | no — settings are read at run start; `Save` writes the file, a run takes the current values | `src/settings.rs` (`save_config`), `src/backup.rs` |
| Auto-update | CloudFront manifests, rollout gate, 30-min grace, stub installer `--quiet/--unattended`, failure dialog | **none** (GitHub Release + manual build per AGENTS.md; `../dev/RELEASE.md`) | — (deliberate: different distribution channel) |
| About window | sheet with version, device serial, send-logs, 3 legal links, copyright | **none** — no About dialog, no third-party-license notice; marked "release-relevant" in AGENTS.md | — (gap) |
| Legal links | ToS / Privacy / Third-Party-Licenses → web URLs | none | — (gap) |
| Send-logs | command + spinner + success/failure dialogs | log facade exists in-app (`UploadShared::log`) but no export/send | `src/upload.rs` |
| Update notification texts | toast "Aktualisierung in {0} Minuten" + popup | n/a | `SPEC_NOTIFICATIONS.md` |

Two deliberate divergences worth restating (per AGENTS.md, so they are not "fixed" by accident):
(1) the **language override** — the original has none; (2) the **persistent pause** and the
**notifications master switch** are real config in ours while the original's `ShowNotifications`
is saved-but-unbound. → **belegt**

---

## 6) UNKNOWN (explicitly not determined)

- **`ShowNotifications` enforcement.** The setting is written on save and defaults to true, but
  no consumer was found that gates the notification centre/status popup on it in this assembly.
  Whether it is honoured outside the decompiled code (nib/legacy layout or another bundle) is
  **UNKNOWN**. → *inferred* dead
- **`EnableNotificationBadging` effect.** Only the business metric on change is observable; the
  dock-badge consumer was not traced. → **UNKNOWN**
- **Proxy consumer.** The `ProxySettingsType`/server/port values are persisted, but the HTTP
  client wiring that applies them (or whether the Mono/http-client stack reads the OS settings
  for the AutoDetect case) was not traced. → **UNKNOWN**
- **Proxy "invalid address" exact rules.** Validation exists (hostname + port range), but the
  precise accepted grammar behind the helpers was not read. → **UNKNOWN**
- **Which weblab treatment this install gets** for `EnableAboutViewSimplification` (T1 vs full),
  and whether the "Account"/"Advanced" legacy tabs (`…preferences_account_title`,
  `…preferences_advanced_title`) ever render in this build. → **UNKNOWN**
- **`UploadMaxPartSize`/chunk size as a client knob**: confirmed *absent* (server-dictated) —
  the actual numbers the server returns for a given marketplace/file size were not captured and
  cannot be without running the app. → **UNKNOWN** (values), **belegt** (mechanism)
- **Update manifest contents / rollout fraction** — the CloudFront `*.update` files were not
  fetched (out of scope, no network recon); only the merge/eligibility logic is known.
  → **UNKNOWN**
- **Whether a save failure of a single key aborts the rest** — the five save steps run in
  parallel and the DB write can fail silently (it logs and returns false); the UI reaction to a
  partial failure was not traced. → **UNKNOWN**
- **Behaviour of the bandwidth fields when the persisted value exceeds the current clamp**
  (e.g. app settings changed to a lower max): the getter clamps on read, whether the UI then
  rewrites the clamped value is not observed. → **UNKNOWN**
- **The exact macOS Automation-permission prompt flow** (the original only shows its own dialog
  and offers the jump). → **UNKNOWN**
- **German labels for the legacy/unreferenced keys** — quoted from the strings file, but since
  no call site exists, whether they ever reached a user is **UNKNOWN** (*inferred* not).

---

## 7) Cross-references

- `SPEC_LOGIN.md` — sign-out button/deregistration, the Login flow the preferences account
  block departs from.
- `SPEC_BACKGROUND_SYNC.md` — backup cadence (`BackupScanIntervalInHours` = 336 h,
  `BackupScannerCheckIntervalInHours`, `BackupDeepScanIntervalInHours`, `BackupWatcherPollIntervalInSeconds`),
  pause, per-schedule records, and the per-backup window that owns interval/content-filter.
- `SPEC_UPLOAD_PROGRESS.md` — transfer UI, queue states, slow-transfer thresholds
  (`CloudDriveServiceUploadMinTransferBytesPerSecond`).
- `SPEC_NOTIFICATIONS.md` — the update notification texts and the popup centre (the
  `ShowNotifications` question, §6).
- `SPEC_DEDUP.md` — MD5 identity and the `EnableDeduplicationForPhotosAndVideos` /
  `FilterOutPotentialDuplicatesFromApplePhotosLibrary` switches.
- `SPEC_FOLDERS.md` — the scan that consumes `ExcludeHiddenFilesAndFolders`.
- `SPEC_ONE_TIME_UPLOAD.md` — the manual-upload queue whose worker count comes from
  `UploadQueues`.
- `UI_FLOWS.md` / `UI_STATES.md` — the clean-room UI spec the port's settings page is
  modelled on.
- `../dev/RELEASE.md` — why our port has no auto-update (different distribution channel).
- Our code: `src/config.rs` (`Config`, `notifications`, `language`, `background_sync`),
  `src/settings.rs` (`settings_ui`, `general_settings`, `bandwidth_settings`),
  `src/app.rs` (`PrimePhotoApp` fields, `set_autostart`, `save_config`), `src/upload.rs`
  (`upload_worker`, per-file pacing), `src/backup.rs` (run start: `slow_mode`/`concurrency`/
  `limit`), `src/folders.rs` (`FolderStore`, `scan_images`, `ConflictResolution`),
  `src/dedup.rs`, `src/platform.rs` (`open_url`, `reveal_in_file_manager`), `src/i18n.rs`,
  `src/tray.rs`, `src/login.rs`.
