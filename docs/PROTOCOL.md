# Amazon Photos Desktop Client — HTTP Protocol Notes

Reconstructed from captured mitmproxy flows of the official macOS desktop client
(`User-Agent: CloudDriveMac/11.3.0.1d7941b5`, internal name `CloudDriveDesktop`,
app name `Amazon Photos`).

Sources:

| Capture | Era | What it contains |
|---|---|---|
| `upload-capture.mitm` | logged-in session | 3 successful file uploads + full account bootstrap |
| `amazon2.mitm` | logged-in session | account bootstrap, folder creation, upload-node queries |
| `amazon-photos.mitm` | login / registration | device registration (`auth/register`), account bootstrap |

Only the desktop-client hosts are documented
(`upload-photos.amazon.de`, `cdws.eu-west-1.amazonaws.com`, `drive.amazonaws.com`,
`api.amazon.com`, `msh.amazon.com`, `unagi-na.amazon.com`). Browser-only traffic
(`www.amazon.de`, `cognito-identity`, Google, CloudFront) is out of scope.

> Anything described here as "observed" was seen verbatim in a capture. Anything
> marked **unverified** was inferred and must not be treated as fact.

---

## 1. Common conventions

### 1.1 Headers used by the desktop client

Every CloudDrive metadata request sends these (values shown with redaction):

```
Accept: application/json
User-Agent: CloudDriveMac/11.3.0.1d7941b5
x-amz-access-token: Atna|<redacted>          # OAuth bearer access token ("Atna|" prefix)
x-amz-clouddrive-appid: YW16bjEuYXBwbGljYXRpb24tb2EyLWNsaWVudC40YTY0NzZkODljYTA0NDQ3OGY3MTI3MDZiN2VjYzYwNA
x-amzn-clouddrive-source: <sourceId>          # returned by POST /drive/v1/account/source
```

The two base64 identifiers decode to non-secret application constants:

```
x-amz-clouddrive-appid  = base64("amzn1.application-oa2-client.4a6476d89ca044478f712706b7ecc604")
```

`x-amzn-clouddrive-source` is **not** constant — it is the `sourceId` the server
assigns in the `POST /drive/v1/account/source` response (e.g. `Su75mvigyWwCyzHji8uSEivJllYuq-uwC`).

`Connection: close` is used on all CloudDrive metadata calls in the captures.

### 1.2 Token formats (header/body prefixes only; values redacted)

| Token | Format / prefix | Where it appears |
|---|---|---|
| refresh token | `Atnr\|<redacted>` | request body of `POST /auth/token` (`source_token`) |
| access token | `Atna\|<redacted>` | `x-amz-access-token` header, `Authorization: bearer`, `access_token` field |
| secure session token | `Sst1\|<redacted>` | `secureSessionToken` field of `/auth/token` response |
| adp token (device reg.) | `{enc:<redacted>}` | `adp_token` in `/auth/register` response |
| customer id | `amzn1.account.<redacted>` | various |

### 1.3 Redaction convention used below

* Access/refresh/session tokens → `Atna|<redacted>`, `Atnr|<redacted>`, `Sst1|<redacted>`.
* Account / customer / device / node ids that are personal → `<accountId>`,
  `<customerId>`, `<deviceId>`, `<sourceId>`, `<parentNodeId>`.
* File/node ids in examples (non-secret, opaque) are shown as captured where useful,
  otherwise `<nodeId>`.

---

## 2. Endpoint reference

### 2.1 `POST https://api.amazon.com/auth/token`

Exchange a refresh token for a fresh access token. **Purpose: refresh the OAuth
access token used by every subsequent call.**

Request headers:
```
Accept: application/json
Accept-Language: de-AT
User-Agent: CloudDriveMac/11.3.0.1d7941b5
Content-Type: application/json; charset=utf-8
```

Request body (591 bytes observed):
```json
{
  "source_token": "Atnr|<redacted>",
  "source_token_type": "refresh_token",
  "requested_token_type": "access_token",
  "app_name": "Amazon Photos",
  "app_version": "11.3.0.1d7941b5"
}
```

Response `200` (shape; tokens truncated):
```json
{
  "access_token": "Atna|<redacted>",
  "secureSessionToken": "Sst1|<redacted>",
  "age_classification": { "age_classification": "ADULT" },
  "response": {
    "token_expires_in": "3600",
    "token_type": "bearer_token",
    "token": "Atna|<redacted>"
  },
  "token_type": "bearer",
  "expires_in": 3600,
  "request_id": "<uuid>"
}
```

Notes: `source_token`/`requested_token_type` are the variable inputs; the rest are
constant. Access token TTL is 3600 s.

**Same endpoint, different mode — website cookies.** The original also calls this
exact endpoint with **query parameters** instead of a JSON body:
`POST /auth/token?source_token=<refresh token>&source_token_type=refresh_token&requested_token_type=auth_cookies&domain=.<tld>&app_name=…&app_version=…`
vends the customer's **website cookies** (written into `NSHttpCookieStorage` for
the embedded web views). The body form with `requested_token_type=access_token`
shown above remains the access-token refresh path. See
`docs/SPEC_LOGIN.md` § 3.8.

### 2.2 `POST https://api.amazon.com/auth/register`

One-time device registration (login era). Exchanges a browser-obtained OAuth
`authorization_code` (PKCE) for device + `mac_dms` tokens. **Purpose: register this
device as a CloudDrive source.**

Request body (690 bytes observed):
```json
{
  "registration_data": {
    "app_name": "Amazon Photos",
    "app_version": "11.3.0.1d7941b5",
    "device_model": "Mac",
    "device_name": null,
    "device_serial": "f56c730bf63b967f0807927840a8c191",
    "device_type": "A2UXDCSFQXA1M7",
    "domain": "Device",
    "os_version": "27.0.1"
  },
  "auth_data": {
    "use_global_authentication": false,
    "authorization_code": "<redacted>",
    "code_verifier": "<redacted>",
    "code_algorithm": "SHA-256",
    "client_id": "<hex>",
    "client_domain": "DeviceLegacy"
  },
  "user_context_map": {},
  "requested_extensions": ["customer_info", "device_info"],
  "requested_token_type": ["bearer", "mac_dms"]
}
```

The `client_id` is ASCII-hex of `f56c730bf63b967f0807927840a8c191#A2UXDCSFQXA1M7`
(device serial `#` device type) — i.e. it is derived, not a fixed client secret.

Response `200` (shape; key material redacted):
```json
{
  "response": {
    "success": {
      "extensions": {
        "device_info": { "device_name": "… (Mac)", "device_serial_number": "<deviceSerial>", "device_type": "A2UXDCSFQXA1M7" },
        "age_classification": "ADULT",
        "customer_info": { "account_pool": "Amazon", "user_id": "amzn1.account.<redacted>", "home_region": "EU", "name": "<redacted>", "given_name": "<redacted>" }
      },
      "tokens": {
        "mac_dms": { "device_private_key": "<redacted>", "adp_token": "{enc:<redacted>}" }
      }
    }
  }
}
```

### 2.3 `GET https://api.amazon.com/user/profile`

**Purpose: fetch the signed-in customer's profile / Amazon account id.**

Headers: `Authorization: bearer Atna|<redacted>`, `Accept: application/json`.

Response `200`:
```json
{
  "user_id": "amzn1.account.<redacted>",
  "name": "<redacted>",
  "shipping_address": { "country": "…", "country_code": "AT", "street_address": "<redacted>", "locality": "<redacted>", "postal_code": "<redacted>" },
  "mobile_number": "<redacted>",
  "postal_code": "<redacted>",
  "email": "<redacted>"
}
```

### 2.4 `GET https://drive.amazonaws.com/drive/v2/account/endpoint`

**Purpose: discover the per-account regional service URLs.** Run once per session,
before any `cdws`/`upload-photos` call.

Headers: `Accept: application/json`,
`x-amz-access-token: Atna|<redacted>`,
`x-amz-clouddrive-appid: <appid>`,
`x-amzn-clouddrive-source: <sourceId>`.

No request body. Response `200` — this response is the source of truth for where to
send uploads:
```json
{
  "contentUrl": "https://content-eu.drive.amazonaws.com/",
  "countryAtSignup": "AT",
  "customerExists": true,
  "downloadServiceUrl": "https://download-photos.amazon.de/",
  "marketplaceAtSignup": "A1PA6795UKMFR9",
  "metadataUrl": "https://cdws.eu-west-1.amazonaws.com/drive/v1/",
  "photosBFFUrl": "https://www.amazon.de/photoapp/",
  "region": "eu-west-1",
  "retailUrl": "www.amazon.de/drive/v1",
  "thumbnailServiceUrl": "https://thumbnails-photos.amazon.de/",
  "uploadServiceUrl": "https://upload-photos.amazon.de/"
}
```

`uploadServiceUrl` is the base for `POST /v2/upload`.

### 2.5 `GET` / `POST https://cdws.eu-west-1.amazonaws.com/drive/v2/account`

Two observed forms:

* `GET` — read account status (first run / login era). Response `200`:
  ```json
  {"accountStatusReason":"TOU_ACCEPTION","country":"AT","creationDate":1.487874264438E9,
   "isValid":true,"lastUpdatedDate":1.48787426446E9,"marketplaceId":"A1PA6795UKMFR9",
   "sharingStatus":"SHARING_ACTIVE","status":"ACTIVE","termsOfUse":"1.0.0"}
  ```
* `POST` — accept terms of use / activate. Body (40 bytes), response `200 {}`:
  ```json
  {"termsOfUse":"1.0.0","status":"ACTIVE"}
  ```

**Purpose: read/activate the CloudDrive account and accept the ToU.**

### 2.6 `POST https://cdws.eu-west-1.amazonaws.com/drive/v1/account/source`

**Purpose: register/refresh this desktop device as a CloudDrive source and obtain
the `sourceId` used as `x-amzn-clouddrive-source` on later calls.**

Request headers include `x-amz-access-token`, `x-amz-clouddrive-appid`,
`Content-Type: application/json; charset=utf-8`.

Request body (392 bytes):
```json
{
  "deviceSerialNumber": "f56c730bf63b967f0807927840a8c191",
  "deviceClass": "DESKTOP",
  "devicePlatform": "MAC",
  "deviceFriendlyName": "<redacted>",
  "osVersion": "Mac 27.0.1",
  "sourceApplicationName": "CloudDriveDesktop",
  "sourceVersion": "11.3.0.1d7941b5",
  "customerId": "amzn1.account.<redacted>",
  "eTag": null,
  "registrationToken": null,
  "pushProvider": null,
  "resourceVersion": "V2"
}
```

Response `201 Created` (shape; ids redacted):
```json
{
  "creationTime": "2026-07-06T12:14:57.949Z",
  "device": {
    "deviceClass": "DESKTOP", "deviceFriendlyName": "<redacted>",
    "deviceId": "<deviceId>", "devicePlatform": "MAC", "deviceStatus": "ACTIVE",
    "osVersionHistory": { "Mac 27.0.1": "…" }
  },
  "sourceApplicationName": "CloudDriveDesktop",
  "sourceId": "<sourceId>",
  "sourceVersionHistory": { "11.3.0.1d7941b5": "…" },
  "moEngageDirectedId": "amzn1.photos.moengage.<redacted>",
  "pushEndpoint": false
}
```

### 2.7 `POST https://msh.amazon.com/mwl/assignments/v2`

Query parameters (all variable): `session-id`, `marketplace-id` (e.g.
`ATVPDKIKX0DER`), `directed-id` (= `amzn1.account.<redacted>`), `stage=production`.

Headers: `Accept: application/json`, `x-client-id: AmazonPhotosDesktop`,
`Content-Type: application/json`.

Request body: `{"Weblabs":[]}`

Observed response: `400` (client sends an empty list the service rejects):
```json
{"message":"1 validation error detected: Value at 'weblabs' failed to satisfy constraint: Member must have length greater than or equal to 1"}
```

Only the `200` (non-empty weblabs) response shape is **unverified**. The client
appears to tolerate the `400` (session continues).

### 2.8 `POST https://unagi-na.amazon.com/1/events/com.amazon.photos.bi.metrics.desktop.prod`

**Purpose: batched desktop client telemetry.**

Headers: `Content-Type: application/json; charset=utf-8`, `User-Agent: CloudDriveMac/…`.
Body is a large JSON `{"events":[…]}` array; each event carries
`SESSION_ID`, `CLIENT_ID` (`A2UXDCSFQXA1M7`), `CLIENT_VERSION` (`11.3.0.1d7941b5`),
`CLIENT_TAG` (`locale=AT`), device fields, `EVENT_NAME`, `EVENT_COUNT`.
Response `200 {}`. Not required for uploading.

### 2.9 `POST https://cdws.eu-west-1.amazonaws.com/drive/v1/changes`

**Purpose: incremental sync of the node tree.**

Request body (61–125 bytes observed; two shapes):
```json
{"checkpoint":"<redacted-base64>","includePurged":"true","limit":"200","resourceVersion":"V2"}
```
```json
{"includePurged":"true","limit":"200","resourceVersion":"V2"}
```

Response `200` — newline-delimited JSON:
```json
{"nodes":[],"checkpoint":"<redacted>","reset":false,"statusCode":200}
{"end":true}
```

Variable: `checkpoint` (absent on first call). `reset` indicates a full resync.

### 2.10 `GET https://cdws.eu-west-1.amazonaws.com/drive/v1/nodes`

**Purpose: list/query nodes (the client uses `limit=1` as a cheap "is there
anything / has the count changed" probe, and `filters` for specific queries).**

Query parameters (variable): `resourceVersion=V2` (constant), `limit`
(observed `1`, `200`), `filters` (URL-encoded), `nextToken` (pagination, taken
from a previous response).

Observed `filters` values:
```
filters=status:AVAILABLE%20AND%20settings.hidden:true
filters=isRoot:true
```

Response `200` (single-node shape; personal fields redacted):
```json
{
  "count": 342278,
  "data": [
    {
      "contentProperties": {
        "contentDate": "2026-05-08T10:31:53.401Z",
        "contentType": "image/x-raw-adobe", "extension": "dng",
        "image": { "make": "…", "model": "…", "width": …, "height": … },
        "md5": "<hex>", "size": 8386554, "version": 1
      },
      "createdBy": "CloudDriveDesktop",
      "id": "<nodeId>", "kind": "FILE", "name": "<redacted>",
      "ownerId": "<customerId>", "parents": ["<parentId>"],
      "parentMap": { "FOLDER": ["<parentId>"] },
      "status": "AVAILABLE", "version": 2,
      "transforms": ["IMAGE_THUMBNAIL"]
    }
  ],
  "nextToken": "<redacted>"
}
```

The root folder (`filters=isRoot:true`) is `kind: "FOLDER"`, `isRoot: true` with an
empty `parents`/`parentMap` — needed as a parent when creating folders in a tree
that is still being built.

### 2.11 `GET https://cdws.eu-west-1.amazonaws.com/drive/v1/nodes/{nodeId}`

**Purpose: fetch a single node by id.** Query param `resourceVersion=V2` (constant).

Response `200` is a single node object (same shape as one `data[]` element in
§2.10). `{nodeId}` is the variable path segment.

### 2.12 `POST https://cdws.eu-west-1.amazonaws.com/drive/v1/nodes`

**Purpose: create a node (folder) — used by the client to mirror the local
backup folder tree.** Confirmed for `kind: "FOLDER"`.

Request headers: `x-amz-access-token`, `x-amz-clouddrive-appid`,
`x-amzn-clouddrive-source`, `Content-Type: application/json; charset=utf-8`.

Request body (115 bytes observed):
```json
{"kind":"FOLDER","name":"Backup","parents":["<parentId>"],"clientProperties":{},"resourceVersion":"V2"}
```

Responses:
* `201 Created` — newly created folder; body is the created node object.
* `409 Conflict` — a node of that name already exists:
  ```json
  {"code":"NAME_ALREADY_EXISTS","info":{"nodeId":"<existingNodeId>"},
   "message":"Node with the requested name already exists under parentId <parentId> conflicting NodeId: <existingNodeId>"}
  ```
  The client then falls back to `GET /drive/v1/nodes/{info.nodeId}` to resolve it.

> Whether an equivalent file node is ever created directly through this endpoint
> (as opposed to via `/v2/upload`) is **unverified** — only FOLDER creations were
> observed here.

### 2.13 `GET https://cdws.eu-west-1.amazonaws.com/drive/v1/account/preferences`

**Purpose: read account preferences (name, avatar node, recognition flags).**

Response `200`:
```json
{
  "autoAddContentToFamilyArchive": false,
  "avatarNodeId": "<nodeId>", "basicRecognitionEnabled": true,
  "familyName": "", "locationRecognitionEnabled": true, "name": "<redacted>",
  "peopleRecognitionEnabled": true, "recognitionEnabled": true,
  "recognitionEnabledFamily": true, "recognitionEnabledPersonal": true
}
```

### 2.14 `GET https://cdws.eu-west-1.amazonaws.com/drive/v1/account/usage`

**Purpose: read byte/count usage by category.**

Response `200`:
```json
{
  "doc":   {"billable":{"bytes":1783311,"count":53}, "total":{"bytes":1783311,"count":53}},
  "other": {"billable":{"bytes":2737644,"count":1},  "total":{"bytes":35380415,"count":18}},
  "photo": {"billable":{"bytes":0,"count":0},        "total":{"bytes":4980130188844,"count":332056}},
  "video": {"billable":{"bytes":0,"count":0},        "total":{"bytes":116921507,"count":11}},
  "lastCalculated": "2026-10-13T09:01:11.912Z"
}
```

### 2.15 `GET https://cdws.eu-west-1.amazonaws.com/drive/v2/account/quota?customerId=UNNEEDED`

**Purpose: read storage quota (the client literally sends `customerId=UNNEEDED`).**

Response `200`:
```json
{
  "available": 5364188165, "grants": [], "personalQuota": 5368709120, "plans": [],
  "quota": 5368709120,
  "unlimitedPhotosFeature": {"criteria":["PRIME"],"enabled":true,"featureType":"UNLIMITED_PHOTOS"},
  "unlimitedPhotosStatus": {"expiration":1.809247577E9,"hasUnlimitedPhotos":true},
  "lastCalculated": 1.791882262678E9
}
```

### 2.16 `GET https://cdws.eu-west-1.amazonaws.com/drive/v1/upload/node`

**Purpose: query nodes that are pending/eligible for upload.** Query params
`resourceVersion=V2&limit=200` (limit variable).

Response `200` (empty in capture):
```json
{"count":0,"data":[]}
```

### 2.17 `POST https://upload-photos.amazon.de/v2/upload` — the actual upload

This is the endpoint that performs a photo upload.

Full path pattern:
```
/v2/upload?name=<url-encoded filename>&fileSize=<bytes>&conflictResolution=NONE&kind=FILE&parentNodeId=<folderNodeId>
```

| Query param | Value / variability |
|---|---|
| `name` | URL-encoded file name (variable) |
| `fileSize` | decimal byte size (variable, must match body length) |
| `conflictResolution` | `NONE` (constant in capture) |
| `kind` | `FILE` (constant in capture) |
| `parentNodeId` | destination folder node id (variable) |

Request headers (all observed on a real upload):
```
POST /v2/upload?... HTTP/1.1
Host: upload-photos.amazon.de
Accept: application/json
Connection: close
x-amzn-file-md5: <hex md5 of body>          # e.g. d4e9c9dfc66d63bb9d15cefd0ca592d6
x-amzn-RequestId: <uuid>                     # client-generated
x-amzn-Trace-Id: <uuid>                      # same value as x-amzn-RequestId here
User-Agent: CloudDriveMac/11.3.0.1d7941b5 CloudDriveMac/11.3.0.1d7941b5
x-amzn-clouddrive-source: <sourceId>
x-amz-access-token: Atna|<redacted>
x-amz-clouddrive-appid: <appid>
Content-Disposition: form-data; name=file; filename=<hex md5 of body>
Content-Length: <fileSize>
Content-Type: application/octet-stream
Content-MD5: <base64 md5 of body>            # e.g. 1OnJ38ZtY7udFc79DKWS1g==
```

Request body: the **raw image bytes** (not multipart — despite the
`Content-Disposition: form-data` header, the body is the bare file; verified: PNG
body starts `\x89PNG\r\n\x1a\n` and length equals `fileSize`).

MD5 relationships (verified across all 3 uploads):
`x-amzn-file-md5` = lowercase hex of MD5(body);
`Content-MD5` = standard base64 of the same 16 bytes;
`Content-Disposition` `filename=` = the same hex MD5.

Response `201 Created` (no `Content-Type`-specific extras in the captures) — the
newly created node:
```json
{
  "id": "<newNodeId>",
  "name": "Bildschirmfoto 2026-10-13 um 11.02.09.png",
  "kind": "FILE", "version": 2,
  "modifiedDate": "2026-10-13T09:06:14.909Z",
  "createdDate": "2026-10-13T09:06:14.482Z",
  "parents": ["<parentNodeId>"], "parentMap": {"FOLDER": ["<parentNodeId>"]},
  "status": "AVAILABLE", "restricted": false, "protectedFolder": false,
  "contentProperties": {
    "size": 170242, "version": 1, "contentType": "image/png", "extension": "png",
    "md5": "<hex md5>", "contentDate": "2026-10-13T00:00:00.000Z",
    "image": {"resolutionUnit":"Pixels/Inch","width":1378,"height":1182},
    "contentSignatures": []
  },
  "properties": {}, "transforms": [], "keywords": [],
  "ownerId": "<customerId>", "subKinds": []
}
```

Notes:
* The server derives `contentType`/`extension`/`image` dimensions itself.
* `contentDate` is normalised to midnight of the file's date
  (`2026-10-13T00:00:00.000Z`), **not** the capture time.
* Characters in `name` are URL-encoded with `+` for spaces.
* All 3 uploads in `upload-capture.mitm` returned `201` with `x-amzn-RequestId`
  echoing the request's `x-amzn-RequestId`.

### 2.18 `POST https://cdws.eu-west-1.amazonaws.com/drive/v2/memories/v1/collage/narratives`

**Purpose: fetch memory collage narrative data.** Not part of the upload path;
listed for completeness.

Request body: `{"nodeList":["<nodeId>"]}`. Response `200`:
`{"collageNarrative":{"nodes":[{"focalBox":{...},"nodeInfo":{<node>}}]}}`.

---

## 3. Upload flow (one file, end to end)

Sequence observed in `upload-capture.mitm` for a logged-in session that uploads 3
files. Steps 1–4 are the session bootstrap (done once, not per file); steps 5–6 are
per file.

1. **Refresh the access token**
   `POST https://api.amazon.com/auth/token`
   with `{"source_token":"Atnr|<stored refresh token>","source_token_type":"refresh_token","requested_token_type":"access_token","app_name":"Amazon Photos","app_version":"11.3.0.1d7941b5"}`
   → `200` with `access_token: Atna|<…>` (TTL 3600 s) and `secureSessionToken: Sst1|<…>`.
   *(Once registered, the client sends only the refresh token; the registration in
   §2.2 happens only on first login. Where the two tokens are stored and how the
   refresh is scheduled — 80 % of the remaining lifetime — is documented in
   `docs/SPEC_LOGIN.md` § 3.8.)*

2. **Resolve the account + regional endpoints**
   * `GET https://api.amazon.com/user/profile` (`Authorization: bearer Atna|<…>`) → account id.
   * `GET https://drive.amazonaws.com/drive/v2/account/endpoint` → returns
     `uploadServiceUrl: https://upload-photos.amazon.de/` and
     `metadataUrl: https://cdws.eu-west-1.amazonaws.com/drive/v1/`.

3. **Activate account + register the device source**
   * `POST https://cdws.eu-west-1.amazonaws.com/drive/v2/account`
     body `{"termsOfUse":"1.0.0","status":"ACTIVE"}` → `200 {}`.
   * `POST https://cdws.eu-west-1.amazonaws.com/drive/v1/account/source`
     body with `deviceSerialNumber`, `deviceClass:"DESKTOP"`, `devicePlatform:"MAC"`,
     `sourceApplicationName:"CloudDriveDesktop"`, `customerId`, etc. →
     `201` returning `sourceId`. From here on, every request carries
     `x-amzn-clouddrive-source: <sourceId>`.

4. **Sync state (enough to know the destination tree)**
   * `POST https://cdws.eu-west-1.amazonaws.com/drive/v1/changes`
     `{"includePurged":"true","limit":"200","resourceVersion":"V2"}` → node delta.
   * Optional/best-effort: `GET /drive/v1/nodes?…&limit=1`,
     `GET /drive/v1/account/preferences`, `GET /drive/v1/upload/node?…&limit=200`,
     `GET /drive/v1/account/usage`,
     `GET /drive/v2/account/quota?customerId=UNNEEDED`.
   * `POST https://msh.amazon.com/mwl/assignments/v2?session-id=…&marketplace-id=…&directed-id=…&stage=production`
     (`{"Weblabs":[]}`) may return `400`; the client proceeds regardless.
   * The destination folder id (used later as `parentNodeId`) comes from this synced
     node tree. If the folder does not yet exist, create it with
     `POST /drive/v1/nodes` `{"kind":"FOLDER","name":…,"parents":[…],"resourceVersion":"V2"}`
     (`201`, or `409` + resolve `info.nodeId`).

5. **Upload the file (per file)**
   * Compute MD5 of the file bytes.
   * `POST https://upload-photos.amazon.de/v2/upload?name=<urlenc name>&fileSize=<bytes>&conflictResolution=NONE&kind=FILE&parentNodeId=<destFolder>`
     with headers: `x-amz-access-token`, `x-amz-clouddrive-appid`,
     `x-amzn-clouddrive-source`, `x-amzn-file-md5: <hex md5>`,
     `x-amzn-RequestId`/`x-amzn-Trace-Id: <uuid>`,
     `Content-MD5: <base64 md5>`, `Content-Type: application/octet-stream`,
     `Content-Length: <bytes>`,
     `Content-Disposition: form-data; name=file; filename=<hex md5>`,
     `Accept: application/json`, `User-Agent: CloudDriveMac/<version>`.
     Body = raw file bytes.

6. **Read the result**
   `201 Created` with the created node JSON (`id`, `contentProperties.md5` verifying
   the upload, `parents`, `status:"AVAILABLE"`). Use the returned `id` to track the
   file locally.

All three files in the capture were uploaded this way (`201`), with MD5s matching
between header and response.

---

## 4. Open questions / unverified

* **`x-amzn-clouddrive-source` vs `x-amzn-clouddrive-source` on the upload**: the
  upload sends it but the capture does not prove the server requires it (all uploads
  had it).
* **`conflictResolution` values other than `NONE`**: `REPLACE`/`KEEP` etc. are
  **unverified** (never observed).
* **Large/multipart uploads**: only small files (< 1 MB) were captured. Whether the
  client switches to a multipart / ranged protocol for large files is **unverified**.
  `fileSize` and body length matched exactly in all captures.
* **Where `parentNodeId` comes from when not already synced**: in the upload capture
  the destination folder id existed already; no node lookup for it appears right
  before the upload. The id is most plausibly resolved from the local sync state
  (`/drive/v1/changes`), not fetched per upload — **unverified**.
* **`auth/register` full token set**: the `mac_dms` response also contains
  `device_private_key` (RSA) and `adp_token`; how these are used on later calls is
  **unverified** (the captured steady-state session used only the `Atna|` access
  token path).
* **`msh .../assignments/v2` success shape**: only the `400` validation-error shape
  was observed; the `200` body is **unverified**.
* **`purchase`/`kind` other than `FILE`**: only `kind=FILE` observed; folder
  creation uses `/drive/v1/nodes` instead.
* **`upload-photos` error responses** (`4xx`/`5xx`): none observed, so their shape is
  **unverified**.
* **`/v2/upload` response `Content-Type`/extra headers**: captures show only JSON
  body + `x-amzn-RequestId`; region/CDN headers are **unverified**.
