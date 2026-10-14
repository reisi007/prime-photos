# Spec: Duplicate handling in the original client

**Status:** clean-room backfill spec (read-only reconstruction of the official macOS
Amazon Photos client, 11.3.0/36). Own words and tables only; no verbatim decompiled
code. Quoting German UI labels from `Contents/Resources/de.lproj/Localizable.strings`
is intentional. No personal data.

**Scope.** This document answers one question end to end: *how does the original
decide what to do with duplicates, and which options does it expose?* It covers the
two independent mechanisms — **content dedup** ("Duplikate überspringen") and
**name-conflict resolution** ("Umgang mit Dateinamenkonflikten") — plus the detection
path, the local state and the 409 behaviour, and closes with a gap analysis against
our own Rust implementation.

**Relationship to `docs/SPEC_DEDUP.md`.** That document is the reference for the
*content-filter taxonomy* and the *MD5-as-identity* mechanics. This document is the
reference for the *option set* (which choices exist, where, with which default) and
the *gap analysis*. Where they overlap, `SPEC_DEDUP.md` owns the extension lists.

**Sources (read-only).** `/Applications/Amazon Photos.app` — primary
`Contents/MonoBundle/Amazon Photos.dll` decompiled with `ilspycmd` (ILSpy 11.1);
`Contents/Resources/de.lproj/Localizable.strings`; local SQLite state under
`~/Library/Application Support/Amazon Cloud Drive/` (`backup`, `uploads`, `cloud.db`).
Symbol names and German label keys are cited per statement; unverifiable points are
collected in §6.

---

## 1. What the original offers

There are **two separate decisions**, and they are orthogonal: *is this file already
in my account by content?* and *what if a file of the same name is already in the
destination folder?* A file can be a content duplicate and a name conflict at the
same time; the original resolves the content question first (§2) and only then the
name question (§4).

### 1.1 Where the options live

| Level | Dedup option | Conflict option |
|---|---|---|
| **Per backup target** (one `ScheduleRecord`, table `schedules`) | `schedules.dedupe_photos_videos` (`DeduplicateOptions`) | `schedules.conflict_resolution` (`ConflictResolution`) |
| **Global** | yes — `CustomerSettings.EnableDeduplicationForPhotosAndVideos`, used only as the **default for new backups** | **no** — there is no global conflict-resolution setting |
| **Manual / one-shot upload** (`schedule_id = "ManualUpload"`) | **not eligible** (see §2.1) | falls back to the per-run interactive prompt |

So: dedup is a per-target toggle **with** a global default; conflict resolution is
strictly **per target**, and the only "remember" mechanism is the per-schedule
write-back from the interactive prompt (§1.3).

### 1.2 Content dedup ("Duplikate überspringen")

* **Internal name:** `Amazon.CloudDrive.Database.ManualUpload.DeduplicateOptions`, a
  `[Flags]` enum `None = 0`, `Photos = 2`, `Videos = 4`, `PhotosAndVideos = 6`.
* **UI surface:** despite the flags enum, the UI exposes **one boolean checkbox**.
  `ScheduledBackupItem.AvoidDuplicates` (bool) maps to
  `DeduplicateOptions.PhotosAndVideos` / `None` when the schedule record is written
  (`ScheduledBackupItemExtensions`, `ScheduledBackupItem.FromScheduleRecord`). The
  per-media values `Photos`/`Videos` exist in the enum but are never set by the UI.
* **German labels:**
  * current dialog — `clouddrive_desktop_upload_settings_skip_duplicates_option_title`
    = "Duplikate überspringen", subtitle
    `..._skip_duplicates_option_subtitle` = "Dateien, die sich bereits in meinem Konto
    befinden, nicht sichern".
  * legacy wording — `clouddrive_desktop_statuswindow_scheduleconfiguration_dedupeoption`
    = "Duplikate vermeiden: Keine Fotos und Videos sichern, die sich bereits in meinem
    Konto befinden".
* **Default:** a schedule created from **onboarding** starts with dedup **on** when the
  content filter is not "Everything" (`AppSettings.DefaultOnboardingDedupeChecked`
  defaults to `true`; `BackupScannerWorker` builds the record). A schedule created
  **after** onboarding (preferences/context menu) takes
  `CustomerSettings.EnableDeduplicationForPhotosAndVideos`, whose default is
  `AppSettings.DefaultPostOnboardingDedupeChecked` = **false**.
* **Hard interaction:** selecting the content filter `Everything` forces
  `AvoidDuplicates = false` (`ScheduledBackupItem.ContentFilter` setter); dedup only
  makes sense for photos/videos. This matches `docs/SPEC_DEDUP.md` §3.4.
* **UI location:** the "Upload-Einstellungen für „<Ordner>" bearbeiten" window —
  `BackupConfigurationViewController.CreateAvoidDuplicatesFieldBelow`, checkbox
  identifier `BackupView_SettingsAvoidDuplicates_Checkbox`. The global default lives
  as a customer setting (`EnableDeduplicationForPhotosAndVideos`,
  `PreferencesViewModel`), surfaced under the preferences window.

### 1.3 Name-conflict resolution ("Umgang mit Dateinamenkonflikten")

* **Internal name:** `Amazon.CloudDrive.Database.ManualUpload.ConflictResolution`,
  enum `None`, `Ignore`, `SaveAsNew`, `Replace`.
* **German labels (current dialog,** `BackupConfigurationViewController` radio group
  `BackupView_FileNameConflictResolution_*_RadioButton`**):**

  | Enum value | Label key | German label |
  |---|---|---|
  | `None` | `clouddrive_desktop_upload_settings_file_name_conflicts_ask_option` | "Jedes Mal nachfragen" |
  | `Ignore` | `..._ignore_option` | "Neue Datei ignorieren" |
  | `Replace` | `..._replace_option` | "Alte Datei ersetzen" |
  | `SaveAsNew` | `..._auto_rename_option` | "Neue Datei automatisch umbenennen" |

  A second, **legacy** label set exists (`..._statuswindow_scheduleconfiguration_filenameconflict_*`:
  "Ignorieren", "Ersetzen", "Als neue Dateien speichern", "Jedes Mal nachfragen").
  It is only referenced from the generated `Strings` class, not from any decompiled
  view — treat it as unused/legacy.
* **Default:** `None` ("Jedes Mal nachfragen"). The column is added with
  `alter table schedules add column conflict_resolution integer default 0`
  (`SchedulesDatabase`, migration step 3), and `0` is `ConflictResolution.None`. A new
  `ScheduleRecord` never assigns the field, so the enum default applies.
* **The interactive prompt** (`FileNameConflictViewModel` + `FileNameConflictWindowService`,
  modal tag `NameConflictModal`): when the schedule's value is `None` and conflicts are
  pending, the client shows one modal listing the conflict count with three buttons —
  "Als neue Datei speichern" (`SaveAsNew`), "Ignorieren" (`Ignore`), "Ersetzen"
  (`Replace`) — plus a checkbox
  `clouddrive_desktop_filenameconflict_overlayview_donotaskagain_checkbox_text`
  = "Nicht noch einmal nach {Ordner}-Ordner fragen". If checked, the chosen value is
  written **back into that schedule** (`ConflictResolution = …`), and all pending
  conflicts of the schedule are resolved in the DB
  (`ManualUploadDatabase.ResolveNameConflictsForSchedule`). So "ask every time" is
  really "ask once per schedule until the user says don't ask again".
* **Wire mapping** (`CloudDriveConflictResolutionExtensions`): the enum sent to the
  service only has `None → "NONE"`, `Rename → "RENAME"`, `RenameExisting →
  "RENAME_EXISTING"`. `SaveAsNew` becomes `RENAME`; `Replace` does **not** use a wire
  enum value — it sets `overwrite = true` and reuses the existing destination
  `nodeId`. `None` keeps the existing node (server reports the conflict).

---

## 2. Detection path

The duplicate identity is the **MD5 of the whole file body** (lower-case hex, 32
chars), not name/size/date (`docs/SPEC_DEDUP.md` §1). The original asks two different
questions and answers them on two different paths.

### 2.1 Content dedup — local pre-check, then server

1. **Eligibility.** `UploadRecordExtensions.GetSuppressFileUpload` returns
   `SuppressFileUpload.Deduplication` unless the record's `UploadType` is
   `Deduped` or `PhotosImport`. `EnumerationService.GetUploadRecord` sets
   `UploadType.Deduped` only when the schedule's `DeduplicateOptions ==
   PhotosAndVideos`; manual one-shot uploads (`schedule_id = "ManualUpload"`) and
   dedup-off schedules are `UploadType.Normal` → **not** content-deduped.
2. **PreUploadCheck** (`CloudDriveStorageService`, two overloads) runs before any
   network call, against the local `cloud.db` node cache:
   * resolve the candidate node by `nodeId`, else by name + parent + `"FILE"`
     (`GetNodeByNameAndParentIfExists`);
   * `duplicate = node.status == "AVAILABLE" && node.contentProperties.md5 == md5`;
   * when eligible for dedup, additionally `GetNodeByMd5(md5)` (any `AVAILABLE` node
     with that MD5) and set `duplicate = true` on a hit.
3. On a hit (and not a timed-out "reupdate" retry) it raises
   `FileIgnoredPreUploadException` → the record becomes `TransferStatus.Ignored`
   ("Zuvor hochgeladen") without a single HTTP request.
4. **Server safety net.** Even with a stale cache the endpoint rejects a duplicate
   body; see §4.

**The two cases the user asked about:**

* **Same path/name, different content** → *not* a content duplicate. It is a **name
  conflict**, decided by the separate `ConflictResolution` flow (§4): the client looks
  up the node by name + parent in the local cache and applies the schedule's policy.
* **Different path, same content** → a **content duplicate**. `GetNodeByMd5` matches
  the MD5 across folders, so a re-scan of a moved/renamed file is skipped. This is the
  cross-path dedup the user is asking about, and it is wired (§3).

### 2.2 Name conflict — client-side, from the local cache

The name-conflict decision is made **client-side**, not from the server: the uploader
looks up `GetNodeByNameAndParentIfExists(fileName, parentNodeId, "FILE")` and feeds
the resulting `destinationNodeId` into `NameConflictStrategy.GetConflictResolution`.
`SaveAsNew`/`Replace`/`Ignore` are applied directly; `None` with an existing
destination node raises `FileUploadNameConflictException` (→ `TransferError.FileConflict`,
`TransferStatus.Errored`) and later triggers the modal (§1.3). The schedule policy is
read via `GetScheduleConflictResolution(scheduleId)`.

### 2.3 Apple Photos heuristic (side channel)

For `.heic` files inside an Apple Photos library the client additionally checks the
first two bytes for a JPEG BOM and skips such files via
`FileIgnoredForPotientialDuplicationException` (status
`IgnoredDueToPotentialConflictWithiOSUploads`). This is a byte-level heuristic on top
of the MD5 rule, driven by `FilterOutPotentialDuplicatesFromApplePhotosLibrary` /
`HasCustomerSeenApplePhotosDuplicatesPrompt`. Details and the prompt strings are in
`docs/SPEC_DEDUP.md` §2.2/§6.

---

## 3. Local state

Two SQLite databases under `~/Library/Application Support/Amazon Cloud Drive/`:

| Store | Table / column | Carries |
|---|---|---|
| `uploads` (ManualUpload DB) | `precomputed_md5` | the MD5 computed once per attempt (dedup identity cache) |
| | `upload_type` | `Normal`/`PhotosImport`/`Deduped`/`Reupload` — gates dedup eligibility |
| | `node_id`, `destination_node_id` | the existing cloud node / destination folder |
| | `status`, `retry_reason`, `error_code` | `Ignored`/`Skipped`/`Errored` + `PreUploadCheckFailed`/`FileConflict` |
| | `local_id` | client GUID for resumable-upload lookup (not a dedup key) |
| `backup` (ManualUpload DB) | `node_id`, `status`, `error_code` | per-file scan state; name conflicts sit here as `Errored`/`FileConflict` |
| `schedules` (ManualUpload DB) | `dedupe_photos_videos`, `conflict_resolution` | the two per-target options (defaults `0`) |
| `cloud.db` (`CloudDriveDatabase`) | `nodes.content_md5` (+ index `IX_md5`) | the server-side node cache; `GetNodeByMd5`/`NodeExistsWithMd5` read it |

`PreUploadCheck` **is** the explicit pre-upload step; it is not folded into the
upload. A retry can suppress it: `UploadRecordExtensions.ShouldSuppressMd5Check`
returns true only for `RetryReason.Reupdate`, and `ShouldForceNodeIdLookupUsingLocalId`
covers timeout/pending-completion retries.

Relevant enums: `TransferStatus` (`Ignored = 0x400`, `Skipped = 0x800`,
`IgnoredDueToPotentialConflictWithiOSUploads = 0x1000`, `Errored = 0x200`),
`TransferError` (`PreUploadCheckFailed`, `FileConflict`, `CloudDriveNodeConflict`),
`RetryReason` (`NameConflictSaveAsNew = 13`, `NameConflictReplace = 14`,
`Reupdate = 999`).

---

## 4. Behaviour on HTTP 409

The endpoint answers a duplicate/conflict with **409**. The original splits the 409 by
cause (`CloudDriveStorageService.HandleNodeResponse` / `HandleNodeResponseSync`):

| 409 cause | Normalised to | Outcome |
|---|---|---|
| returned node's `contentProperties.md5 == local md5` | `CloudDriveNodeConflict` | treated as duplicate → status set to 200 internally, node cached, **`FileIgnoredException` → `Ignored` ("Zuvor hochgeladen")** |
| `ErrorType == "MD5_DUPLICATE"` | `CloudDriveNodeConflict` | same as above |
| `ErrorType == "DuplicatesConflictError"` | `CloudDriveNodeConflict` | same as above |
| `ErrorType == "NameAlreadyExists"` (same name, different content) | `FileConflict` | **not** a duplicate → error path, resolved by the name-conflict policy |

So **a 409 is a success-ish "skip", never a hard error**, *as long as the MD5 matches*.
A 409 that is a genuine name conflict is a separate error and goes through
`ConflictResolution`. Because `Replace`/`SaveAsNew` overwrite/rename, they normally do
not produce a 409 at all.

> **Clarification for *our* path (not the original's):** the paragraph and table above
> describe the **original client**, which pre-checks the MD5 client-side against its
> `cloud.db` and therefore never reaches the server with a content duplicate. **Our**
> path is server-driven: a **content** duplicate (`errorCode = "DuplicatesConflictError"`)
> yields a **409 even with `RENAME`** (live proven) — `SaveAsNew` avoids a *name* clash,
> not a *content* duplicate. Our default is now **`SaveAsNew`** (was `Ignore`). The
> local cross-path MD5 check (`DedupIndex::content_seen`) demotes that 409 to a safety
> net. See AGENTS.md.

**Per conflict option:**

| Option | Sends | 409 result |
|---|---|---|
| `None` (ask) | `NONE`, existing node kept | 409 MD5-match → `Ignored`; 409 name-conflict → `FileConflict` → modal |
| `Ignore` | `NONE`, existing node kept | same as `None`; a name conflict becomes `Skipped`/`Ignored` |
| `SaveAsNew` | `RENAME` | no 409 — server stores a renamed copy (original client; on our server-driven path a *content* duplicate still returns 409, see the note above) |
| `Replace` | `overwrite = true` + existing `nodeId` | no 409 — server replaces the node |

---

## 5. Gap analysis against our implementation

Our current state (read-only): persistent MD5 index **per path** (`src/dedup.rs`,
`uploads` table with `ix_uploads_md5`), skip rule = same path + same MD5; four
conflict options per target (`src/folders.rs` `ConflictResolution`, default `SaveAsNew`
since 2026-10-14 — was `Ignore`);
server 409 treated as "already there" (`src/upload.rs` `FileOutcome::ConflictSkipped`,
`src/prime_photos.rs::conflict_node_id`); interactive per-file prompt with a run-wide
"don't ask again" (`resolve_conflict`); dialog in `src/backup_config.rs`.

| Topic | Original | Our stand | Effort | Recommendation |
|---|---|---|---|---|
| **Cross-path content dedup** (different path, same MD5) | Wired: `GetNodeByMd5` over `cloud.db`; the identity is content, not path | **Missing** — `should_skip`/`stat_matches` are path-only; `ix_uploads_md5` exists but is unused | **S** | Add a `should_skip_by_md5` query on the existing index and use it when `dedup` is on. Cheap (index already there), high user value ("I moved the folder"). |
| **Dedup option granularity** | `DeduplicateOptions` flags exist, UI only boolean | Boolean `dedup` per folder — same practical surface | — | **No change.** The flags are dead weight in the original too. |
| **Dedup default** | Onboarding → on; post-onboarding → off (global default) | `default_true()` → **on** | — | **Keep.** Our default is the friendlier one; documented in AGENTS.md. |
| **Conflict default** | `None` = "Jedes Mal nachfragen" | `SaveAsNew` (was `Ignore`; changed 2026-10-14, AGENTS.md) | — | **Keep.** A file's identity is its content; `SaveAsNew` + `dedup` is the deliberate default. Note the divergence so nobody "fixes" it to Ask. |
| **Global dedup default setting** | `EnableDeduplicationForPhotosAndVideos` in preferences, seeds new backups | **Missing** — each new folder starts `dedup = true` hard-coded | **S** | Optional. Only worth it if a preferences screen lands; a single config field + default for new folders. |
| **Global conflict setting** | **Does not exist** | We also have none | — | **No change** — parity already. |
| **Name conflict detected client-side** | Via `cloud.db` name+parent lookup, before upload | **Missing** — we only learn of it from the server 409 | **L** | Not worth it without a local node cache (we have none by design, AGENTS.md). Our 409-driven path is the pragmatic equivalent. |
| **409 cause split (MD5-match vs NameAlreadyExists)** | Split: MD5 → Ignored; NameAlreadyExists → `FileConflict` error | **Missing** — any 409 becomes `ConflictSkipped`; we do not read `ErrorType` | **S** | Read `ErrorType` (`MD5_DUPLICATE`/`DuplicatesConflictError` vs `NameAlreadyExists`) and only skip on the former; otherwise surface a name-conflict error. Avoids silently "skipping" a same-name-different-content file. |
| **`Replace` wire value** | `overwrite = true` + existing `nodeId`; wire enum has no `OVERWRITE` | We send `conflictResolution=OVERWRITE` | **S** | Verify `OVERWRITE` is accepted by our cookie endpoint; if not, adopt the overwrite+nodeId form. Flagged in SPEC_DEDUP §7 / open point. |
| **Per-media dedup (Photos only / Videos only)** | Enum exists, UI never exposes it | none | — | **No change** — deliberately not exposed by the original either. |
| **Apple Photos `.heic` JPEG-BOM heuristic** | Present (iOS-duplicate guard) | **Missing** | **L** | **Out of scope** — only meaningful for an Apple Photos library import, which is out of MVP (AGENTS.md "Scope-Grenzen"). |
| **Dedup state after a 409** | Node cached, record `Ignored` | We `record_upload` on `ConflictSkipped` so the next scan skips cheaply | — | **Keep** — our behaviour is the equivalent and avoids a repeat 409. |
| **Interactive prompt trigger** | Client-side `FileConflict` error + per-schedule modal | Server 409 + per-run prompt | — | **Keep** (see "name conflict detected client-side"). |
| **"Don't ask again" scope** | Per **schedule**, persisted in `schedules.conflict_resolution` | Per **run**, in-memory (`conflict_override`) | **S** | Persist the answer to the folder when "nicht noch einmal fragen" is checked (write `conflict_resolution`), matching the original. Post-MVP is fine; AGENTS.md already keeps the breakdown in memory only. |

**Deliberately not doing:** a local `cloud.db`-style node cache, the Apple-Photos
import path, per-media dedup flags. These are documented boundaries, not oversights.

---

## 6. UNKNOWN

* **Our cookie endpoint's 409 `ErrorType` vocabulary.** The original's device/router
  flow uses `MD5_DUPLICATE` / `DuplicatesConflictError` / `NameAlreadyExists`. Whether
  our `content-<zone>.drive.amazonaws.com/v2/upload` cookie flow returns the same
  strings (and which one for a pure name conflict) is **unverified** — needs a live
  probe.
* **`conflictResolution=OVERWRITE` acceptance.** Our `Replace` assumes this value; the
  original never sends it (it overwrites via `nodeId`). Unverified against our flow.
* **Whether `GetNodeByMd5` dedup is gated by any additional server-side flag** beyond
  `UploadType` (e.g. a weblab). Only the client-side gating was observed.
* **Exact button→outcome mapping of the iOS-dedup prompt** — carried over from
  `docs/SPEC_DEDUP.md` §9, unchanged here.
* **Search path used (for reproducibility):** `ilspycmd -l c`/`-l e` over
  `Amazon Photos.dll` filtered by `conflict|dedup|duplicate|preupload|uploadtype`;
  full decompile to a scratch tree, then `grep` for `PreUploadCheck`, `GetNodeByMd5`,
  `ShouldSuppressMd5`, `AvoidDuplicates`, `DeduplicateOptions`, `ConflictResolution`,
  `EnableDeduplicationForPhotosAndVideos`, `FileNameConflict`, `skip_duplicates`,
  `file_name_conflicts`; German labels via `grep -iE` over
  `de.lproj/Localizable.strings` for
  `duplikat|doppelt|konflikt|überspringen|ersetzen|nachfragen|neue datei|bereits`.

---

## 7. Sources / evidence

Symbol names, label keys and file locations cited above (no code copies):

* **Enums:** `Amazon.CloudDrive.Database.ManualUpload.ConflictResolution`,
  `…DeduplicateOptions`, `…UploadType`; `Amazon.CloudDrive.Services.CloudDriveConflictResolution`;
  `Amazon.CloudDrive.Services.CloudDrive.SuppressFileUpload`;
  `Amazon.CloudDrive.Database.RetryReason`, `…TransferStatus`, `…TransferError`.
* **Detection:** `CloudDriveStorageService.PreUploadCheck` (two overloads),
  `…CloudNodeCheck`, `…HandleNodeResponse`, `…HandleNodeResponseSync`;
  `CloudDriveDatabase.GetNodeByMd5` / `NodeExistsWithMd5` / `IX_md5`;
  `UploadRecordExtensions.GetSuppressFileUpload` / `GetConflictResolution` /
  `ShouldSuppressMd5Check` / `ShouldForceNodeIdLookupUsingLocalId`;
  `NameConflictStrategy.GetConflictResolution` / `GetScheduleConflictResolution`;
  `EnumerationService.GetUploadRecord`.
* **Status/errors:** `ExceptionExtensions.ToTransferStatus` / `ToTransferError` /
  `TryParseErrorType`; `BackupStorageService` (catch blocks for
  `FileIgnoredException` / `FileIgnoredPreUploadException` / `FileSkippedException` /
  `FileIgnoredForPotientialDuplicationException`).
* **State:** `ManualUploadDatabase` (schema/migrations: `uploads.precomputed_md5`,
  `upload_type`, `node_id`; `schedules.dedupe_photos_videos`, `conflict_resolution`),
  `SchedulesDatabase` migration 3 (`conflict_resolution integer default 0`),
  `ScheduleRecord` (`[Table("schedules")]`), `CloudDriveDatabase` (`nodes.content_md5`).
* **Defaults / settings:** `AppSettings.DefaultOnboardingDedupeChecked`,
  `…DefaultPostOnboardingDedupeChecked`, `…DefaultOnboardingContentFilter`;
  `CustomerSettings.EnableDeduplicationForPhotosAndVideos` /
  `FilterOutPotentialDuplicatesFromApplePhotosLibrary`;
  `ScheduledBackupItem.AvoidDuplicates` / `ConflictResolution`;
  `BackupScannerWorker` (onboarding record).
* **UI:** `BackupConfigurationViewController` (identifiers
  `BackupView_SettingsAvoidDuplicates_Checkbox`,
  `BackupView_FileNameConflictResolution_{SaveAsNew,Ignore,Replace,AskMeEveryTime}_RadioButton`),
  `FileNameConflictViewModel`, `FileNameConflictWindowService`,
  `ResolveScheduleNameConflictsEventHandler`, `ManualUploadDatabase.ResolveNameConflictsForSchedule`.
* **German labels (`de.lproj/Localizable.strings`):**
  `clouddrive_desktop_upload_settings_skip_duplicates_option_title` /
  `_subtitle`, `clouddrive_desktop_statuswindow_scheduleconfiguration_dedupeoption`,
  `clouddrive_desktop_upload_settings_file_name_conflicts_{auto_rename,ignore,replace,ask}_option`,
  `clouddrive_desktop_upload_settings_file_name_conflicts_title`,
  `clouddrive_desktop_statuswindow_scheduleconfiguration_filenameconflict_{askmeeverytime,ignore,replace,saveasnewfile}` (legacy),
  `clouddrive_desktop_filenameconflict_overlayview_{title,body,ignore_button_text,replace_button_text,saveasnew_button_text,donotaskagain_checkbox_text}`,
  `clouddrive_desktop_upload_statuswindow_statusignored` ("Zuvor hochgeladen"),
  `clouddrive_desktop_statuswindow_statusskipped` ("Verworfen"),
  `clouddrive_desktop_iosdedupeprompt_{header,information}`,
  `clouddrive_desktop_preferences_filter_potentialduplicates_{title,description}`.
* **Our code (read-only, for the gap table):** `src/folders.rs` (`ConflictResolution`,
  `BackupFolder.dedup` / `conflict_resolution`), `src/dedup.rs` (`should_skip`,
  `stat_matches`, `record_upload`, `ix_uploads_md5`), `src/upload.rs`
  (`FileOutcome`, `resolve_conflict`, `UploadShared.conflict_*`),
  `src/backup_config.rs`, `src/prime_photos.rs` (`conflict_node_id`,
  `CONFLICT_PREFIX`, 409 branch), `src/i18n.rs` (conflict/dedup strings).
* **Cross-reference:** `docs/SPEC_DEDUP.md` (identity, filter taxonomy, Apple-Photos
  prompt), `docs/SPEC_ONE_TIME_UPLOAD.md` (manual/one-shot path),
  `docs/PROTOCOL.md` (upload headers/409), `AGENTS.md` (our deliberate boundaries).
