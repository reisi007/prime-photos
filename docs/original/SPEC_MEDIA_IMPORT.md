# Apple Photos / iPhoto / Aperture Import, Videos & Media Handling — Clean-Room Spec

Behavioural specification of the **original** Amazon Photos desktop client's photo-library import
path and its media (photos vs. videos vs. non-media) handling, plus the mapping onto this port. As
with `SPEC_ONE_TIME_UPLOAD.md`, the behaviour below is **reverse-engineered from the shipped
managed assembly**, not inferred from string tables alone.

This document is **clean-room**: it records *observed behaviour* in our own words and tables. It
contains **no verbatim decompiled code** and **no personal data**. German user-facing labels are
quoted from the shipped `de.lproj/Localizable.strings` (product strings, not code); type and member
names are cited only as far as needed to describe the mechanism.

**Confidence convention.**
- **belegt** = read directly out of the reverse-engineered `Amazon Photos.dll` (member/branch), the
  German strings file, the bundle's `Info.plist`, or our committed code.
- *inferred* = reasoned from adjacent evidence, not read off a line.
- **UNKNOWN** = could not be determined from the inspected sources.

---

## 1. Purpose & scope

This doc owns the **Apple-Photos / iPhoto / Aperture import** and everything that makes a *media*
file different from a generic file in the original client: photo-library detection and resolution,
the one-time media import (`EnumerationService.OneTimeMediaImport`) with its cloud destination,
the iOS duplicate prompt (`iosdedupeprompt_*`), the video-vs-photo upload differences, the
handling of non-photo files, and the Autoplay/`mediaimport_autoplayaction` trace.

**Not owned here** (cross-referenced): drag & drop and the one-time queue row →
`SPEC_ONE_TIME_UPLOAD.md`; folder scan, `is_supported_image`, folder persistence →
`SPEC_FOLDERS.md`; MD5 duplicate identity, conflict handling, the full extension taxonomy →
`SPEC_DEDUP.md`; node/upload protocol → `../protocol/PROTOCOL.md`, `SPEC_UPLOAD.md`; cadence,
watchers, pause → `SPEC_BACKGROUND_SYNC.md`; progress UI and notifications →
`SPEC_UPLOAD_PROGRESS.md`, `SPEC_NOTIFICATIONS.md`.

---

## 2. Reverse-engineering method (belegt)

- Tooling: `ilspycmd` at `~/.dotnet/tools` (v11.1.0.9782), `PATH` extended and
  `DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`. A full type listing plus the
  whole decompiled dump and the targeted classes below were read. Nothing was installed.
  → **belegt**
- Target (read-only, never launched): `/Applications/Amazon Photos.app` →
  `Contents/MonoBundle/Amazon Photos.dll`, `Contents/Resources/de.lproj/Localizable.strings`,
  and `Contents/Info.plist` (read with `PlistBuddy`, no launch). → **belegt**
- Assembly version in the bundle is `CFBundleShortVersionString = 11.3.0`,
  `CFBundleIdentifier = com.amazon.clouddrive.mac`. → **belegt**
- Classes used as evidence: `Services.{PhotoLibraryService, MacPhotoLibrary}`,
  `Services.FileSystem.{PhotoLibraryEnumerator, PhotoLibraryFallbackEnumerator,
  StandardFileSystemEnumerator, FileSystemEnumeratorResolver, FolderFileCounter, FileFilter}`,
  `Services.FileSystem.Model.PhotoLibraryFileSystemObject`,
  `Services.Enumeration.EnumerationService` (`OneTimeMediaImport`, `EnumeratePaths`,
  `GetUploadRecord`, `IsValid`, `Walk`, `EnumerateSearchForMediaFiles`),
  `Database.ManualUpload.{ContentFilter, ContentFilterExtensions, ContentType, UploadType,
  TransferStatus, TransferError, UploadRecordExtensions, ManualUploadDatabase}`,
  `Helpers.{SupportedMimeMapping, SupportedContentTypes, SupportedExtension, MacFolderManager}`,
  `Services.ManualUpload.UploadStrategy.{WeighedUploadStrategy, SmallFileSizeUploadStrategy,
  LargeFileSizeUploadStrategy}`, `StartupTasks.{InstallAutoplayTask, UploadArgStartupTask,
  ProcessContextMenuPathsTask}`, `StartupContext` (CLI parsing),
  `Services.CloudDrive.CloudDriveStorageService` (`PreUploadCheck`, `UploadViaSimpleOrLegacyUploader`,
  `PublishHealthMetricsZeroByteAndOverFiveGigaByteVideoFile`),
  `Services.Events.{Models.MediaImportStartEvent, Notifications.MediaImportStartNotification,
  EventHandlers.MediaImportStartNotificationHandler}`,
  `UI.IntentHandlers.ShowPromptForApplePhotosDuplicatesIntentHandler`,
  `UI.{ContextMenuHelper, AutoSavePhotoLibraryNode}`,
  `UI.DriveWindow.DesktopBackupLegacyViews.{BackupConfigurationViewModel,
  BackupConfigurationView}`, `UI.OnboardingWindow.{FtueWindowViewModel, FtueFolderListViewModel,
  OnboardingInfo, SyncOnboardingPageViewModel}`, `UI.Helpers.TransferEnumExtensions`, `Constants`.
  → **belegt**

---

## 3. Mechanik, states, fields, persistence, UI texts

### 3.1 What counts as a "photo library" (belegt)

`PhotoLibraryService` holds a fixed UTI map. A path is a *library root* only when it ends in
`.photoslibrary` or `.migratedPhotoLibrary` (case-insensitive) **and** its UTI (read via
`NSUrl.TypeIdentifierKey`) is one of: `com.apple.photos.library` (Apple Photos),
`com.apple.photo.library` (iPhoto), `com.apple.aperture.library` (Aperture),
`com.apple.migratedphotolibrary` (migrated → treated as iPhoto). A path is *library content* when
it contains one of those two extensions and the prefix up to and including the bundle name is
itself a library root; a deny-list (home, Desktop, Documents, Music, Pictures, Movies, `~/Library`,
`/Applications`) is excluded first so a plain folder can never match. `TryGetPhotoLibrary(path)`
also resolves the **parent**, so a single file inside a library yields the library too. → **belegt**

Two consequences: **iPhoto and Aperture are recognised by file-type identifier, not by the Photos
framework** — `MLMediaLibrary` only understands the *current* library, so an iPhoto/Aperture
bundle must be walked by the fallback enumerator (*inferred*; the UTI map is **belegt**); and a
library is always media-only downstream (§3.4). → **belegt** / *inferred*

### 3.2 Which enumerator walks a path (belegt)

`FileSystemEnumeratorResolver.GetEnumerator(path)` picks, in order:

| Enumerator | `CanHandlePath` | Behaviour |
|---|---|---|
| `PhotoLibraryEnumerator` | path **is** a library root | `MLMediaLibrary` loaded with a source-type bitmask (the literal `6`) restricted to the Photos source identifier. Observes `mediaSources` → the photos source → its `libraryURL`, which **must equal the walked path** (else fallback), then `rootMediaGroup` → `mediaObjects`. |
| `PhotoLibraryFallbackEnumerator` | path is *inside* a library | derives from `StandardFileSystemEnumerator`: a plain recursive walk, each result wrapped into a `PhotoLibraryFileSystemObject`. Only `Masters` or `Originals` (top level, case-insensitive regex) are enumerated as content roots. |
| `StandardFileSystemEnumerator` | anything else | normal recursive walk, honouring `IFileSystemFilter.IsIgnored` and `ExcludeHiddenFilesAndFolders`. |

Each walked `MLMediaObject` becomes a `PhotoLibraryFileSystemObject`: `Path` = the object's
**original** URL, `EditedFilePath` = the edited URL (nullable), `CreatedDate`/`ModifiedDate`,
`IsHidden`/`Size` from URL resources, `PhotoLibraryPath` = the library root. Objects are
de-duplicated by original path (`MLMediaObjectOriginalPathComparer`) — one media object with several
source URLs yields one record. If `MLMediaLibrary` cannot be initialised, is unavailable, or its
`libraryURL` does not match the walked path, the enumerator switches to the `Masters`/`Originals`
fallback and logs it. `MacPhotoLibrary.ShouldSkipDirectoryInPhotoLibrary` mirrors the rule from the
other side: a path only matters when its path relative to the library root contains `Masters` or
`Originals`. → **belegt**

Single-flight guard: two concurrent walks of the **same** directory return the in-flight task; a
walk of a *different* directory while one runs throws
`PhotoLibraryEnumerationInProgressException`. → **belegt**

### 3.3 The edited version wins (belegt)

`GetUploadRecord` copies `EditedPath` from the photo-library FSO, and the upload stream is opened
from `EditedPath ?? Path` — **if Photos exposes an edited rendition, the edited file is uploaded,
not the original**. During a quick scan, a library record whose `EditedFilePath` no longer exists
is skipped entirely (not re-queued, not deleted). `TransferError.PhotoLibraryEditedContentNotFound`
(= 41) exists, but no raise site was found. → value **belegt**, use **UNKNOWN**

### 3.4 Library content is media-only, always (belegt)

`IsValid(fsoPath, targetType, filter, photoLibraryPath)`: when `photoLibraryPath` is set the `Other`
bit is cleared (`filter &= ~ContentFilter.Other`) — a library can never contribute non-media files;
files go through `ContentFilterExtensions.Matches(path)` (§3.9); folders that are *children* of the
library root are rejected (`IsPathChildOf`). For the one-time path the filter is always
`ContentFilter.Everything.ForMediaOnly()`, i.e. `PhotosAndVideo`. Everything filtered out only feeds
an `UnknownExtension` health metric. → **belegt**

### 3.5 `UploadType.PhotosImport` and its consequences (belegt)

`GetUploadRecord` starts at `UploadType.Normal` and upgrades to `UploadType.PhotosImport` when the
FSO is a `PhotoLibraryFileSystemObject` **or** when a `photoLibraryPath` was passed — note the
second half: *any* path that `TryGetPhotoLibrary` resolves to a library becomes `PhotosImport`,
not just the library root itself. `UploadType` = `Normal=0, PhotosImport=1, Deduped=2,
Reupload=99`. → **belegt**

| Extension point | `Normal` | `PhotosImport` |
|---|---|---|
| `GetConflictResolution()` (wire) | `NONE` | `RenameExisting` |
| `GetSuppressFileUpload()` | `Deduplication` (suppressed) | `null` (full MD5 check runs) |
| iOS HEIC heuristic | not eligible | eligible (§3.8) |

A library import is therefore the **one** manual path that participates in the cross-account MD5
check and the **one** that renames on conflict. This refines
`SPEC_ONE_TIME_UPLOAD.md` §5.4 ("a one-time drop has no `record`, so it stays `Normal`"): a
one-time drop *of a photo library* is **not** `Normal`. → **belegt**

### 3.6 The one-time media import and its cloud destination (belegt)

`EnumerationService.OneTimeMediaImport(paths)` (async, fire-and-forget):

1. Builds four destination segments: "Bilder", "Desktop", "Import von Medien", and today's date as
   `yyyy-MM-dd` (all four are *strings*, not constants).
2. `IBackupPathMapper.GetOrCreateBackupDestination(segments)` resolves/creates them from the
   account root as cloud folders.
3. If a node came back: `EnumerateOneTimeBackupUploadPaths(paths, node.NodeId)` — the same
   DB-backed one-time queue as a drag-drop, `schedule_id = "ManualUpload"` — **and** publishes
   `MediaImportStartEvent { UploadDestination = segments joined with "/" }`.
4. If the node is `null`, **nothing happens**: no enumeration, no event, no user-visible error.

→ **belegt**

### 3.7 Entry points: Autoplay, CLI, Finder service (belegt)

`OneTimeMediaImport` has exactly **one** caller: `StartupTasks.UploadArgStartupTask`
(`RunFrequency.Always`). It returns early unless the customer is signed in and `args.UploadPath` is
non-blank, wraps the path in a single-element list, then branches on
`args.StartupSource == AppStartupFlags.AutoPlay`: Autoplay → `OneTimeMediaImport`, otherwise →
`EnumerateOneTimeBackupUploadPaths`. The CLI (`StartupContext.Parse`) accepts `--u|--upload=<path>`
and a generic `--source-<flag>` switch for every `AppStartupFlags` member (`AutoPlay` is one of
them). → **belegt**

Two important observations:

- **The Autoplay action is not installed.** `InstallAutoplayTask.InternalRun` is a **stub**
  returning `Task.FromResult(true)`, yet it is registered in `StartupTaskManager.Tasks`. There is
  **no** `LSSetDefaultRoleHandlerForContentType`, no `PlugIns`, no app extension, no `LSHandlers`
  in the bundle; the only LaunchServices P/Invoke serves the Finder *sidebar* shared-file list, and
  `Info.plist` declares no Autoplay action. The task is a no-op (**belegt**); the media-import
  destination being unreachable on a stock install is *inferred*.
- **The Finder service is a different feature.** `Info.plist` `NSServices` declares one service,
  message `addBackupFromMacContextMenu`, item "Upload to Amazon Photos", accepting
  `public.directory` **only**. `ProcessContextMenuPathsTask` hands each path to
  `ContextMenuHelper.AddBackup(path)`, which validates it and builds a **`ScheduledBackupItem`**
  with `ContentFilter`/`Interval`/`AvoidDuplicates` from customer settings and `IsExternal` from the
  external-drive service — i.e. it creates a *scheduled backup*, not a one-time import. → **belegt**

### 3.8 The iOS duplicate prompt (`iosdedupeprompt_*`) (belegt)

Triggered from the enumeration batch, not from the UI. In `BatchRunDatabaseOperations`, if
`HasCustomerSeenApplePhotosDuplicatesPrompt == false` and the batch contains at least one
`PhotoLibraryFileSystemObject` whose path ends `.heic` (case-insensitive), the enumeration service
publishes `ShowPromptForApplePhotosDuplicatesIntent`. The handler runs once per app session
(interlock + settings flag), waits for the main drive window to be open, then shows a **modal** with
`iosdedupeprompt_header`, `iosdedupeprompt_information` and the buttons "Übergehen" / "Beibehalten".
Either way `HasCustomerSeenApplePhotosDuplicatesPrompt` becomes `true`;
`DialogResult.NegativeActionClicked` → filtering **off** plus
`RetryIgnoredDueToPotentialConflictsWithiOSUploads()` and metric `IosHeicDedupePromptDesktopAndIos`,
otherwise filtering **on** and metric `IosHeicDedupePromptIosOnly`. → **belegt** (the button→outcome
mapping is inherited from `SPEC_DEDUP.md` §6 and unverified there too)

Enforcement at upload (`PreUploadCheck`, plus a second site for the resumable path): when
`FilterOutPotentialDuplicatesFromApplePhotosLibrary` **or** the prompt has never been seen, **and**
the record's `DatabaseRecord.PhotoLibraryPath != null`, **and** the path ends `.heic`, the first
two bytes are read; a JPEG byte-order mark throws `FileIgnoredForPotientialDuplicationException`
and the row becomes `TransferStatus.IgnoredDueToPotentialConflictWithiOSUploads` (= 0x1000).
`RetryIgnoredDueToPotentialConflictsWithiOSUploads()` finds every schedule with rows in that
status and rewrites them to `Waiting`. The row tooltip is
`iosdedupeprompt_backupitem_tooltip`. → **belegt**

The heuristic is therefore **photo-library-only and `.heic`-only**: a plain folder full of HEIC
files neither prompts nor is filtered. → **belegt**

### 3.9 Filter taxonomy and non-photo files (belegt; taxonomy owned by SPEC_DEDUP)

`ContentFilter` is a bitmask: `None=0, Photos=2, Videos=4, Other=8, PhotosAndVideo=6,
Everything=14`. `Matches(path)` classifies through `SupportedMimeMapping`, which loads an
**embedded manifest** (`…SupportedContentTypes.supported-content-types.json`) at first use: the
whole text is lower-cased before deserialisation and the lookup lower-cases the extension, so
matching is case-insensitive. The manifest's `supported_content_types_prefix` is
`["image", "video"]`; extracted from the assembly, its categories are:

| Category | Count | Meaning |
|---|---|---|
| `IMAGE` | 83 | photos incl. RAW (`cr2`, `cr3`, `arw`, `nef`, `dng`, `raf`, `rw2`, `orf`, `srw`, `pef`, `3fr`, `iiq`, `erf`, `mos`, `kdc`, `dcr`, `mrw`, `raw`, …) |
| `VIDEO` | 47 | videos (`3g2`, `3gp`, `asf`, `avi`, `divx`, `dv`, `flv`, `m1v`, `m2t`, `m2ts`, `m4v`, `mkv`, `mod`, `mov`, `mp4`, `mpe(g)`, `mpg`, `mqv`, `mts`, `ogv`, `qt`, `ts`, `vbk`, `webm`, `wmv`, …) |
| `IMAGE_SIDECAR` / `VIDEO_SIDECAR` | 1 + 1 | both are `xmp` |
| `APPLICATION` / `AUDIO` / `TEXT` / `MESSAGE` | 344 / 36 / 105 / 4 | not in the prefix list → dead weight |

`Matches` is deliberately strict: image or image-sidecar → needs `Photos`; video or video-sidecar
→ needs `Videos`; **everything else returns `false`**. The `Other` bit never *admits* a file — it
can only survive as "not excluded". `ForMediaOnly()` maps `Everything` to `PhotosAndVideo` and
leaves anything else untouched. → **belegt**

How a non-media file is handled, in order:

1. `IFileSystemFilter.IsIgnored` (`FileFilter`) drops files with the `System`, `Temporary` or
   `ReparsePoint` attribute; junk names (`.lnk` extension, `.DS_Store`, `.localized`,
   `icon\r`); temporary names (`~$*`, `*.tmp`); hidden files/folders (honouring
   `ExcludeHiddenFilesAndFolders`); and photo-library junk (§3.10). → **belegt**
2. `IsValid` applies the content filter. Everything that fails is **silently dropped** — only a
   `FileType.UnknownExtension` health metric (dimension: the extension) is recorded. No error row,
   no user feedback. → **belegt**
3. `TransferError.FileTypeNotSupported` (= 49) exists, and `IsFolderEmpty` deliberately ignores
   junk files, but no raise site for `FileTypeNotSupported` was found → value **belegt**, use
   **UNKNOWN**

`FolderFileCounter` / `DirStats` does classify for a folder-size preview (`AddItem` routes by
`GetContentType(name)` into `ImageStats`/`VideoStats`/`OtherStats`, `.app` bundles forced to
`OtherStats`) — but the count strings that would display it are **unreferenced** and
`MacFolderManager.CalculateFileCount` is a **stub returning 0** on macOS, so the FTUE folder tile
never shows a media count here. → **belegt** / display path **UNKNOWN**

### 3.10 Photo-library junk (belegt)

`FileFilter.IsPhotosLibraryJunk(path, allowEdits)`: when the path is library content and is not the
library root itself, it is junk **unless** its path contains `Masters` or `Originals` — or, with
`allowEdits`, `resources/media/version/`. This is what keeps a library walk from uploading the
bundle's databases, thumbnails and internals; the `Masters`/`Originals` exception is exactly the
fallback enumerator's content root. → **belegt**

### 3.11 Cloud-side folder tree and the content-filter UI (belegt)

`UI.AutoSavePhotoLibraryNode` is the library variant of the folder-tree node: `FriendlyName` is the
bundle name **without** extension, `GetWebLink()` points at
`CloudDriveLink.PhotosSystemPreferencePane`, and error rows with
`TransferError.NeedsPhotoLibraryPermissions` show the record's own tooltip. → **belegt**

The backup-configuration window renders exactly **two** independent checkboxes, "Fotos sichern" and
"Videos sichern", derived from the stored `ContentFilter`. On load an `Everything` value is
**rewritten to `PhotosAndVideo`** (both view and view model do this); the view model derives
`Photos` / `Videos` / `PhotosAndVideo` from the two checkboxes, and
`ScheduledBackupItem.UpdateContentFilter` guarantees at least `PhotosAndVideo`. There is no "Other"
checkbox and no third state. `BackupConfigurationParams.Default` sets
`DefaultContentFilter = PhotosAndVideo`, `DefaultAvoidDuplicates = false`, interval `Immediate`.
`BackupScannerWorker` additionally forces `DeduplicateOptions.None` whenever the default content
filter is `Everything` — "everything" and "avoid duplicates" are mutually exclusive on the schedule
record. → **belegt**

### 3.12 Videos: where they differ from photos (belegt)

| Aspect | Photos | Videos |
|---|---|---|
| Filter bit | `Photos` (2) | `Videos` (4) |
| Content type on the row | `ContentType.Image` | `ContentType.Video` |
| Upload-strategy weight | 10 | 3 (both server-overridable via `UploadStrategyWeights`) |
| Multipart threshold | `Size > MultipartUploadMinSizeInBytes` | a video **larger than 5 GiB is never multipart** (`IsVideoAndSizeLargerThan5GB` → `IsMultipartUpload` returns false) |
| Extra metric | — | `ResumableUploads.OverFiveGigaByteVideoFileTotalRequests` |
| Queue type order | — | `TypeOrder = Image, Video, Other, Sidecar, None` (round-robin); while serving a non-image slot an image is dequeued alongside it if one is queued |

Size ceilings are shared: `Size > CloudDriveServiceUploadMaxFileSizeInBytes` throws
`FileSizeTooLargeException` → `TransferError.CloudDriveFileSizeLimitExceeded` (193), and
`MultipartUploadFileSizeLimitExceeded` (3094) exists for the multipart path; a zero-byte file gets
`ResumableUploads.ZeroByteFileTotalRequests`. `WeighedUploadStrategy` runs the weighted round-robin;
`SmallFileSizeUploadStrategy` / `LargeFileSizeUploadStrategy` split the queue below/above
`MultipartUploadMinSizeInBytes` and serve small files first. There is **no** video-specific upload
endpoint or host — uploads go through the same `/v2/upload` family (`../protocol/PROTOCOL.md`); the only
"zone" concept is the HTTP content host. → **belegt**

### 3.13 Autoplay label and the missing media player (belegt / UNKNOWN)

`mediaimport_autoplayaction` = "Fotos und Videos importieren" is the only trace of an
Autoplay/quick-action UI, and it is **defined but not referenced** by the managed code; the bundle
declares no Autoplay handler (§3.7). → **belegt** / render site **UNKNOWN**

There is **no media player** in the assembly: no `AVPlayer`, `AVPlayerView`, `AVKit`, playback or
preview type exists, and the only thumbnail work is server-side thumbnail caching for the transfer
list. `no_photos_or_videos_found` is likewise **unreferenced** — render site **UNKNOWN**.
→ **belegt**

The persistent media-import toast (`MediaImportStartNotification`, `IsPersistent = true`) is
published by `MediaImportStartNotificationHandler` on `MediaImportStartEvent`; its "Details" button
publishes `ShowDriveWindowIntent { InitialScreen = Backup, LaunchSource = FromAutoplay }`.
→ **belegt**

The `mac_onboard_noautosave_*` strings are the interesting contradiction: this build *does*
support Apple Photos/iPhoto/Aperture (§3.1–§3.2), yet the shipped disclaimer says it does **not**,
and both strings are unreferenced by the managed code. → *inferred* leftovers from an older,
library-incapable build. → **belegt** (unreferenced) / *inferred* (why)

---

## 4. String evidence (keys → German)

Keys cited in this doc, verbatim from `de.lproj/Localizable.strings`. The right-hand column says
whether the managed code references the key (grep for `Strings.<key>` / the literal key).
`…` = `clouddrive_desktop_`. → **belegt**

| Key (… = `clouddrive_desktop_`) | German | Referenced? |
|---|---|---|
| `…_mediaimport_autoplayaction` | "Fotos und Videos importieren" | **no** |
| `…_mediaimport_picturesfolder` | "Bilder" | yes (`OneTimeMediaImport`) |
| `…_mediaimport_desktopfolder` | "Desktop" | yes (`OneTimeMediaImport`) |
| `…_mediaimport_mediaimportfolder` | "Import von Medien" | yes (`OneTimeMediaImport`) |
| `…_no_photos_or_videos_found` | "Keine Fotos oder Videos gefunden" | **no** |
| `…_iosdedupeprompt_header` | "Wenn Sie darüber hinaus Amazon Photos für iOS zum automatischen Speichern Ihrer Apple Fotobibliothek verwenden, kann dies dazu führen, dass von einigen Bildern Duplikate erstellt werden." | yes (intent handler) |
| `…_iosdedupeprompt_information` | "Möchten Sie die Sicherung dieser Elemente auf diesem Computer vermeiden?" | yes (intent handler) |
| `…_iosdedupeprompt_backupitem_tooltip` | "Sie können diese Einstellung in den Voreinstellungen ändern." | yes (`TransferEnumExtensions`) |
| `…_skip` / `…_keep` | "Übergehen" / "Beibehalten" | yes (iOS prompt buttons) |
| `…_mac_NSPhotoLibraryUsageDescription` | "„Amazon Photos“ möchte auf Ihre Fotos zugreifen. Klicken Sie auf „OK“, um die Sicherung Ihrer Apple Photos-Bibliothek zu erlauben." | yes (TCC purpose; `Info.plist` carries the English original) |
| `…_photolibrary_grantpermissions` / `…_photolibrary_confirmpermissions` | "Klicken Sie hier, um Berechtigungen zu gewähren" / "Wenn Sie erlauben möchten, dass Ihre Apple Photos-Bibliothek gesichert wird, klicken Sie auf den Link, um Ihre Photos-Datenschutzeinstellungen anzuzeigen." | yes (`TransferError` long strings) |
| `…_photoslibrary_dialog{title,message,_learnmorebutton,_okbutton}` | "Apple Fotobibliothek" / "{0} Die Originale der Fotos und Videos werden hochgeladen. Duplikate werden übersprungen." / "Weitere Informationen" / "OK" | **no** (OK only as the generic LiteModal default) |
| `…_mac_onboard_noautosave_select_disclaimer` | "Das Hochladen von Apple Photos-, iPhoto- oder Aperture-Dateien wird derzeit nicht unterstützt" | **no** |
| `…_mac_onboard_noautosave_confirmation_body` / `…_win_…` | "Greifen Sie über die Taskleiste jederzeit auf die App zu, um Uploads durchzuführen, den Upload-Status anzuzeigen oder Ihr Amazon Photos im Internet zu öffnen." / (Windows: "über Ihre Menüleiste") | **no** |
| `…_notification_mediaimport_{title,message,button}` | "Ihr Upload wurde gestartet" / "Die importierten Fotos und Videos werden in Ihrem Amazon Photos-Konto gespeichert." / "Details" | yes (`MediaImportStartNotificationHandler`) |
| `…_backupconfiguration_selectcontentfiltertitle` | "Was soll gesichert werden:" | **no** (the view composes its own label) |
| `…_upload_settings_backup_photos_checkbox` / `…_videos_checkbox` | "Fotos sichern" / "Videos sichern" | yes (content-filter checkboxes) |
| `…_photos_videos_count{,_partial}` / `…_photos_folder_size_partial` | "{0} Fotos, {1} Videos" / "{0} Fotos, {1} Videos und mehr..." / "{0} und mehr..." | **no** |

The "**no**" rows are defined but not referenced by the managed assembly — *inferred* dead or
rendered only by a nib / alternate layout. `…_backupconfiguration_selectcontentfiltertitle` is the
exception that is only *partly* dead: the strings entry exists, but the configuration view in this
build composes its own label. → **belegt**

---

## 5. Gap analysis vs. our app (committed state)

Our state was read from `src/` (`folders.rs`, `backup.rs`, `backup_config.rs`, `prime_photos.rs`,
`upload.rs`, `i18n.rs`, `config.rs`, `app.rs`).

| Capability | Original | Our app | File reference |
|---|---|---|---|
| Photo-library detection | UTI map (`.photoslibrary`, `.migratedPhotoLibrary`) → Photos/iPhoto/Aperture/migrated | **none** | `src/folders.rs` (`is_supported_image`) |
| Library-aware enumeration | `MLMediaLibrary` root enumerator + `Masters`/`Originals` fallback, wrapped as `PhotoLibraryFileSystemObject` | **none** — every path is a plain directory or file | `src/folders.rs` (`scan_images`) |
| Edited rendition preference | uploads `EditedPath ?? Path` | **n/a** — no notion of an edited version | `src/prime_photos.rs` (`upload_picture`) |
| `PhotoLibraryPath` on the record | set for any library-resolved path; drives the HEIC/iOS heuristic and media-only filtering | **none** | `src/folders.rs`, `src/prime_photos.rs` |
| Media-only guarantee inside a library | `filter &= ~Other`, folders under the root rejected, junk paths skipped | **none** — recursion is unfiltered apart from the extension list | `src/folders.rs` (`scan_images`) |
| Media-import destination | cloud tree "Bilder / Desktop / Import von Medien / `<yyyy-MM-dd>`" | **none** — cookie upload into the account root, no destination concept | `src/prime_photos.rs` (`web_dir_node`) |
| Media-import entry point | Autoplay / `--upload` + `--source-autoplay` (task is a no-op stub) | **none** | `src/app.rs` (`update`) |
| Media-import notice | persistent in-app toast, "Details" → Backup screen | **none** | `src/notify.rs` |
| iOS HEIC duplicate prompt | one-shot modal on first `.heic` in a library batch + pre-upload JPEG-BOM check | **none** | `src/dedup.rs`, `src/upload.rs` |
| `.heic` handling | supported image, plus the iOS heuristic | supported image (`heic` in `STANDARD_EXT`) | `src/folders.rs` (`STANDARD_EXT`) |
| Video upload | 47 video extensions, weighted queue, no multipart above 5 GiB | **none** — no video extension in either list | `src/folders.rs` (`STANDARD_EXT`, `RAW_EXT`) |
| Content filter | two checkboxes, `Everything` forced to `PhotosAndVideo` | **inert UI only** — three disabled buttons ("Alles"/"Fotos und Videos"/"Fotos"), no backend | `src/backup_config.rs` (`render_backup_config`) |
| Folder media counts / queue priority | `ImageStats`/`VideoStats`/`OtherStats`; weighted round-robin, images first | **none** — plain `tokio` spawn, unordered | `src/backup.rs` (`Row`), `src/upload.rs` (`upload_worker`) |
| File-size ceiling | `CloudDriveServiceUploadMaxFileSizeInBytes` → `CloudDriveFileSizeLimitExceeded` | **none** — no size check | `src/prime_photos.rs` (`upload_picture`) |
| Junk/temporary/symlink skipping | `FileFilter` attributes + regexes | **none** — `scan_images` only filters by extension | `src/folders.rs` (`scan_images`) |
| `.xmp` sidecars | image/video sidecar group (would pass `Photos`/`Videos`) | explicitly rejected | `src/folders.rs` (`is_supported_image`) |
| Autoplay/CLI start · Finder service | `--upload`, `--source-*`; `NSServices`, directories only → scheduled backup | **none** | `src/main.rs`, `src/app_menu.rs`, `src/tray.rs` |
| i18n layer | `Localizable.strings` | central de/en table | `src/i18n.rs` |

**Net assessment.** Every capability in §3.1–§3.13 is **unimplemented** in our port; only the
general image-extension filter and the one-time drag-drop queue (owned by
`SPEC_ONE_TIME_UPLOAD.md`) exist. Nothing here is a deliberate divergence — it is out of the
current scope, which `AGENTS.md` already records ("Apple-Photos-/iPhoto-/Aperture-Import" under
**Out (bewusst, nicht MVP)**). → *inferred* (scope intent) / **belegt** (code state)

One *in* scope point of contact: our `one_time` field. Our port now persists the dropped queue in
the MD5 SQLite and deletes it after completion (`AGENTS.md`, decision 2026-10-14), which is closer
to the original's DB-backed `schedule_id="ManualUpload"` rows than the older "never persisted"
rule in `SPEC_ONE_TIME_UPLOAD.md` §10.3 — a `OneTimeMediaImport` drop of a **library** would
land in exactly that queue, but as `UploadType.PhotosImport`, not `Normal` (§3.5). → **belegt**

---

## 6. UNKNOWN (explicitly not determined)

- **Whether the Autoplay action is registered outside this bundle** — no `LSSetDefaultRoleHandler`
  call, no `PlugIns`, no `LSHandlers`, no Actions extension, and `InstallAutoplayTask` is a stub.
  Whether the launcher registers the action, or an older installer edition does, is **UNKNOWN**.
  → *inferred* not reachable on a stock install
- **Render sites of the unreferenced strings** — `…_mediaimport_autoplayaction`,
  `…_no_photos_or_videos_found`, `…_photoslibrary_dialog*` and `…_mac_onboard_noautosave_*` are
  defined but unreferenced by the managed code, and the bundle ships only three nibs
  (`DriveThemedWindow`, `SelectiveSyncView`, `UpsellView`); none of them was searched for these
  keys. → **UNKNOWN** (*inferred* nib/legacy layout)
- **The source-type bitmask value** passed to `MLMediaLibrary` (the literal `6`) — which
  `MLMediaSourceType` bits that denotes was not verified against the framework headers, only that
  the include-list is restricted to the Photos source identifier. → **UNKNOWN**
- **Which of iPhoto/Aperture the fallback walk reaches in practice.** The UTI map recognises both,
  but no iPhoto or Aperture bundle was available to test the `Masters`/`Originals` regex against a
  real layout. → **UNKNOWN**
- **Absolute mapping of "Übergehen"/"Beibehalten" to filtering on/off.** The dialog result branch
  is **belegt**; which of the two buttons produces `DialogResult.NegativeActionClicked` is not
  verified (`SPEC_DEDUP.md` §6 flags the same gap). → **UNKNOWN**
- **`TransferError.PhotoLibraryEditedContentNotFound` (41) and `FileTypeNotSupported` (49)** —
  values are **belegt**, no raise site was found. Whether they are reachable is **UNKNOWN**.
- **Why an edited file that has vanished skips the record silently** rather than deleting the row
  or re-uploading the original — behaviour is **belegt**, intent **UNKNOWN**.
- **Whether the media-import destination is ever cleaned up.** The date-stamped folders
  accumulate; no deletion or rotation was found. → **UNKNOWN**
- **Whether the destination folder names can change.** The four segments are strings, not
  constants, so a server-side translation could alter the tree; no override mechanism was found.
  → **UNKNOWN**
- **Whether `--upload --source-autoplay` is the real OS integration path** or a developer/test
  affordance. The log line ("passed in from the command line") suggests the latter. → *inferred*
- **`IsPhotosLibraryJunk(allowEdits: true)` callers.** The `resources/media/version/` edit branch
  exists, but the only call site seen passes `false`. → **UNKNOWN** whether an edit-aware walk is
  reachable.
- **Video acceptance of the cookie flow.** Our upload endpoint and the original's differ; whether
  `content-<zone>.drive.amazonaws.com/v2/upload` accepts a video is unverified (same gap as
  `SPEC_DEDUP.md` §7 for non-media files). → **UNKNOWN**

---

## 7. Cross-references

- `SPEC_ONE_TIME_UPLOAD.md` §4 (one-time destinations), §5 (queue row), §6 (persistence),
  §10.3 (our transient one-time row — superseded for the persistence part by the 2026-10-14
  decision in `AGENTS.md`), §10.5 (deliberate dedup divergence).
- `SPEC_FOLDERS.md` §2.1 (`is_supported_image`), §2.2 (`scan_images`), §2.7 (global pause),
  §3.4 (content filter / dedupe / conflict on schedule records).
- `SPEC_DEDUP.md` §1 (MD5 identity), §2 (pre-upload check and `UploadType` eligibility),
  §3 (extension taxonomy — the authoritative list; this doc only adds the manifest counts),
  §4.2 (content-dedup errors), §6 (the Apple-Photos prompt flow, incl. the unverified button
  mapping), §7 (open points for our project).
- `SPEC_UPLOAD.md` / `../protocol/PROTOCOL.md` — endpoints, headers, `conflictResolution`, multipart
  parameters.
- `SPEC_BACKGROUND_SYNC.md` — cadence, pause; a one-time import is outside the scheduler.
- `SPEC_UPLOAD_PROGRESS.md` — the transfer-item list the library rows appear in.
- `SPEC_NOTIFICATIONS.md` — notification surfaces; `MediaImportStartNotification` is a
  persistent *in-app* toast, distinct from desktop notifications.
- `UI_FLOWS.md` §4.4 ("Einmalig" vs "Plan"), §4.8 (drag-and-drop flow), §11.2 (status
  precedence). `UI_STATES.md` §3.8 (`uploads` row for `ManualUpload`), §4.1 (`ContentFilter` /
  `DeduplicateOptions` bitmasks).
- `AGENTS.md` — "Scope-Grenzen (In/Out)": the Apple-Photos/iPhoto/Aperture import is explicitly
  **Out (bewusst, nicht MVP)**.
- Our code: `src/folders.rs` (`is_supported_image`, `scan_images`, `ConflictResolution`),
  `src/backup_config.rs` (`render_backup_config` — inert content-filter buttons),
  `src/prime_photos.rs` (`upload_picture`), `src/upload.rs` (`run_upload`, `upload_worker`),
  `src/dedup.rs`, `src/i18n.rs`.
