# Spec: Duplicate avoidance (dedup) & content-filter taxonomy

**Status:** clean-room backfill spec (read-only reconstruction of the official macOS
Amazon Photos client). Own words and tables only; no verbatim decompiled code.
Quoting German UI labels from `de.lproj/Localizable.strings` is intentional.
No personal data. Read alongside `docs/UI_STATES.md` (enums/tables), `docs/UI_FLOWS.md`,
`docs/PROTOCOL.md`, and `docs/openapi.yaml`.

**Scope.** This document owns two things:

1. How the original client decides that a photo/video already exists in the account
   (duplicate avoidance), and the conflict/error paths around that.
2. The `ContentFilter` / `ContentFilterExtensions` taxonomy: which extensions fall
   into the Photos / Videos / Other groups.

`docs/SPEC_FOLDERS.md` (Scan/Filter-Semantik) should **reference this document**
for the extension taxonomy and must not redefine it. Our own Rust implementation
does not use these lists yet; they are the reference for the P1 "Dedup" work and
the backlog "exakter Extension-Filter" item.

**Sources (read-only).** `/Applications/Amazon Photos.app` — primary
`Contents/MonoBundle/Amazon Photos.dll` via `ilspycmd`; embedded resource
`Amazon.CloudDrive.Helpers.SupportedContentTypes.supported-content-types.json`;
`Contents/Resources/de.lproj/Localizable.strings`; local SQLite state under
`~/Library/Application Support/Amazon Cloud Drive/` (`backup`, `uploads`,
`cloud.db`). Verification of a point is marked inline; unverifiable points are
listed in §9.

---

## 1. Duplicate identity

**The duplicate key is the MD5 of the complete file bytes**, not name/size/date:

| Item | Observed value |
|---|---|
| Algorithm | MD5 over the entire file body |
| Text form | Lower-case hexadecimal, 32 characters (`"d4e9..."`) — produced by `byte.ToString("x2")` |
| Request header | `x-amzn-file-md5: <hex md5>` |
| Wire/response field | `contentProperties.md5` (field on the returned node JSON) |
| Local column (upload queue) | `uploads.precomputed_md5` |
| Local column (node cache) | `nodes.content_md5` (with index `IX_md5`) |

Consequences of this definition:

- Two files with identical bytes but different names/paths are treated as the same
  content. This is what makes cross-folder and re-scan dedup work.
- A rename on disk, a changed timestamp, or a re-encoded photo is **not** a
  duplicate; it will be uploaded again. The original never compares names or
  sizes for dedup, only MD5 (plus, for a single rename/clobber decision, the
  separate `ConflictResolution` flow in §4).
- The MD5 is computed once per upload attempt from a seekable `HashStream` and
  then cached in `uploads.precomputed_md5`, so a retry does not re-read the
  file. If computation fails once, the client retries once (observed).
- Content/date fields (`contentDate`, EXIF capture date) are carried on the upload
  but are **not** part of the duplicate identity.

**Apple-Photos special case.** For `.heic` files that live inside an
Apple Photos library (`PhotoLibraryFileSystemObject`), the client additionally
checks whether the first two bytes of the stream are a JPEG BOM (`FF D8`).
If so, the file is treated as a *potential* duplicate of something already
saved by the iOS app and is skipped via `FileIgnoredForPotientialDuplicationException`
(§5). This is a byte-level heuristic on top of the MD5 rule, not a second
identity.

Sidecar `.xmp` files are part of the taxonomy (image/video sidecar) but are
**rejected by our own app** (`is_supported_image` excludes them) and are
therefore out of P1 scope.

## 2. Where the check happens

The dedup decision is made in **three layers**, in this order. Only the first
two are required to reproduce "skip already uploaded files"; the third is a
server-side safety net.

### 2.1 Enumerate only what the filter allows

Before any MD5 is touched, the walker applies the schedule's `content_filter`
 bitmask via `ContentFilterExtensions.Matches(path)` (§3). Files whose extension
does not match the filter are never considered.

### 2.2 Pre-upload check in the client (local index)

`PreUploadCheck` runs against the local SQLite cache (`cloud.db`) before a
network request is issued. The logic, in own words:

1. Resolve candidate node:
   - by `nodeId` if known (`GetNodeByNodeId`),
   - otherwise by name + parent + `"FILE"` (`GetNodeByNameAndParentIfExists`).
2. Set `duplicate = (node.status == "AVAILABLE" && node.contentProperties.md5 == fileMd5)`.
3. **If the record is eligible for MD5-based dedup** (our interpretive label;
   no `CrossAccount` identifier exists in the assembly), also run
   `cloud.db.GetNodeByMd5(fileMd5)` (any `AVAILABLE` node with that MD5) and set
   `duplicate = true` on a hit, storing the found node.
4. Eligibility for step 3 is controlled by `UploadType`:
   - `UploadType.Deduped` (normal scheduled backup **with**
     `dedupe_photos_videos = PhotosAndVideos`) → eligible. The schedule's UI calls
     this case "Duplikate vermeiden: Keine Fotos und Videos sichern, die sich
     bereits in meinem Konto befinden".
   - `UploadType.PhotosImport` (Apple Photos library import) → eligible
     regardless of the schedule toggle.
   - `UploadType.Normal` (dedup toggle off, or manual upload) → **not**
     eligible; only the exact name+parent node from step 1 is considered.
5. If `duplicate` is true and `ShouldSuppressMd5Check` is false (i.e. not a
   timed-out/pending "reupdate" retry), the client raises
   `FileIgnoredPreUploadException` and the transfer is marked **Ignored**
   ("Zuvor hochgeladen") without any HTTP request.
   The schedule item carries `ScheduleId`; `ManualUpload` is used for one-shot
   uploads.
6. Secondary, unrelated local gate: if the schedule touches the Apple Photos
   library and `FilterOutPotentialDuplicatesFromApplePhotosLibrary` is on and
   the file is a `.heic` starting with a JPEG BOM, raise
   `FileIgnoredForPotientialDuplicationException` (§5).

Other late local checks (quota, `CanUploadFile`) do not affect the duplicate
decision.

### 2.3 Server-side safety net

Even when the local cache is stale, the endpoint rejects a duplicate body:

- The server is queried by **our** committed app through
  `POST content-<zone>.drive.amazonaws.com/v2/upload` (cookie auth; see
  `docs/openapi.yaml` `/v2/upload` and `docs/SPEC_UPLOAD.md` §1 — the cookie host
  is *our* web path, **not** the desktop client's; the desktop device flow is
  `upload-photos.amazon.de/v2/upload` with `x-amz-access-token`, `PROTOCOL.md`
  §2.17, and is unused by us). The original client's `/v2/upload` strings are
  relative router paths on `drive.amazonaws.com/drive/v2`, with the concrete host
  taken from `uploadServiceUrl`. For a dedup run the original effectively uses
  `conflictResolution=NONE` (our committed app instead ships `RENAME` — open
  point O-3 in SPEC_UPLOAD); **unverified** for our cookie path.
- The server compares the uploaded MD5 with existing nodes. On a hit it responds
  with HTTP **409 Conflict** and one of (the first bullet is **unverified** for
  our cookie flow; only the two `ErrorType` constants are DLL-backed):
  - a node JSON whose `contentProperties.md5 == local md5` *(unverified shape)*, or
  - `ErrorType == "MD5_DUPLICATE"`, or
  - `ErrorType == "DuplicatesConflictError"` (string constant in the client's
    `NodeHelper`).
  All three are normalized to `TransferError.CloudDriveNodeConflict`. The client
  treats the response as a duplicate: it saves the returned node into the local
  cache, sets the HTTP status to 200 internally, and raises `FileIgnoredException`
  so the item becomes **Ignored** ("Zuvor hochgeladen") rather than failed.

### 2.4 Where the external-ID check is used

When a resumable upload has a `LocalId` (a client-generated GUID) and the client
is looking it up, it calls `GetNodeIdForLocalId((localId + md5).ToHashString())`
on the server. That hash is **not** the duplicate identity; it is only a lookup
key for resumable/pending uploads. Do not reuse it as a dedup key.

## 3. Content-filter taxonomy (Photos / Videos / Other)

### 3.1 Bitmask

`ContentFilter` (`uint`-flags enum used in `schedules.content_filter`):

| Value | Name | Meaning |
|---|---|---|
| 0 | `None` | no files |
| 2 | `Photos` | image media |
| 4 | `Videos` | video media |
| 8 | `Other` | everything else in the supported-types list |
| 6 | `PhotosAndVideo` | `Photos \| Videos` |
| 14 | `Everything` | `PhotosAndVideo \| Other` |

`DeduplicateOptions` is a separate, smaller enum used in
`schedules.dedupe_photos_videos` (also 0/2/4/6). Important: the UI exposes dedup
only as a single boolean toggle, mapped internally to
`DeduplicateOptions.PhotosAndVideos` or `None`. The per-media values
(`Photos=2`, `Videos=4`) exist in the enum but the schedule stores them as a
whole either way (observed).

### 3.2 Classification algorithm

`ContentFilterExtensions.Matches(path)` classifies a path by extension:

1. If `SupportedMimeMapping.IsImageType(path)` or
   `IsImageSidecar(path)` → requires `ContentFilter.Photos`.
2. Else if `SupportedMimeMapping.IsVideoType(path)` or
   `IsVideoSidecar(path)` → requires `ContentFilter.Videos`.
3. Otherwise → returns `false`. The `Other` bit is never honored by this
   method even when set; non-media files therefore never upload in the backup flow,
   consistent with the "block non-media uploads" notice the client has shown since
   2023-11-30 (`clouddrive_desktop_backupconfiguration_blocknonmediaupload_text`).

   `ContentFilterExtensions.ForMediaOnly` additionally rewrites
   `Everything → PhotosAndVideo` when building a schedule record, so the stored
   value is normally `PhotosAndVideo` rather than `Everything`.

`SupportedMimeMapping` reads `supported-content-types.json` embedded in the DLL,
lower-casing the whole file first, so all category comparisons are
case-insensitive.

### 3.3 Group definitions (extension → group)

A file's **primary** group is decided by the JSON category of its extension:

- **Photos group** — category `image` (83 extensions) plus `image_sidecar`
  (`.xmp`). Extension matching is by lower-cased suffix without the dot.
- **Videos group** — category `video` (47 extensions) plus `video_sidecar`
  (`.xmp`). `.xmp` therefore belongs to both groups as a sidecar; in
  `Matches()` the **photo-sidecar test comes first**, so an `.xmp` is
  treated as a Photo whenever `Photos` is in the filter, and only as a Video
  when `Photos` is absent.
- **Other group** — the remaining supported extensions: `application` (344),
  `audio` (36), `message` (4) and `text` (105). Plus anything whose extension
  is not in the embedded list, which `SupportedMimeMapping.IsOtherType` also
  reports as "other".

A subtlety: the two entries `dcr` and `srf` also appear in the embedded
`pending_categorization` block classified as `application` and `text`
respectively, but the runtime loads only `supported_extensions`, where both are
classified as `image` (Hasselblad/Sony RAW). Runtime behaviour follows the
`supported_extensions` entry; the pending block is not loaded by the client
(observed indirection — exact use unverified).

The user-facing labels are German and checkout-box driven:
`clouddrive_desktop_statuswindow_scheduleconfiguration_contenteverything` → "Alles",
`..._statuswindow_scheduleconfiguration_contentimagesandvideos` → "Fotos und Videos",
`..._statuswindow_scheduleconfiguration_contentimages` → "Fotos", plus the two checkboxes
`..._upload_settings_backup_photos_checkbox` ("Fotos sichern") and
`..._backup_videos_checkbox` ("Videos sichern").

#### 3.3.1 Photos group (84 entries: 83 image + xmp)

ID set:

3fr, ari, art, arw, avif, bay, bmp, cap, cmx, cod, cr2, cr3, crw, cs1, dc2, dcr, dcs, dib, dng, drf, eip, erf, fff, gif, heic, heif, ico, ief, iiq, jfif, jpe, jpeg, jpg, jxl, k25, kdc, mac, mdc, mef, mos, mrw, nef, nrw, orf, pbm, pct, pef, pgm, pic, pict, png, pnm, pnt, pntg, pnz, ppm, psd, ptx, pxn, qti, qtif, r3d, raf, ras, raw, rf, rgb, rw2, rwl, rwz, sr2, srf, srw, svg, tif, tiff, wbmp, wdp, webp, x3f, xbm, xmp, xpm, xwd

#### 3.3.2 Videos group (48 entries: 47 video + xmp)

ID set:

3g2, 3gp, 3gp2, 3gpp, asf, asr, asx, avi, dif, divx, dv, flv, ivf, lsf, lsx, m1v, m2t, m2ts, m2v, m4v, mkv, mod, mov, movie, mp2, mp2v, mp4, mp4v, mpa, mpe, mpeg, mpg, mpv2, mqv, mts, nsc, ogv, qt, ts, tts, vbk, webm, wm, wmp, wmv, wmx, wvx, xmp

#### 3.3.3 Other group (489 entries)

`application` (344):

7z, aaf, aca, accda, accdb, accdc, accde, accdr, accdt, accdw, accft, acx, ade, adobebridge, adp, afm, ai, air, amc, application, asa, asax, ascx, asd, ashx, asi, asmx, aspx, atom, axs, bcpio, bin, cab, calx, cat, cdf, cer, chm, class, clp, config, coverage, cpio, crd, crl, crt, csh, cur, dat, datasource, deploy, der, dgml, dir, dll, doc, docm, docx, dot, dotm, dotx, dsp, dvi, dwf, dwp, dxr, emz, eot, eps, etl, evy, exe, fdf, fif, filters, fla, flr, fsscript, fsx, generictest, gtar, gz, hdf, hhc, hhk, hhp, hlp, hqx, hta, hxa, hxc, hxd, hxe, hxf, hxh, hxi, hxk, hxq, hxr, hxs, hxv, hxw, ics, iii, inf, ins, ipa, ipg, ipsw, isp, ite, itlp, itms, itpc, jar, java, jck, jcz, jnlp, jpb, js, json, latex, library-ms, lit, loadtest, lpk, lzh, m13, m14, man, manifest, master, mda, mdb, mde, mdp, me, mfp, mix, mmf, mny, mpf, mpp, ms, msi, mso, mtx, mvb, mvc, mxp, nc, ocx, oda, odb, odc, odf, odg, odi, odm, odp, ods, odt, one, onea, onepkg, onetmp, onetoc, onetoc2, orderedtest, osdx, otg, oth, otp, ots, ott, oxt, p10, p12, p7b, p7c, p7m, p7r, p7s, pcast, pcx, pcz, pdf, pfb, pfm, pfx, pko, pma, pmc, pml, pmr, pmw, pot, potm, potx, ppa, ppam, pps, ppsm, ppsx, ppt, pptm, pptx, prf, prm, prx, ps, psc1, psess, psm, psp, pub, pwz, qtl, qxd, rar, rat, rdlc, resx, rm, rmp, roff, rtf, ruleset, safariextz, scd, sdp, sea, searchconnector-ms, setpay, setreg, settings, sgimb, sh, shar, sit, sitemap, skin, sldm, sldx, slk, slupkg-ms, smi, snippet, snp, spc, spl, src, ssm, sst, stl, sv4cpio, sv4crc, svc, swf, t, tar, tcl, testrunconfig, testsettings, tex, texi, texinfo, tgz, thmx, thn, toc, tr, trm, trx, ttf, u32, ustar, vcproj, vcxproj, vdx, vscontent, vsd, vsi, vsix, vsmdi, vss, vst, vsto, vsw, vsx, vtx, wbk, wcm, wdb, webarchive, webtest, wiq, wiz, wks, wlmp, wlpginstall, wlpginstall3, wmd, wmf, wmlc, wmlsc, wmz, wpl, wps, wri, wrl, wrz, x, xaf, xaml, xap, xbap, xht, xhtml, xla, xlam, xlc, xld, xlk, xll, xlm, xls, xlsb, xlsm, xlsx, xlt, xltm, xltx, xlw, xmta, xof, xps, xsc, xsn, xss, xtp, z, zip

`audio` (36):

aa, aac, aax, ac3, adt, adts, aif, aifc, aiff, au, caf, cdda, gsm, m3u, m3u8, m4a, m4b, m4p, m4r, mid, midi, mp3, pls, ra, ram, rmi, rpm, sd2, smd, smx, smz, snd, wav, wave, wax, wma

`message` (4):

eml, mht, mhtml, nws

`text` (105):

323, addin, asm, bas, c, cc, cd, cnf, contact, cpp, cs, csdproj, csproj, css, csv, cxx, dbproj, def, disco, dll.config, dlm, dsw, dtd, dtsconfig, etx, exe.config, group, h, hdml, hpp, htc, htm, html, htt, hxt, hxx, i, idl, inc, inl, ipproj, iqy, jsx, jsxbin, lst, mak, map, mk, mno, odh, odl, pkgdef, pkgundef, qht, qhtm, rc, rc2, rct, rgs, rqy, rtx, s, sct, sgml, shtml, sln, sol, sor, ssisdeploymentmanifest, tlh, tli, tsv, txt, uls, user, vb, vbdproj, vbproj, vbs, vcf, vcs, vddproj, vdp, vdproj, vml, vsct, vsixlangpack, vsixmanifest, vspscc, vsscc, vssettings, vssscc, vstemplate, wml, wmls, wsc, wsdl, xdr, xml, xoml, xrm-ms, xsd, xsf, xsl, xslt

`pending_categorization` overrides noted in §3.3.

### 3.4 Filter interactions observed in backup

- Choosing `ContentFilter.Everything` forces `AvoidDuplicates = false`; dedup
  is only meaningful for photos/videos. Attempting to back up non-media triggers
  the block notice (§3.2).
- When the walker has a `PhotoLibraryPath` (Apple Photos), it masks out the
  `Other` bit (`filter &= ~ContentFilter.Other`) before classification, so
  only photos/videos from the library are ever offered.
- Folders themselves are traversed; only files are classified. A folder inside
  the Photo library is skipped if it is part of that library.

## 4. Conflict and error paths

### 4.1 Name conflicts (separate from content dedup)

`ConflictResolution`: `None`, `Ignore`, `SaveAsNew` (`Rename`), `Replace`.
UI labels: "Jedes Mal nachfragen", "Ignorieren", "Als neue Dateien speichern",
"Ersetzen" (alternate wording "Neue Datei automatisch umbenennen", "Neue Datei
ignorieren", "Alte Datei ersetzen"). When a file with the same name already
exists in the destination folder, the chosen policy applies; `SaveAsNew` turns
into `cloudDriveConflictResolution=Rename` on the wire.

### 4.2 Content dedup errors

| Trigger | Exception / status | UI label |
|---|---|---|
| MD5 matches an `AVAILABLE` node in local cache, or `GetNodeByMd5` hit | `FileIgnoredPreUploadException` | "Zuvor hochgeladen" (`clouddrive_desktop_upload_statuswindow_statusignored`) |
| Server 409 with matching MD5 / `MD5_DUPLICATE` / `DuplicatesConflictError` | `FileIgnoredException` (after saving returned node) | "Zuvor hochgeladen" |
| Apple-Photos `.heic` with JPEG BOM (potential iOS dup) | `FileIgnoredForPotientialDuplicationException` | skip (collects into ignored set) |
| File too large for Prime tier etc. | `FileOverQuotaException` / `FileSizeTooLargeException` | separate error path |

Normalised error code on the wire: `DuplicatesConflictError` and `MD5_DUPLICATE`
both become `TransferError.CloudDriveNodeConflict`, logged at `Verbose` level.
They do **not** retry (they are terminal "skip" outcomes).

### 4.3 `DuplicateLimitExceeded` (unrelated guard metric)

`HealthMetrics.Crash.DuplicateLimitExceeded` is used by the crash-report limiter
(`LimitCrashDuplicates`): when the same crash has already been reported more than
`Settings.MaxCrashDuplicateCount` times, the app emits this metric and quits.
It has nothing to do with photo/video dedup and should not be cited as part of
the upload path.

## 5. What `DedupedPing` is

`HealthMetrics.Session.DedupedPing` is a **session-telemetry** metric, not a
file-dedup concept. The client runs a periodic tick (`SessionPingTimeInSeconds`);
once per interval it stamps `cloud.db.LastSessionPingReported` and, if the gap
since the last report is at least the configured interval, increments
`DedupedPing` (pivoted by software version). The name only means "a ping that
is de-duplicated against the previous one within the interval" so OnTick does
not flood the metrics pipeline. Unknown: whether the value is sent on every tick
or only on cold start beyond the first interval.

## 6. Apple-Photos duplicates prompt flow

Triggered from the enumeration batch path: when a walked batch contains a
`.heic` under an Apple Photos library path (`PhotoLibraryFileSystemObject`) and
the customer has never been prompted (`HasCustomerSeenApplePhotosDuplicatesPrompt == false`).

- The client waits for the main window to be open, then shows a modal
  (`ShowModalDialog`) with:
  - header `clouddrive_desktop_iosdedupeprompt_header`:
    "Wenn Sie darüber hinaus Amazon Photos für iOS zum automatischen Speichern
    Ihrer Apple Fotobibliothek verwenden, kann dies dazu führen, dass von
    einigen Bildern Duplikate erstellt werden."
  - information text `clouddrive_desktop_iosdedupeprompt_information`:
    "Möchten Sie die Sicherung dieser Elemente auf diesem Computer vermeiden?"
  - buttons `clouddrive_desktop_skip` ("Übergehen") and
    `clouddrive_desktop_keep` ("Beibehalten").
- On click it sets `HasCustomerSeenApplePhotosDuplicatesPrompt = true` and one of:
  - **filtering on** (`FilterOutPotentialDuplicatesFromApplePhotosLibrary = true`),
    emitting business metric `IosHeicDedupePromptIosOnly`; or
  - **filtering off** (`= false`) plus `RetryIgnoredDueToPotentialConflictsWithiOSUploads()`,
    emitting business metric `IosHeicDedupePromptDesktopAndIos`.
  Which button maps to which outcome depends on the dialog's return value
  (`DialogResult.NegativeActionClicked` → filtering off in the decompiled
  ordering); the absolute mapping of "Übergehen"/"Beibehalten" to the two
  outcomes is not verified here and should be checked against the XIB if needed.
- When filtering is on, eligible `.heic` records with a JPEG BOM raise
  `FileIgnoredForPotientialDuplicationException` at upload time (§2.2 step 6).
- `FilterOutPotentialDuplicatesFromApplePhotosLibrary` is also a customer setting
  exposed under preferences (truth property `EnableDeduplicationForPhotosAndVideos`,
  string "Duplikate überspringen"); the prompt's tooltip points users there
  (`clouddrive_desktop_iosdedupeprompt_backupitem_tooltip`).

## 7. Open points / not implemented (for our project)

- Our app does **not** run a local `cloud.db` node cache; P1 "only upload new
  files" will therefore start with the `uploads.precomputed_md5` index and may
  later add a small content-MD5 index. Until a local cloud cache exists, the
  server-side 409 path (§2.3) is the effective second line of defence.
- Our app hard-codes `conflictResolution=RENAME` in `PrimePhotos::upload_picture`
  instead of `NONE`. With RENAME, a duplicate name produces a renamed copy
  rather than being detected as content duplicate. Aligning to `NONE` (or, better,
  an explicit pre-flight) is part of P1.
- The desktop-client device flow
  (`upload-photos.amazon.de/v2/upload` with `x-amz-access-token`,
  `Content-MD5` base64, etc.) is documented in `docs/PROTOCOL.md` but unused by
  us; its dedup behaviour is assumed to be equivalent but **unverified**.
- Whether `GetNodeIdForLocalId(localId + md5)` is ever used by the backup flow
  to short-circuit duplicates, beyond resumable-upload recovery, is unverified.
- Whether non-media extensions in the Other group are accepted by the server at
  all (`/v2/upload` with kind=FILE) is unverified in our cookie flow; the
  official client blocks them in backup since 2023-11-30.

## 8. Cross-reference

- `docs/UI_STATES.md` §4.1: `ContentFilter` / `DeduplicateOptions` bitmasks,
  German labels, and the "Everything forces dedup off" constraint.
- `docs/PROTOCOL.md`: desktop upload headers (`content-md5` hex + base64), 409
  behaviour, node endpoints.
- `docs/openapi.yaml`: `/v2/upload` parameters, `node.contentProperties.md5`.
- Impl backlog: `AGENTS.todo.md` P1 "Nur neue Dateien hochladen (Dedup)" and
  "exakter Extension-Filter".

## 9. Verification status of statements above

- **Verified from the binary/resource/DB** (this session): MD5-as-identity and its
  hex form; `uploads.precomputed_md5` / `nodes.content_md5` columns; PreUploadCheck
  algorithm including `UploadType.Deduped` gating and `GetNodeByMd5`; server 409
  normalisation for `MD5_DUPLICATE` / `DuplicatesConflictError`; ContentFilter
  bitmasks; `ContentFilterExtensions.Matches` classification order; full
  extension list in `supported-content-types.json`; Apple-Photos prompt strings
  and settings.
- **Inferred / not decoded**: exact UI button-to-outcome mapping for the
  iOS-dedup prompt; whether `DedupedPing` reports on every cold start or only
  when the interval elapsed; whether `pending_categorization` is consumed
  by any code (the string occurs once — only as the JSON key in the embedded
  resource; no reference was found in decompiled code).
- **Unknown** (marked in §7): cookie-flow acceptance of non-media `Other`
  extensions; full parity of the unused device flow; whether a local `cloud.db`
  cache is mandatory to reproduce server parity in our app.

