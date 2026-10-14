# About / Info / Rechtliches — Clean-Room-Spec (reverse-engineered)

Scope: das **About-/Info-Umfeld** der Original-App *Amazon Photos* (macOS) — „Über", Version/Build,
Datenschutz/Nutzungsbedingungen, **Drittanbieter-Lizenzen**, Versionshinweise (Release Notes) und die
zugehörigen Menü-/Tray-Pfade. Reine Beobachtung des ausgelieferten Artefakts, in eigenen Worten;
**keine** Code-Auszüge. UI-Texte sind wörtlich zitiert (erlaubt).

**Belegtes Ziel-Artefakt (2026-10-14, gelesen, nur lesend):**
`/Applications/Amazon Photos.app` — `CFBundleShortVersionString = 11.3.0`, `CFBundleVersion = 36`,
`CommitId = 1d7941b5`, `CFBundleIdentifier = com.amazon.clouddrive.mac`, `LSUIElement = true`.
Managed Assembly: `Contents/MonoBundle/Amazon Photos.dll`. Werkzeug: `ilspycmd 11.1.0.9782`
(Decompile der gesamten Assembly + gezielte Typen), `plutil`/`strings`/`find` auf dem Bundle.

**Datenschutz-Hinweis:** Keine Personendaten in diesem Dokument. Fundstellen mit Personenbezug
(Geräte-Seriennummer, Gerätename) sind als „Feld existiert, Inhalt redigiert" beschrieben.

---

## 1. Welche Fenster/Dialoge gehören dazu?

| # | Bereich | Ist es ein Fenster? | Auslöser (belegt) |
|---|---|---|---|
| 1 | **Über / About** (`AboutWindow`, `IAboutWindowService`) | **Ja** — eigenes NSWindow, als **Sheet** am Hauptfenster | Tray-Menü-Eintrag „Mehr dazu"; macOS-App-Menü erster Eintrag „Mehr dazu" |
| 2 | **Nutzungsbedingungen (ToS)** | **Nein** — kein Fenster, öffnet den **Standard-Browser** | Link „Nutzungsbedingungen" **im About-Fenster** |
| 3 | **Datenschutzerklärung (Privacy)** | **Nein** — Browser | Link „Datenschutzerklärung" **im About-Fenster** |
| 4 | **Drittanbieter-Lizenzen (Third-Party)** | **Nein** — Browser (Web-Seite) | Link „Drittanbieterlizenzen" **im About-Fenster** |
| 5 | **Versionshinweise / Release Notes** (`ReleaseNotesWindow`) | **Ja** — eigenes, nicht-modales Fenster (Web-View) | Tray-Menü und App-Menü „Versionshinweise"; zusätzlich automatisch beim Start (`ShowReleaseNotesWindowTask`) |
| 6 | **Version-/Update-Info** | **Nein** — kein eigenes Fenster | Benachrichtigung/Dialoge des Auto-Updaters → **siehe `docs/SPEC_PREFERENCES.md` §3.10** |
| 7 | Diagnose/Support (`DiagnosticsWindow`) | Ja, aber anderes Feature | nicht Teil dieses Auftrags → `docs/UI_FLOWS.md` #22 |

**Kernbefund:** Es gibt **genau ein** „About"-Fenster. Datenschutz, Nutzungsbedingungen und
Drittanbieter-Lizenzen sind darin **nur Links**, die den **System-Browser** öffnen — es existiert
**kein** In-App-Textfenster für Rechtstexte. → **belegt**
(`AboutViewController` → `LaunchCloudDriveLinkCommand` / `LaunchCloudDriveTermsOfUseLinkCommand`
→ `CloudDriveWebNavigator.NavigateTo` → `BrowserLauncher.LaunchUrl` → `NSWorkspace.OpenUrl`).

---

## 2. About-Fenster (belegt)

### 2.1 Auslöser und Darstellung

- **Zwei Menü-Pfade** (Details in §5): Tray-Statusmenü und macOS-App-Menü, beide mit demselben
  Menütext `clouddrive_desktop_contextmenu_support_about` und demselben Ikonen-Asset `about-grey16`.
- Präsentation: **Sheet** am Hauptfenster-DriveWindow (`_driveWindowController.BeginSheet(...)`),
  also **kein** eigenständiges Fenster. Fenster-`EventTag = "About"` (Telemetrie),
  Health-Metric `WindowOpened.About`. → **belegt**
- Fenster-Chrome: `TitleVisibility = Hidden`, `TitlebarAppearsTransparent = true`,
  `MovableByWindowBackground = true`, `StyleMask = FullSizeContentView` (d. h. **keine Titlebar,
  kein Resize-Griff, keine Schließen-Ampel**); Hintergrundfarbe Clear, Inhalt mit Eckradius 12.
  → **belegt**
- **Der Standard-`NSApplication`-About-Dialog wird nie benutzt**: kein Aufruf von
  `orderFrontStandardAboutPanel`/`standardAboutPanel` in der Assembly; die App ersetzt stattdessen
  das gesamte Hauptmenü durch ihr eigenes (zwei Aufrufe von `NSApplication.SharedApplication.MainMenu = …`,
  beide in `MenuBarController`/`PreloadedMenuBarController`). → **belegt**

### 2.2 Fenstergröße — Widerspruch im Code, hier offengelegt

| Quelle | Wert |
|---|---|
| explizit gesetztes Fenster-Frame (`AboutWindowService` → `SetFrame`) | **504 × 410 pt** |
| eigene Auto-Layout-Constraints der **vollen** Ansicht (`AboutView`) | Breite **≤ 900**, Höhe **572** |
| eigene Auto-Layout-Constraints der **vereinfachten** Ansicht (`AboutViewSimplification`) | **624 × 572** |

`docs/UI_FLOWS.md` §2.1 nennt 504 × 410. Der Code setzt diesen Frame auch, **die Inhaltsansicht
fordert jedoch 572 pt Höhe** (und bis 900 pt Breite). Welche Größe macOS beim Sheet tatsächlich
annimmt, hängt davon ab, ob die Content-View-Constraints die Fenstergröße treiben — das ist hier
**nicht belegt**, weil kein Lauf beobachtet wurde. → Größe **UNKNOWN** (beide Zahlen sind belegt,
ihre Auflösung nicht). Ein `preferredContentSize` ist **nicht** gesetzt. → **belegt**

### 2.3 Zwei Layout-Varianten (Weblab)

Ein Weblab-Treatment entscheidet das Layout: `DriveWindowViewModel.EnableAboutViewSimplification`
ist genau dann wahr, wenn die Behandlung des Experiments aus
`AppSettings.EnableAboutViewSimplification` gleich `"T1"` ist. → **belegt**

- `RemoteExperiment`-Default im Code: Control **100**, T1 **0**, T2 **0** ⇒ die Behandlung ist bei
  fehlender Experiment-Zuteilung immer **"C"** (Control). Die eingebettete
  `Amazon.CloudDrive.Config.MinervaCustomConfig.json` (einziges Config-Embedded-Resource der
  Assembly) enthält **keinen** Eintrag zu `EnableAboutViewSimplification`. ⇒ Die installierte App
  zeigt **die volle Variante** (`AboutView`), **sofern** der Server nicht remote etwas anderes
  zuweist. → **inferred** (Server-Zuteilung ist zur Laufzeit nicht einsehbar = **UNKNOWN**)

### 2.4 Inhalt und Reihenfolge

Beide Varianten enthalten dieselben Elementtypen; nur die Anordnung/Betonung unterscheidet sich.
Reihenfolge von oben nach unten:

| Position | Element | Voll (`AboutView`) | Vereinfacht (`AboutViewSimplification`) |
|---|---|---|---|
| 1 | Logo | `ImageType.AmazonPhotosLogo` (200 × 38 pt) | ebenso |
| 2 | Version | **eine** Zeile „Version: {0}" | **zwei** Zeilen: Label „Version:" (groß) + Wert (klein) |
| 3 | Geräte-Seriennummer | **eine** Zeile „Geräte-Seriennummer: {0}" | **zwei** Zeilen: Label + Wert |
| 2+3 | Rahmen | beide in **einer** umrandeten Box (`_versionView`) | Label/Wert-Paare **ohne** gemeinsamen Rahmen |
| 4 | Send-Logs-Hinweistext | Hinweistext (`clouddrive_desktop_send_logs_tooltip`) | Hinweistext in **umrandetem** Container |
| 5 | Send-Logs-Button | „Protokolle senden" + Spinner links daneben | ebenso, in der Box |
| 6 | Links-Zeile | „Nutzungsbedingungen" · „Datenschutzerklärung" · „Drittanbieterlizenzen" (horizontal, gleichabständig, zentriert) | ebenso |
| 7 | Copyright | Copyright-Zeile | ebenso |
| 8 | Trennlinie | 1 pt | ebenso |
| 9 | Schließen | Capsule-Button „Schließen", rechts unten | ebenso |

Schrift: die volle Variante nutzt System-Font **15 pt** für Version/Seriennummer und **14 pt** für
Hinweis- und Copyright-Zeile; die vereinfachte Variante Ember-Font **16/14 pt** (Label/Wert) und
**12 pt** für Hinweis- und Copyright-Zeile (Sekundärfarbe). → **belegt**

**Klickbar / kopierbar:**
- Die drei Links sind Hyperlink-Buttons (`DriveButton.HyperlinkButton`).
- Send-Logs-Button ist ein echter Button; während des Sendens läuft ein Spinner, Button gesperrt.
- Version und Seriennummer sind **`Selectable = true`** NSTextFields — d. h. **diese beiden Werte
  lassen sich markieren/kopieren**. Hinweis- und Copyright-Zeile werden dagegen über einen
  Hilfs-Konstruktor erzeugt, der `Selectable = false` (und `Editable = false`) setzt — sie sind
  **nicht** markierbar. Es gibt **keinen** „Kopieren"-Button. → **belegt**
  (Die Hinweis-/Copyright-Zeile bekommt zusätzlich denselben Text als Tooltip.)
- **Kein** „Systemeinstellungen öffnen" im About-Fenster. → **belegt**
  (Ein `x-apple.systempreferences:`-Link existiert in der Assembly, aber in Preferences/Onboarding,
  nicht in About — siehe `CloudDriveLink.PhotosSystemPreferencePane` / `…AutomationSystemPreferencePane`.)
- **Kein** „Nach Updates suchen", **kein** Kanal-/Beta-Umschalter, **kein** Support-/Log-Ordner-Link
  im About-Fenster. → **belegt**

### 2.5 UI-Identifikatoren (Accessibility/Automation-Belege)

`AboutView_Version_Label`, `AboutView_Version_Text`, `AboutView_DeviceSerialNumber_Label`,
`AboutView_DeviceSerialNumber_Text`, `AboutView_SendLogs_Button`, `AboutView_TermsOfUse_Button`,
`AboutView_PrivacyNotice_Button`, `AboutView_ThirdPartyLicense_Button`, `AboutView_Close_Button`.
→ **belegt**

### 2.6 Send-Logs (Aktion im About-Fenster)

- Buttontexte/Hinweis wörtlich (de): Hinweis
  „Helfen Sie uns dabei, Amazon Photos zu verbessern und Probleme zu beheben, indem Sie uns Ihre
  Aktivitätsprotokolle senden." (`…_send_logs_tooltip`), Button „Protokolle senden"
  (`…_send_logs_button_text`). **Nicht** die `…_preferences_send_logs_*`-Paarung. → **belegt**
- Ablauf: Doppelklick-Schutz (Interlock); vor dem Senden wird ein Report-Event publiziert; danach
  Erfolg-/Misserfolgs-Dialog:
  - Erfolg: Titel „Protokolle gesendet", Text „Vielen Dank, dass Sie uns bei der Fehlersuche geholfen haben."
  - Fehler: Titel „Protokolle können nicht gesendet werden", Text „Bitte versuchen Sie es später erneut."
  → **belegt**

---

## 3. Versions-/Build-Angabe

Die im About-Fenster gezeigte Version ist **zusammengesetzt**, nicht die Bundle-Version: → **belegt**

| Bestandteil | Quelle | Wert der Installation |
|---|---|---|
| `AssemblyVersion` | `Info.plist` → `CFBundleShortVersionString` | `11.3.0` |
| `CommitId` | `Info.plist` → `CommitId` | `1d7941b5` |
| **angezeigte Version** | `AssemblyVersion + "." + CommitId` | **`11.3.0.1d7941b5`** |

- Anzeige im vollen Layout als **eine** Zeile: „Version: 11.3.0.1d7941b5"
  (`clouddrive_desktop_aboutwindow_versionformat` = „Version: {0}"); im vereinfachten Layout steht
  „Version:" als Label und der Wert separat. → **belegt**
- `CFBundleVersion = 36` (Build 36) wird im About-Fenster **nicht** angezeigt; ebenso gibt es
  **keinen** Kanal-/Beta-Suffix und **keine** Kennzeichnung „Nightly/Stable". → **belegt**
- `IAppInfo.Name` liefert hart `"Amazon Photos"`; `SpectatorName`/`PackageName` = `com.amazon.clouddrive.mac`;
  `InternalAppName` = `CloudDriveDesktop`. **Keines dieser Felder wird im About-Fenster als Text
  ausgegeben** (der App-Name erscheint dort nur als Logo). → **belegt**
- Die **Geräte-Seriennummer** (`IAppInfo.DeviceSerialId`, Anzeige „Geräte-Seriennummer: {0}") ist
  kein Hardware-String, sondern ein **persistierter Hash** aus Hardware-UUID + Hardware-Seriennummer
  + Benutzername (Länge 30–35 Zeichen), einmalig gespeichert. → **belegt** (Feld existiert,
  Inhalt redigiert — kein Wert in diesem Dokument)
- Abweichung zum Bundle-Copyright: `Info.plist` → `NSHumanReadableCopyright` lautet
  „© 1996-2021, Amazon.com Inc. or its affiliates"; die **In-App**-Copyright-Zeile stammt dagegen
  aus `…_aboutwindow_copyright` und trägt das **laufende Jahr** (`DateTime.Now.Year`, bei
  View-Model-Erzeugung): „© 2012-**{Jahr}** Amazon.com, Inc. und Tochtergesellschaften. Alle Rechte
  vorbehalten." (installiert 2026 ⇒ „© 2012-2026 …"). → **belegt**

Sofern ein Versions-Fenster fehlt, aber ein Update ansteht: das regelt der Auto-Updater über
Benachrichtigungen/Dialoge, **siehe `docs/SPEC_PREFERENCES.md` §3.10**.

---

## 4. Drittanbieter-Lizenzen — gibt es eine Liste?

**Ergebnis: NEIN — es existiert keine eingebettete/native Drittanbieter-Lizenzliste.** Der einzige
Lizenz-Artefakt ist ein **Link auf eine Web-Seite**. → **belegt**

Suchweg (alle Prüfungen am installierten Bundle/Assembly, nur lesend):

| Prüfung | Ergebnis |
|---|---|
| Dateisystem-Suche im Bundle (`find … -iname "*licen*" -o -iname "NOTICE*" -o -iname "*third*" -o -iname "*eula*" -o -iname "*.txt"`) | **keine** Lizenz-/Notice-Datei, **keine** `.txt` überhaupt. Die Treffer waren reine Teilstring-Zufälle (z. B. enthält „Pho**tos**" die Zeichenfolge `tos`) sowie die About-**Icons** (`about-grey16@1x/2x/3x.png`) |
| Embedded Resources der Assembly (`ilspycmd --list-resources`) | **4** Ressourcen: `cloud_drive_scanner_cold_boot.sql`, `cleanup_filerecord_dupes.sql`, `supported-content-types.json`, `MinervaCustomConfig.json` — **keine** Lizenzdatei |
| `strings` in der Assembly nach Lizenz-Floskeln (`Apache License`, `MIT License`, `BSD`, `GNU General Public`, `Copyright (c)`, `NOTICE.txt`, `THIRD-PARTY`) | **keine** Lizenztexte; nur der String-Schlüssel `…_aboutwindow_thirdpartylicenses` und der Enum-Wert `ThirdPartyLicenses` |
| Lokalisierungsdateien `Resources/<lang>.lproj/Localizable.strings` | nur **ein** Lizenz-bezogener Eintrag: der **Linktext** „Drittanbieterlizenzen" / „Third-Party Licenses" |
| `.storyboardc`/`.nib` für Legal-Texte | keine; es gibt nur `Diagnostics`, `Onboarding`, `SelectiveSyncView`, `UpsellView`, `DriveThemedWindow` |
| Aufruf-Ziel des Links | `CloudDriveLink.ThirdPartyLicenses` → `UrlPath = "/photos/desktop/thirdpartylicenses"` → `DomainProvider.BuildLink` ⇒ `https://www.amazon.<marktplatz-TLD>/photos/desktop/thirdpartylicenses`, geöffnet im **System-Browser** |

**Form/Umfang der Web-Seite:** nicht Teil des Artefakts ⇒ **UNKNOWN**. Ob dort Namen +
Lizenztypen stehen, ist im Bundle **nicht** belegbar (die Seite ist nicht mitgeliefert und wurde in
diesem Auftrag nicht abgerufen). → **UNKNOWN**

`docs/SPEC_PREFERENCES.md` §3.11 hält bereits fest, dass ein Drittanbieter-Lizenzhinweis die
einzige harte Pflicht für uns ist; **dieser Abschnitt ist der Suchbeleg dafür**, dass die Vorlage
dafür im Original **nicht** lokal existiert.

### 4.1 Nutzungsbedingungen / Datenschutz (nur zur Vollständigkeit)

| Link | Ziel-URL (gebaut aus `DomainProvider.GetDomain()`) | Signiert-aus-Variante |
|---|---|---|
| Nutzungsbedingungen | `https://www.amazon.<TLD>/photos/desktop/termsofuse` | `https://www.amazon.com/cd/tou/flagpage` (`TermsOfUseFlag`) |
| Datenschutzerklärung | `https://www.amazon.<TLD>/photos/desktop/privacy` | keine |
| Drittanbieterlizenzen | `https://www.amazon.<TLD>/photos/desktop/thirdpartylicenses` | keine |

Der ToS-Link **wechselt die URL je nach Anmeldestatus** (`LaunchCloudDriveTermsOfUseLinkCommand`
prüft `IsCustomerSignedIn`). Alle drei werden per `NSWorkspace.OpenUrl` im **System-Browser**
geöffnet, nicht in einem eingebetteten WebView. Die TLD ergibt sich aus der Marktplatz-Region
(`DomainProvider`, Region→Domain-Tabelle). → **belegt**

---

## 5. Menü-/Tray-Pfad und Tastenkürzel

Zwei Wege führen zum About-Fenster; beide werden in `MenuBarController` (bzw. dem „preloaded"
Pendant `PreloadedMenuBarController`) gebaut. Der **verwendete** String ist
`clouddrive_desktop_contextmenu_support_about` (de: **„Mehr dazu"**, en: **„About"**); der
zusätzlich definierte Schlüssel `clouddrive_desktop_contextmenu_about` (de ebenfalls „Mehr dazu")
ist im Code **unbenutzt**. → **belegt**

### 5.1 Tray-/Statusleisten-Menü (`AppMenu`, `NSStatusItem.Menu`)

Reihenfolge (von oben): Status-/Pausen-Zeile · „Voreinstellungen" (⌘,) · **Trenner** ·
„Fotos-Website" · „Speicher verwalten" · „Help Center" · **Trenner** · „Versionshinweise" ·
„Feedback senden" · **„Mehr dazu"** · **Trenner** · „Amazon Photos verlassen" (⌘Q).
Ikonen der Einträge (13 pt): `setting-icon-gray15`, `photo-icon-gray15`, `manage-storage-icon-gray`,
`help-icon-gray15`, `ic-notes-grey-16`, `icon-feedback-grey-16`, **`about-grey16`**, `exit-icon-gray17`.
Der About-Eintrag trägt **kein** Tastenkürzel. → **belegt**

Die **preloaded** Variante (vor dem Laden des vollen Menüs) enthält nur Status-Zeile,
„Mehr dazu", „Amazon Photos verlassen" — About ist also auch dort erreichbar. → **belegt**

### 5.2 macOS-Menüleiste (App-Menü, erstes Submenü)

Reihenfolge: **„Mehr dazu"** (About, **erster Eintrag, ohne Kürzel**) · **Trenner** ·
„Voreinstellungen" (⌘,) · „Help Center" · „Feedback senden" · „Versionshinweise" · **Trenner** ·
„Fenster schließen" (⇧⌘W) · **Trenner** · „Amazon Photos verlassen" (⌘Q).
Das About-Item im App-Menü hat **kein Icon**. Zusätzlich existieren ein Menü „Bearbeiten"
(Ausschneiden ⌘X / Kopieren ⌘C / Einfügen ⌘V) und ein Menü „Fenster" (Minimieren ⌘M); die App
setzt damit `NSApplication.SharedApplication.MainMenu` selbst. → **belegt**

### 5.3 Belegte Tastenkürzel im Umfeld (nur zur Abgrenzung)

⌘, = Voreinstellungen · ⌘Q = Beenden · ⇧⌘W = Fenster schließen · ⌘M = Minimieren ·
⌘X/⌘C/⌘V = Ausschneiden/Kopieren/Einfügen. **Für About gibt es kein Kürzel.** Der einzige
„Gesture"-Wert im Umfeld ist `clouddrive_desktop_contextmenu_support_help_windows_gesture` = `Alt+H`
(Windows-Variante der Hilfe, nicht About). → **belegt**

---

## 6. Versionshinweise (Release Notes) — eigenes Fenster, Web-Inhalt

| Aspekt | Belegt |
|---|---|
| Fenster | eigenes `NSWindow`, `StyleMask = Titled \| Closable`, **nicht modal**, zentriert |
| Titel | `clouddrive_desktop_title` = „Amazon Photos" |
| Größe | **370 × 382 pt** (`SetFrame`) |
| Inhalt | **eingebetteter Web-View** (`WebKit.WebView`), lädt eine HTML-Seite; Spinner während des Ladens, 30 s Request-Timeout, Cache-Policy „ReloadRevalidatingCacheData" |
| Quelle | CloudFront: `https://d180lfws7k02gz.cloudfront.net/mac/releasenotes/release_notes_{lang}.html?currentAppVersion={AssemblyVersion}&activeFeatures={…}` (bzw. ein zweiter Bucket-Host, wenn Feature-Gate `EnableNewReleaseNotes` aus ist) |
| Version im Query | **`AssemblyVersion`** = `11.3.0` (ohne CommitId) |
| Sprache | 2-Buchstaben-Code aus der Amazon-Culture, **nur** wenn in `Constants.ReleaseNotesLanuageCodes` = {`de`,`en`,`es`,`fr`,`it`,`ja`}; sonst `en` |
| Fußzeile | 40 pt hohe Leiste mit Button „Schließen" (`clouddrive_desktop_releasenotes_close`), Identifier `ReleaseNotesView_Close_Button` |
| Auslöser | Menüpunkte „Versionshinweise" (`…_contextmenu_releasenotes`) in Tray- und App-Menü; **zusätzlich automatisch** beim Start über `ShowReleaseNotesWindowTask` (Feature-Gates `EnableNewReleaseNotes` / `ShouldShowReleaseNotesWindow`) — der Tray-Eintrag ist an `IsNetworkAvailable` gekoppelt |
| Merker | „gesehene" Version wird in den CustomerSettings gespeichert (`ReleaseNotesVersion`, aktuell `19`) |

Alles → **belegt**, außer: **ob** die Seite beim Start tatsächlich erscheint (Feature-Gate-Zustand
zur Laufzeit) → **UNKNOWN**.

Abgrenzung: Der Auto-**Update**-Pfad („Wurde aktualisiert", Download-Dialog) ist **nicht** Teil der
Release Notes — **siehe `docs/SPEC_PREFERENCES.md` §3.10**.

---

## 7. Lokalisierung

- Die About-/Menü-Schlüssel existieren in **allen** mitgelieferten Sprachdateien:
  `de, en, en-GB, fr, it, es, pt, ja, zh, ne`. → **belegt**
- **Vollständig lokalisiert** (deutsche Fassung wörtlich):
  „Über Amazon Photos" (`…_aboutwindow_title`, ungenutzt), „Version: {0}", „Geräte-Seriennummer: {0}",
  „Nutzungsbedingungen", „Datenschutzerklärung", „Drittanbieterlizenzen", „Schließen",
  „© 2012-{0} Amazon.com, Inc. und Tochtergesellschaften. Alle Rechte vorbehalten.",
  „Mehr dazu", „Versionshinweise", „Feedback senden", „Voreinstellungen", „Amazon Photos verlassen",
  „Fenster schließen", „Bearbeiten/Fenster/Minimieren/Kopieren/Ausschneiden/Einfügen",
  „Protokolle senden", der Send-Logs-Hinweis und die Erfolg-/Fehlerdialoge. → **belegt**
- **Nicht übersetzt / englisch-technisch:**
  - **„Help Center"** — bleibt in der deutschen Fassung **englisch** (`…_contextmenu_support_help`).
  - Der **Versions-String** selbst (`11.3.0.1d7941b5`) und die **Geräte-Seriennummer** sind
    technische Werte, nicht lokalisiert.
  - Der **Copyright-Text** ist lokalisiert, enthält aber den Firmennamen englisch.
  → **belegt**
- `en-GB` übersetzt „Licences" (statt „Licenses"). → **belegt**
- Auffälligkeit ohne Erklärung: die `ne.lproj`-Datei enthält an diesen Stellen **Sprachpräfixe im
  Wert**, z. B. `"deDrittanbieterlizenzen"` und `"itInformazioni su"` — offenbar ein fehlerhaftes
  Bundle. Ursache und Auswirkung → **UNKNOWN**. → **belegt** (Werte)

Vollständige String-Tabelle bereits in `docs/SPEC_PREFERENCES.md` §4 und `docs/UI_STATES.md` §5.3.

---

## 8. Was andere Docs schon abdecken (nur Verweise)

- **`docs/SPEC_PREFERENCES.md` §3.11** — About-Fenster: Elementtabelle, Sheet-Präsentation,
  Web-URLs, Send-Logs-Ablauf, Hinweis auf die Lizenz-Pflicht. Dort steht bereits, dass der
  Titel-String ungenutzt ist.
- **`docs/SPEC_PREFERENCES.md` §4** — String-Schlüssel→Deutsch-Tabelle (inkl. `aboutwindow_*`,
  `about_close`, `send_logs_*`).
- **`docs/SPEC_PREFERENCES.md` §5 / `AGENTS.todo.md`** (Scope-Zeile 14) — Gap „About fehlt".
- **`docs/SPEC_PREFERENCES.md` §3.10** — Auto-Update (Version-/Update-Info ohne eigenes Fenster).
- **`docs/UI_STATES.md` §5.3** — About-Fenster-Inhalt kurz; §3 (Feld `serial-number` „shown in About").
- **`docs/UI_FLOWS.md` §1 (#15 About, #16 Release Notes)** und **§2.1** (Größen 504 × 410 / 370 × 382).
- **`docs/SPEC_OS_INTEGRATION.md`** (~Zeilen 273, 354) — App-Menü mit About/Preferences/Help/
  Release notes; dortiger Hinweis auf den fehlenden Window-Menü-Nachbau bei uns.

**Neu in diesem Dokument** (nicht in den obigen): der zusammengesetzte Versions-String samt
CommitId; der vollständige Suchbeleg „**keine** lokale Lizenzliste"; beide Menüpfade mit
Reihenfolge/Kürzeln und Beleg „About ohne Tastenkürzel"; die Weblab-Herkunft des
Layout-Umschalters samt Code-Default; das Release-Notes-Fenster im Detail; Logo-/Ikonen-Assets;
„kein Kopieren-Button, nur selektierbarer Text"; die `ne`-Bundle-Auffälligkeit; der Widerspruch
504 × 410 vs. 572 pt Höhe.

---

## 9. Gap-Tabelle

| Bereich | Original-Verhalten (belegt) | Unser Stand | Sachliche Empfehlung |
|---|---|---|---|
| About-Fenster | Sheet am Hauptfenster, Logo + Version + Seriennummer + Send-Logs + 3 Rechtslinks + Copyright + „Schließen" | **fehlt** (`aboutwindow_*`; kein About in `src/`) | About-Fenster bauen — **ohne** Amazon-Links und **ohne** Seriennummer (Branding-Regel, `AGENTS.md`) |
| Versionsangabe | `CFBundleShortVersionString` + `.` + `CommitId` (z. B. `11.3.0.1d7941b5`) | fehlt (Version existiert nur in `Cargo.toml`) | Version aus `CARGO_PKG_VERSION` zeigen; Commit-ID nur, wenn sie zur Build-Zeit verfügbar ist — sonst weglassen, nichts erfinden |
| Copyright-Zeile | „© 2012-{laufendes Jahr} Amazon.com, Inc. und Tochtergesellschaften …" | fehlt | Eigene Copyright-Zeile (Projekt/Kontakt), **nicht** die Amazon-Formulierung übernehmen |
| Nutzungsbedingungen-Link | Link → `https://www.amazon.<TLD>/photos/desktop/termsofuse` (Browser) | fehlt | **Nicht nachbauen** — durch die Branding-Regel ausgeschlossen; nur eigener Text, wenn wir eigene Bedingungen haben |
| Datenschutz-Link | Link → `…/photos/desktop/privacy` (Browser) | fehlt | dito — eigene Datenschutzerklärung nur, falls überhaupt Daten anfallen |
| **Drittanbieter-Lizenzen** | **keine** lokale Liste; nur Link → `…/photos/desktop/thirdpartylicenses` (Browser) | **fehlt** | **Pflicht für ein öffentliches Release.** Eigene Liste aus den Cargo-Abhängigkeiten erzeugen (MIT/Apache-2.0) und **im** About-Fenster/als eigene Seite ausliefern — die Original-Vorlage ist dafür nur ein Anlass, keine Vorlage |
| Send-Logs | Hinweistext + Button „Protokolle senden", Erfolg-/Fehlerdialog | fehlt | Optional/Post-MVP; ohne Log-Upload nicht sinnvoll |
| Release Notes | eigenes Fenster 370 × 382, Web-View aus CloudFront, mehrsprachig, Start-Auto-Anzeige | fehlt | Optional/Post-MVP; ohne Web-Backend nur als lokale `CHANGELOG`-Ansicht sinnvoll |
| Version-/Update-Info | kein eigenes Fenster; Update-Benachrichtigung | fehlt (Auto-Update ist **Out**) | bleibt Out; **siehe `SPEC_PREFERENCES.md` §3.10** |
| Menüführung zum About | Tray-Menü + macOS-App-Menü („Mehr dazu"), **ohne** Tastenkürzel | Tray existiert (`src/tray.rs`), App-Menü `src/app_menu.rs`; About fehlt | Eintrag in beiden Pfaden ergänzen, konsistent ohne Kürzel (⌘, bleibt Preferences vorbehalten) |

---

## 10. UNKNOWN (offen, mit Suchweg)

| Frage | Warum offen | Suchweg, der schon gelaufen ist |
|---|---|---|
| **Existiert eine lokale Drittanbieter-Lizenzliste?** | Suchbeleg sagt **nein**; der Inhalt der Web-Seite ist nicht Teil des Artefakts und wurde nicht abgerufen | Bundle-`find` nach licen/notice/third/eula/txt · `ilspycmd --list-resources` (4 Ressourcen) · `strings` nach Lizenz-Floskeln · `Localizable.strings` |
| **Form/Umfang/Namen der CloudFront-Lizenzseite** | nicht mitgeliefert | nur die URL-Route im Code (`/photos/desktop/thirdpartylicenses`); Seite nicht abgerufen |
| **Tatsächliche Fenstergröße des About-Sheets** | `SetFrame` (504 × 410) widerspricht den Content-View-Constraints (572 pt hoch; 624 bzw. ≤900 breit); kein Lauf beobachtet | Constraints beider Views ausgelesen |
| **Welches Layout die Installation zeigt** | weblab-/server-getrieben; Code-Default ⇒ **voll**, Remote-Zuteilung nicht einsehbar | `RemoteExperiment`-Defaults (100/0/0) + `MinervaCustomConfig.json` (kein Eintrag) |
| **Erscheinen die Release Notes automatisch beim Start?** | Feature-Gate-Zustand (`EnableNewReleaseNotes` / `ShouldShowReleaseNotesWindow`) zur Laufzeit unbekannt | `ShowReleaseNotesWindowTask`, `ReleaseNotesVersion = 19` |
| **Ursache der fehlerhaften `ne.lproj`-Werte** | Bundle-Auffälligkeit ohne Codebezug | Werte in `ne.lproj/Localizable.strings` |
| **Ob die App überhaupt je unterschrieben/notarisiert ausgeliefert wird** | außerhalb dieses Auftrags | nicht geprüft (siehe `docs/RELEASE.md`) |
