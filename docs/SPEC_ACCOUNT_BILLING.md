# Account Identity, Storage/Quota, Banners & Multiple Accounts — Clean-Room Spec (reverse-engineered)

Behavioural specification of the **original** Amazon Photos desktop client's **account
identity**, **storage/quota**, **status & upsell banner** and **multiple-account**
behaviour, plus the mapping onto this port. The original behaviour below is
**reverse-engineered from the shipped managed assembly**, not inferred from string
tables alone.

This document is **clean-room**: it records *observed behaviour* in our own words and
tables. It contains **no verbatim decompiled code** and **no personal data**. German
user-facing labels are quoted from the shipped `de.lproj/Localizable.strings`
(product strings, not code); type and member names are cited only to the extent
needed to describe the mechanism.

**Confidence convention.**
- **belegt** = read directly out of the reverse-engineered `Amazon Photos.dll` (member/branch),
  the German strings file, or our committed code.
- *inferred* = reasoned from adjacent evidence, not read off a line.
- **UNKNOWN** = could not be determined from the inspected sources.

---

## 1. Purpose & scope

This doc owns four tightly coupled surfaces of the original client: (1) **account
identity** — the in-memory `AccountInfo` model, the worker that refreshes it, which
service/endpoint each field comes from, when the refresh runs, and whether anything is
cached or persisted; (2) **storage / quota** — the `youraccount_*` footer block, its
units, the fill computation, the Prime/unlimited-photos dependency and the help
popover; (3) **banners & notifications** — `notification_{overquota,trialexpired,
campaignwindow}_*`, `outage_banner_text`, their triggers, gating and dismissal
semantics; (4) **multiple accounts** — the `folderconflict_*` family ("anderes
Benutzerkonto") and the per-account state layout.

Not duplicated here: **login / session / cookies** → `docs/SPEC_LOGIN.md`. The general
notification inventory → `docs/SPEC_NOTIFICATIONS.md`. The window/footer chrome and
measured sizes hosting these blocks → `docs/UI_FLOWS.md` §3, `docs/UI_STATES.md` §3.
The upload-side reaction to a spent quota (`TransferError.CloudDriveOverQuota`) →
`docs/SPEC_UPLOAD_PROGRESS.md`.

---

## 2. Reverse-engineering method (belegt)

- Tooling: `ilspycmd` is installed at `~/.dotnet/tools/ilspycmd` (v11.1.0.9782), run with
  `PATH` extended and `DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`. No
  tool was installed for this task. → **belegt**
- Target (read-only, never launched): `/Applications/Amazon Photos.app` →
  `Contents/MonoBundle/Amazon Photos.dll` (managed assembly, 2486 types) and
  `Contents/Resources/de.lproj/Localizable.strings`. A full type dump (175 559 lines)
  plus the targeted classes below were read. → **belegt**
- Classes used as evidence: `Infrastructure.{AccountInfo, AppSettings, CustomerInfo,
  CustomerInfoExtensions, DomainProvider}`, `Services.CloudDrive.Workers.
  CloudDriveAccountRefreshWorker`, `Services.CloudDrive.{CloudDriveAccountService,
  CloudDriveService, ServiceUrlProvider}`, `Services.CloudDrive.Model.
  CloudDriveAccountService.{GetQuotaResponse, HouseholdStorageDetails,
  FeatureEvaluation, FeatureType}`, `Services.CloudDrive.Model.{GetAccountInfoResponse,
  GetPreferencesResponse, GetUsageResponse, Subscription, Benefit, AccountInfoStatus}`,
  `Services.{SubscriptionStringHandler, Photos.{AccountFeatureExtensions,
  PhotosService}}`, `Services.Events.EventHandlers.{AccountStatusNotificationHandler,
  NearQuotaEventHandler, OverQuotaUploadEventHandler, PersistedRepeatableTriggerState}`,
  `Services.PopupNotificationCenter.{PopupNotificationCenter, NotificationFactory,
  Quota.AccountQuotaNotification, NotificationType}`, `UI.Controls.{QuotaUsageView,
  DriveAccountButton}`, `UI.DriveWindow.{StorageUsageView, DriveWindowViewModel,
  HomeTab.ShowYourPlanIntentHandler}`, `UI.LseBanner.{LseBannerViewModel,
  LseBannerView}`, `UI.UpsellWindow.*`, `UI.CampaignWindow.{CampaignViewModel,
  CampaignWindowService}`, `UI.PreferencesWindow.GeneralPreferencesTabView`,
  `OnboardingWindow.SyncOnboardingPageViewModel`, `BaseAppSyncFolder`,
  `OutageSettings`. → **belegt**
- Corroboration: our own committed sources in `src/` (Grep/Read, see §9). → **belegt**

---

## 3. Account identity — `AccountInfo` and `RefreshAccountInfo` (Q1)

### 3.1 The model is an in-memory singleton

`Infrastructure.AccountInfo` implements `IAccountInfo` and is registered in the DI
container as a **single instance** (`RegisterType<AccountInfo>().As<IAccountInfo>().
SingleInstance()`). Every consumer — footer storage block, status-popup header,
FTUE, upsell gating — observes the same reactive object. → **belegt**

| Field | Source | Notes |
|---|---|---|
| `Quota` / `Available` (long, bytes) | `GetQuotaResponse.{quota,available}` | **raw bytes**; `Available <= 0` **is** the over-quota signal |
| `UsedQuota` | derived = `Quota - Available` | not a server field |
| `AccountName` | `GetPreferencesResponse.name` | **the real display name** |
| `AvatarPath` | derived from `GetPreferencesResponse.avatarNodeId` | local thumbnail file path, §3.4 |
| `AccountInfoLastUpdated` | `UpdateAccountInfoLastUpdated()` = `DateTimeOffset.Now` | client clock, not a server value |
| `LastUsageInfoCalculated` | `GetQuotaResponse.lastCalculated` (unix seconds) / `GetUsageResponse.lastCalculated` (ISO-8601) | two formats, two endpoints |
| `Status` (`AccountInfoStatus`) | `GetAccountInfoResponse.status`, parsed case-insensitively | `NONE, PRECREATED, ACTIVE, LOCKED, DELETE_IN_PROGRESS, REPARTITIONING_TO_BE_DELETED, MIGRATED, ERROR`; unparsable → `ERROR` |
| `Plans` | `GetQuotaResponse.plans` | plan identifiers |
| `UsageByTypesInfo` | `GetUsageResponse` | `UsageByDocuments / Photos / Videos / Others`, each `{ billable, total } × { bytes, count }` |
| `IsUnlimitedPhotosFeatureEnabled` | `GetQuotaResponse.unlimitedPhotosFeature.enabled` **and** `featureType == UNLIMITED_PHOTOS` | server-side feature evaluation |
| `RecognitionEnabled` | `GetPreferencesResponse.recognitionEnabled` | |
| `Benefits` / `Subscription` | **never written** by the refresh worker | `Benefit{expiration,benefit}`, `Subscription{planId,status,autoRenewEnabled,contractEndDate,nextBillAmount,…}` |

`Benefits` and `Subscription` are **declared and read but never populated** by
`CloudDriveAccountRefreshWorker` — the only writer of the quota/preferences path
(a `GetSubscriptionResponse` model and a photos subscriptions call exist, but no
assignment into `AccountInfo.Subscription` was found). → **belegt** / *inferred*
reserved for a code path outside this build.

Derived flags (all **belegt**): `UsedQuota` = `Quota - Available`. `HasPlans` =
non-empty `Plans`. `HasUnlimitedEverything` = `Plans` contains `CDSPUS0002` or
`CDSPUS0000`. `HasUnlimitedPhotos` = `!HasPrimeBenefit && !IsUnlimitedPhotosFeature-
Enabled` → fall back to `Plans` containing one of `CDSPB00001, CDSPUS0002, CDSPUS0000,
CDSPUS0003, CDSPUS0001`, else `true`. `HasPrimeBenefit` = any `Benefit` with
`benefit == "CDSPB00001"` and `expiration > UtcNow` — this reads the
**never-populated** `Benefits`, so it is effectively always `false` here (*inferred*).
`IsOverQuota` = `AccountInfoLastUpdated.HasValue` **and** `Available <= 0`.
`ShouldShowUpgradeOption` = `IsOverQuota` **or** `IsNearQuota`.

`IsNearQuota` is the one worth spelling out: `!IsOverQuota` **and**
(`Quota < 1 TiB` → `UsedQuota/Quota × 100` inside `[NearQuotaValueInPercent, 100]`;
else `Available` inside `[0, NearQuotaValueInBytes]`). The thresholds default to
**`NearQuotaValueInPercent = 90.0`** and **`NearQuotaValueInBytes = 10737418240`**
(= 10 GiB) — so for accounts of 1 TiB or more "near quota" means *10 GiB left*, not
*90 % used*. → **belegt**

### 3.2 The refresh worker and its four endpoints

`Services.CloudDrive.Workers.CloudDriveAccountRefreshWorker` is a registered worker task
and owns the whole refresh. `RefreshAccountInfo(dateTimeOffset)` is its single entry
point and is **re-entrancy guarded** (`_lockedForUpdate`): a call while one is running
returns immediately. It runs four updates **in parallel** and swallows their exceptions
into a warning log:

| Update | Service call | Resolved URL | Writes |
|---|---|---|---|
| `UpdateQuotaInfo()` | `ICloudDriveAccountService.GetQuota()` | `<metadataUrl-host>/drive/v2/account/quota?customerId=UNNEEDED` | `Plans, Available, IsUnlimitedPhotosFeatureEnabled, Quota, LastUsageInfoCalculated, AccountHyperLinkText` |
| `UpdatePreferences()` | `ICloudDriveService.GetPreferences()` | `<metadataUrl>/drive/v1/account/preferences` | `AccountName, RecognitionEnabled, AvatarPath` |
| `UpdateUsageInfo()` | `ICloudDriveService.GetUsage()` | `<metadataUrl>/drive/v1/account/usage` | `UsageByTypesInfo` |
| `UpdateAccountFeatures()` | `IPhotosService.GetAccountFeatures()` | `photoapp/v3/account/features/DESKTOP?customerId=…&features=upsell` | `ICustomerSettings.AccountFeatures` (persisted) |

URL resolution (**belegt**): every account path is composed from the
`GetEndpointsResponse.metadataUrl` discovered at login (`CloudDriveService.
GetAccountUrl(path)` → `GetMetadataUrl("account" + path)`). The quota call is
different: `CloudDriveAccountService` builds its own CDRS base as
`scheme://host(metadataUrl) + "/drive/v2"` and appends `/account/quota`, with the
literal query `?customerId=UNNEEDED`. The three legacy paths match our
already-captured `docs/PROTOCOL.md` §2.13–§2.15. → **belegt**

**Fetch timing (belegt):** on worker start one immediate
`RefreshAccountInfo(DateTimeOffset.Now)` on a background task; then a repeating tick
every **`CloudDriveAccountRefreshTimeInSeconds`**, default **86400 s = 24 h**; plus on
demand via `UpdateAccountInfo()` (public, runs **only** `UpdateQuotaInfo()` behind the
same lock), called by the campaign window when it reaches `CampaignState.Complete`.
`NotifyOverQuotaAfterTimeInSeconds`, default **86400 s**, throttles the repeated
over-quota native toast (§5.4).

**Cache/persistence — the decisive answer for our port (belegt):** `AccountInfo` is
**not persisted**. It is an in-memory singleton; nothing writes it to disk. Its only
serialised appearance is `IAccountInfo`'s JSON attributes plus a crash-report dump that
appends quota/plan/status values into a StringBuilder for `App.WriteCrashLog`.
Consequence: after a restart the client has **no** name, no avatar and no quota until
the first refresh round-trip completes. The avatar *thumbnail file* does live on disk
(`Constants.ThumbnailsPath`), but the path string pointing at it does not. → **belegt**

### 3.3 Account status validation

`AccountStatusValidator.ValidateAccountStatus()` runs at startup (*inferred* — it is the
consumer of `GetEndpointsResponse.customerExists`) and gates the app on the parsed
status: `NONE`/`ACTIVE` are valid (`IsAccountValid`); `LOCKED` raises a non-closeable
modal with the "gesperrt" strings; `DELETE_IN_PROGRESS`, `PRECREATED`,
`REPARTITIONING_TO_BE_DELETED`, `MIGRATED` and `ERROR` share one generic "kill" dialog.
The status fetch picks the CDAS or the legacy `account/info` endpoint behind the
`EnableAccountOperationsOnCdas` treatment flag ("T1" → CDAS). → **belegt**

### 3.4 The avatar — the documented path to a real profile picture

This is the part our port is missing, and it **is** recoverable in the original
(**belegt**, every step): (1) `GetPreferences` returns `avatarNodeId`, a cloud **node
id**, not a URL; if `null`, the worker stops. (2) Otherwise `GetProfileImage(nodeId)`
computes a local path `Constants.ThumbnailPath/<nodeId>_60_60.jpeg` — a **60×60** view box
(the const `AvatarViewBoxSize = 60`).
(3) If `IThumbnailService.IsThumbnailValid(path)` says the cached file is usable it is
reused as-is. (4) Else `GetSmartCroppedThumbnail(nodeId, ownerId: null, 60, 60, path)`
downloads the smart-cropped thumbnail into that path. (5) On success
`AccountInfo.AvatarPath = path`; on any exception the worker deletes the thumbnails,
returns an empty string and the caller sets `AvatarPath = null` and returns `false`.

Both UI consumers behave identically and defensively: `StatusPopupHeaderView` loads
`new NSImage(AvatarPath)` in a try/catch and falls back to a bundled placeholder icon,
`DriveAccountButton` subscribes to `AvatarPath` and swaps to
`ImageType.AvatarPlaceholder` whenever the value is `null` or loading throws — and
`DriveAccountButton` additionally renders **`AccountName` as its button title**, so the
account name is a first-class UI element, not an extra. → **belegt**

**Thumbnail URL — traced end-to-end (belegt, 2026-10-14).** `GetThumbnailUrl(nodeId)`
(`Services.CloudDrive.EndpointsProvider`) returns the concatenation
**`thumbnailServiceUrl + "v1/thumbnail/" + nodeId`**, where `thumbnailServiceUrl` is the
string the login `GET …/endpoints` call puts into `GetEndpointsResponse` (our capture in
`docs/PROTOCOL.md` §2.x has `https://thumbnails-photos.amazon.de/`).
`CloudDriveService.DownloadThumbnail` collects the parameters into a `NameValueCollection`
and sets it as the URL query; every key/value is `WebUtility.UrlEncode`d
(`Helpers.QueryExtensions.ToUrlEncodedQuery`), so a comma is sent as `%2C`. It is a plain
**`GET`** on the shared CloudDrive client, and the response body is the **image itself** —
no JSON wrapper, no `tempLink` (contrast the node path `…/nodes/{id}?tempLink=true`, which
hands back the 13.5 MB CR3 raw, §9).

| Query parameter | Value | When |
|---|---|---|
| `ownerId` | node owner id | only if non-empty — the **avatar passes `null`**, so it is absent |
| `viewBox` | `"W,H"`; avatar `"60,60"` | always (passed by `ThumbnailWorker.GetThumbnail`) |
| `fit` | `"clip"` (const `ThumbnailWorker.ThumbnailClipFit`) | always |
| `cropOffset` | `"left,top"` | only on the smart-crop path (focal box computed) |
| `cropSize` | `"w,h"` | only on the smart-crop path (focal box computed) |
| `quality` | int | only if the caller passes one — `ThumbnailWorker.GetThumbnail` does **not** |
| `encoding` | `"jpeg"` | always (added last) |

So the avatar request is, in full (**belegt**):

`GET https://thumbnails-photos.amazon.de/v1/thumbnail/<avatarNodeId>?viewBox=60%2C60&fit=clip[&cropOffset=…&cropSize=…]&encoding=jpeg`

The crop pair only appears on the *smart* path: `GetSmartCroppedThumbnail` first calls
`IPhotosService.CreateCollageNarrative(nodeId)` (`POST …/drive/v2/memories/v1/collage/
narratives` on the photos-BFF url) to read a focal box and the image dimensions; if that
yields usable geometry it derives the crop from the focal box, otherwise it falls back to
`PerformMiracleCrop` (one uncropped request, then a centred crop). A bare thumbnail with no
focal box is still just `viewBox` + `fit` + `encoding`.

**Auth and content type (belegt).** The download goes through
`Services.CloudDrive.CloudDriveHttpMessageHandler`, which stamps every CloudDrive request
with `x-amz-access-token` (a device/refresh-token access token from `IAuthService`),
`x-amz-clouddrive-appid` (base64 of the compiled-in OAuth app id, `=` padding stripped) and
`x-amzn-clouddrive-source` (the persisted `CustomerSettings.DeviceSourceId`). **No cookies
are used by the original** — that is its device flow. The expected payload is **JPEG**
(`encoding=jpeg`); the cache validator `IsThumbnailValid` accepts a file only if it is
≥ 8 bytes and either a JPEG (SOI header **and** EOI trailer) or a PNG signature, and
`GetThumbnail` copies the raw response stream straight into the file.

**Our port reaches the same image with cookies (belegt, 2026-10-14).** Live test: the URL
above answers to our **cookie set alone** — HTTP 200, `Content-Type image/jpeg`, a JPEG at
the requested 60×60 — so the port can load the avatar with the credentials it already holds,
no device token. The `…/nodes/{id}?tempLink=true` detour is the **wrong** path: it hands back
the **raw** file (a 13.5 MB CR3 for the tested account), not the cropped thumbnail.

**Local filename (belegt).** `ThumbnailWorker.CreateDefaultPath(nodeId, viewBox)` combines
`Constants.ThumbnailPath` with the `Constants.DefaultThumbnailFormat` `"{0}_{1}.jpeg"`
(the `viewBox` string with `,` → `_`), and the directory is `Constants.ThumbnailPath` =
`LocalStoragePath/Thumbnails` = `~/Library/Application Support/Amazon Cloud Drive/Thumbnails`
(singular `ThumbnailPath` — the earlier note here wrote `ThumbnailsPath`). For the avatar
the view box is 60×60, so the file is **`<nodeId>_60_60.jpeg`**; `ownerId` is **not** part
of the name.

---

## 4. Storage / quota — the `youraccount_*` block (Q2)

### 4.1 Two renderings of the same numbers

Both live in `UI.Controls.QuotaUsageView` and
`UI.DriveWindow.StorageUsageView`; they read the same `IAccountInfo` fields and
recompute on every `Available` / `Quota` property change. Neither persists anything.

| Aspect | `QuotaUsageView` (footer, main + popup) | `StorageUsageView` (circle variant) |
|---|---|---|
| Bar | `SquareProgressIndicator`, bar style, min 0 / max 1, height 8, fixed width 200 | `CustomCircularProgress`, 30 pt |
| Fill | `UsedQuota / Quota` (double 0..1) | `UsedQuota / Quota × 100`, then /100 for the circle |
| Title | `AccountHyperLinkText` as a hyperlink button | fixed label `…_storage_usage_label` |
| Value | `…_youraccount_usageformat`, swapped to `…_yourprimeaccount_usageformat` when `HasPrimeBenefit \|\| IsUnlimitedPhotosFeatureEnabled` | `…_storage_usage_value`, swapped to `…_prime_storage_usage_value` when `HasUnlimitedPhotos` |
| Upgrade link | `…_youraccount_upgradenow_buttontitle`, **hidden** when `!ShouldShowUpgradeOption`; in the full layout the title link is hidden instead | hyperlink = `…_notification_overquota_unlimitedphotos_button` when `HasUnlimitedPhotos`, else `…_notification_overquota_freetier_button` |
| Help | `?` button (18×18) → popover | none |
| Trigger | `Available`, `Quota`, `AccountHyperLinkText` merged | `Available`, `Quota` merged |

The bar colour is the only other visual quota signal besides the number:
**`NormalQuotaColor`** (theme accent) → **`NearQuotaColor`** (warning) →
**`OverQuotaColor`** (danger), in exactly that precedence. → **belegt**

### 4.2 Units and the format string

Quota and available are **raw byte counts** (`long`). Presentation goes through
`Amazon.CloudDrive.FileSize`, which owns a `FileSizeUnit` ladder
(`Bytes, KiloBytes, MegaBytes, GigaBytes, TeraBytes`) and a "condensed" unit picker
escalating B → kB → MB → GB → TB; GB/TB are rendered without re-condensing so a
fractional value keeps its digits. Both the bar's `{1} von {2}` values and the
`AccountHyperLinkText` fallback (`youraccount_yourstoragetitle` + `: ` +
`new FileSize(quota, "0.##")`) use this one formatter, so the two surfaces cannot
disagree on units. Note the argument positions: the call passes `null` as `{0}` and the
two `FileSize` strings as `{1}`/`{2}` — which is why the key reads "{1} von {2}".
→ **belegt**

### 4.3 The Prime dependency, spelled out

- **Which format string is used** depends on `HasPrimeBenefit || IsUnlimitedPhotosFeatureEnabled`
  (footer) resp. `HasUnlimitedPhotos` (circle). Non-Prime accounts get "X von Y belegt";
  Prime/unlimited-photos accounts get "X von Y **für verwendetes Video**" — under
  unlimited photos the *number shown is the non-photo usage*, because photos are free.
- **The hyperlink text** is either the plain `…_youraccount_yourstoragetitle` or, when
  `IsUnlimitedPhotosFeatureEnabled`, that title + ": " + `…_unlimitedphotos`.
- **The help popover** (`StorageHelpText`) branches on
  `!HasPrimeBenefit && !IsUnlimitedPhotosFeatureEnabled` → the non-Prime tooltip
  ("Für Ihr Speicherlimit werden Fotos und Videos gezählt."), else the Prime tooltip
  ("Mit Prime erhalten Sie unbegrenzten Fotospeicherplatz. …"). The sub-line
  (`…_youraccount_primecustomers_storagetooltip_subtext`) links to
  `CloudDriveLink.SupportedTypesHelp` regardless of tier.
- **"Last updated"** is `…_youraccount_lastupdatedformat` filled with
  `(AccountInfoLastUpdated ?? DateTimeOffset.MinValue).ToAgoShortString()` — a purely
  client-side "x ago" string, **not** the server's `lastCalculated`.
- **Both upgrade affordances** fire `LaunchCloudDriveLinkIntent(CloudDriveLink.ManageStorage)`
  → `https://<marketplace-domain>/photos/storage` in the default browser. → **belegt**

### 4.4 Household / per-member detail is fetched but not rendered

`GetQuotaResponse` carries `personalQuota`, `householdStorageDetails`
(`totalHouseholdQuota`, `householdAvailable`, `personalQuotaContribution`, `memberUsage[]`
with `customerId` + `householdUsageBytes`) and `grants`. The refresh worker reads only
`quota`, `available`, `plans`, `unlimitedPhotosFeature` and `lastCalculated`; the
household models have **no consumer** in this build. → **belegt** (no assignment found)
→ *inferred* dead. `GetUsageResponse`'s per-category split *is* consumed, but only into
`UsageByTypesInfo`; its two derived helpers (`TotalNonMediaBytesUsage`,
`HasNonMediaFiles`) also have no observed renderer. → **belegt** / *inferred*

---

## 5. Banners, notifications and upsell (Q3)

### 5.1 The three families and their true wiring

| Key family | Rendered where | Triggered by | Dismissible? |
|---|---|---|---|
| `…_notification_overquota_*` | (a) in-app status-popup row, (b) native toast | quota refresh crossing zero | yes (row), yes (toast) |
| `…_notification_trialexpired_*` | **nowhere in this build** | — | — |
| `…_notification_campaignwindow_title/message` | **nowhere in this build** | — | — |
| `…_notification_campaignwindow_button` | sync mass-delete notification (a completely different feature) | mass local/cloud deletes | yes |
| `…_outage_banner_text` | main-window banner, 40 pt tall | outage-settings regions | **no** |

**belegt** for each cell: the three `trialexpired` strings and the two `campaignwindow`
title/message strings have exactly one reference each — the generated `Strings.*`
accessor — and no call site; only `…_notification_trialexpired_message` has a second
one, in `SubscriptionStringHandler.GetSubscriptionExpiredMessage()`, which itself has
no caller. `…_notification_campaignwindow_button` is reused verbatim as the action
label of the sync mass-delete notification. → **belegt**

### 5.2 The in-app quota row

`NotificationType` is an ordered enum (`OverQuota = 0, NearQuota = 100,
UpdateAvailable = 1000, ThisDay = 10000, Story = 100000`) and the status-popup table
gives quota rows a fixed **50 pt** height. `AccountOverQuotaNotification` uses the
error icon and `…_statuspopup_notification_overquota`, `AccountNearQuotaNotification`
the warning icon and `…_statuspopup_notification_nearquota`; both derive from one
`AccountQuotaNotification : TextNotification`, share
`IntentText = …_preferences_manageStorage` ("Speicher verwalten") and
`CreateIntent` → `LaunchCloudDriveLinkIntent(CloudDriveLink.ManageStorage)`, and
inherit `IsDismissible => true` / `AllowMultiple => false`. → **belegt**

`PopupNotificationCenter.Publish` enforces single-instance-per-type: if a notification
of the same type is already present **without** an expiration the new one is dropped,
else the old one elapses and is replaced. Quota notifications are created with no
expiration, so `CanAutoElapse` is false and they **never auto-expire** — removed only
by an explicit `ElapseTypes(OverQuota|NearQuota)` or by the user. Dismissal is the
"Schließen" button (`…_statuspopup_notification_dismiss` → `DismissNotification-
Command`), and executing the intent **also** dismisses the row: two user-initiated
paths, no automatic one. → **belegt**

### 5.3 When the quota row appears

`NotifyQuotaChanges(wasNear, wasOver)` runs at the end of `UpdateQuotaInfo()`, i.e.
once per refresh (start + every 24 h + on-demand): if it *was* over and now is not →
`ElapseTypes(OverQuota)`; else if it *was* near and now is not →
`ElapseTypes(NearQuota)`. Then, if **over** now → elapse every NearQuota row and
`Publish(OverQuota, isNew)`; else if **near** now → elapse every OverQuota row and
`Publish(NearQuota)`, plus a `NearQuotaEvent` when `CustomerSettings.FtueVersion == 1`.
`isNew` = never shown before, or `LastShownOverQuotaNotificationTime + 24 h < now`.

So the two states are **mutually exclusive in the popup** and neither row can survive
the condition that produced it; `isNew` only drives the unread badge
(`PopupNotificationUnreadEvent` / `PopupNotificationReadEvent`). → **belegt**

### 5.4 The native over-quota toast (and the re-arm)

`AccountStatusNotificationHandler` listens for `AccountStateEvent`, which has two
states. On **OverQuota** and `ShouldShowOverQuota()` it publishes an
`AccountStatusNotification` with `Priority = ToastNotificationPriority.High` whose
title/message/button come from `SubscriptionStringHandler` (§5.1), stamps
`LastShownOverQuotaNotificationTime = now`, and navigates to
`CloudDriveLink.UpgradePlan` on action. On **QuotaAvailable** it calls
`ResetErroredToWaiting(TransferError.CloudDriveOverQuota)`: every upload row that had
failed for over-quota is put back into the queue, so uploads resume by themselves once
space returns. → **belegt**

`ShouldShowOverQuota()` is the 24 h throttle — no stored timestamp → show;
`LastShownOverQuotaNotificationTime + NotifyOverQuotaAfterTimeInSeconds (86400) < now`
→ show; else suppress — and it is **persisted**, so the throttle survives restarts.
`AccountStateEvent(OverQuota)` is published not only by the refresh worker on the
available→spent edge (`_wasQuotaAvailable`) but also by transfer workers mapping a
`FileOverQuotaException` / `TransferError.CloudDriveOverQuota` onto the same event; the
refresh worker emits `AccountStateEvent(QuotaAvailable)` on the spent→available edge.
→ **belegt**

### 5.5 The outage banner

`LseBannerViewModel` exposes one string (`…_outage_banner_text`) and one boolean.
`ShouldShowBanner()` compares the outage **regions** (`OutageRegion` = `NA, FE, EU,
ALL`) from `OutageSettings.Outages.Regions` against the signed-in customer's region via
`LocalizationHelper.IsNorthAmerica / IsEuropeanUnion / IsFERegion` and records a health
metric with the matched region (or `LseBanner.Hidden`). The banner is a plain `NSView`:
warning icon + centred text, warning background, **no close button and no intent**,
driven by `ShouldShowLseBanner` with `DistinctUntilChanged` in `DriveMainView`. Its
source is a **remote config file**, not an API call: `OutageSettingsWorker` fetches
`https://d29x207vrinatv.cloudfront.net/config/outage-settings.config` into
`LocalStoragePath/outage-settings.json` (`refresh_interval_secs` default **1800 s**),
and a file watcher republishes the change — that is what re-evaluates the banner.
→ **belegt**

### 5.6 Upsell windows

Four `UpsellType` values (`OversaturatedUploadQueue, NearQuota, OverQuotaUpload,
HomeTab`); two are quota-driven — `NearQuotaEventHandler` on the `NearQuotaEvent` from
§5.3, `OverQuotaUploadEventHandler` on `AccountStateEvent(OverQuota)`. Both are gated
by the same `PersistedRepeatableTriggerState` (keys `…NearQuotaEventHandler{,End}` /
`…OverQuotaEventHandler{,End}` in the **persisted**
`CustomerSettings.SatisfiedConditonalEventHandlers`; `RepeatPeriod = 30 days`,
`TimeFromFirstSuccessToFinal = 90 days`; `CanRepeat()` = a previous-show timestamp
exists **and** `previous + 30 d <= now` **and** `firstSuccess + 90 d >= now`, so the
upsell stops entirely after 90 days), then by `UpsellAvailabilityService.
ShouldDisplayUpsellWindow()`: marketplace `A1VC38T7YXB528` (Japan) with
`UpsellEnabledInJapan == false` → no; `AppSettings.UpsellWindowEnabled` (default
**true**); EU requires `UpsellEnabledInEuropeanUnion`; and
`CustomerSettings.AccountFeatures[Upsell]` must be `Enabled` (the server-driven
feature from §3.2). → **belegt**

Modal geometry, string variants and the cookie-injected manage-storage navigation are
documented in `docs/UI_FLOWS.md` §10; the only account-conditioned extra branch here
is the FTUE's `ShouldShowUpsellView => !_accountInfo.HasPlans`. → **belegt**

### 5.7 The campaign window

`UI.CampaignWindow` hosts a browser-based window whose view model exposes a
`CampaignState` and a `CampaignUri`. On `CampaignState.Closed` it closes the window; on
`CampaignState.Complete` it calls `IAccountService.UpdateAccountInfo()` — the on-demand
quota refresh from §3.2. That is why this doc mentions it: it is the channel by which a
remote campaign can force a re-read of the storage numbers.
`ICampaignWindowService.ShowAsync(url)` has **no caller inside this assembly**, so
which URL is loaded and when is decided elsewhere. → **belegt** (no managed caller) /
**UNKNOWN** (who drives it).

---

## 6. Multiple accounts (Q4)

### 6.1 `folderconflict_*` is a *sync-folder* conflict, not an account switcher

The string family `…_onboarding_folderconflict_*` is used in exactly one place:
`OnboardingWindow.SyncOnboardingPageViewModel` (both `CreateSyncFolder()` and
`ChooseSyncFolder()`). Both call `BaseAppSyncFolder.ValidateCustomer(path)` before
accepting a local folder as the **sync root**, and on failure show
`…_folderconflict_warningheader` ("Konflikt mit Synchronisierungsordner") plus
`…_folderconflict_warningmessage` ("Der ausgewählten Ordner wird bereits von einem
anderen Benutzerkonto verwendet. …"). Two sibling warnings exist and must not be
confused with it: `…_onboarding_invalidsyncfolder_*` (bad drive location) and
`…_onboarding_syncfolder_overlappingwithautosave_*` (overlaps a scheduled backup).
→ **belegt**

`ValidateCustomer` is the "other account" check, and its mechanism is the only place the
model knows about foreign accounts (**belegt**): it walks the chosen path **and every
parent directory** upwards; at each level it reads the extended file attribute
**`com.amazon.drive.sync.root`**; if that attribute is non-empty **and** different from
the current `ICustomerInfo.CustomerId`, the path is rejected. On rejection it records
`HealthMetrics.Onboarding.SyncFolderConflict` and a `BusinessMetrics.
SyncFolderSelection` metric with dimension `"FolderConflict"`.

So "anderes Benutzerkonto" means literally: *another Amazon account that has used this
machine stamped its customer id into a folder at or above this path*. The companion
`CustomerId => GetAttribute(Path, "com.amazon.drive.sync.root")` and
`BelongsToAuthenticatedCustomer()` (`CustomerId == _customerInfo.CustomerId`)
properties complete the same rule. The dialog path (`…_folderconflict_folderdialog-
header`) only offers picking another folder — never a different account. → **belegt**

### 6.2 The per-account state layout — proof of multi-account design

`CustomerInfoExtensions.GetCustomerDatabaseFileName(fileName)` returns
`CustomerId + "-" + fileName`, and every stateful store is built with it
(**belegt**, call sites):

| Store | File |
|---|---|
| settings | `<id>-settings.json` (`LocalStoragePath`) |
| uploads / schedules / download / cloud cache / sync | `<id>-{uploads,schedules,download,cloud,sync}.db` |
| feedback campaigns | `<id>-feedback-campaign-interactions.json` |
| app settings, outage settings | `app-settings.json`, `outage-settings.json` — **shared**, not per-customer |

`CustomerInfo` carries `Email, PostalCode, CustomerId, MobileNumber, Name, Region,
ServiceRegion, CustomerBaseUri`, with an `UnknownCustomer` sentinel
(`"UNKNOWN_CUSTOMER"`, …) and `IsAuthenticated => !IsNullOrWhiteSpace(CustomerId)`;
`Region`/`ServiceRegion` come from `GetEndpointsResponse.countryAtSignup` / `.region`.
→ **belegt**

### 6.3 Sign-out, and why "switch account" is not in this build

Sign-out is reachable from three places, all routed to
`ICustomerSignOut.SignOut(shouldRestart)`: `DeregisterUserIntentHandler` (the
preferences "Abmelden" button), `AccountStatusValidator.SignOut`, and
`AppRestartInvalidRefreshTokenHandler`, which clears the stored tokens and restarts the
app. → **belegt**

The preferences general tab is the account block itself: "Angemeldet als:"
(`…_preferences_signed_in_title`), then the customer name and a `(<customerId>)`
identifier in parentheses, a "Abmelden" button (`…_preferences_signout`) and a
"Speicher verwalten" hyperlink (`…_preferences_manageStorage`). No email is displayed —
only the display name and the opaque id. → **belegt**

**Account switching (UNKNOWN):** no account picker, no second-customer login flow and
no re-resolution of `CustomerInfo` after sign-out were found. `CustomerInfo` is a DI
singleton resolved at startup, and the sign-out helpers remove tokens and restart.
→ *inferred* one signed-in account per process; switching means sign out, restart,
sign in. Whether the *original* keeps multiple customers' credentials and lets the
user choose on the login screen is **UNKNOWN** — `docs/SPEC_LOGIN.md` owns that
question.

---

## 7. String evidence (keys → German)

Keys cited in this doc, verbatim from `de.lproj/Localizable.strings`. The right-hand
column says whether the managed assembly references the accessor or only defines it.
→ **belegt**

| Key (… = `clouddrive_desktop_`) | German | Referenced? |
|---|---|---|
| `…_youraccount_yourstoragetitle` | "Ihr Speicherplatz" | yes (`AccountInfo`, `QuotaUsageView`, `UpdateQuotaInfo`) |
| `…_youraccount_usageformat` | "{1} von {2} belegt" | yes (`QuotaUsageView`) |
| `…_yourprimeaccount_usageformat` | "{1} von {2} für verwendetes Video" | yes (`QuotaUsageView`) |
| `…_youraccount_lastupdatedformat` | "Ihr verfügbarer Speicherplatz wurde aktualisiert {0}" | yes (`AccountInfo.QuotaLastUpdatedText`) |
| `…_youraccount_nonprimecustomers_storagetooltip` | "Für Ihr Speicherlimit werden Fotos und Videos gezählt." | yes (`AccountInfo.StorageHelpText`) |
| `…_youraccount_primecustomers_storagetooltip` | "Mit Prime erhalten Sie unbegrenzten Fotospeicherplatz. …" | yes (`AccountInfo.StorageHelpText`) |
| `…_youraccount_primecustomers_storagetooltip_subtext` | "Klicken Sie hier, um mehr über unterstützte Dateitypen zu erfahren." | yes (`AccountInfo.StorageHelpSubTextHyperLinkText`) |
| `…_youraccount_primphotos_plan` | "Amazon Photos" | **no** |
| `…_youraccount_upgradenow_buttontitle` | "Upgrade jetzt vornehmen" | yes (`QuotaUsageView`) |
| `…_storage_usage_label` | "Speichernutzung" | yes (`StorageUsageView`) |
| `…_storage_usage_value` | "{0}/{1} gebraucht" | yes (`StorageUsageView`) |
| `…_prime_storage_usage_value` | "{0}/{1} für Video verwendet" | yes (`StorageUsageView`) |
| `…_unlimitedphotos` | "Unbegrenzte Fotos" | yes (`UpdateQuotaInfo`) |
| `…_folderselectview_unlimitedphotos_withoutplan` | "Unbegrenzter Speicherplatz von Fotos + {0} zusätzlicher Speicherplatz für Videos" | **no** |
| `…_folderselectview_unlimitedphotos_withplan` | "Unbegrenzter Speicherplatz für Fotos + {0} Speicherplatz für Videos" | **no** || `…_notification_overquota_freetier_title` | "Ihr Speicherplatz ist aufgebraucht" | yes (`SubscriptionStringHandler`) |
| `…_notification_overquota_freetier_message` | "Sie benötigen ein Upgrade für Ihr Abonnement, um mehr Speicherplatz zu bekommen." | yes (`SubscriptionStringHandler`) |
| `…_notification_overquota_freetier_button` | "Upgrade" | yes (`SubscriptionStringHandler`, `StorageUsageView`) |
| `…_notification_overquota_unlimitedphotos_title` | "Kontolimit erreicht" | yes (`SubscriptionStringHandler`) |
| `…_notification_overquota_unlimitedphotos_message` | "Video- und Dateispeicherplatz ist aufgebraucht. Foto-Uploads werden fortgesetzt." | yes (`SubscriptionStringHandler`) |
| `…_notification_overquota_unlimitedphotos_button` | "Speicherplatz hinzufügen" | yes (`SubscriptionStringHandler`, `StorageUsageView`) |
| `…_notification_trialexpired_title` | "Speicherplan wählen" | **no** |
| `…_notification_trialexpired_message` | "Wechseln Sie zu \"Speicher verwalten\", um einen neuen Plan zu starten." | accessor only; reached only through `GetSubscriptionExpiredMessage()`, which itself has no caller |
| `…_notification_trialexpired_button` | "Speicher verwalten" | **no** |
| `…_notification_campaignwindow_title` | "Neue Nachricht von Amazon Photos" | **no** |
| `…_notification_campaignwindow_message` | "Sie haben eine neue Nachricht über Ihr Konto." | **no** |
| `…_notification_campaignwindow_button` | "Details" | yes — but for the **sync mass-delete** notification, not for the campaign window |
| `…_statuspopup_notification_nearquota` | "Ihr Kontingent ist fast aufgebraucht" | yes (`AccountNearQuotaNotification`) |
| `…_statuspopup_notification_overquota` | "Sie haben das Kontingent überschritten" | yes (`AccountOverQuotaNotification`) |
| `…_statuspopup_notification_dismiss` | "Schließen" | yes (status-popup cell) |
| `…_preferences_manageStorage` | "Speicher verwalten" | yes (`AccountQuotaNotification.IntentText`, preferences, `UpsellWindowService`) |
| `…_outage_banner_text` | "Wir haben technische Schwierigkeiten, die den Zugriff auf Fotos und das Hochladen von Fotos vorübergehend beeinträchtigen können" | yes (`LseBannerViewModel`) |
| `…_upsell_modal_title` | "Dir geht der Speicherplatz aus" | yes (`UpsellModalController`) |
| `…_upsell_title` | "Schalte mehr Speicherplatz frei!" | yes (`UpsellInfoView`) |
| `…_upsell_buy_button` | "Mehr Speicherplatz kaufen" | yes (both upsell views) |
| `…_upsell_cancel_button` | "Nicht jetzt" | yes (both upsell views) |
| `…_onboarding_folderconflict_warningheader` | "Konflikt mit Synchronisierungsordner" | yes (`SyncOnboardingPageViewModel`) |
| `…_onboarding_folderconflict_warningmessage` | "Der ausgewählte Ordner wird bereits von einem anderen Benutzerkonto verwendet. Verschieben Sie den vorhandenen Ordner, nennen Sie ihn um oder wählen Sie einen anderen Ordner aus." | yes (`SyncOnboardingPageViewModel`) |
| `…_onboarding_folderconflict_folderdialogheader` | "Ihren Synchronisierungsordner auswählen" | yes (`SyncOnboardingPageViewModel.ChooseSyncFolder`) |
| `…_preferences_account_title` | "Konto" | yes — only as the **tooltip** of the avatar button in `StatusPopupHeaderView` |
| `…_preferences_signed_in_title` | "Angemeldet als:" | yes (`GeneralPreferencesTabView`) |
| `…_preferences_signout` | "Abmelden" | yes (`GeneralPreferencesTabView`) |
| `…_preferences_signout_dialog_title` | "Wollen Sie sich von diesem Gerät abmelden?" | **no** (no call site found) |
| `…_lockedaccount_title` | "Ihr Amazon Photos-Konto wurde gesperrt." | yes (`AccountStatusValidator`) |
| `…_lockedaccount_subtitle` | "Bitte kontaktieren Sie den Kundenservice, um mehr über gesperrte Konten zu erfahren." | yes (`AccountStatusValidator`) |

The "**no**" rows plus the `trialexpired_message` accessor-only row are the interesting
ones: the **entire trial-expired and campaign-window notification family is dead in this
build** (*inferred* — it may be triggered by a server-side payload or a later build),
while `…_notification_campaignwindow_button` lives on as a *different* feature's button
label. Note also that `…_preferences_account_title` ("Konto") is **not** a section
header anywhere — the only reference is an avatar tooltip; the preferences account
block is titled "Angemeldet als:". → **belegt**

---

## 8. Gap analysis vs. our app (committed state)

| Capability | Original | Our app | File reference |
|---|---|---|---|
| Account name | `GetPreferences.name` → `AccountName`, rendered as the `DriveAccountButton` **title** | **none** — deliberately omitted ("we cannot fetch the real one yet") | `src/app.rs` (`fn sidebar`, ~l. 392) |
| Avatar / profile picture | `avatarNodeId` → 60×60 smart-cropped thumbnail → `AvatarPath`, placeholder on failure | **none** — only a drawn marketplace flag badge | `src/icons.rs` (`draw_flag_badge`), `src/app.rs` |
| Storage bar (`Speichernutzung`) | `QuotaUsageView` / `StorageUsageView`, byte units, 3 colour states | **none** — intentionally out ("no placeholder is shown") | `src/app.rs`, `src/backup.rs` |
| Quota endpoints | `/drive/v1/account/preferences`, `/drive/v1/account/usage`, `/drive/v2/account/quota` | **none** — engine only knows `/drive/v1/nodes…`, `/photos/auth/token`, `/v2/upload` | `src/prime_photos.rs` |
| Periodic quota refresh | worker, immediate + every 24 h + on demand | **none** | — |
| Over/near-quota row | in-app status-popup row, 50 pt, dismissible | **none** | — |
| Over-quota native toast | `AccountStatusNotificationHandler`, 24 h throttle, persisted | **none** (we have a run-finished toast only) | `src/notify.rs` |
| Auto-resume after quota returns | `ResetErroredToWaiting(CloudDriveOverQuota)` on `QuotaAvailable` | **none** | `src/upload.rs` |
| Upsell modal | 650×433 panel, cookie-injected manage-storage URL, 30/90-day repeat | **none** | — |
| Outage banner | region-matched `OutageSettings` banner, no dismiss | **none** | — |
| Multi-account state | `<customerId>-*.db` per-store layout | **none** — single `config.toml`, single `dedup.sqlite3` | `src/config.rs`, `src/dedup.rs` |
| "Other account" folder guard | `com.amazon.drive.sync.root` xattr vs. `CustomerId` | **none** | — |
| Sign-out / account block | preferences "Angemeldet als:" + name + `(id)` + "Abmelden" | flag badge + "Marktplatz" + "amazon.<domain>" + "Abmelden" | `src/app.rs` (`fn sidebar`) |
| Marketplace domain | `DomainProvider.BuildLink`, endpoints-derived | static table `amazon_domain(country)` | `src/login.rs` |
| i18n | `Localizable.strings` | central de/en table | `src/i18n.rs` |

**The name/avatar gap, concretely.** Our sidebar block (read in `src/app.rs`) is a
30 pt flag badge with a green/red presence dot, a "Marktplatz" label, the
`amazon.<domain>` string, and an "Abmelden" button. The code comment states the reason:
no real name is fetchable yet, and a stand-in would be wrong. The original's answer is
`GET /drive/v1/account/preferences` → `name` (+ `avatarNodeId`) — an **ordinary
authenticated metadata call on the same `metadataUrl` we already resolve**. So the gap
is not that the route is unknown; it is that our `PrimePhotos` client has no client for
it, and our cookie session is the only credential we hold. Whether that endpoint
answers to the same cookie set as `/drive/v1/nodes` is **confirmed by live test (see
change log / §9): yes**. → **belegt**

**The storage-bar gap.** Our `AGENTS.md` records the decision as deliberate for MVP
("only applies to Prime members and needs a real quota fetch"). The reverse-engineered
evidence refines that in two ways: (a) the numbers are **plain bytes** from one GET, so
the fetch is not Prime-specific — the Prime dependency lives only in *which format
string* is used and in the help text; and (b) for `quota >= 1 TiB` the near-quota rule
switches from *percent used* to *bytes remaining* (default 10 GiB), so a port copying
"90 %" would be wrong for exactly the accounts with the most data. → **belegt**

---

## 9. UNKNOWN (explicitly not determined)

- **Does `/drive/v1/account/preferences` accept our cookie session?** **Resolved (live test
  2026-10-14, belegt): yes** — it answered `name` and `avatarNodeId` to our cookie set, the
  same auth we already use for `/drive/v1/nodes`. (Was listed here as the single blocking
  unknown for the name/avatar feature; now established.)
- **Where does the *avatar thumbnail* come from, and does our port reach it?** **Resolved
  (2026-10-14, belegt — see §3.4):** the route is `GET <thumbnailServiceUrl>
  v1/thumbnail/<avatarNodeId>?viewBox=60%2C60&fit=clip[&cropOffset=…&cropSize=…]&encoding=jpeg`
  on host `thumbnails-photos.amazon.de`, direct JPEG body, crop pair only on the smart-crop
  path. The **auth** on that host is now proven **for our cookies** by live test: with
  **exactly** our `build_common_headers` cookie set (`session-id`, `x-acb<cc>`, `at-acb<cc>`,
  `ubid-acb<cc>`, `x-amzn-SessionId`) and **no** `x-amz-access-token`, the request answered
  **HTTP 200**, `Content-Type image/jpeg`, 2135 bytes, JPEG 60×60. The original's device
  token is **not** our path — adding `x-amz-access-token` produced **HTTP 401**. → URL
  **belegt**, cookie-auth on the thumbnail host **belegt**.
- **Is `GetPreferences.name` the display name or the account holder's real name?** Only
  "the string the client shows" is documented here; no further semantics were derived and
  no sample value was inspected (clean-room: no personal data). → **UNKNOWN**
- **Which accounts actually reach `HasPrimeBenefit`?** `Benefits` is never populated, so
  in this build it is always `false`; the Prime branches are driven by
  `IsUnlimitedPhotosFeatureEnabled` / `Plans` instead. → *inferred* / **UNKNOWN**
- **The trial-expired condition.** The strings and `GetSubscriptionExpiredMessage()`
  exist, but nothing calls it and `AccountInfo.Subscription` is never filled — the
  trigger for "Speicherplan wählen" cannot be reconstructed from this build. → **UNKNOWN**
- **The campaign window's URL and display policy.** `ShowAsync(url)` has no in-assembly
  caller. → **UNKNOWN**
- **Account switching.** No switch-account UI or second-customer login flow was found;
  whether the login screen offers a customer choice is owned by `docs/SPEC_LOGIN.md`.
  → **UNKNOWN**
- **The outage regions we would be in.** `OutageSettings` is a downloaded CloudFront
  file; its content for our marketplaces at any given time is runtime data. → **UNKNOWN**
- **`UpsellWindowEnabled` / `UpsellEnabledInEuropeanUnion` / `UpsellEnabledInJapan`** are
  app-settings defaults (`true` / unresolved / unresolved) that the remote
  `app-settings.json` can override, so the effective upsell behaviour for a given install
  is **UNKNOWN**, and so is whether our marketplaces take the EU branch of
  `ShouldDisplayUpsellWindow` / `EvaluateNavigation` (that depends on
  `LocalizationHelper.IsEuropeanUnion(region)`, which was not read). → *inferred* overridable
- **Account status gating in our port.** The original blocks startup on
  `AccountInfoStatus`; we hold no equivalent, so a `LOCKED`/`MIGRATED` account surfaces
  only as upload failures. Whether that matters for us is **UNKNOWN**.
- **Retention of the status-popup rows across restart.** The notification centre is an
  in-memory dictionary and nothing re-publishes quota notifications on startup, so
  *inferred* they are gone after a restart until the next refresh proves the condition
  again. → *inferred*

---

## 10. Cross-references

- `docs/PROTOCOL.md` §2.13 (`GET /drive/v1/account/preferences`), §2.14
  (`/drive/v1/account/usage`), §2.15 (`/drive/v2/account/quota?customerId=UNNEEDED`) —
  the captured shapes this doc's endpoints match.
- `docs/SPEC_LOGIN.md` — login/session/cookies, the auth behind every call in §3.2, and
  the account-switch question (§6.3).
- `docs/SPEC_NOTIFICATIONS.md` — the full notification inventory; this doc owns only the
  quota/over-quota/outage subset and the banner stack in §5.
- `docs/UI_FLOWS.md` §3.1 (footer + banner stack, measured sizes), §3.3 (the app menu the
  avatar button pops), §10 (upsell geometry and strings).
- `docs/UI_STATES.md` §3 (footer storage-block states), §3.8 (`uploads` row states that
  `ResetErroredToWaiting` acts on).
- `docs/SPEC_UPLOAD_PROGRESS.md` — the transfer side of `TransferError.CloudDriveOverQuota`
  and the error-window rows the `QuotaAvailable` event re-queues.
- Our code: `src/app.rs` (`fn sidebar`, the account block), `src/icons.rs`
  (`draw_flag_badge`), `src/login.rs` (`amazon_domain`, `login_url`, `logout`),
  `src/config.rs` (`Config`, `config.toml`), `src/prime_photos.rs` (HTTP client and its
  endpoint list), `src/notify.rs`, `src/settings.rs`, `src/i18n.rs`, `src/countries.rs`.
- `AGENTS.md` ("Speichernutzung"/storage bar is intentionally omitted) and
  `AGENTS.todo.md` scope-inventory rows 7, 12 and 19 ("Speicher-/Quota-Balken",
  "Upsell-/Trial-/Overquota-Banner", "Mehrere Konten").

---

## 11. Change log

- **2026-10-14:** §3.4 — traced the avatar thumbnail end-to-end in `Amazon Photos.dll`: the
  full URL form (`thumbnailServiceUrl + "v1/thumbnail/" + nodeId` plus the `viewBox`, `fit`,
  `cropOffset`, `cropSize`, `encoding` query), the host `thumbnails-photos.amazon.de`, the
  device-token headers (`x-amz-access-token` &c.), the direct-JPEG body (no `tempLink`), and
  the exact local filename `<nodeId>_60_60.jpeg`. §9 — the "avatar thumbnail" entry is
  resolved on the route; whether our **cookie** session is accepted on the thumbnail host was
  still open here and is confirmed by live test in the entry below (as is the
  `preferences`-cookie entry). Evidence came from the
  read-only assemblies: `Services.CloudDrive.EndpointsProvider` (`GetThumbnailUrl`),
  `Services.CloudDrive.CloudDriveService` (`DownloadThumbnail`),
  `Services.Photos.ThumbnailWorker` (`GetSmartCroppedThumbnail`/`GetThumbnail`/
  `CreateDefaultPath`/`IsThumbnailValid`), `Services.CloudDrive.CloudDriveHttpMessageHandler`,
  `Workers.CloudDriveAccountRefreshWorker` (`GetProfileImage`, `AvatarViewBoxSize`),
  `Helpers.QueryExtensions` (`ToUrlEncodedQuery`), `Constants`
  (`ThumbnailPath`/`DefaultThumbnailFormat`).
- **2026-10-14:** live proof of the **cookie** auth on the **thumbnail host**: `GET
  https://thumbnails-photos.amazon.de/v1/thumbnail/<avatarNodeId>?viewBox=60%2C60&fit=clip
  &encoding=jpeg` with exactly our `build_common_headers` cookie set (no
  `x-amz-access-token`) → HTTP 200, `image/jpeg`, 2135 bytes, JPEG 60×60; the same call
  **with** the stored `x-amz-access-token` → HTTP 401. Same cookie set also answers
  `GET https://www.amazon.<cc>/drive/v1/account/preferences` (200, `name` +
  `avatarNodeId`). §3.4 gains the port-facing line; §9's avatar-thumbnail entry drops its
  auth caveat.

