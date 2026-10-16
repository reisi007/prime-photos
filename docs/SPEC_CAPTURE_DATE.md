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
stellt sie gar nicht. **Präzisiert (2026-10-16):** weil kein Reader-Aufruf `parse_subsec` erreicht,
blieb das Feld `nanosecond` ungesetzt ⇒ die von uns **früher** gesendete Sub-Sekunde war **immer**
`.000`. Der Server dagegen trägt echte dreistellige Sub-Sekunden (Live-Test, §12.1/§12.2). Der
gesamte EXIF-Pfad und damit auch dieser Wert sind seit `fcbc14d` entfernt — wir senden **kein**
Datum mehr (§10, §12.5; `docs/SPEC_UPLOAD.md` §2.4).

**Beleg.** Negative Symbolsuche (`SubSecTime`, `Subsec`, `nanosecond`); `kamadak-exif-0.6.1`
(`parse_subsec` als einziger Setzer von `nanosecond`, kein Aufrufer); `docs/SPEC_UPLOAD.md` §2.4
(unser Stand).

**Einordnung.** **belegt** für den Client (keine Sub-Sekunden-Übernahme, weil kein Datum).
Ob/wie der Server Sub-Sekunden übernimmt → **beantwortet** (Live-Test §12.2: er übernimmt die
EXIF-Sub-Sekunde aus `SubSecTimeOriginal`; ein gesendeter Wert wird wörtlich genommen).

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

> **Stand 2026-10-16:** Die Punkte 1, 2 (teilweise) und 3 sowie die Projektfrage, ob der
> `Replace`-`PUT` ein `contentDate` braucht bzw. das Knoten-Datum verändert, sind durch den
> Live-Test in **§12** beantwortet. Sie bleiben als Frage stehen, damit sie nicht erneut
> aufgeworfen werden. **Offen** bleiben Punkt 4 (`fallbackContentDate`) und Punkt 5
> (Galerie-Sortierung der BFF).

1. **Server-seitige Tag-Priorität. → beantwortet (§12.2).** Welches EXIF-/XMP-Feld der Server nimmt,
   wenn mehrere vorhanden sind (`DateTimeOriginal` vs. `DateTimeDigitized` vs. `DateTime`), und ob er
   `SubSecTime*` / `OffsetTime*` berücksichtigt — außerhalb des Artefakts. *Mitschnitt:* ein
   Upload je Testdatei-Variante, Vergleich der Antwort-`contentDate`.
   **Ergebnis:** der Server nimmt `DateTimeOriginal` + `SubSecTimeOriginal`; `ModifyDate`, mtime und
   Upload-Zeit waren **nicht** die Quelle, und `OffsetTime*` rechnet er **nicht** um (naiv `Z`).
2. **Fallback bei EXIF-los. → teilweise beantwortet (§12.1/§12.4).** Ob die Mitternachts-`contentDate`
   aus dem **Datei**-Datum, aus `clientProperties.dateCreated` oder aus einem Server-Default stammt.
   Der Client liefert mit `clientProperties.dateCreated/dateUpdated` (Dateisystem-Zeiten) eine
   Kandidatenquelle, aber **nicht** `contentDate` und **nicht** `fallbackContentDate`. *Mitschnitt:*
   eine EXIF-lose Datei mit bewusst abweichender mtime hochladen und die Antwort-`contentDate` gegen
   beide Zeiten stellen.
   **Teilergebnis:** die Mitternachts-Form ist der **EXIF-lose** Fall (§12.4) — **nicht** eine
   allgemeine Normalisierung. Woher genau der Tagesanteil kommt, bleibt offen: der Live-Test führte
   **keinen** EXIF-losen Upload.
3. **Zeitzonen-Interpretation. → beantwortet (§12.2).** Ob eine EXIF-Lokalzeit mit `OffsetTime*`
   umgerechnet wird (Antwortformat ist stets `…Z`; die Eingangsinterpretation ist unbelegt).
   **Ergebnis:** die EXIF-Wanduhrzeit wird **naiv als `Z`** abgelegt, **nicht** mit `OffsetTime*`
   nach UTC umgerechnet (`19:47:36` bleibt `19:47:36Z` bei `OffsetTimeOriginal = +02:00`).
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
| Wer ermittelt das Datum | Server | **Server** — wir ermitteln nichts mehr (seit `fcbc14d`) |
| EXIF-Leser | **keiner** | **keiner** im Upload-Pfad — der `exif`-Lesevorgang ist entfernt |
| `contentDate` auf der Leitung | **nie** (Feld bleibt `null`) | **nie** — weder Query-Parameter des POST noch des `Replace`-PUT |
| Zwischenfall ohne EXIF | Server-Fallback (Tag-Granularität) | **kein** eigener Fallback mehr — der Server entscheidet |
| Format des gesendeten Werts | — | **kein** Wert — es wird nichts mehr formatiert |
| Zeitzone | — | — (kein Datum auf der Leitung) |

**Wichtig — auf diesem Feld kein Widerspruch mehr zum Original:** Unser Client verhält sich
inzwischen wie der Original-Client und überlässt `contentDate` dem Server; die frühere Abweichung
ist mit `fcbc14d` entfallen.

**Historie — wie es vor `fcbc14d` war:** Unser Client ermittelte das Datum selbst — EXIF über die
`exif`-Crate (Tag-Priorität `DateTimeDigitized` → `DateTimeOriginal` → `DateTime`, primärer IFD,
`Value::Ascii`) mit einem **Dateizeit-Rückfall**, wenn kein EXIF lesbar war, und der Regel, den
Upload abzubrechen, wenn die Datei jünger als 10 Minuten war; der Wert ging als `contentDate` mit
(POST und `Replace`-PUT), formatiert als `YYYY-MM-DDTHH:MM:SS.<n>Z` mit **naiv lokaler** EXIF-Zeit +
`Z` und der immer `.000`-Sub-Sekunde. Bei einem Lightroom-Export setzte der Rückfall die
**Export**zeit statt der Aufnahmezeit. **Warum geändert:** der Live-Test §12.2 zeigt, dass ein
**mitgesendeter** `contentDate`-Wert **wörtlich** übernommen wird und die Server-Ableitung
**überschreibt** — unsere eigene, potenziell falsche Ermittlung konnte die richtige
Server-Ableitung also nur verschlechtern. Seit `fcbc14d` („fix(upload): kein contentDate mehr senden
- der Server leitet das Aufnahmedatum ab") sendet unser Client deshalb **keinen** `contentDate` mehr
und überlässt die Ableitung dem Server wie das Original; vertieft in `docs/SPEC_UPLOAD.md` §2.4.

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

---

## 12. Live-Test 2026-10-16: wie `contentDate` tatsächlich entsteht

**Methode.** Eine eigene, in derselben Sitzung aus dem **Refresh-Token** gemintete Cookie-Session
(der Geräteweg aus `docs/SPEC_LOGIN.md` § 3.8) gegen das echte Konto, ausschließlich gegen
Testordner/-knoten, die anschließend gelöscht wurden. Schreibtests liefen gegen
`POST content-<zone>.drive.amazonaws.com/v2/upload` bzw. `PUT /v2/upload/<nodeId>`; der lesende
Test gegen `…/drive/v1/nodes/<folder>/children`. **Keine Live-Werte** (keine Cookies, keine
Node-IDs, keine Kontonamen) werden hier übernommen; Testdateinamen wie `_pptest_a.jpg` bleiben.

**Trennregel für diesen Abschnitt.** *Beobachtet* = HTTP-Status und Antwortfelder; *Schluss-
folgerung* = die Deutung, **welche** Quelle der Server benutzt. Was der Test nicht berührt, bleibt
ausdrücklich offen (`UNKNOWN`).

### 12.1 Test 1 — lesender `GET` auf den Upload-Ordner (beobachtet)

`GET …/drive/v1/nodes/<folder>/children` auf unseren eigenen Upload-Ordner. Die Knoten tragen
`contentProperties.contentDate` mit **voller** Aufnahmezeit **und** echten **dreistelligen**
Sub-Sekunden. Belegte Beispielwerte aus dem Bestand:

| `contentDate` (beobachtet) |
|---|
| `2025-05-31T19:42:30.083Z` |
| `2025-03-02T18:05:11.002Z` |
| `2025-06-10T20:04:31.046Z` |

Keiner dieser Werte steht auf Mitternacht, und die Sub-Sekunde ist echt (nicht `.000`).

**Schlussfolgerung.** Für Bilder **mit** auswertbarem EXIF steht im Konto die **volle**
Aufnahmezeit samt Sub-Sekunde — es gibt **keine** verallgemeinerbare Mitternachts-Normalisierung
(vgl. §12.4).

### 12.2 Test 2 — A/B gegen `POST /v2/upload` (beobachtet)

Zwei synthetische JPEGs mit **identischem** EXIF, bewusst **abweichendem** `ModifyDate` und
verschiedenen Bytes (`_pptest_a.jpg`, `_pptest_b.jpg`):

| EXIF-Feld | Wert |
|---|---|
| `DateTimeOriginal` | `2026:10:13 19:47:36` |
| `SubSecTimeOriginal` | `123` |
| `ModifyDate` | `2026:10:20 08:00:00` (bewusst abweichend) |
| `OffsetTimeOriginal` | `+02:00` |

| Test | Query | HTTP | Antwort-`contentDate` |
|---|---|---|---|
| A | **ohne** `contentDate` | **201** | `2026-10-13T19:47:36.123Z` |
| B | mit `contentDate=2026-10-13T19:47:36.000Z` | **201** | `2026-10-13T19:47:36.000Z` |

Weitere beobachtete Antwortfelder bei A: `createdDate`/`modifiedDate` = das heutige
Upload-Datum (`2026-10-16T09:02:…Z`) — **nicht** das, was in `contentDate` steht.

**Schlussfolgerung (A).** Ohne gesendeten Wert leitet der Server `contentDate` aus dem
**`DateTimeOriginal` + `SubSecTimeOriginal`** der Datei ab. Er nahm **nicht** `ModifyDate`
(`2026-10-20`), **nicht** die Datei-mtime (heute) und **nicht** die Upload-Zeit
(`createdDate`/`modifiedDate`).

**Schlussfolgerung (B).** Ein **mitgesendeter** `contentDate`-Wert wird **wörtlich übernommen** und
überschreibt die Server-Ableitung — die Sub-Sekunde bleibt `.000`, obwohl die Datei `.123` trägt.

**Zeitzone (beobachtet + Schlussfolgerung).** Der Server legt die EXIF-**Wanduhrzeit** naiv als `Z`
ab: `19:47:36` bleibt `19:47:36Z`, obwohl die Datei `OffsetTimeOriginal = +02:00` trägt. Er rechnet
**nicht** nach UTC um.

### 12.3 Test 3 — der `PUT`-Weg (`Replace`) (beobachtet)

| # | Aufruf | Ergebnis |
|---|---|---|
| 1 | `POST /v2/upload` **ohne** `contentDate` ⇒ Knoten C | **201**, `contentDate = 2026-10-13T19:47:36.123Z` |
| 2 | `POST /v2/upload` mit **gleichem Namen**, andere Bytes | **409** `NameAlreadyExists`; `errorDetails.conflictNodeIds[0]` benennt C |
| 3 | `PUT /v2/upload/<C>` mit dem POST-Query-Satz **ohne** `contentDate` | **200**, **gleiche** Node-Id; Inhalt nachweislich getauscht (Größe **und** MD5 der zweiten Datei), `contentDate` **unverändert** `…36.123Z` (auch nach 3 s unabhängig nachgelesen) |
| 4 | Aufräumen: `DELETE …/drive/v1/nodes/<id>` je Testknoten | **202**; eine Nachkontrolle über die `children`-Abfrage findet keinen der Testnamen mehr |

**Schlussfolgerung.** Der `PUT` funktioniert **ohne** `contentDate` und lässt das Datum des Knotens
**stehen** — die Ersetzung ist ein reiner Inhalts-Tausch, kein neues Aufnahmedatum. (Damit ist die
seit Welle 2 offene Live-Abnahme des `Replace`-PUT geführt; Beleg in
`docs/features/LIVE_ACCEPTANCE.md`.)

### 12.4 Korrektur eines früheren Befunds (§5, `docs/PROTOCOL.md` §2.17)

`docs/PROTOCOL.md` §2.17 hielt fest, die Server-Antwort normalisiere `contentDate` „auf Mitternacht
des Dateidatums". Das ist **nur der EXIF-lose Fall** — die damalige Beobachtung war ein **PNG ohne**
EXIF. Mit auswertbarem EXIF liefert der Server die **volle** Aufnahmezeit samt Sub-Sekunden
(Test 1 + 2). Die Mitternachts-Aussage bleibt für **EXIF-lose** Dateien gültig, ist aber **keine**
allgemeine Normalisierung.

### 12.5 Was das für unseren Client bedeutet (geliefert in `fcbc14d`)

Die Umsetzung ist **geliefert und unabhängig verifiziert** (Commit `fcbc14d`, „fix(upload): kein
contentDate mehr senden - der Server leitet das Aufnahmedatum ab"); dokumentiert in
`docs/SPEC_UPLOAD.md` §2.4. Unser Client sendet `contentDate` **gar nicht mehr** und überlässt die
Ableitung dem Server. Was der Live-Test dafür stützt:

* **Ein gesendeter Wert gewinnt wörtlich** (Test 2 B): unser eigenes Datum hat die Server-Ableitung
  also nur deshalb überschrieben, weil wir es überhaupt gesendet haben.
* **Unsere Sub-Sekunde war immer `.000`.** Wir lesen sie über `kamadak-exif-0.6.1`; das Feld
  `nanosecond` wird dort nur in `parse_subsec` gesetzt, und **kein** Reader-Aufruf ruft
  `parse_subsec` auf (belegt im Crate-Quelltext, vgl. §3). Der Server dagegen übernimmt die echte
  Sub-Sekunde (Test 2 A, `.123`).
* **Zwei Schwachstellen unserer eigenen Ermittlung:** (a) Priorität `DateTimeDigitized` **vor**
  `DateTimeOriginal` — der Server nutzt `DateTimeOriginal`; (b) **Rückfall auf die Dateizeit**, wenn
  kein EXIF gelesen werden kann — bei einem Lightroom-Export ist das die **Export**zeit, nicht die
  Aufnahmezeit.
