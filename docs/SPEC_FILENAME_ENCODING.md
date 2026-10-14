# Spec: Filename encoding in the original client

**Status:** clean-room backfill spec (read-only reconstruction of the official macOS
Amazon Photos client, 11.3.0/36). Own words and symbol citations only; no verbatim
decompiled code, no personal data.

**Question answered.** *What does the original app do with the encoding of umlauts in
file names — does it normalise them before upload?* This matters because macOS delivers
**NFD** (`Ö` = `U+004F U+0308`) and Windows **NFC** (`U+00D6`); if the two are not made
to agree, the same file can end up twice in the account.

**Sources (read-only).** `/Applications/Amazon Photos.app` — primary
`Contents/MonoBundle/Amazon Photos.dll`, decompiled with `ilspycmd` (ILSpy 11.1) to a
single C# file. Symbols are cited by name; the full search path is in §6. Our own repo
findings about the server are used only as background and are marked as such — they are
**not** part of this recon.

---

## 1. Normalisiert das Original den Upload-Namen?

**Nein.** The client never calls `String.Normalize`, never uses `NormalizationForm`,
`StringNormalizationExtensions`, `IsNormalized`, `FormC`/`FormKC`/`FormKD`, and has no
helper named `NormalizeName`, `SanitizeFileName` or `EscapeFileName`. An exhaustive grep
of the whole decompiled assembly for all of these returns **zero hits** (§6). The only
`Normalize*` occurrences in the entire DLL are unrelated:

* `PathEx.NormalizePathWithTrailingSlash` / `NormalizePathWithoutTrailingSlash` — add or
  strip a trailing directory separator. **No** Unicode work.
* `GetNormalizedPathForNode` (selective sync) and CPU-metric names — unrelated.

The string that goes on the wire is the **raw file-system name**. It is `fso.Name`, which
resolves to `FileSystemObject.Name => PathEx.GetFileName(Path)`; `PathEx.GetFileName` is a
pure substring operation that splits at path separators. There is no character-form
conversion anywhere on the upload path, so on macOS the app sends the on-disk **NFD** form
unchanged, and there is no C/D conversion step.

## 2. Wo im Ablauf

All three upload request builders take the raw name; none post-processes it:

| Path | Request object | Where `Name` is set |
|---|---|---|
| Query-param upload (the one we use) | `UploadRequest.Name` (`[QueryParameter("name")]`) | `SimpleUploader`, async state machine `<UploadFile>d__3` → `Name = fso.Name` |
| Legacy JSON-body upload | `CloudDriveFileMetadata.Name` (`[JsonProperty("name")]`) | `LegacyUploader` `<UploadFile>` → `Name = fso.Name` |
| Multipart upload | `MultipartUpload.Model.InitiateMultipartUploadRequest` | ctor → `Name = PathEx.GetFileName(uploadRecord.Path)` |

Upstream of the request, the local record's `Path` comes from
`LocalFileSystemObjectExtensions.CreateFileSystemObject` →
`PathEx.NormalizePathWithoutTrailingSlash(fsi.FullName)` — again only a trailing-separator
normalisation. So there is **no** name transform at scan time, at record creation, or at
request build time: raw in, raw out.

## 3. Vergleich/Dedupe-Schlüssel

**Not normalised, and the name is not the dedupe key.**

* Cloud-side pre-upload check: `CloudDriveStorageService.PreUploadCheck(IFileSystemObject
  fso, …)` first looks up the destination node with
  `CloudDriveDatabase.GetNodeByNameAndParentIfExists(fso.Name, parentNodeId, "FILE")`.
  That query is an **exact** `n.name = ?` match (ordinal / SQLite binary). The actual
  duplicate decision is then made on the **MD5** (`node.ContentProperties.MD5 == md5`,
  with a fallback lookup `GetNodeByMd5(md5)`), never on the name.
* Local dedupe key: `ManualUploadDatabase.InternalSaveUploadRecord` matches an existing
  record on `Path == record.Path && DestinationNodeId == … && ScheduleId == …` — exact
  string equality, no normalisation. Re-upload matching is likewise exact
  (`Path.GetFileName(rec.Path) == nodeName`).

Consequence: two names that differ only in Unicode form (NFD vs NFC) are **different
strings** for every name comparison in the client. Only the content MD5 unifies them;
the name path does not.

## 4. Umschreibung von Sonderzeichen

* **No transliteration.** No `RemoveDiacritics`, no `Encoding.ASCII` on names, no
  `Latin`/`ToAscii` mapping — all zero hits (§6). `Encoding.ASCII` appears only in an AES
  key derivation, never for a file name.
* **The only rewrite helpers** are `PathEx.GetCloudName` and `PathEx.GetLocalName`. They
  replace characters with `_`. On macOS the invalid-character set is exactly `{'/'}` for
  cloud names (`MacInvalidCloudNameChars`) and control characters + `/` for local names
  (`MacInvalidFileNameChars`); `GetLocalName` additionally masks "restricted" names (the
  list is empty on macOS). These helpers run in the **cloud→local (download/sync)**
  direction — e.g. `NodeHelper.GetLocalName`/`GetCloudName`, `FilesSyncRecord`,
  `CloudDriveDatabase.GetPathNodes` — and are **not** applied to the upload name
  (`fso.Name`). No character outside that tiny set is touched.
* **URL encoding only:** query parameters are `WebUtility.UrlEncode`d (via
  `QueryExtensions` / `QueryParameterAttribute`). That percent-encodes bytes but does not
  change the character form; JSON bodies declare `charset=utf-8`. So NFD stays NFD on the
  wire in both the query-param and the JSON-body path.

## 5. Empfehlung für uns

**Belegt:** the original sends the raw file-system name on all three upload paths, never
normalises it, and never normalises for comparison or dedupe; its only special-character
rewrite (`GetCloudName`/`GetLocalName`) serves the download direction and touches only
`/` and control characters. **Vermutung (not observable from the client):** what the
server does with the name — our own repo background says the server stores it as-is, so
an NFD name and an NFC name become two distinct nodes.

**Recommendation: normalise the name to NFC before sending (and before writing our local
dedupe key), on every platform.** NFC is the canonical Unicode interchange form and the
Windows default; if our macOS *and* Windows builds both emit NFC, the same file lands
under one name regardless of the source OS — exactly the cross-platform consistency that
is wanted. Sending the raw name (as the original does) reproduces the macOS-NFD vs
Windows-NFC split. Content dedupe (MD5) is unaffected either way, but the name/conflict
path is not. Caveat, to be stated as a known limit: files already in the account under an
NFD name (uploaded by the original or an earlier run of ours) would no longer be matched
*by name* once we send NFC — MD5 dedupe still catches identical content, but a name-level
"already there" is not guaranteed.

## 6. UNKNOWN

* **Server-side normalisation** — not decidable from the client; the client only ever
  sends what it has and compares locally. Marked as assumption in §5.
* **The exact byte form the OS hands the app** — assumed NFD on macOS because the app does
  no conversion; not measured in this recon.
* **Search path taken:** full `ilspycmd` decompile of `Amazon Photos.dll` into one C# file;
  greps for `Normalize`, `NormalizationForm`, `FormC`, `FormD`, `FormKC`, `FormKD`,
  `IsNormalized`, `StringNormalizationExtensions`, `NormalizeToFormC`, `NormalizeName`,
  `SanitizeFileName`, `EscapeFileName`, `RemoveDiacritic`, `Diacritic`, `Transliterat`,
  `Encoding.ASCII`, `ToAscii`, `Latin`, `EscapeUriString`, `HttpUtility`,
  `WebUtility.UrlEncode`, `Uri.EscapeDataString`; then traced all three upload-request
  builders (§2) and both dedupe lookups (§3).

## 7. Belege

Symbols (assembly `Contents/MonoBundle/Amazon Photos.dll`, ILSpy 11.1):

* `Amazon.CloudDrive.Services.CloudDrive.UploadRequest` — field `Name` with
  `[QueryParameter("name")]`.
* `Amazon.CloudDrive.Services.SimpleUploader` — `<UploadFile>d__3` state machine:
  `Name = fso.Name`.
* `Amazon.CloudDrive.Services.LegacyUploader` — `CloudDriveFileMetadata` initialiser:
  `Name = fso.Name`.
* `Amazon.CloudDrive.Services.MultipartUpload.Model.InitiateMultipartUploadRequest` —
  ctor: `Name = PathEx.GetFileName(uploadRecord.Path)`.
* `Amazon.CloudDrive.Services.CloudDrive.CloudDriveFileMetadata` /
  `CloudDriveMetadata` — `[JsonProperty("name")]`.
* `Amazon.CloudDrive.Services.FileSystem.Model.FileSystemObject` —
  `Name => PathEx.GetFileName(Path)`; `LocalFileSystemObject`;
  `LocalFileSystemObjectExtensions.CreateFileSystemObject`;
  `ManualUploadFileSystemObject` (no `Name` override).
* `Amazon.CloudDrive.Helpers.PathEx` — `GetFileName`, `GetCloudName`, `GetLocalName`,
  `NormalizePathWithTrailingSlash` / `…WithoutTrailingSlash`, `MacInvalidFileNameChars`,
  `MacInvalidCloudNameChars`.
* `Amazon.CloudDrive.Services.CloudDrive.CloudDriveStorageService` — `PreUploadCheck`.
* `Amazon.CloudDrive.Database.CloudDrive.CloudDriveDatabase` —
  `GetNodeByNameAndParentIfExists` (`n.name = ?`), `GetNodeByMd5`, `NodeExistsWithMd5`.
* `Amazon.CloudDrive.Database.ManualUpload.ManualUploadDatabase` —
  `InternalSaveUploadRecord` (Path + DestinationNodeId + ScheduleId), `GetUploadsByPartialPath`,
  `UploadRecord.PrecomputedMd5`.
* `Amazon.CloudDrive.Services.CloudDrive.NodeHelper` — `GetLocalName`/`GetCloudName`
  extensions (download direction).
* `Amazon.CloudDrive.Helpers.QueryExtensions` / `QueryParameterAttribute` —
  `WebUtility.UrlEncode`.
* **Absent** (0 grep hits): `NormalizationForm`, `FormC`, `FormKC`, `FormKD`,
  `IsNormalized`, `StringNormalizationExtensions`, `NormalizeToFormC`, `NormalizeName`,
  `SanitizeFileName`, `EscapeFileName`, `RemoveDiacritic`, `Transliterat`, `ToAscii`.

**Bereits dokumentiert (Frage 5).** `docs/PROTOCOL.md` shows the request shape
`/v2/upload?name=<url-encoded filename>…` (§ around lines 463 and 586) but says nothing
about the character form (NFC/NFD). `docs/SPEC_DUPLICATES.md` and `docs/SPEC_DEDUP.md`
describe the *dedupe* mechanics but not name encoding. No existing doc in this repo
addresses the NFC/NFD question; this file is the first.
