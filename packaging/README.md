# packaging/ — Paketierung (PORT-6)

Werkzeuge für das **macOS-Bundle** und das **Linux-`.deb`** von *Prime Photos*. Windows ist hier
nur als Stichpunkt dokumentiert (kein Installer). Die System-Voraussetzungen stehen in
`docs/dev/BUILD.md` (§3 macOS, §4 Windows, §5 Linux).

Dateien:

| Datei | Zweck |
|---|---|
| `Info.plist` | Vorlage für `Contents/Info.plist` des Bundles; `__VERSION__` / `__BUILD__` werden von `make-app.sh` ersetzt |
| `make-app.sh` | baut `cargo build --release`, assembliert `target/dist/Prime Photos.app`, signiert und verifiziert |
| `linux/prime-photos.desktop` | Desktop-Eintrag des `.deb` (Name, `Exec=prime-photos`, Icon, Kategorien) |
| `make-deb.sh` | baut `release/Prime-Photos-linux-x64.deb` aus Binary, Iconset und Lizenzen |
| `README.md` | diese Anleitung |

Das Skript legt sein Ergebnis unter `target/dist/` ab — also unter dem bereits ignorierten
`target/`, damit kein unversionierter `dist/`-Ordner im Repo entsteht. Der Pfad lässt sich per
`DIST_DIR=<dir>` überschreiben.

## macOS: bauen, signieren, starten

Voraussetzungen: Rust-Toolchain und die Xcode Command Line Tools (C-Compiler für `rusqlite`
`bundled`) — siehe `docs/dev/BUILD.md` §3. Danach:

```sh
./packaging/make-app.sh
```

Das erzeugt `target/dist/Prime Photos.app` und signiert sie **ad-hoc** (`codesign --sign -`), das ist
der Standard. Eine andere Signatur-Identität — auch die optionale lokale Identität
`Prime Photos Local Signer` — wird ausschließlich explizit über `CODESIGN_IDENTITY` gewählt. Starten:

```sh
open "target/dist/Prime Photos.app"
```

Das Bundle trägt ein **App-Icon** (Dock-/Taskbar-Icon): `build.rs` rastert die SVG-Quelle
`assets/logo.svg` in die zehn iconset-Größen nach `target/pp-macos.iconset/`, und `make-app.sh`
bündelt dieses Verzeichnis mit `iconutil -c icns` zu `Contents/Resources/AppIcon.icns`. Darauf
verweist `CFBundleIconFile = AppIcon` in `packaging/Info.plist`. Das Icon entsteht also **im Build
und ist nicht eingecheckt** — im Repo liegt keine `.icns`/PNG-Datei, sie liegt unter dem
ignorierten `target/`.

Die macOS-Raster sitzen auf Apples Icon-Raster: das Motiv füllt 824 von 1024 px (80,47 % der
Kante), zentriert, der Rest bleibt transparent, und die Ecken sind auf Apples Radius von ≈22,5 %
der Motivkante nachgerundet (das SVG zeichnet 19,8 %). Sonst wirkt die randlos gemalte Kachel im
Dock größer als die System-Icons. Skaliert/maskiert wird nur beim Rastern in `build.rs`;
`assets/logo.svg` bleibt unverändert, weil dasselbe Motiv auch Fenster- und Sidebar-Logo speist.
Bei 16/32/64 px rundet die ganzzahlige Skalierung auf 13/16, 26/32 und 52/64 px (81,25 %), der
ungerade Rest geht auf eine Seite (16er Raster: Rand 2/2/1/1).

### Umgebungsvariablen

| Variable | Wirkung | Standard |
|---|---|---|
| `SKIP_BUILD=1` | überspringt `cargo build --release` (nutzt das vorhandene Binary) | aus |
| `VERSION=x.y.z` | `CFBundleShortVersionString` | `0.1.10` |
| `BUILD=n` | `CFBundleVersion` | `10` |
| `CODESIGN_IDENTITY="<Name>"` | Signatur-Identität statt des Standards | `-` (ad-hoc); ein Name, z. B. `Prime Photos Local Signer`, muss explizit übergeben werden |
| `DIST_DIR=<dir>` | Ausgabeordner des Bundles | `target/dist` |

Beispiel:

```sh
SKIP_BUILD=1 VERSION=0.2.0 BUILD=2 ./packaging/make-app.sh
```

### Signatur: ad-hoc als Standard, lokale Identität optional

- **Standard (ad-hoc, `codesign --sign -`):** keine Identität und kein Schlüsselbund-Eintrag nötig,
  funktioniert auf jedem Rechner und ist der Standard; die Bundles aus 0.1.4–0.1.7 trugen die
  lokale Identität;
  `codesign --verify` prüft die Bundle-Integrität. Bekannter Nachteil: bei ad-hoc ist das Designated
  Requirement der cdhash, der sich mit jedem Build ändert — die Keychain-ACL des gespeicherten
  Refresh-Tokens passt danach nicht mehr, und macOS kann beim Start nach dem Passwort für den
  Schlüsselbund „Anmeldung" fragen. Für eine Verteilung/Notarisierung ist ad-hoc ohnehin nicht
  geeignet.
- **Lokale Identität (optional):** nur über `CODESIGN_IDENTITY="Prime Photos Local Signer"` (oder
  einen anderen exakten Namen aus `security find-identity -v -p codesigning`). Eine
  Zertifikats-Identität hat ein über Rebuilds **stabiles** Designated Requirement (Identifier +
  Zertifikat); erwartete Folge: die Keychain-ACL des gespeicherten Refresh-Tokens bleibt bei selbst
  gebauten Updates gültig und der Schlüsselbund-Dialog entfällt — **belegt ist das nicht**, siehe
  „Nicht verifiziert" am Ende dieser Datei.
- **Andere Identität:** jeder nicht-leere `CODESIGN_IDENTITY`-Wert gewinnt und wird unverändert an
  `codesign --sign` übergeben; der Name muss exakt dem Eintrag aus
  `security find-identity -v -p codesigning` entsprechen.

Es gibt keine automatische Erkennung im Schlüsselbund: ohne `CODESIGN_IDENTITY` signiert das Skript
immer ad-hoc (`-`). Das Skript meldet die gewählte Identität in der Zeile `==> Signatur: …`.

Scheitert `codesign` mit einer explizit gewählten Identität (gesperrter Schlüssel, fehlende
Freigabe), bricht das Skript ab; in diesem Fall `CODESIGN_IDENTITY` weglassen (ad-hoc ohne den
Keychain-ACL-Vorteil) oder eine funktionierende Identität übergeben.

**Selbstsigniertes Zertifikat im Schlüsselbund anlegen (optional, nur Beschreibung — dieses Repo
legt kein Zertifikat an und fasst den Schlüsselbund nicht an):**

1. *Schlüsselbundverwaltung* öffnen (Programme → Dienstprogramme → Schlüsselbundverwaltung).
2. Menü *Schlüsselbundverwaltung* → *Zertifikatassistent* → *Zertifikat erstellen …*.
3. **Name**: frei wählbar (im Repo-Beispiel `Prime Photos Local Signer`); dieser Name wird später
   exakt an `CODESIGN_IDENTITY` übergeben.
4. **Identitätstyp**: *Code-Signierung* („Code Signing").
5. **Zertifikatstyp**: *Selbstsigniertes Zertifikat* („Self Signed Root").
6. Erstellen und ablegen im **Anmelde-Schlüsselbund (login)**. Den eigenen Schlüsselbund
   autorisiert man selbst; für das Signieren kann macOS einmalig nach der Freigabe fragen.

Danach den **exakten Namen** der Identität ermitteln:

```sh
security find-identity -v -p codesigning
```

Der Name aus der Ausgabe wird exakt als `CODESIGN_IDENTITY` übergeben — nur so signiert das Skript
mit einer Identität statt ad-hoc (im Beispiel die frei gewählte Identität `Prime Photos Codesign`):

```sh
CODESIGN_IDENTITY="Prime Photos Codesign" ./packaging/make-app.sh
```

Das Skript ruft `codesign --force --options runtime --sign "<Name>"` auf (ohne
`CODESIGN_IDENTITY` mit `-`) und prüft mit
`codesign --verify --verbose=2`. Zum erneuten Signieren nur der Signatur-Schritt reicht
`SKIP_BUILD=1` (bzw. ein erneuter vollständiger Lauf — das Bundle wird dabei sauber neu gebaut); mit
`SKIP_BUILD=1` muss das vom Build erzeugte `target/pp-macos.iconset` allerdings schon aus einem
früheren Build vorliegen, sonst bricht das Skript beim Icon-Schritt ab.

## Windows (nur Hinweis)

Bauen mit dem MSVC-Target (`cargo build --release`, `docs/dev/BUILD.md` §4). Für die **Verteilung**
wäre zusätzlich eine Authenticode-Signatur (SignTool) nötig — **hier nicht umgesetzt**, es gibt
keinen Installer im Repo.

## Linux: `.deb` (`make-deb.sh`)

`packaging/linux/prime-photos.desktop` ist der Desktop-Eintrag (Name *Prime Photos*,
`Exec=prime-photos`, `Icon=prime-photos`, Kategorien `Utility;Graphics;`), `packaging/make-deb.sh`
baut daraus das Paket `Prime-Photos-linux-x64.deb`. Nötig sind nur die Debian/Ubuntu-Werkzeuge
`dpkg-deb` und `dpkg-shlibdeps`; **kein** Codesigning, kein macOS-Werkzeug — das macOS-Bundle
bleibt unverändert.

Das Paket installiert:

- `/usr/bin/prime-photos` (0755)
- `/usr/share/applications/prime-photos.desktop`
- das App-Icon als `prime-photos.png` in **16, 32, 64, 128, 256 und 512 px** unter
  `/usr/share/icons/hicolor/<N>x<N>/apps/` (Quelle ist der **vollflächige** Rastersatz
  `target/pp-appicon.iconset`; 64 px kommt aus `icon_32x32@2x.png`). Bewusst **nicht**
  `target/pp-macos.iconset`: dessen Apple-Rand machte das Linux-Icon ~19,6 % kleiner als das
  Fenster-Icon `logo.png`, das die App zur Laufzeit zeigt.
- `/usr/share/doc/prime-photos/` mit `copyright`, `LICENSE-MIT` und `LICENSE-APACHE`

**Deklarierte Abhängigkeiten:** `Depends` ermittelt `dpkg-shlibdeps` aus der ELF-Binary — darunter
`libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libssl3` und die weiteren Bibliotheken, gegen die gelinkt
wird. Die **Tray-Bibliothek lädt die App per `dlopen`**; sie ist damit für `dpkg-shlibdeps`
unsichtbar und wird vom Skript fest als Alternative
`libayatana-appindicator3-1 | libappindicator3-1` ergänzt. Für den Secret-Service (gespeicherter
Refresh-Token) deklariert das Paket `Recommends: gnome-keyring | kwalletmanager` — ohne ihn
startet die App, kann sich aber keine Anmeldung merken.

**Lokal bauen** (Binary und Iconset müssen vorliegen; `cargo build --release` erzeugt beides):

```sh
./packaging/make-deb.sh
# Ergebnis: release/Prime-Photos-linux-x64.deb
```

Optionen und Umgebungsvariablen:

| Option / Variable | Wirkung | Standard |
|---|---|---|
| `--binary <datei>` | zu verpackende Release-Binary (muss ausführbar sein) | `target/release/prime-photos` |
| `--iconset <dir>` | Iconset-Verzeichnis des Builds | `target/pp-appicon.iconset` |
| `--output <dir>` | Ausgabeordner des `.deb` | `release/` (gitignoriert) |
| `VERSION=x.y.z` | Paketversion (ein führendes `v` wird entfernt) | `version = "…"` aus `Cargo.toml` |

Die Architektur kommt aus `dpkg --print-architecture` (auf dem CI-Runner `amd64`); einen
ARM64-Build gibt es nicht. Prüfen und installieren:

```sh
dpkg-deb -I release/Prime-Photos-linux-x64.deb
dpkg-deb -c release/Prime-Photos-linux-x64.deb
sudo apt install ./Prime-Photos-linux-x64.deb
```

**Abgrenzung:** Das `.deb` ist **nicht** dpkg-signiert (das wäre `debsig-verify` bzw. eine
signierte Paketquelle — ein eigener Vertriebskanal, out of scope); im Release liegt nur die
abgetrennte GPG-Signatur `.deb.asc` (`docs/dev/RELEASE.md` §3.2). **Flatpak, AppImage und RPM
bleiben out of scope**, ebenso Snap und AUR; der Release-Workflow baut und veröffentlicht
Tarball **und** `.deb`; das erste Release, das ein `.deb` enthalten soll, ist 0.1.8.

## Nicht verifiziert

- **Ob `--options runtime` (Hardened Runtime) für ad-hoc- *und* selbstsignierte lokale Nutzung
  überhaupt nötig ist, ist nicht verifiziert.** Das Skript setzt das Flag wie beauftragt
  (vorgegebenes Verhalten), aber es ist nicht getestet, ob es für den lokalen Start einen
  Unterschied macht. Für eine echte Verteilung wäre Hardened Runtime + Notarisierung ohnehin
  nur mit einer Developer-ID-Signatur sinnvoll.
- Ob macOS-Benachrichtigungen im signierten Bundle zuverlässig erscheinen, ist nicht geprüft
  (siehe `docs/dev/BUILD.md` §7).
- Der komplette Ablauf wurde auf diesem Rechner ausgeführt (ad-hoc, `EXIT=0`): Bundle unter
  `target/dist/Prime Photos.app`, `Info.plist` ersetzt, `codesign -dv` meldet `Signature=adhoc`,
  `codesign --verify --verbose=2` meldet „valid on disk / satisfies its Designated Requirement".
  Der Icon-Schritt ist ebenso belegt: `iconutil -c icns target/pp-appicon.iconset` liefert eine
  gültige `.icns`, die sich über alle zehn Größen (16–1024 px) zurückwandeln lässt; das Bundle
  enthält `Contents/Resources/AppIcon.icns` und `CFBundleIconFile = AppIcon` im `Info.plist`.
  (Der Lauf stammt aus der Zeit vor #34; seither liest `make-app.sh` den eingerückten Satz
  `target/pp-macos.iconset`, der Rückweg über die zehn Größen ist unverändert belegt.)
  Der Pfad mit einer selbstsignierten Identität (`CODESIGN_IDENTITY=<Name>`) wurde am 2026-10-26
  für die lokale Identität `Prime Photos Local Signer` auf diesem Rechner mit `SKIP_BUILD=1`
  geprüft: `codesign -dv --verbose=2` meldet `Authority=Prime Photos Local Signer`,
  `codesign --verify --strict` läuft fehlerfrei durch. Mit einem **anderen** Namen ist der Pfad
  nicht geprüft. Damals war diese Identität der Default; heute ist ad-hoc der Standard (§ „Signatur"
  oben), der Identitäts-Pfad bleibt als optionale, explizite Wahl bestehen. **Nicht** belegt ist
  zudem, ob der Schlüsselbund-Dialog mit einer Identität dauerhaft ausbleibt; erwartet wird es, weil
  das Designated Requirement über Rebuilds gleich bleibt — über mehrere aufeinanderfolgende Builds
  ist das noch nicht abgenommen.
- **Das Linux-`.deb` ist nicht live verifiziert:** gebaut und mit `dpkg-deb -I`/`dpkg-deb -c`
  geprüft wurde es lokal, aber aus keinem Release heruntergeladen und auf keinem
  Debian/Ubuntu-Rechner installiert; Desktop-Eintrag und Icon in der Anwendungsübersicht sind
  damit nicht abgenommen (offener Punkt, `AGENTS.todo.md`).
