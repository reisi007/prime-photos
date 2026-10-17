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
  anderen OS werden dort **nicht** gebaut.
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
# Ergebnis: target/dist/Prime Photos.app   (arm64, ad-hoc signiert, CODESIGN_IDENTITY=-)
```

- Das Skript baut `cargo build --release`, assembliert das Bundle und signiert **ad-hoc**
  (`codesign --sign -`). Eine eigene Identität ist per `CODESIGN_IDENTITY="<Name>"` möglich.
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

2. **Bundle bauen** (Default `VERSION=0.1.2`, `BUILD=2`, ad-hoc signiert):

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
   **unsignierten Downloads** ist §5; das hier lokal installierte Bundle betrifft das nicht.

5. **Ans GitHub-Release hängen:** das DMG hochladen.
   Der **Asset-Name ist der Dateiname** — siehe der vorstehende Aufzählungspunkt.

> **Nicht verifiziert:** die `hdiutil`-Aufrufe (`attach`/`detach`/`create`) warnen inzwischen und
> verweisen auf `diskutil image attach --mountOptions nobrowse --readOnly`, `diskutil eject` bzw.
> `diskutil image create from/blank --volumeName --format`. Ob/wann hier umgestellt wird, ist offen.

---

## 5. Gatekeeper-Workaround (unsignierte macOS-Downloads)

**Funktionsweise:** Das Bundle ist **unsigniert** und **nicht notarisiert**. Lädt man es aus dem
Netz, setzt macOS die Quarantäne-Markierung, und **Gatekeeper blockt** den Start. Dann greift einer
dieser Wege:

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
- **macOS-Bundle (lokal aus dem Tag gebaut, §4):** `./packaging/make-app.sh` mit `VERSION=0.1.1`,
  `BUILD=2`, ad-hoc signiert, Volname `Prime Photos`; `CFBundleShortVersionString = 0.1.1`,
  `CFBundleVersion = 2`, `CFBundleIdentifier = de.reisinger.prime-photos`; der Commit-Stempel im
  Über-Fenster steht dank `touch build.rs` auf `57ca172`; `hdiutil verify` auf dem DMG: **VALID**.
- **Windows unsigniert:** die Authenticode-Schritte des Workflows wurden ohne Secrets übersprungen
  (§3) — dieselbe Lage wie bei v0.1.0.
- **Release-Notes:** `generate_release_notes: true` erzeugte diesmal eine echte Commit-Liste —
  `**Full Changelog**: https://github.com/reisi007/prime-photos/compare/v0.1.0...v0.1.1`.
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
  `BUILD=2`, ad-hoc signiert, Volname `Prime Photos`; der Commit-Stempel im Über-Fenster steht dank
  `touch build.rs` auf `1aceb25`; `hdiutil verify` auf dem DMG: **VALID**.
- **Windows unsigniert:** die Authenticode-Schritte des Workflows wurden ohne Secrets übersprungen
  (§3) — dieselbe Lage wie bei v0.1.0 und v0.1.1.
- **Release-Notes:** `generate_release_notes: true` erzeugte die Commit-Liste
  `**Full Changelog**: https://github.com/reisi007/prime-photos/compare/v0.1.1...v0.1.2`.
- **Inhalt dieses Releases:** das neue **Hintergrund-Verhalten** — das X versteckt nur das Fenster,
  die App läuft weiter, das **Dock-Icon bleibt**; zurück kommt man per Dock-Klick, App-Neustart oder
  Tray-Eintrag, und es läuft **immer nur eine Instanz** (macOS `LSMultipleInstancesProhibited` plus
  Socket-Wächter). Dazu die README-Aktualisierung (jetzt versionsfrei, Regel in `AGENTS.md`) und die
  macOS-Asset-Policy (nur DMG).
- **Live abgenommen:** der User hat das Hintergrund-Verhalten am 0.1.2-Build am 2026-10-17 manuell
  abgenommen („manuell ok").
- **Weiterhin offen (kein 0.1.2-Defekt):** Windows/Linux werden weiterhin nur **kompiliert**, nicht
  laufen getestet; die Live-Abnahme der **Laufwerk-Rückkehr** aus einem älteren Befund steht noch
  aus (eigener Ledger-Punkt).

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
- `packaging/README.md` — `make-app.sh`, `Info.plist`, ad-hoc vs. eigene Identität.
- `AGENTS.md` „CI/Release" — die getroffenen Entscheidungen.
- `.github/workflows/ci.yml`, `.github/workflows/release.yml` — die Workflows selbst.
