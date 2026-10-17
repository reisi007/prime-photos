# Release Notes nach einem Update — Clean-Room-Spec (reverse-engineered)

Scope: **wie die Original-App *Amazon Photos* (macOS) nach einem Update „Release Notes" / „Was ist neu"
zeigt** — Auslöser, Speicherort des Merkers, Ablauf beim Start, das Fenster und sein Inhalt sowie die
Grenzfälle. Reine Beobachtung des ausgelieferten Artefakts in eigenen Worten; **keine** Code-Auszüge,
keine Personendaten. Sichtbare Strings des Originals sind wörtlich zitiert und als **Beobachtung**
gekennzeichnet.

**Belegtes Ziel-Artefakt (gelesen 2026-10-17, ausschließlich lesend):**
`/Applications/Amazon Photos.app` — `CFBundleShortVersionString = 11.3.0`, `CFBundleVersion = 36`,
`CommitId = 1d7941b5`, `CFBundleIdentifier = com.amazon.clouddrive.mac`,
`LSMinimumSystemVersion = 10.15`.
Managed Assembly: `Contents/MonoBundle/Amazon Photos.dll` — 4.591.616 B,
MD5 `7a9cd34f72be0973047ea88034afd15f`, Assembly-Attribut `AssemblyVersion("0.0.0.0")`
(die App-Version kommt aus der `Info.plist`, nicht aus der Assembly).

**Methode:** `ilspycmd 11.1.0.9782` (`ICSharpCode.Decompiler 11.1.0.9782`) — 2026-10-17 wurden die
vier Kern-Typen **gezielt und neu** dekompiliert (`-t <Typ>`) und gegen den vorhandenen
Voll-Decompil-Dump (`amzdec/`, 2026-10-16) gediffed: **identisch** (Abweichung nur in der
`base._002Ector()`-Schreibweise zwischen Projekt- und `-t`-Modus). Zusätzlich: `plutil -p` (Info.plist),
`strings` über die Assembly, `sips` (Icon-Maße), `grep` über `Resources/*.lproj/Localizable.strings`,
und das **Auswerten bereits vorhandener Mitschnitte** unter
`/private/var/folders/vq/db6hdqsx32b1xg4z2mnzt8nm0000gn/T/opencode/` (`amazon-photos.mitm`,
`amazon2.mitm`, `upload-capture.mitm`) — **kein neues Capture**. Die REA-Tools wurden **nicht**
eingesetzt; für managed .NET ist `ilspycmd` der Weg (vgl. `SPEC_FILE_IDENTITY.md`, Vermerk zu REA).

**Datenschutz-Hinweis:** Keine Personendaten. Die unten genannte Konto-Datei enthält nur einen
Zähler; ein konkreter Kundenwert wurde nicht ausgelesen.

---

## 1. Abgrenzung: was hier neu ist (und was in anderen Specs steht)

| Thema | Steht schon in | Rolle dieses Dokuments |
|---|---|---|
| Release-Notes-**Fenster** (Größe 370 × 382, CloudFront-URL, Sprach-Whitelist, Fußzeile, Menüpfade) | `SPEC_ABOUT.md` **§6** | hier **vertieft**: Merker, Auslöser-Logik, Reihenfolge, Inhalt, Grenzfälle |
| **Auto-Update** (Check, Manifeste, Eligibility, Installer, Dialoge) | `SPEC_PREFERENCES.md` **§3.10** | hier nur **verwiesen**, nicht wiederholt (§9) |
| Start-Reihenfolge der Tasks | `SPEC_SYNC.md` **§3.9** | hier der **Notes-relevante Ausschnitt** (§3) |
| Fenstergrößen/Screens | `UI_FLOWS.md` **§1 (#16)**, **§2.1** | Bestätigung + Widerspruch (§4.3) |
| Konto-`settings.json`-Felder | `UI_STATES.md` **§4.5** | `ReleaseNotesVersion` als **Zähler** präzisiert (§2) |

**Neu hier** (in den obigen Specs **nicht**): der Auslöser ist **nicht** ein Versionsvergleich, sondern
ein **build-fester Zähler** gegen einen gespeicherten Wert; der **Speicherort/-key** samt Default **0**;
die **drei** möglichen Auslöse-Bedingungen (Zähler, Feature-Content, keiner) inklusive der Gate-Rolle;
der **Ausschluss** der Update- und Autostart-Starts; das **Schreiben des Merkers vor der Anzeige**; der
**tatsächlich ausgelieferte Seiteninhalt** (Mitschnitt) und die JS-Filterung; der Reset beim Abmelden.

---

## 2. Auslöser und Speicherort — „wir wurden aktualisiert" ist **kein** Versionsvergleich

Das Original prüft **nicht**, ob die App-Version sich geändert hat. Es vergleicht einen
**im Build fest verdrahteten Zähler** mit einem **pro Konto gespeicherten Zähler**:

| Bestandteil | Wert / Ort | Beleg |
|---|---|---|
| Soll-Wert (im Code) | `CustomerSettings.CurrentReleaseNotesVersion = 19` | `Amazon.CloudDrive.Infrastructure/CustomerSettings.cs:23` |
| Ist-Wert (persistiert) | `ReleaseNotesVersion`, **Default 0**, `AutoGet(0, "ReleaseNotesVersion")` | `CustomerSettings.cs:341-352` |
| Speicherort | `~/Library/Application Support/Amazon Cloud Drive/<CustomerId>-settings.json` | `Constants.cs:541` (`LocalStoragePath`) + `CustomerInfoExtensions.cs:136,144-154` (`<CustomerId>-settings.json`) |
| Vergleich | `customerSettings.ReleaseNotesVersion != 19` | `StartupTasks/ShowReleaseNotesWindowTask.cs:56` |
| Fortschreibung | `SaveReleaseNotesVersion()` setzt den Wert auf das Literal **19** | `UI.ReleaseNotesWindow/ReleaseNotesViewModel.cs:45-52` |

**Kernbefund:** Dies ist ein **Generations-Zähler**, kein Versionsstring-Vergleich. Ein Update löst die
Notes nur aus, wenn der **neue Build einen anderen Wert** für `CurrentReleaseNotesVersion` trägt als der
zuletzt gespeicherte. Bleibt die Konstante gleich (z. B. 19 → 19), erscheint **nichts**, obwohl die
App-Version sich geändert hat. Umgekehrt erscheint die Anzeige auch **ohne** Update, wenn der Zähler
nicht 19 ist (Erstinstallation: fehlender Key ⇒ 0). → **belegt**

**Es gibt keinen „letzte Version"-Key.** Eine Suche nach `LastVersion` / `PreviousVersion` /
`LastRunVersion` / `WasUpdated` / `JustUpdated` / `VersionSeen` über die dekompilierte Assembly liefert
**keinen** Treffer; der **einzige** „gesehen"-Merker im Umfeld ist `ReleaseNotesVersion`. Der
Startup-Quelle `AppStartupFlags.Updated` (siehe §3.2) wird **kein** eigener Merker zugeordnet.
→ **belegt** (Negativbefund)

**Nebenbefund:** `AppStartupFlags` kennt zwar einen Wert `Updated`, aber der Merker hängt **nicht** an
ihm (`AppStartupFlags.cs:7`). Der Aktualisierungs-Relaunch selbst zeigt also nichts (§3.2).

---

## 3. Ablauf beim Start

### 3.1 Position im Start

Der Notes-Task läuft **unmittelbar nach** dem Öffnen des Hauptfensters:

`… → ShowFtueStartupTask → ShowSyncFolderStartupTask → ShowDriveWindowStartupTask →
ShowReleaseNotesWindowTask → StartupCompleteTask → …`
(`StartupTasks/StartupTaskManager.cs:89-94`) → **belegt**

Für den „externen" Pfad (App lief schon, z. B. über Verknüpfung/Dock) existiert die kürzere Kette
`ExternalStartupTaskManager` (6 Tasks, `ShowReleaseNotesWindowTask` an letzter Stelle)
→ **belegt** (`StartupTasks/ExternalStartupTaskManager.cs:8-16`)

### 3.2 Vorbedingungen (alle müssen erfüllt sein)

| # | Bedingung | Beleg |
|---|---|---|
| 1 | Netz verfügbar (`INetworkDetector.IsAvailable`) | `ShowReleaseNotesWindowTask.cs:47` |
| 2 | **benutzer-initiierter** Start (`ArgsFilter.IsUserInitiatedStartup`) | `Helpers/ArgsFilter.cs:12-22,39-46` |
| 3 | Hauptfenster-Startbildschirm ist **`Screen.Backup`** (Default) | `ShowReleaseNotesWindowTask.cs:47`, `UI.DriveWindow/DriveWindowParams.cs:15` |
| 4 | `ICustomerSettings` vorhanden (angemeldet / Settings-DB verfügbar) | `ShowReleaseNotesWindowTask.cs:51-55` |

Zu (2): benutzer-initiiert sind nur `None, Installed, StartMenu, Desktop, AutoPlay, TrayActive,
TrayInactive, RestartFromSignout`. **Nicht** enthalten und damit **ausgeschlossen** sind
**`Updated`** (Aktualisierungs-Relaunch), **`AutoStart`** (Login-Item), `Restart`, `Testing`
→ **belegt** (`Helpers/ArgsFilter.cs:12-22`; Enum in `Amazon.CloudDrive/AppStartupFlags.cs:3-17`).
Die Flags kommen als `--source-<name>` auf der Kommandozeile (`StartupTasks/StartupContext.cs:140-150`);
`Updated` wird intern auf `SessionStartSource.UpgradeRestart` abgebildet
(`Amazon.CloudDrive/App.cs:433`) → **belegt** für die Existenz/Bedeutung, **UNKNOWN** dafür, ob der
Stub-Installer `--source-updated` tatsächlich übergibt (seine Argumente kommen aus dem Update-Manifest,
`SPEC_PREFERENCES.md` §3.10 Schritt 2/7).

**Praktische Folge:** Nach einem Update, das den Relaunch mit `Updated` macht, erscheint die Anzeige
**nicht sofort**; sie erscheint beim **nächsten** benutzer-initiierten Start (Dock/Programme/Desktop/
Tray-Öffnen). → **inferred** (Code-Lage klar, kein Lauf beobachtet)

### 3.3 Die zwei Auslöse-Bedingungen (ODER-verknüpft)

`InternalShouldRun` liefert `ReleaseNotesVersion != 19` **oder** – nur wenn das Feature-Gate
`EnableNewReleaseNotes` **aktiv** ist – das Ergebnis von `FeatureGates.ShouldShowReleaseNotesWindow()`:
→ **belegt** (`ShowReleaseNotesWindowTask.cs:56-64`)

| Bedingung | Prüft | Wirkung |
|---|---|---|
| **A – Zähler** | `ReleaseNotesVersion != 19` | zeigt die Anzeige (unabhängig vom Gate) |
| **B – Feature-Content** | lädt die Notes-Seite (mit `activeFeatures=none`) und prüft, ob darin ein **für dieses Gerät gerade `Active`** geschaltetes Feature erwähnt wird, das laut gespeicherter `FeatureGates`-Liste noch nicht `Active` war | zeigt die Anzeige **zusätzlich** (nur bei aktivem Gate) |

Zu (B): `Helpers/FeatureGateManager.cs:149-173` (`ShouldShowReleaseNotesWindow`),
`:102-107` (`GetActiveFeaturesForUrl`), `:109-129` (`IsFeatureGateEnabled` = Treatment `T1`/`T2`),
`:206-221` (`SaveActiveFeatureGates`, das die aktuell aktiven Gates in die Konto-Settings schreibt).
Die Menge der Gates ist **reflexiv** aus allen `RemoteExperiment`-Properties der App-Settings
(`FeatureGateManager.cs:66-77`). → **belegt**

**Kuriosum (latenter Aufruf):** Ist das Gate **aus**, ruft der Code
`ShouldShowReleaseNotesWindow()` trotzdem auf und **verwirft** das Ergebnis (`ShowReleaseNotesWindowTask.cs:57-61`).
Dieser Aufruf endet in einem **synchron blockierenden** HTTP-GET (`GetReleaseNotesContent()` →
`_httpClient.GetStringAsync(...).Result`, `FeatureGateManager.cs:175-187`). → **belegt**
(Auswirkung/Dauer nicht gemessen → **UNKNOWN**)

**Einfluss einer Einstellung:** Es gibt **keine** benutzerseitige Option zu den Release Notes (kein
Schalter „beim Start nach Updates suchen", kein Beta-Kanal). Das Gate `EnableNewReleaseNotes` ist
**server-getrieben** (`RemoteExperiment`, Default im Code unbelegt zugewiesen → siehe §9).
→ **belegt** (Negativbefund: keine Einstellung)

### 3.4 Merker wird **vor** der Anzeige geschrieben

`InternalShow()` ruft zuerst `ViewModel.SaveReleaseNotesVersion()` und **danach** Frame/Anzeige:
→ **belegt** (`UI.ReleaseNotesWindow/ReleaseNotesWindowService.cs:59-85`, Aufruf in Zeile 76).
Der Wert steht also auf 19, **sobald das Fenster gezeigt wurde** — auch wenn es sofort geschlossen
wird. „Gesehen" heißt im Original faktisch **„angezeigt"**. → **belegt**

### 3.5 Verhältnis zum Update-Check

Kein mechanischer Zusammenhang. Der Auto-Updater (`AppUpdaterWorker`, `SPEC_PREFERENCES.md` §3.10)
installiert still ~30 min nach Launch bzw. nach Benachrichtigung; der Installer startet die App neu.
Der Notes-Task kennt den Updater nicht und prüft keinen Update-Zustand — er liest nur den Zähler (§2).
→ **belegt** (Negativbefund: keine Kopplung)

---

## 4. Das Fenster

### 4.1 Fenster und Präsentation

| Aspekt | Beobachtung | Beleg |
|---|---|---|
| Typ | eigenes `NSWindow`, **kein** Sheet, **nicht** modal | `ReleaseNotesWindowService.cs:65-73` |
| StyleMask | `Titled \| Closable` | ebd. Zeile 71 |
| Titel | `clouddrive_desktop_title` = **„Amazon Photos"** | ebd. Zeile 69 |
| Größe | `SetFrame(new CGRect(0,0,370,382))`, danach `Center()` | ebd. Zeile 78-79 |
| Vordergrund | `OrderFrontRegardless()` + `MakeKeyWindow()` auf der Main-Queue | ebd. Zeile 80-84 |
| Instanz | einmalig erzeugt (`WindowController/ViewController/ViewModel == null` ⇒ neu); erneutes `Show()` nutzt dieselbe Instanz | ebd. Zeile 61-75 |
| Schließen | `InternalClose()` → `WindowController?.Close()`; Beobachter auf `WillCloseNotification` wird entfernt | ebd. Zeile 87-97 |

### 4.2 Inhalt (Web-View + Fußzeile)

Der Inhalt ist ein **eingebetteter Web-View** (Klasse `ReleaseNotesView : WebKit.WebView`), kein
gebündelter Text:

| Element | Beobachtung | Beleg |
|---|---|---|
| Lade-Spinner | `NSProgressIndicator`, unbestimmt, zentriert, läuft bis `CommitedLoad` | `UI.ReleaseNotesWindow/ReleaseNotesView.cs:66-73,85,88-91` |
| Fußzeile | Container **40 pt** hoch, Hintergrund = UI-Hintergrundfarbe | ebd. Zeile 45,77-84,113-116 |
| Trennlinie | 1 pt, über der Fußzeile | ebd. Zeile 74,109-112 |
| Schließen-Button | `CapsuleButtonTertiary` mit Text `clouddrive_desktop_releasenotes_close` (de: **„Schließen"**), Identifier `ReleaseNotesView_Close_Button`, rechts, 10 pt Abstand | ebd. Zeile 75-76,107-108 |
| Klick | publiziert `CloseReleaseNotesWindowIntent` → Handler ruft `Close()` | ebd. Zeile 122-125; `UI.IntentHandlers/CloseReleaseNotesWindowIntentHandler.cs:16-19` |
| Nach dem Laden | Spinner stoppt, Scroll-Content-Inset unten 40 pt, Overlay-Scroller | ebd. Zeile 89-94 |
| Request | `NSUrlRequest(uri, ReloadRevalidatingCacheData, 30.0)` ⇒ **30 s Timeout**, Cache-Policy „revalidierend" | ebd. Zeile 95-96 |

**Der Inhalt kommt remote, nicht gebündelt.** Belege dafür: (a) die Ansicht lädt ausschließlich die
`ReleaseNotesUri`; (b) im Bundle existiert **keine** Release-Notes-Datei — eine Dateinamensuche nach
`*release*`/`*notes*` liefert **nur** das Menü-Icon `ic-notes-grey-16@1x/2x/3x.png` (15/30/45 px);
(c) `strings` über die Assembly kennt **keinen** „what's new"-/„changelog"-String, nur die
Schlüssel `clouddrive_desktop_contextmenu_releasenotes` und `clouddrive_desktop_releasenotes_close`
→ **belegt**.

### 4.3 Größen-Widerspruch (offengelegt, wie bei About)

| Quelle | Wert |
|---|---|
| fenster-setzender Frame (`ReleaseNotesWindowService.InternalShow`) | **370 × 382 pt** |
| Auto-Layout-Constraints der Content-View (`ReleaseNotesView.CreateConstraints`) | **640 × 570 pt** |

`UI_FLOWS.md` §2.1 nennt 370 × 382 (passt zum `SetFrame`), der View fordert aber 640 × 570. Welche
Größe macOS tatsächlich annimmt (ob die Content-Constraints den Frame treiben), ist **nicht beobachtet**
→ **UNKNOWN**. Dieselbe Widerspruchs-Klasse wie beim About-Fenster (`SPEC_ABOUT.md` §2.2). → **belegt**
(beide Zahlen)

### 4.4 Menüpfade und Ikonen

Zwei Wege öffnen das Fenster; beide senden `ShowReleaseNotesWindowIntent("ApplicationMenu")` und setzen
die Metrik `OverflowMenuClick … "ReleaseNotes"`: → **belegt**
(`Services.MenuBar/MenuBarController.cs:157-164,201` für das Statusleisten-/Overflow-Menü;
`Amazon.CloudDrive/…` App-Menü, siehe `SPEC_ABOUT.md` §5.2)

- Text: `clouddrive_desktop_contextmenu_releasenotes` (de **„Versionshinweise"**), Icon
  `ic-notes-grey-16` (13 pt dargestellt), Identifier `ProfileContextMenu_ReleaseNotes_Button`
  → **belegt**
- **Kein** Tastenkürzel (Details `SPEC_ABOUT.md` §5.2). → **belegt**
- Im Statusleisten-Menü ist der Eintrag **netzabhängig gesperrt**:
  `_releaseNotesMainMenuItem.Enabled = _appStatus.IsNetworkAvailable` → **belegt**
  (`MenuBarController.cs:341`)
- Der sichtbare Handler `ShowReleaseNotesWindowIntentHandler` ruft `_releaseNotesWindowService.Show()`
  (und zählt die Metrik) → **belegt** (`UI.IntentHandlers/ShowReleaseNotesWindowIntentHandler.cs:22-27`)

**Erneut zeigbar:** ja, über das Menü (Fenster wird wiederverwendet). Ein **automatisches** erneutes
Zeigen gibt es nicht, weil der Zähler nach dem ersten Anzeigen 19 ist (§3.4). → **belegt**

---

## 5. Was remote tatsächlich ausgeliefert wird (Befund aus den Mitschnitten)

Belege aus dem vorhandenen Mitschnitt (`amazon-photos.mitm`, gelesen 2026-10-17; gleicher Inhalt in
`amazon2.mitm`, `upload-capture.mitm`):

- **Angefragt** wurde: `GET https://d29x207vrinatv.cloudfront.net/mac/releasenotes/release_notes_en.html?currentAppVersion=11.3.0&activeFeatures=none`
  → der **alte** Bucket-Host, d. h. das Gate `EnableNewReleaseNotes` war zur Aufnahmezeit **aus**.
  → **belegt**
- `Accept-Language: de-DE`, das Sprachsegment ist aber **`en`** — die Sprache folgt dem Amazon-Culture
  der App, **nicht** dem HTTP-Header. → **belegt** (deckt sich mit dem Code, §5.2)
- Der Body ist eine **einzige HTML-Seite** mit einer Liste von Versions-Blöcken. Struktur (beobachtet,
  in eigenen Worten): Wurzel `<div id="container">`; je Version ein
  `<div appVersion="X.Y.Z" class="versionContainer">` mit `<div class="version">Version X.Y.Z</div>`,
  `<div class="date">…</div>`, einer `<ul><li>…</li></ul>`-Liste und einem `<div class="hline"></div>`;
  im `<head>` Verweise auf `release_notes.css` und `release_notes.js`; `<body onload="hideReleaseNotesForUnReleasedVersions()">`
  → **belegt** (Mitschnitt)
- Beispiel-Struktur der Sichttexte (Original, gekürzt zitiert, Beobachtung):
  „Version 10.13.0", „Jul 28 2025", „General bug fixes and improvements."
  → **belegt**
- **Aufnahmezeit-Realität:** Der höchste Eintrag der ausgelieferten Seite war **10.13.0**, während die
  installierte App **11.3.0** war — es gab also **keinen** Eintrag zur laufenden Version.
  Die Seite wird unabhängig vom App-Release gepflegt. → **belegt** (Momentaufnahme 2026-10-13)
- **Client-JS:** `release_notes.js` definiert
  `hideReleaseNotesForUnReleasedVersions()`: liest `currentAppVersion` aus der Query; ist der Wert
  `null`, `""` oder `"0.0.0"`, passiert **nichts**; sonst werden alle `.versionContainer` mit
  `appVersion` **größer** als `currentAppVersion` per `style.display = "none"` versteckt
  (Vergleich über `compareVersions`). → **belegt** (Mitschnitt)
- **Folge für die Anzeige:** Das Fenster zeigt **alle** Einträge bis einschließlich der laufenden
  Version — **kein** Delta „seit deiner letzten Version". Ein Sprung über mehrere Versionen liefert
  daher die komplette Liste der dazwischenliegenden Einträge (und alle älteren). → **inferred**
  (JS-Lage eindeutig; Fensterhöhe/Scrollverhalten nicht beobachtet)

> Ein **Capture ist Evidenz, kein Beweis für den heutigen Server**; der Inhalt kann sich jederzeit
> ändern. Es wurde **kein** neues Capture erstellt (Auftragsgrenze).

### 5.1 URL-Bau (belegt)

| Gate `EnableNewReleaseNotes` | Host | Query `activeFeatures` |
|---|---|---|
| **aus** | `https://d29x207vrinatv.cloudfront.net` (`Constants.ReleaseNotesUrlFormat`) | `"none"` |
| **an** | `https://d180lfws7k02gz.cloudfront.net` (`…FromPipelineBucket`, `ReleaseNotesEndpoint`) | Liste der aktiven Gates (`GetActiveFeaturesForUrl`, komma-separiert) |

Format jeweils: `/mac/releasenotes/release_notes_{lang}.html?currentAppVersion={AssemblyVersion}&activeFeatures={…}`
→ **belegt** (`Amazon.CloudDrive/Constants.cs:563,565,653`;
`UI.ReleaseNotesWindow/ReleaseNotesViewModel.cs:41-42`)

### 5.2 Sprache und Versionsparameter (belegt)

- `currentAppVersion` = **`IAppInfo.AssemblyVersion`** = `CFBundleShortVersionString` = **11.3.0**
  (**ohne** `CommitId`, **ohne** `CFBundleVersion`) (`Infrastructure/AppInfo.cs:33`;
  vgl. `SPEC_ABOUT.md` §3).
- `{lang}` = ersten zwei Zeichen der `AmazonCulture`, **nur** wenn in
  `Constants.ReleaseNotesLanuageCodes = { de, en, es, fr, it, ja }`, sonst `en`
  (`Constants.cs:617`; `ReleaseNotesViewModel.cs:54-70`).

---

## 6. Erstinstallation vs. Update vs. Sprung über mehrere Versionen

Wegen §2 ist der auslösende Zustand immer derselbe — **der gespeicherte Zähler ≠ 19** —, nur sein
Zustandekommen unterscheidet sich:

| Fall | Zustand des Merkers | Ergebnis | Beleg-Lage |
|---|---|---|---|
| **Erstinstallation** | `settings.json` existiert nicht / Key fehlt ⇒ `AutoGet` liefert **0** | 0 ≠ 19 ⇒ Anzeige **einmal** (nach Onboarding, sofern Netz + benutzer-initiiert + Startbildschirm Backup) | **belegt** (Default 0, §2); **UNKNOWN**: praktische Interleaving mit dem FTUE-Fenster (beide Tasks laufen in derselben Kette, Notes direkt nach dem Hauptfenster) |
| **Update mit neuem Zähler** | gespeichert ist der Wert des **vorigen** Builds (≠ 19) | Anzeige **einmal** beim nächsten benutzer-initiierten Start | **inferred** |
| **Update mit gleichem Zähler** | gespeichert = 19 | **keine** Anzeige | **inferred** (Logik belegt, Konstante ist build-fest) |
| **Sprung über mehrere Versionen** | Zähler ≠ 19 | Anzeige zeigt **alle** Einträge ≤ laufende Version (**kein** Delta) | **inferred** (§5, JS) |
| **Autostart / Update-Relaunch** | unverändert (≠ 19 möglich) | **keine** Anzeige (Task läuft wegen §3.2 nicht) | **belegt** (`ArgsFilter.cs:12-22`) |
| **Abmelden und neu anmelden** | `settings.json` wird beim Sign-out **gelöscht** ⇒ Zähler **0** | Anzeige erscheint **erneut** | **belegt** (`Infrastructure/CustomerSignOut.cs:106`; `RestartFromSignout` ist benutzer-initiiert, `ArgsFilter.cs:21`) |

---

## 7. Grenzfälle

| Fall | Verhalten (belegt/inferred) | Beleg |
|---|---|---|
| **Kein Netz beim Start** | Bedingung 1 (§3.2) schlägt fehl ⇒ **kein** Task-Lauf; der Menüeintrag ist **deaktiviert**. Der Zähler bleibt unangetastet ⇒ **nicht verloren**, nächster benutzer-initiierter Start mit Netz zeigt die Anzeige | `ShowReleaseNotesWindowTask.cs:47`; `MenuBarController.cs:341` |
| **Notes-Seite nicht abrufbar / Fehlerantwort** | Der Merker-Pfad **A** zeigt das Fenster trotzdem; der Web-View kann nicht laden ⇒ Spinner läuft weiter (kein `CommitedLoad`), Seite bleibt leer. Der Zähler wurde **bereits gesetzt** ⇒ keine automatische Wiederholung in diesem Build | Laden/Speichern belegt; „leeres Fenster + Dauer-Spinner" **inferred** (kein Fehlerpfad im View) |
| **Gate aktiv, aber Seite ohne passende Feature-Erwähnung und Zähler = 19** | beide Bedingungen falsch ⇒ keine Anzeige | `ShowReleaseNotesWindowTask.cs:56-64` |
| **Feature-Content-Check: Seite nicht ladbar** | `GetReleaseNotesContent()` fängt jede Exception und liefert `null` ⇒ Bedingung **B** wird `false` (Bedingung **A** bleibt unberührt) | `FeatureGateManager.cs:175-187` |
| **Update fehlgeschlagen** | Keine Kopplung: der Updater scheitert unabhängig; da kein neuer Build läuft, ändert sich der Zähler nicht ⇒ keine Notes aus diesem Anlass (Modal des Updaters: `SPEC_PREFERENCES.md` §3.10) | §3.5 |
| **Benutzer schließt sofort** | Zähler war schon 19 ⇒ in diesem Build **kein** erneutes Auto-Zeigen (Menü bleibt) | §3.4 |
| **App läuft schon (zweiter Start)** | Der externe Pfad enthält den Task ebenfalls; Vorbedingungen unverändert | `ExternalStartupTaskManager.cs:8-16` |
| **Startbildschirm ≠ Backup** (z. B. Sync) | Task läuft nicht (`InitialScreen != Screen.Backup`) | `ShowReleaseNotesWindowTask.cs:47` |
| **Schreiben des Merkers scheitert** | `Set` protokolliert nur einen Fehler; das Fenster zeigt trotzdem (Anzeige folgt dem Speichern) ⇒ Anzeige kann sich wiederholen | `Database.Settings/SettingsDatabase.cs:92-109`; Reihenfolge in `ReleaseNotesWindowService.cs:76-78` |

---

## 8. Lokalisierung

Es gibt **genau zwei** UI-Strings für dieses Feature:

| Schlüssel | de | en | weitere (belegt) |
|---|---|---|---|
| `clouddrive_desktop_contextmenu_releasenotes` | **„Versionshinweise"** | „Release Notes" | es „Notas de la versión", fr „Notes de mise à jour", it „Note di rilascio", ja „リリースノート", pt „Notas de release", zh „发布说明", en-GB „Release Notes" |
| `clouddrive_desktop_releasenotes_close` | **„Schließen"** | „Close" | es „Cerrar", fr „Fermer", it „Chiudi", ja „閉じる", pt „Fechar", zh „关闭", en-GB „Close" |

→ **belegt** (`Resources/<lang>.lproj/Localizable.strings`).

**Nicht übersetzt:** der Fenstertitel (nur `clouddrive_desktop_title` = „Amazon Photos") und die
**web-remote** gelieferten Inhaltstexte (die Sprache kommt aus dem URL-Segment, §5.2). → **belegt**

**`ne`-Auffälligkeit (bekannt):** die `ne.lproj` trägt **Sprachpräfixe in den Werten** —
`"clouddrive_desktop_releasenotes_close" = "deSchließen"` und
`"clouddrive_desktop_contextmenu_releasenotes" = "frNotes de mise à jour"`
→ **belegt** (Werte; Ursache/Wirkung unbekannt, vgl. `SPEC_ABOUT.md` §7).

**Ohne Belegstelle:** Im Bundle existiert der Enum-Wert `ImageType.ReleaseNotes16`
(`UI/ImageType.cs:148`), aber **kein** Verwendungsort und **kein** Asset dieses Namens → vermutlich
totes Erbe. → **belegt** (Fund), **UNKNOWN** (Zweck).

---

## 9. Abgleich mit bestehenden Specs (Präzisierungen, keine harten Widersprüche)

1. **`SPEC_ABOUT.md` §6** beschreibt Fenster, Quelle, Sprach-Whitelist und die Menüpfade korrekt.
   Zwei **Präzisierungen**: (a) der Merker `ReleaseNotesVersion` hat **Default 0** und wird als
   **Literal 19** geschrieben (nicht „aktuell 19" als Konto-Wert), und er ist ein
   **Generations-Zähler**, kein Versions-Merker; (b) die dort offengelassene Frage „ob die Seite beim
   Start tatsächlich erscheint (Feature-Gate-Zustand)" ist **teilweise auflösbar**: der
   **Zähler**-Pfad **A** zeigt die Anzeige **unabhängig** vom Gate; das Gate steuert nur den
   **zweiten** Auslöser **B** und die URL (§3.3, §5.1).
2. **`SPEC_ABOUT.md` §6** ordnet die Bucket-Hosts wie im Code zu (Pipeline-Bucket zuerst genannt,
   „zweiter Host wenn Gate aus") — **deckt sich** mit §5.1; zur Klarheit: **Gate an ⇒ Pipeline-Bucket
   (`d180lfws7k02gz`), Gate aus ⇒ `d29x207vrinatv`** (so auch im Mitschnitt, §5).
3. **`SPEC_PREFERENCES.md` §3.10** (Auto-Update) bleibt die Quelle für den Update-Pfad; dieses
   Dokument **widerspricht** ihm nicht, sondern stellt fest: **keine** Kopplung zwischen Update und
   Release-Notes-Anzeige (§3.5).
4. **`UI_FLOWS.md` §2.1** („Release Notes 370 × 382") stimmt mit dem `SetFrame` überein; die
   Content-Constraints fordern 640 × 570 (§4.3) — derselbe **Widerspruchstyp** wie beim About-Fenster
   (`SPEC_ABOUT.md` §2.2), **hier ausdrücklich gemeldet**.
5. **`SPEC_SYNC.md` §3.9** nennt `… → ShowDriveWindowStartupTask → ShowReleaseNotesWindowTask` am Ende
   der Kette; die Position **stimmt** (`StartupTaskManager.cs:91-92`); die Liste dort ist eine
   Auswahl — die **exakten** Vorbedingungen stehen in §3.2.
6. **`SPEC_SCOPE_INVENTORY.md` #13** („Auto-Update … **Out**") bleibt gültig; die Release-Notes-Anzeige
   ist davon **unabhängig** und kann ohne Auto-Updater existieren (eigener Auslöser, §2).

---

## 10. Gap-Tabelle (Original vs. unser Stand) und offene Fragen für die Umsetzung

| Bereich | Original (belegt) | Unser Stand | Anmerkung für die Umsetzung |
|---|---|---|---|
| Auslöser | build-fester Zähler (`CurrentReleaseNotesVersion = 19`) gegen Konto-Key `ReleaseNotesVersion`, Default 0 | **fehlt** | Ein **eigener, monoton steigender** Notes-Zähler (oder eine bewusst gesetzte „Notes-Generation") schlägt einen Versionsstring-Vergleich — er zwingt zu einer bewussten Editor-Entscheidung pro Release. Ohne Netz/Server läuft das lokal (§11) |
| Speicherort | `~/Library/Application Support/Amazon Cloud Drive/<KontoId>-settings.json`, Key `ReleaseNotesVersion` | `config.toml` / `folders.toml` (eigene Pfade) | Pro **Konto** oder global? Original ist pro Konto und **löscht die Datei beim Abmelden** (§6) |
| Anzeige | **remote** (CloudFront-HTML, Web-View) | kein Web-Backend, **kein** `CHANGELOG` im Repo | Eine lokale, gebündelte Liste ist der einzige Weg ohne eigenen Server — **Inhalt** dann aus unserem Repo (z. B. eine `CHANGELOG`-Datei), **nicht** vom Original übernehmen |
| Fenster | eigenes, nicht-modales Fenster 370 × 382, Web-View + 40-pt-Fußzeile mit „Schließen" | About existiert als egui-Fenster (`src/about.rs`), **keine** Release-Notes-Ansicht | Fenstergröße ist Geschmack; der Merker-Zeitpunkt („**vor** der Anzeige setzen") ist die interessante Design-Entscheidung (§3.4) |
| Menü | „Versionshinweise" in Status- **und** App-Menü, ohne Kürzel, netzabhängig gesperrt | Tray (`src/tray.rs`) + App-Menü (`src/app_menu.rs`) vorhanden | Wenn wir die Notes **lokal** halten, entfällt die Netz-Sperre |
| Version im Query | `CFBundleShortVersionString` (11.3.0) | `CARGO_PKG_VERSION` (+ optionaler Commit, `src/about.rs:110-113`) | passt; nichts erfinden |
| Auto-Update | vorhanden, **nicht** mit den Notes gekoppelt | **Out** (`SPEC_SCOPE_INVENTORY` #13) | Falls Auto-Update später kommt: Notes-Anzeige nicht daran koppeln (§3.5) |

**Offene Fragen für unsere Umsetzung**

1. Merker **pro Konto** (wie das Original) oder global — und was passiert beim Abmelden (Original
   **setzt zurück**, §6)?
2. Zähler-Generation (wie das Original) oder „Anzeige bei jedem neuen Build"? Letzteres zeigt die
   Notes auch bei Builds ohne nennenswerte Änderung.
3. Lokale Liste: eine Datei mit **mehreren** Versionen (wie die Original-Seite) und clientseitigem
   Filtern auf ≤ laufende Version (§5) — oder nur „die Notes dieser Version"?
4. Zeitpunkt: **nur** bei benutzer-initiiertem Start (wie das Original, §3.2) oder auch beim
   Login-Item-/Update-Relaunch?

---

## 11. UNKNOWN (offen, mit Suchweg)

| Frage | Warum offen | Suchweg, der schon gelaufen ist |
|---|---|---|
| **Tatsächliche Fenstergröße** | `SetFrame` 370 × 382 vs. Content-Constraints 640 × 570; kein Lauf beobachtet | Constraints + `SetFrame` ausgelesen (§4.3) |
| **Ob der Stub-Installer `--source-updated` übergibt** | Installer-Argumente kommen aus dem Update-Manifest, das nicht im Bundle liegt | `AppStartupFlags`-Enum + `--source-`-Parsing + `SessionStartSource.UpgradeRestart`-Mapping (§3.2); `SPEC_PREFERENCES.md` §3.10 |
| **Praktisches Interleaving mit Onboarding/FTUE bei Erstinstallation** | beide Tasks laufen in derselben Kette; Notes-Fenster ist nicht modal, kein Lauf beobachtet | Task-Reihenfolge `StartupTaskManager.cs:89-92` + Vorbedingungen §3.2 |
| **Aussehen/Dauer des latenten `ShouldShowReleaseNotesWindow()`-GETs bei ausgeschaltetem Gate** | Ergebnis wird verworfen; blockierender Aufruf im Should-Run-Pfad | `ShowReleaseNotesWindowTask.cs:57-61`, `FeatureGateManager.cs:175-187` |
| **Zweck von `ImageType.ReleaseNotes16`** | kein Verwendungsort, kein Asset | `UI/ImageType.cs:148` + Bundle-Suche nach Assets |
| **Heutiger Inhalt/Zustand der CloudFront-Seite** | Mitschnitt ist Evidenz, kein Live-Beweis; kein neues Capture erlaubt | gelesene Mitschnitte (§5) |
| **Default-Treatment von `EnableNewReleaseNotes`** | `RemoteExperiment`-Default ist Control 100 / T1 0 / T2 0; die Zuteilung ist server-/weblab-getrieben | `AppSettings.cs:515`; Mitschnitt zeigt den **alten** Host ⇒ zur Aufnahmezeit Gate aus (§5) |

---

## 12. Evidenz-Anhang

**Artefakt / Werkzeug**

| Gegenstand | Wert |
|---|---|
| Bundle | `/Applications/Amazon Photos.app` — 11.3.0 (Build 36, Commit `1d7941b5`), Bundle-ID `com.amazon.clouddrive.mac`, `LSMinimumSystemVersion 10.15` (`plutil -p Contents/Info.plist`) |
| Assembly | `Contents/MonoBundle/Amazon Photos.dll`, 4.591.616 B, MD5 `7a9cd34f72be0973047ea88034afd15f` |
| Decompiler | `ilspycmd 11.1.0.9782` (2026-10-17, `-t` für 4 Typen; verifiziert gegen den Dump `amzdec/` vom 2026-10-16 — identisch) |
| Voll-Dump | `/private/var/folders/…/T/opencode/amzdec/<Namespace>/…` (aus derselben DLL) |
| Mitschnitte | `/private/var/folders/…/T/opencode/amazon-photos.mitm` (u. a. Offsets 8087 HTML-Body, 15423817 `release_notes.js`), `amazon2.mitm`, `upload-capture.mitm` |
| Lokalisierung | `Contents/Resources/{de,en,en-GB,es,fr,it,ja,pt,zh,ne}.lproj/Localizable.strings` |

**Aussage → Beleg (Datei:Zeile im Decompil)**

| Aussage | Beleg |
|---|---|
| Auslöser-Zähler, Gate-Rolle, Merker-vor-Anzeige | `Amazon.CloudDrive.StartupTasks/ShowReleaseNotesWindowTask.cs:45-65` (56, 57-64), `:67-79` |
| Konstante 19 | `Amazon.CloudDrive.Infrastructure/CustomerSettings.cs:23` |
| Key/Default/Fortschreibung | `CustomerSettings.cs:341-352`; `Amazon.CloudDrive.UI.ReleaseNotesWindow/ReleaseNotesViewModel.cs:45-52` |
| Speicherpfad | `Amazon.CloudDrive/Constants.cs:541`; `Amazon.CloudDrive/CustomerInfoExtensions.cs:136,144-154` |
| benutzer-initiierte Starts | `Amazon.CloudDrive.Helpers/ArgsFilter.cs:12-22,39-46`; `Amazon.CloudDrive/AppStartupFlags.cs:3-17`; `StartupTasks/StartupContext.cs:140-150`; `Amazon.CloudDrive/App.cs:433` |
| Task-Reihenfolge | `Amazon.CloudDrive.StartupTasks/StartupTaskManager.cs:89-94`; `ExternalStartupTaskManager.cs:8-16` |
| Feature-Content-Bedingung | `Amazon.CloudDrive.Helpers/FeatureGateManager.cs:149-173`; `:175-187`; `:206-221`; `:66-77`; `IFeatureGateManager.cs:20,22` |
| Fenster | `Amazon.CloudDrive.UI.ReleaseNotesWindow/ReleaseNotesWindowService.cs:59-97` (69, 71, 76, 78-84) |
| Ansicht/URL/Timeout/Fußzeile | `ReleaseNotesWindow/ReleaseNotesView.cs:35-51,64-97,99-118,120-126`; `ReleaseNotesViewModel.cs:19-43,54-70` |
| URL-Konstanten / Sprachliste | `Amazon.CloudDrive/Constants.cs:563,565,617,653` |
| Menüpfade / Netz-Sperre | `Amazon.CloudDrive.Services.MenuBar/MenuBarController.cs:43,157-164,201,341` |
| Handler | `Amazon.CloudDrive.UI.IntentHandlers/ShowReleaseNotesWindowIntentHandler.cs:22-27`; `CloseReleaseNotesWindowIntentHandler.cs:16-19` |
| Metriken | `Amazon.CloudDrive/HealthMetrics.cs:783-796` (`WindowOpened.ReleaseNotes`) |
| Merker-Reset beim Abmelden | `Amazon.CloudDrive.Infrastructure/CustomerSignOut.cs:106` |
| Schreiben toleriert Fehler | `Amazon.CloudDrive.Database.Settings/SettingsDatabase.cs:92-109` |
| Icon | `Contents/Resources/ic-notes-grey-16@1..3x.png` (15/30/45 px, `sips`) |
