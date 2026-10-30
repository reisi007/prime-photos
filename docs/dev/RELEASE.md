# Prime Photos — Release & Verteilung

Wie ein Release dieses Projekts entsteht und wie ein heruntergeladener Build geprüft wird.
System-Voraussetzungen zum Bauen: `BUILD.md`. macOS-Bundle lokal: `packaging/README.md`.
Quelle der Entscheidungen: `AGENTS.md` „CI/Release".

---

## 1. Trigger

Ein Release wird **nur** durch einen Git-Tag `v*` ausgelöst (z. B. `v0.1.0`):

```sh
git tag v0.1.0
git push origin v0.1.0
```

`.github/workflows/release.yml` baut daraufhin die Matrix **Windows + Linux** und legt ein
GitHub Release mit den Artefakten an (`generate_release_notes: true` erzeugt die Release-Notes
automatisch aus den Commits seit dem letzten Tag).

**macOS wird im CI _nicht_ gebaut** (User-Entscheidung 2026-10-13). Das macOS-Bundle entsteht
lokal über `packaging/make-app.sh` und wird bei Bedarf manuell an ein Release gehängt (§4).

### Build-Gates (User-Entscheidung 2026-10-13)

- **Push/PR gegen `main`:** nur **Linux** — `cargo build` + `cargo test` als Pflicht-Gate; die
  anderen OS werden dort **nicht** gebaut. Zusätzlich läuft davor der Schritt
  `Ratchet quality gates` (`ratchet-cli`, Modus `validate`, eingeführt mit `d3896f2`) — **nur** in
  `ci.yml`; `release.yml` hat keinen Ratchet-Schritt (§9, fünfter Lauf).
- **Release:** **Git-Tag `v*`** → CI-Matrix-Build **Windows + Linux**; alle Beine müssen grün
  sein (`needs:`), sonst kein Release → GitHub Release mit den Artefakten.
- **macOS wird NICHT im CI gebaut** (User-Entscheidung 2026-10-13: „nur lokal macOS bauen ist
  ok") — lokal über `packaging/make-app.sh` (arm64) und bei Bedarf manuell ans Release hängen.
- **Signierung:** Windows wird bei konfiguriertem `SIGNPATH_API_TOKEN` über **SignPath**
  Authenticode-signiert, Linux bei konfiguriertem `GPG_PRIVATE_KEY` mit einer abgetrennten
  **GPG**-Signatur (`.asc`) versehen (§3). macOS bleibt unverändert: lokale (selbstsignierte)
  Codesignatur, damit Benachrichtigungen/Bundle-Identifier sauber funktionieren (§4–§6).
  Signiert wird **nur** bei einem Tag-Push; ein konfigurierter, aber fehlschlagender Signaturschritt
  lässt den Job **fehlschlagen**, statt ein unsigniertes Artefakt still zu veröffentlichen.
- **Warnungs-Baseline 0:** `[lints.rust] warnings = "deny"` in `Cargo.toml` — jede Warnung ist
  ein harter Build-Fehler. Kein `.cargo/config.toml` mit `-Dwarnings` (würde auch Dependencies
  hart nehmen). `cargo fmt --check` ist **kein** Gate — das Repo ist nicht fmt-clean.
- **Secrets nur in `main`/Tag-Läufen** (fork-PRs erhalten keine Secrets). Windows: SignPath
  optional (sonst unsigniert + SmartScreen-Hinweis); Linux: GPG optional (sonst unsigniert).

---

## 2. Was das CI-Release erzeugt

Der `build`-Job ist eine Matrix (`fail-fast: false`). Der `release`-Job hat `needs: [build]` und
läuft damit **nur, wenn beide Beine grün sind** — sonst entsteht kein Release.

| Bein | Runner | Artefakt |
|---|---|---|
| Windows x64 | `windows-latest` | `Prime-Photos-windows-x64.zip` (enthält `prime-photos.exe`) |
| Linux x64 | `ubuntu-latest` | `Prime-Photos-linux-x64.tar.gz` (enthält `prime-photos`) (+ `.asc` und `Prime-Photos-release-key.asc`, wenn GPG konfiguriert) |

- Beide bauen mit `cargo build --release --locked`.
- Der Linux-Job installiert die System-Abhängigkeiten aus `BUILD.md` §5:
  `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libssl-dev pkg-config build-essential`.
- **Es werden keine SHA256-Prüfsummen erzeugt** (nicht implementiert) — im Release liegt **keine**
  mitgelieferte Prüfsummendatei. Stattdessen erzeugt der Release-Workflow **ab dem ersten mit ihm
  gebauten Release** GitHub Artifact Attestations (Build-Provenance, §3.3), prüfbar mit
  `gh attestation verify`; live verifiziert sind sie noch **nicht** (§3.3). Davon zu unterscheiden ist
  der `sha256`-**Digest**, den die GitHub-API **je Asset** liefert (abfragbar z. B. mit
  `gh release view <tag> --json assets`); das ist keine Datei im Release.

---

## 3. Signierung und Herkunftsnachweis

Alle Signaturschritte laufen **ausschließlich** im Release-Workflow, also nur bei einem Tag-Push im
Haupt-Repo. Fehlt das jeweilige Secret, wird unsigniert gebaut und veröffentlicht; ist es gesetzt und
der Schritt scheitert, **scheitert der Job** — es entsteht kein stilles unsigniertes Release.

### 3.1 Windows: SignPath (Authenticode)

Der Windows-Build wird über die **SignPath Foundation** Authenticode-signiert. Voraussetzung ist ein
`SIGNPATH_API_TOKEN`; zusätzlich müssen die Organisation/Projekt-/Policy-Slugs als Repository-
**Variablen** hinterlegt sein.

Ablauf im Workflow (`release.yml`, Windows-Bein, nur wenn `SIGNPATH_ENABLED == 'true'`):

1. Die unsignierte `target/release/prime-photos.exe` wird als Workflow-Artefakt
   `windows-signing-input` hochgeladen (`if-no-files-found: error`, `retention-days: 1`).
2. `signpath/github-action-submit-signing-request@f6d0478` (`# v3`) stellt die Signieranfrage mit
   `github-artifact-id: ${{ steps.signpath-upload.outputs.artifact-id }}`, `wait-for-completion: true`
   und `output-artifact-directory: signed`. Weil `actions/upload-artifact@v4` das Artefakt immer als
   ZIP ablegt, entpackt die Action das signierte ZIP nach `signed/` (Default `skip-decompress: false`);
   dort liegt dann `signed/prime-photos.exe`.
3. Die signierte Binary ersetzt die unsignierte:
   `Copy-Item -Force signed/prime-photos.exe target/release/prime-photos.exe`.
4. `Get-AuthenticodeSignature` auf `target/release/prime-photos.exe`; bei `Status -ne 'Valid'` bricht
   der Job ab (PowerShell `throw`).

**Eignungsprüfung (SignPath Foundation):** Der Dienst ist für **Open-Source-Projekte** ohne
kommerzielle Absicht gedacht; Voraussetzung sind ein öffentliches Repo, eine freie Lizenz
(hier `MIT OR Apache-2.0`), ein nachvollziehbarer Build im CI und ein Projektverantwortlicher, der
die Bedingungen akzeptiert. Die Freigabe erfolgt durch SignPath, nicht durch dieses Repo.

**Manuelle Schritte zur Registrierung** (einmalig, außerhalb des Repos):

1. Auf <https://signpath.io> mit dem GitHub-Konto anmelden und die **SignPath Foundation** als
   Signatur-Anbieter beantragen (Projekt-URL, Lizenz, Build-Workflow benennen).
2. **Nach der Freigabe:** Organisation anlegen, Projekt `prime-photos` und eine Signing-Policy
   (z. B. `release-signing`) erstellen.
3. Das vordefinierte **Trusted Build System `GitHub.com`** mit dem Projekt verknüpfen und die
   **SignPath GitHub App** für dieses Repository installieren. Die App ist empfohlen, weil die
   Policy-Auswertung anhand des Audit-Logs sie voraussetzt; ohne sie braucht der Connector die
   Berechtigungen `actions: read` und `contents: read` des Jobs, die `release.yml` bereits
   gewährt. Dokumentation:
   <https://about.signpath.io/documentation/trusted-build-systems/github>.
4. **Artifact Configuration** anlegen — Wurzelknoten ist das ZIP-Artefakt, darin die zu
   signierende PE-Datei:

   ```xml
   <artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
     <zip-file>
       <pe-file path="prime-photos.exe">
         <authenticode-sign/>
       </pe-file>
     </zip-file>
   </artifact-configuration>
   ```

   `actions/upload-artifact@v4` erzeugt immer ein ZIP (es gibt dort keinen `archive`-Input), deshalb
   muss die Wurzel der Konfiguration `zip-file` sein. `archive: false` gäbe es erst mit einer neueren
   `upload-artifact`-Hauptversion und wird mit dieser Änderung bewusst nicht eingeführt.
5. API-Token für den CI-Benutzer erzeugen und als Secret hinterlegen; die Slugs als Variablen.

> **Timeout:** Die Signing-Policy muss **ohne manuelle Freigabe** auskommen. Der Workflow wartet mit
> dem Default der SignPath-Action (`wait-for-completion-timeout-in-seconds: 600`) und schlägt danach
> fehl; ist ein Approval-Gate gewollt, muss der Wert in `release.yml` erhöht werden.

| Name | Art | Inhalt |
|---|---|---|
| `SIGNPATH_API_TOKEN` | Secret | API-Token des CI-Benutzers; sein Vorhandensein schaltet die Signatur ein |
| `SIGNPATH_ORGANIZATION_ID` | Variable | Organisations-ID aus SignPath |
| `SIGNPATH_PROJECT_SLUG` | Variable | Projekt-Slug (z. B. `prime-photos`) |
| `SIGNPATH_SIGNING_POLICY_SLUG` | Variable | Signing-Policy (z. B. `release-signing`) |
| `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` | Variable | Slug der Artifact Configuration |

**Grenze SmartScreen:** Eine gültige Authenticode-Signatur belegt Urheber und Unverändertheit, baut
aber **kein** SmartScreen-Reputation auf. Der Hinweis „Unbekannter Herausgeber" kann trotz gültiger
Signatur erscheinen, bis genügend Downloads ohne Beanstandung vorliegen.

> **Nicht live verifiziert:** Weder die Signieranfrage noch `Get-AuthenticodeSignature` wurden
> ausgeführt — es existiert noch kein SignPath-Konto/Token für dieses Projekt (offener Punkt,
> `AGENTS.todo.md`). Bis dahin wird Windows **unsigniert** veröffentlicht.

### 3.2 Linux: abgetrennte GPG-Signatur

Ausgeliefert wird **genau ein** Linux-Format: `Prime-Photos-linux-x64.tar.gz`. Es gibt **kein**
AppImage, **kein** `.deb` und **kein** `.rpm`. Bei konfiguriertem `GPG_PRIVATE_KEY` legt der Workflow
zusätzlich `Prime-Photos-linux-x64.tar.gz.asc` (abgetrennt, ASCII-armored) und
`Prime-Photos-release-key.asc` (öffentlicher Schlüssel) ins Release.

Ablauf in `scripts/sign-linux-release.sh` (vom Linux-Bein aufgerufen, nur wenn `GPG_PRIVATE_KEY`
gesetzt ist):

1. Temporäres `GNUPGHOME` (Modus 700) und eine temporäre Schlüsseldatei anlegen; `trap` räumt beides
   auf (`gpgconf --kill all`, `rm -rf`).
2. Schlüssel importieren (`gpg --batch --import`).
3. Signatur erzeugen: `gpg --batch --yes --pinentry-mode loopback --armor --detach-sign`, mit
   `--local-user "$GPG_KEY_ID"` falls gesetzt; eine nicht-leere `GPG_PASSPHRASE` wird über
   `--passphrase-fd 0` eingespeist, sonst wird ohne Passphrase signiert.
4. Signatur gegen das Artefakt prüfen (`gpg --verify`).
5. Öffentlichen Schlüssel nach `Prime-Photos-release-key.asc` exportieren (mit `$GPG_KEY_ID`, falls
   gesetzt).

**Schlüssel-Einrichtung** (einmalig, lokal — das Schlüsselmaterial gehört **nicht** ins Repo):

```sh
gpg --quick-generate-key "Prime Photos Release <release@example.invalid>" rsa4096 sign 2y
gpg --armor --export-secret-keys <FINGERPRINT>   # -> Secret GPG_PRIVATE_KEY
gpg --armor --export <FINGERPRINT>               # -> öffentlicher Schlüssel (Publikation)
gpg --fingerprint <FINGERPRINT>                  # Fingerprint für die Doku
```

| Name | Art | Inhalt |
|---|---|---|
| `GPG_PRIVATE_KEY` | Secret | ASCII-armored privater Signaturschlüssel; sein Vorhandensein schaltet die Signatur ein |
| `GPG_PASSPHRASE` | Secret | Passphrase des Schlüssels (leer/fehlend = keiner) |
| `GPG_KEY_ID` | Secret | Schlüssel-ID/Fingerprint, falls der Import mehrdeutig ist |

**Fingerprint (Vertrauensanker):** Der Fingerprint des Release-Schlüssels wird nach der
Schlüsselerzeugung **hier eingetragen** — siehe `AGENTS.todo.md`; es gibt ihn noch nicht, und es wird
hier bewusst keiner erfunden:

```
FINGERPRINT: <wird nach der Schlüsselerzeugung hier eingetragen — siehe AGENTS.todo.md>
```

Derselbe Fingerprint gehört zusätzlich in die Release-Notes. Nur über ihn lässt sich der importierte
öffentliche Schlüssel (§7) gegen eine **unabhängige** Quelle prüfen.

**Prüfung durch Nutzer** (nach dem Download; vollständige Reihenfolge in §7):

```sh
gpg --import Prime-Photos-release-key.asc
gpg --fingerprint                     # mit dem oben veröffentlichten Fingerprint vergleichen
gpg --verify Prime-Photos-linux-x64.tar.gz.asc Prime-Photos-linux-x64.tar.gz
```

**Grenze:** Desktop-Umgebungen (GNOME/KDE) validieren eine solche Signatur **nicht** automatisch; sie
ist ein manueller Prüfschritt. Es gibt keine Signatur des Paketmanagers und keine Paketquellen.

> **Nicht live verifiziert:** Der GPG-Schlüssel für dieses Projekt existiert noch nicht (offener
> Punkt, `AGENTS.todo.md`); der Workflow-Schritt wurde daher noch nie mit echten Secrets ausgeführt.
> Das Skript selbst wurde lokal mit einem Wegwerf-Schlüssel in `/tmp` geprüft (Signatur erzeugt,
> `gpg --verify` grün, Public Key exportiert); im Repo liegt kein Schlüsselmaterial.

### 3.3 Artifact Attestations (Build-Provenance)

Der `release`-Job erzeugt **GitHub Artifact Attestations** für alle heruntergeladenen
Release-Artefakte (`actions/attest-build-provenance@4d10147`, `# v4.2.2`,
`subject-path: "artifacts/Prime-Photos-*"`). Die Attestation wird **vor** dem Release-Upload erzeugt
und belegt, dass die Dateien aus diesem Repo/Workflow/Commit gebaut wurden (Sigstore/OIDC).

Benötigte Job-Permissions: `id-token: write` (OIDC-Token) und `attestations: write` (Attestation
schreiben); `contents: write` für den Release-Upload, `actions: read` für die Artefakt-Metadaten.

Prüfung durch Nutzer:

```sh
gh attestation verify Prime-Photos-linux-x64.tar.gz --repo reisi007/prime-photos
gh attestation verify Prime-Photos-windows-x64.zip --repo reisi007/prime-photos
```

> **Nicht live verifiziert:** Es wurde noch **keine** Attestation erzeugt — der erste Beleg ist der
> erste Release, der mit diesem Workflow gebaut wird. Belegt ist bis dahin nur die
> Workflow-Konfiguration, nicht `gh attestation verify` gegen ein echtes Release.

### 3.4 Was die Mechanismen belegen

| Mechanismus | Belegt | Belegt **nicht** |
|---|---|---|
| SignPath Authenticode | Urheber (SignPath Foundation), Unverändertheit der `.exe` seit der Signatur | SmartScreen-Reputation, Freiheit von Schadcode, Laufzeitverhalten |
| GPG-Signatur (`.asc`) | dass die Datei mit dem Release-Schlüssel signiert und nicht verändert wurde | Vertrauen in den Schlüssel selbst (Fingerprint-Prüfung nötig), automatische Prüfung durch das OS |
| Artifact Attestation | Herkunft: Repo, Workflow, Commit und Digest des Artefakts | Wer den Commit erstellt hat, Qualität/Laufzeitverhalten |
| macOS-Codesignatur (lokal) | Bundle-Integrität und Identität auf dem eigenen Rechner | Vertrauen auf fremden Macs — keine Notarisierung (§5–§6) |

---

## 4. macOS: lokaler Build

```sh
./packaging/make-app.sh
# Ergebnis: target/dist/Prime Photos.app   (arm64, mit der lokalen Identität
# "Prime Photos Local Signer" signiert; ad-hoc nur als Rückfall)
```

- Das Skript baut `cargo build --release`, assembliert das Bundle und signiert mit der lokalen
  Codesigning-Identität `Prime Photos Local Signer`, wenn sie im Schlüsselbund liegt. **Ad-hoc**
  (`codesign --sign -`) wird nur verwendet, wenn die Identität fehlt **und** `CODESIGN_IDENTITY`
  nicht gesetzt oder leer ist. Jeder nicht-leere Wert in `CODESIGN_IDENTITY` gewinnt immer: ein
  anderer Identitätsname ebenso wie `-` (erzwingt ad-hoc).
- **Warum die Identität der Standard ist:** Bei ad-hoc ist das Designated Requirement der cdhash
  und ändert sich mit jedem Build — die Keychain-ACL des gespeicherten Refresh-Tokens passt nach
  einem selbst gebauten Update nicht mehr, und macOS fragt beim Start nach dem Passwort für den
  Schlüsselbund „Anmeldung" (am 2026-10-26 live beobachtet). Eine Zertifikats-Identität hält das
  Designated Requirement (Identifier + Zertifikat) über Rebuilds stabil; dass der Dialog damit
  dauerhaft ausbleibt, ist die **erwartete** Wirkung. Abgenommen ist sie noch nicht: beim Start des
  0.1.4-Bundles mit dieser Identität erschien der `SecurityAgent`-Dialog erneut und verschwand nach
  wenigen Sekunden von selbst, die App meldete sich danach normal an (§9, fünfter Lauf). Auf
  Rechnern ohne diese Identität (andere Rechner, Rechner ohne den Schlüsselbund-Eintrag) bleibt es
  beim bisherigen ad-hoc-Verhalten, und für **fremde Macs** ändert sich nichts — Gatekeeper blockt
  weiterhin (§6).
- **App-Icon:** das Dock-/Taskbar-Icon wird **im Build** erzeugt, nichts ist eingecheckt —
  `build.rs` rastert `assets/logo.svg` (die SVG-Quelle) in die zehn iconset-Größen nach
  `target/pp-appicon.iconset/`, und `make-app.sh` wandelt das Verzeichnis mit
  `iconutil -c icns` in `Contents/Resources/AppIcon.icns` um. `Contents/Info.plist` verweist
  per `CFBundleIconFile = AppIcon` darauf. (`CARGO_TARGET_DIR` wird dabei nicht
  berücksichtigt: das Skript liest das iconset aus `target/`.)
- Kein `codesign --deep` (von Apple abgeraten). Details: `packaging/README.md`.
- Das fertige Bundle wird bei Bedarf **manuell** an ein bestehendes GitHub Release gehängt — das ist
  **nicht** automatisierter Teil des Workflows. Ausgeliefert wird **nur das DMG** (Entscheidung
  2026-10-17: für macOS ist das ZIP-Asset redundant); als Asset-Name gilt das Schema
  `Prime-Photos-macos-arm64.dmg`:
  - **DMG** (lokale Quelle war `target/dist/PrimePhotos.dmg`): `gh release upload <tag> <datei>`

  Der **Asset-Name ist der Dateiname** der hochgeladenen Datei. `gh release upload <tag> <datei>#<text>`
  setzt **nur das `label`-Feld**, **nicht** den Asset-Namen: ein Upload von `PrimePhotos.dmg` mit
  `#Prime-Photos-macos-arm64.dmg` hieß danach weiterhin `PrimePhotos.dmg`. Korrekturweg — lokale Datei
  korrekt benennen, dann das falsch benannte Asset löschen und neu hochladen:
  `gh release delete-asset <tag> <falscher-name> --yes`.

### Ablauf beim Release (genau diese Reihenfolge)

Zuerst die `.app` bauen, **dann** das DMG — es entsteht aus **demselben** Bundle. Alle erzeugten
Dateien liegen in `target/dist/`.

1. **Commit-Stempel erzwingen** — `touch` **vor** dem Build aus Schritt 2:

   ```sh
   touch build.rs
   ```

   Grund: `build.rs` läuft bei unverändertem Quellstand **nicht** neu, der eingebettete Stempel
   `PRIME_PHOTOS_GIT_COMMIT` (gesetzt in `build.rs`, für das Über-Fenster) bleibt dann auf einem
   **alten** Commit stehen — beim 0.1.1-Build zeigte er `0a44a37` statt des Tag-Commits; erst
   `touch build.rs` machte `57ca172` daraus. Alternative: `cargo clean -p prime-photos`.

2. **Bundle bauen** (Default `VERSION=0.1.7`, `BUILD=7`, mit der lokalen Identität
   `Prime Photos Local Signer` signiert; ad-hoc nur als Rückfall):

   ```sh
   ./packaging/make-app.sh
   # Ergebnis: target/dist/Prime Photos.app
   ```

3. **DMG** — im `target/dist/`, Volname `Prime Photos`:

   ```sh
   hdiutil create -volname "Prime Photos" -srcfolder "Prime Photos.app" -ov -format UDZO Prime-Photos-macos-arm64.dmg
   hdiutil verify Prime-Photos-macos-arm64.dmg
   ```

   `hdiutil` meldet sich inzwischen selbst als veraltet und verweist auf einen Nachfolger —
   Details siehe der Hinweis am Ende dieses Abschnitts.

4. **App installieren** (vom Repo-Wurzelverzeichnis aus):

   ```sh
   rm -rf "/Applications/Prime Photos.app"
   ditto "target/dist/Prime Photos.app" "/Applications/Prime Photos.app"
   codesign --verify --verbose=1 "/Applications/Prime Photos.app"
   open "/Applications/Prime Photos.app"
   ```

   `ditto` statt `cp -R`, weil es Signatur und Bundle-Struktur erhält. Der Startweg bei
   **nicht notarisierten Downloads** ist §5; das hier lokal installierte Bundle betrifft das nicht.

5. **Ans GitHub-Release hängen:** das DMG hochladen.
   Der **Asset-Name ist der Dateiname** — siehe der vorstehende Aufzählungspunkt.

> **Nicht verifiziert:** die `hdiutil`-Aufrufe (`attach`/`detach`/`create`) warnen inzwischen und
> verweisen auf `diskutil image attach --mountOptions nobrowse --readOnly`, `diskutil eject` bzw.
> `diskutil image create from/blank --volumeName --format`. Ob/wann hier umgestellt wird, ist offen.

---

## 5. Gatekeeper-Workaround (nicht notarisierte macOS-Downloads)

**Funktionsweise:** Das Bundle ist **nicht notarisiert** und **nicht von einer Developer-ID
signiert** (selbstsigniert bzw. ad-hoc). Lädt man es aus dem Netz, setzt macOS die
Quarantäne-Markierung, und **Gatekeeper blockt** den Start. Dann greift einer dieser Wege:

**a) Finder (empfohlen)**

1. Im Finder **Rechtsklick** auf `Prime Photos.app` → **„Öffnen"**.
2. Im Warn-Dialog erscheint „Öffnen" als Option → **nochmals „Öffnen"** klicken.

**b) Terminal (Quarantäne entfernen)**

```sh
xattr -dr com.apple.quarantine "/Applications/Prime Photos.app"
open "/Applications/Prime Photos.app"
```

**c) Systemeinstellungen (falls der Dialog dort angeboten wird)**

*Systemeinstellungen → Datenschutz & Sicherheit* → Abschnitt zur blockierten App →
**„Dennoch öffnen"**.

> **Nicht verifiziert:** ob der Workaround auf jeder macOS-Version/Architektur greift, ist hier
> nicht geprüft. Er ist der übliche Weg für ad-hoc-/selbstsignierte Bundles.

---

## 6. Später optional: signiertes macOS-Bundle

**Nicht eingerichtet.** Nur erwähnt, damit die Voraussetzungen klar sind. Ein Build, den **fremde
Macs** ohne Warnung akzeptieren, bräuchte:

- eine **„Developer ID Application"-Identität** aus dem **Apple Developer Program (99 $/Jahr)**,
- **Notarisierung** (`xcrun notarytool submit … --wait`) und **Stapling** (`xcrun stapler staple`).

Erst Developer-ID-Signatur + Notarisierung hebt die Gatekeeper-Blockade für Dritte auf;
ad-hoc/selbstsigniert ist **nur lokal** vertraut. Wäre macOS später doch im CI, bräuchte es als
GitHub-Secrets: `APPLE_CERTIFICATE` (+ Passwort), `APPLE_SIGNING_IDENTITY`, `APPLE_ID` +
`APPLE_APP_SPECIFIC_PASSWORD` und `APPLE_TEAM_ID`. **Aktuell wird davon nichts verwendet.**

---

## 7. Einen heruntergeladenen Build prüfen

**Linux**

```sh
tar -xzf Prime-Photos-linux-x64.tar.gz
./prime-photos
```

Nur wenn GPG konfiguriert war (`.asc` liegt im Release), zusätzlich die Signatur prüfen (§3.2):

```sh
gpg --import Prime-Photos-release-key.asc
gpg --fingerprint                                    # Fingerprint notieren
# Mit dem in §3.2 (und in den Release-Notes) veröffentlichten Fingerprint vergleichen.
gpg --verify Prime-Photos-linux-x64.tar.gz.asc Prime-Photos-linux-x64.tar.gz
```

Die Reihenfolge ist entscheidend: Erst nach dem Fingerprint-Vergleich belegt `gpg --verify`, dass die
Datei von **diesem** Schlüssel stammt. Schlüssel und Artefakt aus **derselben** Quelle zu importieren
beweist nur ihre gegenseitige Konsistenz, **nicht** ihre Authentizität — wer das Release manipuliert,
kann beide ersetzen.

**Windows**

```powershell
Expand-Archive Prime-Photos-windows-x64.zip -DestinationPath .
.\prime-photos.exe
# Nur wenn SignPath aktiv war:
Get-AuthenticodeSignature .\prime-photos.exe   # Status "Valid" bei signiert
```

Unsigniert/ohne gültige Signatur ⇒ SmartScreen: „Weitere Informationen" → „Trotzdem ausführen".
Auch **mit** gültiger Signatur kann SmartScreen warnen — Reputation ist davon unabhängig (§3.1).

**Herkunftsnachweis (alle Plattformen, wenn die Attestation vorhanden ist)**

```sh
gh attestation verify Prime-Photos-linux-x64.tar.gz --repo reisi007/prime-photos
gh attestation verify Prime-Photos-windows-x64.zip --repo reisi007/prime-photos
```

**macOS**

DMG öffnen und **Prime Photos** nach `/Applications` ziehen (Finder-Weg); per Terminal:

```sh
hdiutil attach Prime-Photos-macos-arm64.dmg
ditto "/Volumes/Prime Photos/Prime Photos.app" "/Applications/Prime Photos.app"
codesign --verify --verbose=2 "/Applications/Prime Photos.app"   # gültige Signatur?
codesign -dv "/Applications/Prime Photos.app"                    # Authority / "Signature=adhoc"
spctl -a -vvv --type execute "/Applications/Prime Photos.app"    # Gatekeeper-Bewertung
```

Bei einem ad-hoc-/selbstsignierten Bundle meldet `spctl` „rejected" → Workaround aus §5.

---

## 8. Fork-PRs und Secrets

Secrets werden **nicht** an Läufe aus Forks weitergereicht (GitHub-Verhalten). Ein `pull_request`
aus einem Fork sieht die Signatur-Secrets nicht; Build und Test laufen ohne Secrets, und die CI
(`ci.yml`) verwendet überhaupt keine Secrets. Release-Läufe entstehen ausschließlich durch einen
`push` eines Tags im Haupt-Repo und erhalten die Secrets; die Signaturschritte (SignPath, GPG) sind
damit **tag-only**. Dependabot-PRs laufen wie jeder andere PR ohne Secrets durch die CI; die
Auto-Merge-Workflows (`.github/workflows/dependabot-metadata.yml`,
`.github/workflows/dependabot-auto-merge.yml`) lesen nur Metadaten und führen **keinen** PR-Code aus.

---

## 9. Belegter Stand und offene Punkte

**Durch den ersten echten Lauf belegt (2026-10-17):**

- **Release ausgelöst:** annotierter Tag `v0.1.0` auf Commit `1de3e2b`; GitHub-Release `v0.1.0`,
  veröffentlicht 2026-10-17T09:30:37Z, **nicht** Draft, **nicht** Pre-Release.
- **Run:** GitHub-Actions-Run `39020871814` (`release.yml`, ausgelöst durch den Tag-Push). Jobs
  `Build linux-x64` (5m03s), `Build windows-x64` (7m59s), `Publish GitHub Release` (12s) — alle grün.
  Damit ist auch die Kombination `softprops/action-gh-release@v2` + `actions/download-artifact@v4`
  **durch einen echten Lauf belegt**.
- **Assets** (Name — Bytes — sha256):

  | Asset | Bytes | sha256 |
  |---|---|---|
  | `Prime-Photos-linux-x64.tar.gz` | 12.296.488 | `f9d0a2039f1fd028e98ea78e7edb6e792337245c411a65778f4c17d33570c8e4` |
  | `Prime-Photos-windows-x64.zip` | 8.479.078 | `5233e6e0b72c6e2b9775d022420edcec14a5a97cf3a335870477aaed5cbae4c4` |
  | `Prime-Photos-macos-arm64.dmg` | 9.429.019 | `a2cb9a520ef83ad137f7171758507c8194ab92c6a1ceabdb7127bf4e8ab70843` |

  Das DMG wurde **manuell** angehängt (§4); sein sha256 nach dem Upload war identisch mit der lokalen
  Datei.
- **Release-Notes:** `generate_release_notes: true` erzeugte beim **ersten** Release nur die Zeile
  `**Full Changelog**: https://github.com/reisi007/prime-photos/commits/v0.1.0` (kein Vorgänger-Tag,
  also keine Commit-Liste); die Beschreibung wurde danach von Hand ersetzt. Titel des Releases:
  `v0.1.0`.
- **DMG-Inhalt geprüft:** gemountet → `CFBundleShortVersionString = 0.1.0`, `CFBundleVersion = 1`,
  `CFBundleIdentifier = de.reisinger.prime-photos`; `codesign --verify --deep --strict` auf das
  enthaltene Bundle **ok**; `hdiutil verify` **VALID**.

**Durch den zweiten echten Lauf belegt (2026-10-17):**

- **Release ausgelöst:** annotierter Tag `v0.1.1` auf Commit `57ca172`; GitHub-Release `v0.1.1`,
  veröffentlicht 2026-10-17T17:03:56Z, **nicht** Draft, **nicht** Pre-Release.
- **Run:** GitHub-Actions-Run `39051515118` (`release.yml`, ausgelöst durch den Tag-Push). Jobs
  `Build windows-x64` (8m9s), `Build linux-x64` (7m12s), `Publish GitHub Release` (12s) — alle grün.
- **Assets** (Name — Bytes — sha256):

  | Asset | Bytes | sha256 |
  |---|---|---|
  | `Prime-Photos-linux-x64.tar.gz` | 12.308.405 | `4ccf24e687c3537a5e78e8302ff87132583928d3440e9264dd2afb8673fea1be` |
  | `Prime-Photos-macos-arm64.dmg` | 9.426.818 | `51a4f9425be8a461c381eefa48aca892f816fcd77039ff48fb81573dd1249446` |
  | `Prime-Photos-macos-arm64.zip` | 8.489.069 | `7a18cb9a3851762b9c94458d90f7e407d1e380f55fa27118156981ce0cb83b7b` |
  | `Prime-Photos-windows-x64.zip` | 8.579.930 | `ca02d28cc4561b1a465606db683dce26cdf7e6bfbd546fbf476e16fcc073e4b7` |

  Die Tabelle ist der **historische** Beleg des Release-Stands vom 2026-10-17. Das ZIP-Asset
  `Prime-Photos-macos-arm64.zip` wurde **am 2026-10-17 nachträglich entfernt** (Entscheidung:
  für macOS wird nur noch das DMG ausgeliefert) — `v0.1.1` trägt seither genau drei Assets
  (`linux-x64.tar.gz`, `macos-arm64.dmg`, `windows-x64.zip`); `v0.1.0` war ohnehin DMG-only.

  Zip und DMG wurden **manuell** ans Release gehängt (§4) — der Workflow baut nur Windows/Linux.
  Der `sha256` des DMG war nach dem Upload identisch mit der lokalen Datei (`shasum -a 256`).
- **macOS-Bundle (lokal aus dem Tag gebaut, §4):** `./packaging/make-app.sh` mit `VERSION=0.1.1`,
  `BUILD=2`, ad-hoc signiert, Volname `Prime Photos`; `CFBundleShortVersionString = 0.1.1`,
  `CFBundleVersion = 2`, `CFBundleIdentifier = de.reisinger.prime-photos`; der Commit-Stempel im
  Über-Fenster steht dank `touch build.rs` auf `57ca172`; `hdiutil verify` auf dem DMG: **VALID**.
- **Windows unsigniert:** die Authenticode-Schritte des Workflows wurden ohne Secrets übersprungen
  (§3) — dieselbe Lage wie bei v0.1.0.
- **Release-Notes:** `generate_release_notes: true` erzeugte diesmal eine echte Commit-Liste —
  `**Full Changelog**: https://github.com/reisi007/prime-photos/compare/v0.1.0...v0.1.1`.
  Titel des Releases: `v0.1.1`.
- **Noch nicht live abgenommen:** die beiden 0.1.1-Verhaltensänderungen — Ordner *während* eines
  Laufs hinzufügen; Laufwerk abziehen/anklemmen → Vollscan + Wiederanlauf — sind noch nicht an
  einem echten Laufwerk bestätigt (im Ledger als offener Punkt geführt).

**Durch den dritten echten Lauf belegt (2026-10-17):**

- **Release ausgelöst:** annotierter Tag `v0.1.2` (Tag-Objekt `ca2d140e…`) auf Commit `1aceb25`;
  GitHub-Release `v0.1.2`, veröffentlicht 2026-10-17T18:59:31Z, **nicht** Draft, **nicht**
  Pre-Release.
- **Run:** GitHub-Actions-Run `39059880076` (`release.yml`, ausgelöst durch den Tag-Push,
  `headSha = 1aceb25`). Jobs `Build linux-x64` (4m45s), `Build windows-x64` (8m38s),
  `Publish GitHub Release` (10s) — alle grün.
- **Assets** (Name — Bytes — sha256):

  | Asset | Bytes | sha256 |
  |---|---|---|
  | `Prime-Photos-linux-x64.tar.gz` | 12.318.616 | `eaf7f67f7f8d916d7d654bb63035b568c43d378a1f0da99c943e18931eaedd20` |
  | `Prime-Photos-macos-arm64.dmg` | 9.441.910 | `dcf1ae910738d990bda524669f17f263cc64d7fa67fb0dda8fd9ec3fcf425f7b` |
  | `Prime-Photos-windows-x64.zip` | 8.588.756 | `538a1d0c22a413164f09387ad375e72219cac238fe3b6f37d1c119b4c5a41adc` |

  `v0.1.2` trägt damit von Anfang an nur **drei** Assets (das macOS-ZIP entfällt seit der
  Asset-Entscheidung vom 2026-10-17, §4). Das DMG wurde **manuell** ans Release gehängt (§4) — der
  Workflow baut nur Windows/Linux; seine sha256 wurde gegen die lokale Datei geprüft.
- **macOS-Bundle (lokal aus dem Tag gebaut, §4):** `./packaging/make-app.sh` mit `VERSION=0.1.2`,
  `BUILD=2`, ad-hoc signiert, Volname `Prime Photos`; `CFBundleShortVersionString = 0.1.2`,
  `CFBundleVersion = 2`, `CFBundleIdentifier = de.reisinger.prime-photos` (identisch im lokalen
  `target/dist`-Bundle und im installierten `/Applications`-Bundle, per `plutil -p` geprüft); der
  Commit-Stempel im Über-Fenster steht dank `touch build.rs` auf `1aceb25`; `hdiutil verify` auf dem
  DMG: **VALID**.
- **Windows unsigniert:** die Authenticode-Schritte des Workflows wurden ohne Secrets übersprungen
  (§3) — dieselbe Lage wie bei v0.1.0 und v0.1.1.
- **Release-Notes:** `generate_release_notes: true` erzeugte die Commit-Liste
  `**Full Changelog**: https://github.com/reisi007/prime-photos/compare/v0.1.1...v0.1.2`.
- **Inhalt dieses Releases:** das neue **Hintergrund-Verhalten** — das X versteckt nur das Fenster,
  die App läuft weiter; **zum damaligen Stand blieb das Dock-Icon** beim versteckten Fenster sichtbar
  (durch den 0.1.3-Fix inzwischen überholt: das Dock-Icon folgt der Fenstersichtbarkeit, und beim
  stillen Login-Item-Start wird die erste AppKit-Aktivierung ignoriert); zurück kommt man per
  App-Neustart oder Tray-Eintrag, und es läuft **immer nur eine Instanz** (macOS `LSMultipleInstancesProhibited` plus
  Socket-Wächter). Dazu die README-Aktualisierung (jetzt versionsfrei, Regel in `AGENTS.md`) und die
  macOS-Asset-Policy (nur DMG).
- **Live abgenommen:** der User hat das Hintergrund-Verhalten am 0.1.2-Build am 2026-10-17 manuell
  abgenommen („manuell ok").
- **Weiterhin offen (kein 0.1.2-Defekt):** Windows/Linux werden weiterhin nur **kompiliert**, nicht
  laufen getestet; die Live-Abnahme der **Laufwerk-Rückkehr** aus einem älteren Befund steht noch
  aus (eigener Ledger-Punkt).

**Durch den vierten echten Lauf belegt (2026-10-19):**

- **Release ausgelöst:** annotierter Tag `v0.1.3` (Tag-Objekt `dafed2d…`) auf Commit `bb8b54c`;
  GitHub-Release `v0.1.3`, veröffentlicht 2026-10-19T09:02:52Z, **nicht** Draft, **nicht** Pre-Release.
- **Runs:** Push-CI `39225782646` und Release-Workflow `39226119742` (`release.yml`, ausgelöst durch
  den Tag-Push); Linux (7m03s), Windows (8m06s) und `Publish GitHub Release` (11s) — alle grün.
- **Assets** (Name — Bytes — sha256):

  | Asset | Bytes | sha256 |
  |---|---|---|
  | `Prime-Photos-linux-x64.tar.gz` | 12.319.483 | `fbefabd6ae3243372557e7a967c4e1430b66b9bc38411c5d5a894938efce160b` |
  | `Prime-Photos-macos-arm64.dmg` | 9.441.494 | `98250242b6b43c14aac0cbd047ab84182456274b2d0afa2dbe966feb62542117` |
  | `Prime-Photos-windows-x64.zip` | 8.587.250 | `a80bf168a0d6c364ffa3c7a131feafd5ad05cb445ec218adc2e508ed6b0f8ea0` |

  `v0.1.3` trägt drei Assets. Das macOS-DMG wurde **manuell** angehängt (§4); sein SHA-256-Digest
  stimmt mit der lokalen Datei überein.
- **macOS-Bundle (lokal aus dem Release-Commit gebaut, §4):** `VERSION=0.1.3`, `BUILD=3`, ad-hoc
  signiert, Volname `Prime Photos`; `CFBundleShortVersionString = 0.1.3`, `CFBundleVersion = 3`,
  `CFBundleIdentifier = de.reisinger.prime-photos`; Commit-Stempel `bb8b54c`. `codesign --verify`
  auf Bundle und `hdiutil verify` auf DMG: **gültig**. Das Bundle wurde in `/Applications` installiert.
- **Windows unsigniert:** Authenticode-Schritte wurden mangels Secrets übersprungen (§3).
- **Release-Notes:** `generate_release_notes: true` erzeugte
  `**Full Changelog**: https://github.com/reisi007/prime-photos/compare/v0.1.2...v0.1.3`.
- **Inhalt dieses Releases:** stiller Login-Item-Start ohne Fenster oder Dock-Icon. Die erste
  `NSApplicationDidBecomeActiveNotification` wird beim stillen Start als Launch-Aktivierung behandelt,
  nicht als Fenster-öffnen-Anfrage. Der Nutzer bestätigte das Verhalten nach echtem Ab-/Anmelden.

**Weiterhin offen / nicht verifiziert:**

- **SignPath (§3.1) nicht live:** Registrierung/Freigabe durch die SignPath Foundation steht aus,
  ebenso `SIGNPATH_API_TOKEN` und die vier Slugs; bis dahin wird Windows **unsigniert**
  veröffentlicht. Der Signaturschritt (`Get-AuthenticodeSignature`) wurde nie ausgeführt.
- **GPG (§3.2) nicht live:** Release-Schlüssel, `GPG_PRIVATE_KEY`/`GPG_PASSPHRASE`/`GPG_KEY_ID` und
  der Fingerprint in der Doku stehen aus; bis dahin wird Linux **unsigniert** veröffentlicht. Das
  Skript ist nur lokal mit einem Wegwerf-Schlüssel geprüft.
- macOS-Notarisierung/Stapling (§6) nicht eingerichtet und nicht geprüft.
- Keine SHA256-Prüfsummen-**Dateien** im Release (nicht implementiert); die GitHub-API liefert je
  Asset aber ein `sha256`-Digest (§2).
- **Artifact Attestations (§3.3) nicht live verifiziert:** Sie entstehen erst ab dem ersten mit
  diesem Workflow gebauten Release; `gh attestation verify` wurde gegen noch kein Release ausgeführt.
- **Dependabot-Auto-Merge nicht live verifiziert:** der erste echte Dependabot-PR muss zeigen,
  dass das Label `automerge` bei Patch/Minor gesetzt und der Merge erst nach grünem `CI`-Lauf
  ausgeführt wird; Major-Updates dürfen kein Label erhalten.
- **Beobachtung (nicht Teil dieses Stands):** beide Job-Arten melden `Node.js 20 is deprecated` für
  `actions/checkout@v4`, `actions/upload-artifact@v4`, `actions/download-artifact@v4` (werden auf
  Node.js 24 gezwungen); außerdem der Hinweis, dass `ubuntu-latest` ab 2026-10-19 auf Ubuntu 26
  migriert. Die Anhebung ist eine eigene, spätere Welle.

**Durch den fünften echten Lauf belegt (2026-10-26):**

- **Release ausgelöst:** annotierter Tag `v0.1.4` (Tag-Objekt `e0146a7…`) auf Commit `34b0f05`;
  GitHub-Release `v0.1.4`, veröffentlicht 2026-10-26T06:53:04Z, **nicht** Draft, **nicht**
  Pre-Release. Titel des Releases: `v0.1.4`.
- **Runs:** Push-CI `40281685970` (`ci.yml`, Linux build & test) grün; Release-Workflow `40281848886`
  (`release.yml`, ausgelöst durch den Tag-Push) — Jobs `Build linux-x64` (5m27s),
  `Build windows-x64` (5m42s), `Publish GitHub Release` (12s), alle grün.
- **Ratchet-Gate in der CI:** der Push-CI-Lauf enthält den Schritt `Ratchet quality gates`
  (`ratchet-cli`, Modus `validate`) als grünen Schritt; der Release-Workflow `release.yml` hat diesen
  Schritt weiterhin **nicht**.
- **Assets** (Name — Bytes — sha256):

  | Asset | Bytes | sha256 |
  |---|---|---|
  | `Prime-Photos-linux-x64.tar.gz` | 12.332.746 | `343ae18da13bd2e6e4173f76282b4f631561af3349ede9a3a9afde4f728233fd` |
  | `Prime-Photos-macos-arm64.dmg` | 9.563.611 | `21439fc923800cadb12ac4d19cff83b2d9087bc9d1390e426c206e9bcc8ee667` |
  | `Prime-Photos-windows-x64.zip` | 8.600.210 | `0ad58f4b6458ed03e27125f254ce91655b7ebb5e4471595e97189613514ca072` |

  `v0.1.4` trägt drei Assets. Das DMG wurde **manuell** ans Release gehängt (§4); sein über die
  GitHub-API gemeldeter `sha256`-Digest ist identisch mit dem der lokalen Datei
  (`shasum -a 256`).
- **macOS-Bundle (lokal gebaut, §4):** `VERSION=0.1.4`, `BUILD=4`, erstmals mit dem neuen
  Signatur-Default — Identität `Prime Photos Local Signer` statt ad-hoc (`d360174`); Commit-Stempel
  `34b0f05` dank `touch build.rs`; `CFBundleShortVersionString = 0.1.4`, `CFBundleVersion = 4`,
  `CFBundleIdentifier = de.reisinger.prime-photos`, `LSUIElement = true`; `codesign --verify --strict`
  gültig; `hdiutil verify` auf dem DMG: **VALID**. Volname `Prime Photos`; Installation nach
  `/Applications` mit `ditto`.
- **Live abgenommen (Dock-Icon, am 0.1.4-Build, zwei volle Zyklen):** Beobachtung am **laufenden**
  Build — aus den Artefakten nicht reproduzierbar (die belegen Endzustand, Commit-Stempel und
  Signatur, nicht das Laufzeitverhalten). Fenster mit dem roten X schließen ⇒ `lsappinfo` wechselt
  auf `type="UIElement"`, die **Dock-Kachel verschwindet innerhalb von 1–2 s**, die App läuft weiter;
  erneutes Öffnen (`open`) ⇒ `type="Foreground"`, Dock-Kachel und Fenster sind zurück. Geprüft über
  `lsappinfo info -app de.reisinger.prime-photos` und die Dock-Kachel-Liste per System Events.
- **Signatur-Default (§4), Wirkung noch nicht abgenommen:** erwartet war, dass der
  Schlüsselbund-Dialog nach einem selbst gebauten Update ausbleibt, weil das Designated Requirement
  über Rebuilds stabil ist. Beim Start des 0.1.4-Bundles erschien das `SecurityAgent`-Fenster
  („Prime Photos möchte deine vertraulichen Informationen verwenden …") erneut und verschwand nach
  wenigen Sekunden von selbst; die App meldete sich danach normal an. Der Default bleibt die richtige
  Wahl (stabiles Designated Requirement statt cdhash pro Build) — belegt ist die Wirkung damit
  weiterhin nicht, dieselbe Einordnung wie in §4 und `packaging/README.md`.
- **Windows unsigniert:** Authenticode-Schritte mangels Secrets übersprungen (§3).
- **Release-Notes:** `generate_release_notes: true` erzeugte
  `**Full Changelog**: https://github.com/reisi007/prime-photos/compare/v0.1.3...v0.1.4`.
- **Inhalt dieses Releases:** Fortschritt/ETA zählen nur noch den **aktuellen** Upload (Skips
  erscheinen nur in der Ergebniszeile, Plan-Zeit-Filter); das Dock-Icon folgt der Fenstersichtbarkeit
  (verschwindet beim Schließen, kommt beim Öffnen zurück); Ratchet-Qualitätsgates in der CI; dazu der
  Signatur-Default für **lokale** Bundles (ad-hoc nur noch als Rückfall). Das Wurzel-README bleibt
  versionsfrei.

**Durch den sechsten echten Lauf belegt (2026-10-26):**

- **Release ausgelöst:** annotierter Tag `v0.1.5` auf Commit `2e179fe` (`chore(release): prepare
  v0.1.5`); GitHub-Release `v0.1.5`, veröffentlicht 2026-10-26T22:06:41Z, **nicht** Draft, **nicht**
  Pre-Release — https://github.com/reisi007/prime-photos/releases/tag/v0.1.5
- **Runs:** Push-CI `40405073604` (`ci.yml`: Ratchet-Gate + `cargo build`/`cargo test`) grün;
  Release-Workflow `40408805334` (`release.yml`, ausgelöst durch den Tag-Push) — Jobs
  `Build linux-x64` (5m21s), `Build windows-x64` (7m41s), `Publish GitHub Release` (13s), alle grün.
- **Assets** (Name — Bytes — sha256):

  | Asset | Bytes | sha256 |
  |---|---|---|
  | `Prime-Photos-linux-x64.tar.gz` | 12.335.506 | `a3f9ce0a61c59fa5b0155e8413c491caea9adf474240c32ac6ab64fb9ae96ed3` |
  | `Prime-Photos-macos-arm64.dmg` | 9.564.117 | `b1a4edb50a53d72bcff3e5abbff352932f2ffa404bd03e363abb34f44c2d6739` |
  | `Prime-Photos-windows-x64.zip` | 8.603.858 | `518729cf34a4a48821d52babb7af5ca3aef372ffb0f09407042cea11a3947c4b` |

  `v0.1.5` trägt drei Assets. Der über die GitHub-API gemeldete `sha256`-Digest des DMG ist identisch
  mit dem der lokalen Datei.
- **macOS-Bundle (lokal gebaut, §4):** Version `0.1.5`/Build `5`, mit der Identität
  `Prime Photos Local Signer` signiert; Commit-Stempel `2e179fe`; `codesign --verify` gültig,
  `hdiutil verify` auf dem DMG: **VALID**. Das Bundle wurde in diesem Lauf **nicht** nach
  `/Applications` installiert und **nicht** gestartet; an Amazon-Kontodaten wurde nichts geändert.
- **Release-Notes:** vom Workflow erzeugt (`generate_release_notes: true`), Vergleichsbereich
  `v0.1.4...v0.1.5`.
- **Nicht abgenommen in diesem Lauf:** Windows/Linux werden weiterhin nur **kompiliert**, nicht
  laufen getestet; eine Live-Abnahme eines echten Amazon-Uploads ist mit diesem Lauf **nicht**
  belegt.

**Durch den siebten echten Lauf belegt (2026-10-28):**

- **Release ausgelöst:** annotierter Tag `v0.1.6` auf Commit `0c35621` (`chore(release): prepare
  v0.1.6`); GitHub-Release `v0.1.6`, veröffentlicht `2026-10-28T09:23:37Z`, **nicht** Draft,
  **nicht** Pre-Release — https://github.com/reisi007/prime-photos/releases/tag/v0.1.6
- **Runs:** Push-CI `40662445684` (`ci.yml`: Ratchet-Gate + `cargo build`/`cargo test`) grün;
  Release-Workflow `40662710132` (`release.yml`, ausgelöst durch den Tag-Push) — Jobs
  `Build linux-x64` (5m01s), `Build windows-x64` (8m48s), `Publish GitHub Release` (11s), alle grün.
  Die Windows-Certificate-Prep- und Signaturschritte wurden mangels Secrets übersprungen (§3).
- **Assets** (Name — Bytes — sha256):

  | Asset | Bytes | sha256 |
  |---|---:|---|
  | `Prime-Photos-linux-x64.tar.gz` | 12.385.185 | `02231f8853dcfd509e1c7319ed6081a682f3c447b1d570d612ff9ba0092fc832` |
  | `Prime-Photos-macos-arm64.dmg` | 9.614.426 | `ba4773532e005b52c4fd6b87d3abec2e386bf4048d6de1267323f2525b196acd` |
  | `Prime-Photos-windows-x64.zip` | 8.659.727 | `70de4f35844a3581544135c36dd984e1717c06b641d99bd12e0a4a6c26202c73` |

  Das DMG wurde **lokal** erstellt und manuell ans Release gehängt (§4); sein lokaler SHA-256-Digest
  stimmt mit dem von GitHub gemeldeten Asset-Digest überein.
- **macOS-Bundle (lokal aus dem Release-Commit gebaut, §4):** `VERSION=0.1.6`, `BUILD=6`, mit
  `Prime Photos Local Signer` signiert; About-Commit-Stempel `0c35621`. Das Bundle wurde nach
  `/Applications/Prime Photos.app` installiert; `codesign --verify` war erfolgreich. Das lokal
  erstellte DMG bestand `hdiutil verify` (**VALID**). `hdiutil` gab die bereits in §4 dokumentierte
  Deprecation-Warnung aus; ein Ersatzwerkzeug wurde nicht verwendet. Das installierte Bundle wurde
  **nicht gestartet**.
- **Release-Notes:** vom Workflow erzeugt (`generate_release_notes: true`), Vergleichsbereich
  `v0.1.5...v0.1.6`.
- **Inhalt dieses Releases:** Upload-Ziele spiegeln den Quellpfad sowohl für konfigurierte als auch
  für einmalige Uploads; dazu kommen Empty-/Dedup-Preflight und fail-closed Behandlung des
  Upload-Ziels. Mit diesem Release-Build wurde **kein Live-Upload zu Amazon** durchgeführt.
- **Verifikationsgrenze:** Windows und Linux wurden im Release-CI kompiliert; Laufzeittests oder
  plattformübergreifende Verifikation sind damit nicht belegt.

**Durch den achten echten Lauf belegt (2026-10-29):**

- **Release ausgelöst:** annotierter Tag `v0.1.7` auf Commit
  `7f91ec1ef1787b2231ddcdd7d29cd79f6e2cb48b`; GitHub-Release `v0.1.7`, veröffentlicht
  `2026-10-29T07:44:35Z`, **nicht** Draft, **nicht** Pre-Release —
  [GitHub Release v0.1.7](https://github.com/reisi007/prime-photos/releases/tag/v0.1.7).
  Paketversion `0.1.7`, macOS-Build `7`.
- **Runs:** [Push-CI `40836108085`](https://github.com/reisi007/prime-photos/actions/runs/40836108085)
  (Ratchet-Gate sowie Linux-Build und -Tests) grün; [Release-Workflow `40836346965`](https://github.com/reisi007/prime-photos/actions/runs/40836346965)
  (`release.yml`, ausgelöst durch den Tag-Push) grün — `Build linux-x64` (6m34s),
  `Build windows-x64` (9m22s), `Publish GitHub Release` (13s). Die Windows-Signaturschritte wurden
  mangels Signatur-Secrets übersprungen. Die Linux- und Windows-Jobs kompilieren; daraus sind keine
  Laufzeittests auf diesen Plattformen belegt.
- **Assets** (Name — Bytes — SHA-256-Digest laut GitHub-API):

  | Asset | Bytes | sha256 |
  |---|---:|---|
  | `Prime-Photos-linux-x64.tar.gz` | 12.389.730 | `85e0cffcee49f25bba0ba2e18dff415a00a0a5b45b28fe7d437f209ffa7ac0a3` |
  | `Prime-Photos-macos-arm64.dmg` | 9.619.748 | `b815c3451d068cee1b015ef61980f4d624ddf04121ee417eeffb88ec38d7abbc` |
  | `Prime-Photos-windows-x64.zip` | 8.663.128 | `3af4b67dedc666c7e8a33f1dc86e8c8e410edb562f702835752c5d4477c691fe` |

  Das GitHub-Release enthält genau diese drei Assets. Das DMG wurde lokal erstellt; sein lokaler
  SHA-256-Digest (`shasum -a 256`) stimmt mit dem von GitHub gemeldeten Asset-Digest überein.
- **macOS-Bundle (lokal aus dem Tag-Commit gebaut, §4):** `VERSION=0.1.7`, `BUILD=7`; vor dem Build
  wurde `touch build.rs` ausgeführt. Mit `Prime Photos Local Signer` signiert; Commit-Stempel im
  Release-Binary `7f91ec1`. `Info.plist`: Version `0.1.7`, Build `7`, Identifier
  `de.reisinger.prime-photos`. `codesign --verify --strict --verbose=2` war erfolgreich. Das DMG
  trägt den Volume-Namen `Prime Photos`; `hdiutil verify` meldete **checksum VALID** (zusätzlich
  erschien die in §4 dokumentierte Deprecation-Warnung).
- **Installation und Hintergrundstart:** Das Bundle wurde mit `ditto` nach
  `/Applications/Prime Photos.app` installiert; Codesignatur und `Info.plist`-Werte wurden dort
  erneut geprüft. Anschließend wurde es mit `open -j` verborgen gestartet. `lsappinfo info -app
  de.reisinger.prime-photos` zeigte den Executable-Pfad unter `/Applications/Prime Photos.app`,
  `type="UIElement"`, ARM64, `Version="7"` und PID `98264`. Die Login-Item-Einstellung wurde nicht
  geändert.
- **Lokale Prüfbelege (Befehle → beobachtetes Ergebnis):** Die ersten sechs Befehle liefen mit
  Arbeitsverzeichnis `target/dist-v0.1.7`; die dort angegebenen Pfade `Prime Photos.app` und
  `Prime-Photos-macos-arm64.dmg` sind relativ zu diesem Verzeichnis. Die anschließenden Prüfungen
  am installierten Bundle richten sich gegen `/Applications/Prime Photos.app` und verwenden dort
  absolute Pfade; die `lsappinfo`-Abfrage adressiert die laufende App stattdessen über die
  Bundle-ID `de.reisinger.prime-photos`, nicht über einen Pfad.

  | Befehl (cwd `target/dist-v0.1.7`) | Ergebnis |
  |---|---|
  | `codesign --verify --strict --verbose=2 "Prime Photos.app"` | erfolgreich |
  | `/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "Prime Photos.app/Contents/Info.plist"` | `0.1.7` |
  | `/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "Prime Photos.app/Contents/Info.plist"` | `7` |
  | `/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "Prime Photos.app/Contents/Info.plist"` | `de.reisinger.prime-photos` |
  | `hdiutil verify "Prime-Photos-macos-arm64.dmg"` | `checksum VALID` |
  | `shasum -a 256 "Prime-Photos-macos-arm64.dmg"` | `b815c3451d068cee1b015ef61980f4d624ddf04121ee417eeffb88ec38d7abbc`, identisch mit dem GitHub-API-Digest oben |

  | Befehl (installiertes Bundle) | Ergebnis |
  |---|---|
  | `codesign --verify --strict --verbose=2 "/Applications/Prime Photos.app"` | erfolgreich |
  | `/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "/Applications/Prime Photos.app/Contents/Info.plist"` | `0.1.7` |
  | `/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "/Applications/Prime Photos.app/Contents/Info.plist"` | `7` |
  | `/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "/Applications/Prime Photos.app/Contents/Info.plist"` | `de.reisinger.prime-photos` |
  | `lsappinfo info -app de.reisinger.prime-photos` | `type="UIElement"`, ARM64, `Version="7"`; Executable unter `/Applications/Prime Photos.app` |
- **Manueller UI-Smoke-Test:** Bei isolierten Debug-Starts wurden die nativen macOS-Menülabels unter
  „App“ und „Edit“ auf Deutsch und Englisch geprüft. Dass der Tray vor der Anmeldung nicht vorhanden
  war, war erwartet. Es wurden weder Live-Anmeldung noch ein manuell initiierter Upload verifiziert.
  Die nativen Menüs werden hier als
  Prime-Photos-UI geprüft, nicht aus einem Amazon-Verhalten abgeleitet.
- **Inhalt dieses Releases:** die sechs Audit-Fixes #3, #4, #5, #6, #7 und #8.
- **Release-Notes:** vom Workflow erzeugt (`generate_release_notes: true`):
  `**Full Changelog**: https://github.com/reisi007/prime-photos/compare/v0.1.6...v0.1.7`.

---

## 10. Bekannte Einschränkung: Dependabot-Warnung `glib` (Linux-Backend)

GitHub/Dependabot meldet auf `main` eine **moderate** Schwachstelle in **`glib`**: „Unsoundness in
den `Iterator`/`DoubleEndedIterator`-Impls für `glib::VariantStr`" — betroffen `>= 0.15.0, < 0.20.0`,
Fix wäre `0.20.0`; im Lock steht `glib 0.18.5`.

| Punkt | Stand |
|---|---|
| Herkunft | ausschließlich der **gtk-rs-0.18-Stack im Linux-Backend** (`tray-icon` → `libappindicator 0.9.0`, `muda 0.21.2`, `wry 0.57.0` → `webkit2gtk =2.0.2`) |
| macOS | **nicht betroffen** — diese Crates werden dort nicht kompiliert |
| Fix | derzeit **nicht möglich**: die jeweils neuesten Versionen dieser Crates verlangen weiterhin gtk-rs 0.18, und `gtk 0.18` pinnt `glib ^0.18` — ein isolierter Bump ist damit ausgeschlossen |
| Dependabot-Auto-Update | scheitert deshalb erwartungsgemäß (Job „cargo in `/\.` for glib – Update", Lauf `38542163059`) — kein Konfigurationsfehler |
| Auswirkung | die betroffenen Iterator-APIs ruft die App nicht auf; auflösbar erst, wenn `tray-icon`/`wry`/`muda` auf gtk-rs 0.20 wechseln |

**Regel:** Nicht eigenmächtig patchen. Vendoring oder ein `[patch]` wäre ein neuer, ungeprüfter
Eingriff und auf dem Entwicklungsrechner (macOS) nicht testbar.

---

## 11. Lizenz

Prime Photos steht unter **`MIT OR Apache-2.0`** (Rust-Standard); die beiden Texte liegen als
`LICENSE-MIT` und `LICENSE-APACHE` im Repo-Wurzelverzeichnis, und `Cargo.toml` trägt das Feld
`license = "MIT OR Apache-2.0"`. Die Lizenzen **aller** Abhängigkeiten aus `Cargo.lock`
(inkl. vollständiger Lizenztexte) stehen in `assets/third-party-licenses.txt`; neu erzeugt wird
die Datei mit `packaging/gen-third-party-licenses.sh`.

---

## 12. Referenzen

- `BUILD.md` — System-Voraussetzungen macOS/Windows/Linux.
- `packaging/README.md` — `make-app.sh`, `Info.plist`, Signatur (lokale Identität vs. ad-hoc).
- `AGENTS.md` „CI/Release" — die getroffenen Entscheidungen.
- `.github/workflows/ci.yml`, `.github/workflows/release.yml` — die Workflows selbst.
