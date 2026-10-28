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
- **Signierung ist nur auf macOS relevant.** Dort ist eine (selbstsignierte) Codesignatur nötig,
  damit Benachrichtigungen/Bundle-Identifier sauber funktionieren; Windows/Linux brauchen keine.
- **Warnungs-Baseline 0:** `[lints.rust] warnings = "deny"` in `Cargo.toml` — jede Warnung ist
  ein harter Build-Fehler. Kein `.cargo/config.toml` mit `-Dwarnings` (würde auch Dependencies
  hart nehmen). `cargo fmt --check` ist **kein** Gate — das Repo ist nicht fmt-clean.
- **Secrets nur in `main`/Tag-Läufen** (fork-PRs erhalten keine Secrets). Windows: Authenticode
  optional (sonst SmartScreen-Hinweis); Linux: keine Signatur.

---

## 2. Was das CI-Release erzeugt

Der `build`-Job ist eine Matrix (`fail-fast: false`). Der `release`-Job hat `needs: [build]` und
läuft damit **nur, wenn beide Beine grün sind** — sonst entsteht kein Release.

| Bein | Runner | Artefakt |
|---|---|---|
| Windows x64 | `windows-latest` | `Prime-Photos-windows-x64.zip` (enthält `prime-photos.exe`) |
| Linux x64 | `ubuntu-latest` | `Prime-Photos-linux-x64.tar.gz` (enthält `prime-photos`) |

- Beide bauen mit `cargo build --release --locked`.
- Der Linux-Job installiert die System-Abhängigkeiten aus `BUILD.md` §5:
  `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libssl-dev pkg-config build-essential`.
- **Es werden keine SHA256-Prüfsummen erzeugt** (nicht implementiert) — im Release liegt **keine**
  mitgelieferte Prüfsummendatei. Davon zu unterscheiden ist der `sha256`-**Digest**, den die
  GitHub-API **je Asset** liefert (abfragbar z. B. mit `gh release view <tag> --json assets`); das ist
  keine Datei im Release.

---

## 3. Optionale Windows-Signatur (Authenticode)

Der Windows-Build ist **standardmäßig unsigniert**; Windows zeigt dann beim Start den
SmartScreen-Hinweis („Unbekannter Herausgeber"). Eine Authenticode-Signatur ist **optional** und
nur aktiv, wenn beide Secrets gesetzt sind:

| Secret-Name | Inhalt |
|---|---|
| `WINDOWS_CERTIFICATE` | Base64-kodierte Code-Signing-PFX-Datei (`.pfx`) |
| `WINDOWS_CERTIFICATE_PASSWORD` | Passwort der PFX-Datei |

Der Workflow macht die Secrets als **Job-Env** sichtbar und prüft in den Schritten
`env.WINDOWS_CERTIFICATE != ''` (der `secrets`-Kontext ist in job-level `if` nicht zuverlässig).
Fehlen die Secrets, werden Cert- und Signaturschritt übersprungen; das Release ist unsigniert und
**läuft trotzdem durch**.

> **Nicht verifiziert:** Der Signaturschritt (`signtool` `/f`/`/p`, Zeitstempel über
> `timestamp.digicert.com`) wurde nicht ausgeführt; `signtool` muss auf dem Runner im `PATH` liegen.

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

2. **Bundle bauen** (Default `VERSION=0.1.6`, `BUILD=6`, mit der lokalen Identität
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

**Windows**

```powershell
Expand-Archive Prime-Photos-windows-x64.zip -DestinationPath .
.\prime-photos.exe
# Nur wenn Authenticode aktiv war:
Get-AuthenticodeSignature .\prime-photos.exe   # Status "Valid" bei signiert
```

Unsigniert/ohne gültige Signatur ⇒ SmartScreen: „Weitere Informationen" → „Trotzdem ausführen".

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
aus einem Fork sieht die Signatur-Secrets nicht; Build und Test laufen ohne Secrets. Release-Läufe
(`push` eines Tags) entstehen im Haupt-Repo und erhalten die Secrets.

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

- Windows-Authenticode-Signatur (§3) nicht ausgeführt; `signtool`-Verfügbarkeit auf dem Runner nicht
  geprüft.
- macOS-Notarisierung/Stapling (§6) nicht eingerichtet und nicht geprüft.
- Keine SHA256-Prüfsummen-**Dateien** im Release (nicht implementiert); die GitHub-API liefert je
  Asset aber ein `sha256`-Digest (§2).
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
