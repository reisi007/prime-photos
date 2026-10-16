# Upload engine

Files are uploaded with the browser-cookie path to the user's zone endpoint.
The engine supports all common photo formats, streams file content, and
handles server responses including conflict detection.

## How it works

- Files are uploaded via `POST https://content-<zone>.drive.amazonaws.com/v2/upload`
  with raw body, cookie-based auth (`session-id`, `x-acb/at-acb/ubid-acb`).
- Query parameters: `conflictResolution`, `fileSize`, `name`, `parentNodeId`.
  POST and PUT share the same query set.
- **No `contentDate` is sent** — the server derives the capture date from the
  uploaded bytes (EXIF `DateTimeOriginal` + `SubSecTimeOriginal`), falling back
  to nothing. A `PUT /v2/upload/<nodeId>` without `contentDate` keeps the
  existing date (verified live: **200**, same node ID, content swapped, date
  unchanged).
- Names are uploaded and displayed in **NFC** normalisation, because macOS
  hands out NFD while Windows and the web use NFC, and the server does not
  normalise. Without this, the same file would resolve differently depending
  on which machine uploaded it.
- Progress is aggregated across all targets, while each target row reports
  its own outcome. Results are split into **newly uploaded / already in the
  account / unchanged / errors**.
- The local MD5 index (see [duplicates-conflicts.md](duplicates-conflicts.md))
  is the primary defence; the server 409 is a safety net.
- Desktop-client upload protocol (observed, not used):
  `POST https://upload-photos.amazon.de/v2/upload?name=<urlenc>&fileSize=<n>&conflictResolution=NONE&kind=FILE&parentNodeId=<id>`
  with raw body, `x-amz-access-token`, `x-amzn-file-md5` (hex),
  `Content-MD5` (base64), `x-amz-clouddrive-appid`, `x-amzn-clouddrive-source`.
- Upload response (cookie path, live observed 2025-10-15): a successful upload
  returns JSON whose **metadata is not always present** — for an image without
  EXIF resolution (live: a PNG) the body carries `id`/`name`/`kind`/`version`/…
  but **no `resolutionUnit`**. Those fields must be treated as optional.
- Reference implementation: `entourloop/amazon-photo-uploader` (MIT).

## Decisions

- **Auth (upload engine):** browser cookies (session-id + x-acb/at-acb/ubid-acb)
  -> `content-<zone>.drive.amazonaws.com/v2/upload`.

- **Aufnahmedatum: wir senden kein `contentDate` (User-Entscheidung 2026-10-16,
  „das Wichtige ist, dass die Sortierung online nach Aufnahmedatum geht").**
  Das Aufnahmedatum leitet **der Server** aus den hochgeladenen Bytes ab —
  wie beim Original-Client, der das Feld nie setzt (REA:
  `../original/SPEC_CAPTURE_DATE.md` §1–§7). Live belegt am 2026-10-16 an
  unserem Cookie-Host (`content-<zone>.drive.amazonaws.com/v2/upload`): ein
  `POST` **ohne** `contentDate` ⇒ **201** und das Konto trägt
  `DateTimeOriginal` + `SubSecTimeOriginal` aus der Datei
  (`2026-10-13T19:47:36.123Z`) — nicht `ModifyDate`, nicht mtime, nicht die
  Upload-Zeit; ein **mitgesendeter** Wert wird **wörtlich übernommen** et
  überschreibt die Ableitung (`…36.000Z`); der `PUT /v2/upload/<nodeId>`
  funktioniert **ohne** `contentDate` (**200**, gleiche Node-Id, Inhalt
  getauscht, Datum bleibt). Der Server legt die EXIF-Wanduhrzeit naiv als `Z`
  ab (keine Offset-Umrechnung). **Der Query-Satz ist damit
  `conflictResolution`, `fileSize`, `name`, `parentNodeId`** — POST und PUT
  identisch (ohne `fileSize` ⇒ 400 `MissingFileSize`).
  **Verworfen:** den Wert weiterhin selbst ermitteln und mitsenden — die eigene
  Ermittlung hing an unserem EXIF-Leser und fiel ohne EXIF auf die **Dateizeit**
  zurück (bei einem Lightroom-Export = Exportzeit ⇒ falscher Tag); mit dem Feld
  entfallen EXIF-Priorität, Dateizeit-Rückfall und die Regel „jünger als 10
  Minuten ⇒ Fehler". Doku: `../original/SPEC_UPLOAD.md` §2.4,
  `../protocol/PROTOCOL.md` §2.17, Live-Abnahme in `LIVE_ACCEPTANCE.md`.

- **Desktop-client upload protocol (observed):**
  `POST https://upload-photos.amazon.de/v2/upload?name=<urlenc>&fileSize=<n>&conflictResolution=NONE&kind=FILE&parentNodeId=<id>`
  with raw body, `x-amz-access-token`, `x-amzn-file-md5` (hex),
  `Content-MD5` (base64), `x-amz-clouddrive-appid`, `x-amzn-clouddrive-source`.
  Not used by the app yet (we use the cookie path); documented for parity.

- **Upload response (cookie path, live observed 2025-10-15):** a successful
  upload returns JSON whose **metadata is not always present** — for an image
  without EXIF resolution (live: a PNG) the body carries
  `id`/`name`/`kind`/`version`/… but **no `resolutionUnit`**. `ResponseUpload`
  requires it today, so a **successful** upload is reported as a parse error
  (counted as an error, no index row ⇒ re-read every run). Those fields must
  be treated as optional; the bug and the repro are in the ledger (Welle 2,
  live findings), fix not yet commissioned.

- **Supported photos:** jpg, jpeg, png, heic, heif, tif, tiff, gif, webp, bmp
  plus common RAW (cr2, cr3, arw, nef, dng, raf, rw2, orf, srw, pef, and
  others). `.xmp` sidecars are explicitly rejected.

- **Dateinamen-Kodierung: NFC (User-Entscheidung 2026-10-14):**
  - **Der Upload-Name wird auf NFC normalisiert — und der Anzeigetext ebenso.**
    Belegter Anlass: der Server normalisiert **nicht**. Eine Abfrage in NFD
    findet nur die NFD-Datei, eine in NFC nur die NFC-Datei
    (`27_ÖFB-Reisinger_01.dng`: NFD-Treffer, NFC `count=0`). macOS liefer
    **NFD**, Windows **NFC** ⇒ derselbe Dateiname ist auf den beiden Plattformen
    ein **anderer String**.
  - **Warum das zählt (belegt):** die Namensbehandlung ist **exakt/ordinal** —
    das Original sucht den Zielknoten per `GetNodeByNameAndParentIfExists`
    (exaktes `name = ?`), und unser Konfliktweg (`Ask`/`SaveAsNew`/`Replace`)
    hängt ebenso am Namen. Ohne Normalisierung findet „Ersetzen" auf dem Mac
    die von Windows (NFC) angelegte Datei **nicht** und legt eine zweite an —
    dieselbe Aktion verhält sich also je Plattform verschieden. Dazu die
    Anzeige: macOS-Namen (NFD) rendern in der eingebetteten Schrift als
    `O□FB`.
  - **Was NFC *nicht* verhindert (Korrektur 2026-10-14):** ein zweiter
    Upload **desselben Inhalts** wird schon **serverseitig** abgewiesen — live
    belegt: `409` mit `errorCode = "DuplicatesConflictError"`, die Message
    nennt den `FILE_MD5` und `errorDetails.conflictNodeIds`. Ein früherer
    Schluss, im Konto lägen „beide Formen derselben Datei", war **falsch**:
    `27_ÖFB-Reisinger_01.jpg` existiert dort zweimal, aber als
    **verschiedene Fotos** (NFD 28.09.2026, 6.274.270 B, MD5 `1d16ed8f…` =
    unsere Datei; NFC 28.03.2026, 8.911.096 B, MD5 `c5f0f37e…` = anderes Bild
    mit gleichem Namen). Namensgleichheit aus zwei Shootings — **kein**
    Duplikat derselben Datei.
  - NFC ist die gemeinsame Form: Windows und die Web-Seite sind NFC, und das
    Original sendet auf Windows ebenfalls NFC (auf macOS roh NFD — es
    normalisiert gar nicht, belegt in
    `../original/SPEC_FILENAME_ENCODING.md`).
  - **Unverändert bleiben: der lokale Pfad und der MD5-Index.** Die
    Dedup-Schlüssel sind Pfade, die dürfen sich nicht verschieben; die
    Normalisierung betrifft **nur** den Namen, der an den Server geht, und den
    Anzeigetext.
  - Damit ist die frühere Entscheidung „`unicode-normalization` als
    Abhängigkeit, NFC **nur** für den Anzeigetext" (2026-10-14) **erweitert**
    — sie war zum Zeitpunkt der neuen Frage noch nicht umgesetzt, und sie deckte
    den Upload-Namen nicht ab.
  - **Verworfen:** NFD senden (macht Windows zum Sonderfall, der die
    Doppelungen erzeugt); nichts tun (die Doppelungen bleiben).

## Original reference

- `../original/SPEC_UPLOAD.md` — Upload protocol (endpoints, headers, dedup).
- `../original/SPEC_CAPTURE_DATE.md` — Capture date (EXIF, server derivation).
- `../original/SPEC_FILENAME_ENCODING.md` — Filename encoding (NFC/NFD).
- `../protocol/PROTOCOL.md` — Wire protocol.
