# Spec: Aufnahmedatum (`contentDate`) beim Upload des Original-Clients

**Status:** Clean-Room-Recon (nur lesend) des offiziellen macOS-Amazon-Photos-Clients.
Eigene Worte und Tabellen, keine verbatim dekompilierten Code-Blöcke, keine String-Dumps,
keine Personendaten. Symbol- und Dateinamen aus dem Bundle sind zitiert; sie sind keine
Codekopie.

**Ziel-Identität.**

| Feld | Wert |
|---|---|
| Bundle | `/Applications/Amazon Photos.app` |
| `CFBundleShortVersionString` / `CFBundleVersion` | `11.3.0` / `36` |
| `CommitId` (`Info.plist`) | `1d7941b5` |
| Managed Assembly | `Contents/MonoBundle/Amazon Photos.dll` |
| SHA-256 der DLL | `34a616e77bd54d6288845741b5928e68e7c92975428d16553199b04cd1bcedc0` |
| Format / Architektur | PE32+ (CLI, `il-only`, CLI-Runtime-Version `2.5`), `x86_64` |
| Datum der Recon | 2026-10-16 |

**Frage.** „Das Wichtige ist, dass die Sortierung online nach Aufnahmedatum geht" — wie leitet
das Original-Client-Programm beim Hochladen das Aufnahmedatum ab, und wie kommt es so ins Konto,
dass die Galerie danach sortiert?

**Methode und Belege.** REA-Profil `rea-dotnet-static` (Provider `REA managed static analysis
provider`, v1), ein Beleg je Aufruf:

| Aufruf | Tool | Evidence-ID | Ergebnis |
|---|---|---|---|
| A | `inspect_managed_artifact` (Pfad der DLL) | `ev_ae22f6e9713b8d2be36d3d3d8a7157c7c2ffd49ff026fc82c30f3ade5e74fc52` | Identität + Klassifikation `managed`, `coverage.state = partial` |
| B | `inspect_managed_members` (Pfad der DLL) | `ev_79378b0bc34d0144692a4246505ad37f2cf0a158a352f254979dfd0e959ba09d` | `coverage.state = unavailable` |
| C | `close_binary` | — | am Sitzungsende **ausgeführt**, aber von `target_unavailable` („No app is open") beantwortet ⇒ kein offenes Ziel mehr, **kein** Snapshot geschrieben, nichts zu entwässern |

**Blocker des REA-Managed-Providers (exakt):** beide Aufrufe brechen an derselben Budgetgrenze ab —
`resource-limit`, `scope = metadata.representation`, `offset = 1540426`,
Detail „Managed metadata exceeds the 67108864 byte representation budget".
Aufruf B formuliert die Folge selbst: „The CLI metadata could not be admitted; managed member
inspection is unavailable." **Ausweichroute (genutzt):** Der in früheren Sitzungen gemeldete
`ilspycmd`-Blocker („You must install .NET to run this application") war **nur** eine fehlende
`DOTNET_ROOT`-Auflösung — auf dieser Maschine liegt .NET 10.0.12 unter
`/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`. Mit gesetztem `DOTNET_ROOT` arbeitet
`ilspycmd` 11.1.0.9782 und hat die Assembly vollständig dekompiliert; die Symbol-Namen und die
Zeilenkoordinaten unten sind **build-lokale** Koordinaten aus genau diesem Build (Version +
SHA-256 oben).

**Wichtigster Befund vorweg (belegt, siehe §1–§5):** Der Original-Client **liest auf der
Upload-Seite gar kein EXIF und berechnet kein Aufnahmedatum**. Er besitzt die Query-Felder
`contentDate` und `fallbackContentDate`, **setzt sie aber nie** — sie sind `null` und werden vom
Query-Bauer verworfen. Das Datum entsteht serverseitig; der Client konsumiert es nur. Die
Sortierung der Online-Galerie läuft über `contentProperties.contentDate` — ein **Server**-Feld,
das der Client vorgefertigt bekommt.

---

## 1. Tag-Priorität (EXIF/XMP/Dateizeit)

**Befund.** Es gibt **keine** Tag-Priorität, weil es **keine** Tag-Extraktion gibt. Die Assembly
enthält **keinen** EXIF-/XMP-/IPTC-Leser: kein `Exif`-Bezeichner, kein `MetadataExtractor`,
kein `SubSecTime`, kein `OffsetTime`, kein `DateTimeOriginal`-String als Feldname (case-insensitive
Suche über den gesamten Dekompilierten: `exif` = **0 Treffer**). Der **einzige** Bild-Metadaten-Zugriff
im ganzen Bundle ist `CGImageSource.CreateThumbnail` (ImageIO) — und der dient ausschließlich der
Thumbnail-Erzeugung für die UI, nicht der Datumsermittlung. Die einzigen Datumsquellen, die der
Client überhaupt berührt, sind Dateisystem-Zeitstempel: `IFileSystemObject.CreatedDate` /
`.ModifiedDate` (aus `FileSystemInfo`), die er in lokalen Tabellen (`uploads.created_date` /
`modified_date`) ablegt und auf Wunsch als `clientProperties.dateCreated` / `dateUpdated`
mitschickt — **nicht** als `contentDate`.

Die vom Server gelieferte EXIF-Auswertung sieht der Client nur als **Antwort-Modell**:
`ImageProperties` deklariert `dateTime`, `dateTimeDigitized`, `dateTimeOriginal` (die drei
klassischen EXIF-Datumsfelder), aber die Klasse wird **von keiner Codestelle verwendet** —
sie existiert nur als JSON-Ziel. Der Eigenschafts-Name ist im IL als trivialer
Auto-Property-Backing-Field sichtbar (kein abweichendes Setzverhalten).

| Kandidat | Im Artefakt vorhanden? | Verwendung |
|---|---|---|
| `DateTimeOriginal` / `DateTimeDigitized` / `DateTime` (EXIF) | nur als **Antwort-**Feld `ImageProperties.dateTimeOriginal` u. a. | deklariert, **kein** Aufrufer |
| XMP (`xmp:CreateDate`, `photoshop:DateCreated`) | nein (`.xmp` existiert nur als Dateiendungs-/MIME-Eintrag, kein Parser) | — |
| `SubSecTime*`, `OffsetTime*` | nein (0 Treffer) | — |
| `GPSDateStamp` | nein | — |
| Dateisystem-Zeit (`CreatedDate`/`ModifiedDate`) | ja (`IFileSystemObject`) | lokale DB + `clientProperties`, **nicht** `contentDate` |
| Konstante / „kein Datum ⇒ Fehler" | nein | Client sendet schlicht nichts |

**Beleg.** Negative Suche über den gesamten Dekompilierten dieses Builds (Symbole: `ImageProperties`,
`IFileSystemObject`, `CGImageSource`, `FileSystemObject.CreatedDate/ModifiedDate`,
`UploadOperation.SetCloudClientProperties`, `ClientProperties`); String-Scan über alle
`MonoBundle/*.dll` + die Haupt-Binary: EXIF-Treffer nur in **Framework**-Bibliotheken
(`Microsoft.macOS.dll` = native Konstante `kCGImagePropertyExif…`, `System.Drawing.Common.dll` =
`get_Exif`), **null** Treffer in `Amazon Photos.dll`; Evidence-ID `ev_ae22f6e9…` (A),
`ev_79378b0b…` (B).

**Einordnung.** **belegt** (Null-Fund mit Suchgrenze, siehe §8) — **Schlussfolgerung:** das
Aufnahmedatum ist eine **Server**-Zuständigkeit; der Client ist auf diesem Feld ein reiner
Transporteur. **UNKNOWN:** welchen Tag der *Server* in welcher Reihenfolge nimmt — das liegt
außerhalb des Artefakts.

---

## 2. Zeitzone (EXIF-Lokalzeit → UTC?)

**Befund.** Auf der Client-Seite nicht entscheidbar und **gegenstandslos**: der Client formatiert
kein Datum, rechnet nichts um und hängt kein Suffix an, weil er **kein** Datum auf die Leitung
schickt (§4, §7). Es existiert keine Stelle, die eine EXIF-Lokalzeit mit `OffsetTime*` nach UTC
verschieben würde (0 Treffer für `OffsetTime`).

**Was serverseitig sichtbar wird** (nicht Client-Verhalten, sondern Beobachtung an der Antwort):
Die Upload-Antwort eines EXIF-losen Bildes trägt `contentDate` in UTC-`Z`-Form
(`…T00:00:00.000Z`), siehe `docs/PROTOCOL.md` §2.17; ein Node-`GET` auf ein EXIF-haltiges DNG
zeigt dagegen eine **volle** Zeit (`…T10:31:53.401Z`, `docs/PROTOCOL.md` §2.10).

**Beleg.** Negative Symbolsuche (`OffsetTime`, `TimeZone`, `ToUniversalTime` nahe dem Upload-Pfad);
`QueryExtensions.ToUrlEncodedQuery` (der einzige Ort, der Datum je serialisieren würde);
`docs/PROTOCOL.md` §2.10/§2.17 (Live-Mitschnitt, nicht Artefakt).

**Einordnung.** Client-Seite **belegt** (kein Datum, keine Umrechnung, kein Suffix).
Ob der Server die EXIF-Lokalzeit naiv oder mit Offset interpretiert → **UNKNOWN** (Serverlogik,
außerhalb des Artefakts).

---

## 3. Sub-Sekunden (`SubSecTime`)

**Befund.** Kein `SubSecTime` im Artefakt (0 Treffer); der Client hat keine Stellenzahl-Logik, weil
er kein Datum baut. Der frühere Befund, dass unsere Crate `kamadak-exif-0.6.1` die Sub-Sekunde
nur über `parse_subsec` setzt und **kein** Reader-Aufruf das aufruft, betrifft **unseren** Code und
ist hier nur die Kontrastfolie — der Original-Client hat dieselbe Frage anders beantwortet: er
stellt sie gar nicht.

**Beleg.** Negative Symbolsuche (`SubSecTime`, `Subsec`, `nanosecond`); `docs/SPEC_UPLOAD.md` §2.4
(unser Stand).

**Einordnung.** **belegt** für den Client (keine Sub-Sekunden-Übernahme, weil kein Datum).
Ob/wie der Server Sub-Sekunden übernimmt → **UNKNOWN**.

---

## 4. `fallbackContentDate`

**Befund.** Der Query-Parameter **existiert** — aber nur an **einer** Stelle und **unbenutzt**.

| Eigenschaft | Wert |
|---|---|
| Deklariert in | `Amazon.CloudDrive.Services.MultipartUpload.Model.InitiateMultipartUploadRequest` als `[QueryParameter("fallbackContentDate")]` |
| Ebenfalls dort | `[QueryParameter("contentDate")]` |
| Sichtbar in `UploadRequest` (Simple/Legacy-Upload) | **nein** — dort gibt es nur `contentDate`, **kein** `fallbackContentDate` |
| Zuweisungen im Code | **keine.** Weder der Mehrteil-Konstruktor (Parameter `UploadRecord`, `overwrite`) noch die beiden Aufrufstellen setzen das Feld |
| Folge | Property bleibt `null` ⇒ der Query-Bauer lässt sie weg (§7) ⇒ **wird auf keinem Upload mitgesendet** |

**Beleg.** `InitiateMultipartUploadRequest` (Feld-Deklaration + Konstruktor), die zwei
Konstruktor-Aufrufstellen des Mehrteil-Pfads und die Mehrteil-URL-Bauer; negative Zuweisungssuche
über das gesamte Dekomplilat (`FallbackContentDate = ` = 0 Treffer, `.FallbackContentDate` =
0 Treffer). String-Scan: `fallbackContentDate` + `<FallbackContentDate>k__BackingField` +
`get_FallbackContentDate` nur in `Amazon Photos.dll`. `docs/openapi.yaml` listet
`fallbackContentDate` **nicht** — nur `contentDate` (Beschreibung dort: „Optional; otherwise
derived from EXIF/file time"). Evidence-ID `ev_ae22f6e9…`.

**Einordnung.** Vorhandensein: **belegt**. „Wird auf jedem Upload mitgesendet": **belegt
widerlegt** (wird *nie* gesendet). Wann und mit welchem Wert er *gemeint* war (Dateizeit?
Aufnahmedatum? leer?) → **UNKNOWN**; die Semantik des Namens deckt sich zwar auffällig mit der
beobachteten Mitternachts-Normalisierung (§5), aber ein kausaler Zusammenhang ist **nicht belegt**
und wäre geraten — daher hier nicht behauptet.

---

## 5. Normalisierung (exakter Zeitstempel vs. Mitternacht/Datumsanteil)

**Befund.** Der Client normalisiert **nichts** — weil er nichts sendet (§7). Die beobachtete
Mitternachts-Form ist eine **Antwort**-Erscheinung:

| Beobachtung | Quelle | Wert |
|---|---|---|
| Upload-Antwort eines **EXIF-losen** PNG | `docs/PROTOCOL.md` §2.17 | `contentDate = 2026-10-13T00:00:00.000Z` (Mitternacht) |
| Node-`GET` eines **EXIF-haltigen** DNG | `docs/PROTOCOL.md` §2.10 | `contentDate = 2026-05-08T10:31:53.401Z` (volle Zeit) |

**Schlussfolgerung (aus den zwei Live-Beobachtungen, nicht aus dem Artefakt):** Für ein Bild
**ohne** auswertbares EXIF liefert der Server den **Datumsanteil mit Tagesgranularität**
(Mitternacht UTC) — passend zur `contentDate`-Beschreibung in `docs/openapi.yaml` („otherwise
derived from EXIF/file time"). Für ein Bild **mit** EXIF steht die volle Zeit. Der Client bereitet
davon **nichts** vor.

**Beleg.** Client-Seite: `ToUrlEncodedQuery`/`UploadRequest`/`UploadFile`-URL-Bau (§7) als
einzige mögliche Normalisierungsstelle; `docs/PROTOCOL.md` §2.10/§2.17 und `docs/openapi.yaml`
(fremde Live-/Spez-Belege, ausdrücklich **nicht** Artefakt).

**Einordnung.** Client normalisiert nichts: **belegt**. Die Deutung „Tag-Normalisierung = Fallback
für fehlendes EXIF": **Schlussfolgerung**. Ob die Tagesgranularität vom Datei-Datum, vom
Ordner-Datum oder von „kein EXIF" kommt → **UNKNOWN** (kein Live-Mitschnitt verfügbar).

---

## 6. Sortier-/Anzeigeschlüssel der Client-eigenen Ansicht

**Befund.** In der Assembly existieren **zwei** Sortierschlüssel-Kandidaten, aber nur einer nennt
ein Aufnahme-/Inhaltsdatum:

| Ort | Schlüssel | Status |
|---|---|---|
| `CloudDriveService.CreateSearchKeyUrl` (Such-/Aggregationsaufruf) | `sort = ['contentProperties.contentDate DESC']`, zusätzlich `groupByForTime = year` (Aggregation) | **belegt** als Bauplan; **kein** managed Aufrufer dieser Methode im Assemblilat |
| `nodes`-Tabelle der lokalen SQLite (`NodeRecord`) | Spalte `content_date` (neben `createdDate`, `modifiedDate`, `client_dateCreated`, `client_dateUpdated`) | **belegt** (Spalte existiert, wird aus dem Server-Node gecacht) |
| `PhotoNode` (Memories/Stories-Response) | `dateTaken` | **belegt** deklariert; im Modell `PhotoNode` |
| `ImageProperties.dateTimeOriginal` u. a. | EXIF-Datum aus Server-Sicht | **belegt** deklariert, **kein** Aufrufer |
| Lokale UI-Sortierer | `AlbumMetadata.DateCreated` (Album-Grid), `ScheduledBackupItem.CreatedDate`, `UploadRecord.StatusChanged`, Status + `SortDate` (Transfer-/Backup-Listen) | **belegt** — **keiner** sortiert auf ein Aufnahme-/`contentDate` |

**Beleg.** `CloudDriveService.CreateSearchKeyUrl` (Sort-Ausdruck + `AggregationGroupBy.Year`),
`NodeRecord` (`[Column("content_date")]`), `PhotoNode.DateTaken`, `ImageProperties.dateTimeOriginal`,
`AlbumMetadataViewModel`-Sortierer, `ScheduledBackupItem`-Sortierer, `RecordItem.DefaultComparer`,
`ActivityViewModel.ItemsComparer`; Evidence-ID `ev_ae22f6e9…`.

**Einordnung.**
- **belegt:** Wo die Client-eigene Bild-/Album-Ansicht sortiert, ist der Schlüssel
  `contentProperties.contentDate` — aber als **Such-URL an den Server**, nicht als lokale
  Vergleichslogik; das lokale Bild-Listing selbst ist die BFF-/Web-Seite des Clients
  (`…/photoapp/v3/…`-Pfade) und liegt nicht als managed Vergleich im Assemblilat.
- **belegt:** Der `content_date`-Wert ist im lokalen `nodes`-Cache vorhanden (Server-Wert).
- **UNKNOWN:** ob ein Teil der Online-Galerie zusätzlich nach einem *clientseitig* ermittelten
  Aufnahmedatum gruppiert — kein solcher Vergleich gefunden; Suchgrenze siehe §8.

---

## 7. Der exakte Upload-Request

**Befund.** Das Feld, das das Aufnahmedatum tragen **würde**, heißt `contentDate`
(`[QueryParameter("contentDate")]`). Der Original-Client **befüllt es nicht** und schickt es
somit nicht. Belegt für **alle drei** Upload-Pfade:

| Pfad | Request-Typ | `contentDate` gesetzt? | Codestelle, die den Request baut |
|---|---|---|---|
| Simple (kleine Dateien) | `UploadRequest` | **nein** | `SimpleUploader.UploadFile` — Objekt-Initialisierung setzt `Name`, `FileSize`, `ParentNodeId`, `Kind`, `NodeId`, `Suppress`, `ConflictResolution`, `Method`; **kein** `ContentDate` |
| Legacy/Resumable (große Dateien) | `CloudDriveFileMetadata` (JSON-Body) | Feld existiert dort nicht | `LegacyUploader` über `CloudDriveService.CreateFile`/`OverwriteFile` |
| Mehrteil (`/v2/upload/multipart-upload`) | `InitiateMultipartUploadRequest` | **nein** (`contentDate` **und** `fallbackContentDate` bleiben `null`; Konstruktor setzt nur `Name`, `FileSize`, `ParentNodeId`, `Kind`, `NodeId`, `Method`, `ConflictResolution`, `Suppress`, `Headers`) | `MultipartUploadService.InitiateMultipartUpload` (Konstruktor + beide Aufrufstellen) |

**Der Query-Bauer wirft leere Felder weg.** `QueryExtensions.ToUrlEncodedQuery(UploadRequest)`
iteriert über die `[QueryParameter]`-Properties und hängt eine Property **nur** an, wenn ihr Wert
`!= null` ist. Ein nie gesetztes `ContentDate` fehlt also im Query-String. Für den Mehrteil-Pfad
gilt derselbe `null`-Filter (generischer Overload). Die Ziel-URL lautet im Upload-Fall
`<ResourceVersion>/upload`, d. h. **`v2/upload`** (Put-Fall: `v2/upload/<nodeId>`); `ResourceVersion`
ist konstant `"v2"`.

**Typischer Wert.** Aus dem Client gibt es **keinen** typischen `contentDate`-Wert — das Feld steht
nicht auf der Leitung. Der Wert, den man von außen sieht, ist der **Server**-Wert der Antwort:
`2026-10-13T00:00:00.000Z` (EXIF-los, §5) bzw. volle Zeit bei EXIF (§5). Die
`contentDate`-Beschreibung der Schnittstelle (`docs/openapi.yaml`) nennt das Feld ausdrücklich
optional und server-seitig „otherwise derived from EXIF/file time"; sie kennt **kein**
`fallbackContentDate`.

**Beleg.** `UploadRequest` (Property-Liste), `SimpleUploader.UploadFile` (Objekt-Initialisierung),
`CloudDriveService.UploadFile` (URL-Bau `…/upload`), `QueryExtensions.ToUrlEncodedQuery`,
`InitiateMultipartUploadRequest` + Konstruktor + beide Aufrufstellen + Mehrteil-URL-Bauer,
`CloudDriveFileMetadata` (Feld-Liste); `docs/openapi.yaml` (Zeilen zu `contentDate`).
Evidence-ID `ev_ae22f6e9…`.

**Einordnung.** **belegt.**

---

## 8. Blocker, Suchgrenze und Ausweichroute

**Blocker (exakt, unverändert zitiert):**

- `inspect_managed_artifact`: `classification.evidence[0]` =
  `metadata-not-admitted` / „Managed metadata exceeds the 67108864 byte representation budget",
  `file_offset = 1540426`; `metadata.status = partial`; `coverage.state = partial`.
- `inspect_managed_members`: `coverage.state = unavailable`, gleicher `resource-limit`, plus
  `limitations[0]` = „The CLI metadata could not be admitted; managed member inspection is
  unavailable."
- **Kein** `ilspycmd`-Blocker in dieser Sitzung: der frühere Fehler „You must install .NET to run
  this application" trat **nur** wegen fehlendem `DOTNET_ROOT` auf; mit
  `DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec` lief `ilspycmd` 11.1.0.9782 durch.

**Ausweichroute (genutzt).** Vollständiger C#-Dekompiliert der Assembly in ein temporäres
Verzeichnis; alle Aussagen oben stammen aus gezielten Symbol-/String-Greps über dieses Dekompiliert
plus (für §2/§5) den vorhandenen Live-Notizen `docs/PROTOCOL.md`, `docs/openapi.yaml`. Es wurden
**keine** Live-Zugriffe gefahren.

**Suchgrenze (bewusst gezogen).** Durchsucht wurden: die Assembly `Amazon Photos.dll`
(vollständig) und per String-Scan alle übrigen `Contents/MonoBundle/*.dll`, die native
`amazon_weblab_external_plugin.dylib` und die Haupt-Binary in `Contents/MacOS/` — auf
`exif`, `datetimeoriginal`, `fallbackcontentdate`, `subsectime`, `offsettime`. Treffer nur in
Framework-Bibliotheken, die die App nicht aufruft (§1). **Nicht** durchsucht: die eingebetteten
Web-Ressourcen der BFF-/Galerie-Seite (falls vorhanden) und die Serverlogik — beide außerhalb
dieses Artefakts.

---

## 9. Offene Fragen / was ein Live-Mitschnitt klären müsste

1. **Server-seitige Tag-Priorität.** Welches EXIF-/XMP-Feld der Server nimmt, wenn mehrere
   vorhanden sind (`DateTimeOriginal` vs. `DateTimeDigitized` vs. `DateTime`), und ob er
   `SubSecTime*` / `OffsetTime*` berücksichtigt — außerhalb des Artefakts. *Mitschnitt:* ein
   Upload je Testdatei-Variante, Vergleich der Antwort-`contentDate`.
2. **Fallback bei EXIF-los.** Ob die Mitternachts-`contentDate` aus dem **Datei**-Datum, aus
   `clientProperties.dateCreated` oder aus einem Server-Default stammt. Der Client liefert mit
   `clientProperties.dateCreated/dateUpdated` (Dateisystem-Zeiten) eine Kandidatenquelle, aber
   **nicht** `contentDate` und **nicht** `fallbackContentDate`. *Mitschnitt:* eine EXIF-lose Datei
   mit bewusst abweichender mtime hochladen und die Antwort-`contentDate` gegen beide Zeiten
   stellen.
3. **Zeitzonen-Interpretation.** Ob eine EXIF-Lokalzeit mit `OffsetTime*` umgerechnet wird
   (Antwortformat ist stets `…Z`; die Eingangsinterpretation ist unbelegt).
4. **`fallbackContentDate`-Semantik.** Ob eine **ältere** Client-Version den Parameter gesendet hat
   (in 11.3.0/36 ist er tot) — nur über einen Mitschnitt einer anderen Version oder über die
   Server-Doku zu klären.
5. **Sortierung der Online-Galerie.** `sort=['contentProperties.contentDate DESC']` +
   `groupByForTime=year` ist als Client-Bauplan belegt, hat in dieser Assembly aber **keinen**
   managed Aufrufer — der aufrufende Galerie-Code liegt außerhalb (BFF/Web-Schicht). *Mitschnitt:*
   die tatsächlichen `search`/`search/aggregation`-Requests aus der laufenden App.

---

## 10. Was unser Client heute anders macht

**Getrennt vom Original-Befund; alles hier ist `Schlussfolgerung`** aus unserem Code
(`src/prime_photos.rs` `upload_picture`, `src/urls.rs` `upload_query`/`upload_url`/
`replace_upload_url`) gegen die Befunde §1–§7.

| Aspekt | Original-Client (11.3.0/36) | Unser Client |
|---|---|---|
| Wer ermittelt das Datum | Server | **wir**, vor dem Netz |
| EXIF-Leser | **keiner** | `exif`-Crate (Tag-Priorität `DateTimeDigitized` → `DateTimeOriginal` → `DateTime`, primärer IFD, `Value::Ascii`) |
| `contentDate` auf der Leitung | **nie** (Feld bleibt `null`) | **immer**, als Query-Parameter des POST (und des `Replace`-PUT) |
| Zwischenfall ohne EXIF | Server-Fallback (Tag-Granularität) | Dateizeit, aber nur wenn die Datei **> 10 Minuten** alt ist; sonst **Abbruch** des Uploads |
| Format des gesendeten Werts | — | `YYYY-MM-DDTHH:MM:SS.<n>Z` mit aktuell **naiv lokaler** EXIF-Zeit + `Z` und der Sub-Sekunde aus dem EXIF-`nanosecond`-Feld (mindestens 3 Stellen gepolstert ⇒ bei gesetzter Sub-Sekunde können **mehr als drei** Nachkommastellen entstehen) |
| Zeitzone | — | keine Umrechnung (lokale Zeit wird als `Z` ausgegeben) |

**Wichtig:** Diese Abweichungen sind **kein** Widerspruch zum Original-Befund, sondern dessen
Spiegelbild — das Original überlässt das Feld dem Server, wir besetzen es selbst. Ob der Server
unsere volle Zeit übernimmt oder auf Tagesgranularität kürzt, ist aus dem Artefakt **nicht**
entscheidbar (§5, §9.2). Zwei Punkte sind zusätzlich als **eigener** Code-Fund markiert und
gehören nicht in diese Spec-Korrektur: die naiv-lokale Zeit und die Sub-Sekunden-Überlänge
(vgl. `docs/SPEC_UPLOAD.md` §2.4).

---

## 11. Belege (Symbole, Dateien — keine Codekopien)

* **Managed Static Analysis (REA):** `ev_ae22f6e9713b8d2be36d3d3d8a7157c7c2ffd49ff026fc82c30f3ade5e74fc52`
  (`inspect_managed_artifact`, Identität + `partial`), `ev_79378b0bc34d0144692a4246505ad37f2cf0a158a352f254979dfd0e959ba09d`
  (`inspect_managed_members`, `unavailable`), Provider `rea-dotnet-static` v1.
* **Request-Typen:** `Amazon.CloudDrive.Services.CloudDrive.UploadRequest` (`contentDate`, `ResourceVersion`,
  `Method`), `Amazon.CloudDrive.Services.MultipartUpload.Model.InitiateMultipartUploadRequest`
  (`contentDate`, `fallbackContentDate`), `Amazon.CloudDrive.Services.CloudDrive.CloudDriveFileMetadata`
  (Basis `CloudDriveMetadata`).
* **Uploader:** `SimpleUploader.UploadFile`, `LegacyUploader.UploadFile`, `MultipartUploadService`
  (Initiate/UploadPart/Complete/GetProgress/Abort), `CloudDriveStorageService.UploadViaSimpleOrLegacyUploader`,
  `CloudDriveService.UploadFile`/`CreateFile`/`OverwriteFile`.
* **Query/URL:** `QueryExtensions.ToUrlEncodedQuery` (für `UploadRequest` mit `null`-Filter und
  der generische Overload), `QueryParameterAttribute`, `CloudDriveService.GetUploadUrl`.
* **Datum/Antwort-Modelle:** `ContentProperties` (`contentDate`), `ImageProperties`
  (`dateTime`, `dateTimeDigitized`, `dateTimeOriginal`), `ClientProperties` (`dateCreated`,
  `dateUpdated`), `PhotoNode` (`dateTaken`), `UploadOperation.SetCloudClientProperties`.
* **Lokale Persistenz:** `NodeRecord` (`nodes.content_date`, `createdDate`, `modifiedDate`,
  `client_dateCreated`, `client_dateUpdated`), `UploadRecord` (`created_date`, `modified_date`).
* **Sortierung/Gruppierung:** `CloudDriveService.CreateSearchKeyUrl`
  (`['contentProperties.contentDate DESC']`, `AggregationGroupBy.Year`), `SearchContext`,
  `SortOrder`, `AggregationCategory`, `AlbumMetadataViewModel`-Sortierer, `ScheduledBackupItem`-Sortierer,
  `RecordItem.DefaultComparer`, `ActivityViewModel.ItemsComparer`.
* **Dateisystem-Zeiten:** `IFileSystemObject` (`CreatedDate`, `ModifiedDate`), `FileSystemObject`,
  `LocalFileSystemObject`.
* **Thumbnails (Abgrenzung):** `CGImageSource`/`CreateThumbnail` (ImageIO) — nur Thumbnails.
* **Fremdbelege (nicht Artefakt):** `docs/PROTOCOL.md` §2.10/§2.17, `docs/openapi.yaml`
  (`contentDate`-Beschreibung), `docs/SPEC_UPLOAD.md` §2.4, `docs/SPEC_DEDUP.md` §1.
* **Bundle:** `Contents/Info.plist` (`CFBundleShortVersionString` = `11.3.0`,
  `CFBundleVersion` = `36`, `CommitId` = `1d7941b5`), `Contents/MonoBundle/Amazon Photos.dll`
  (SHA-256 `34a616e7…`).
