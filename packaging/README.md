# packaging/ — Paketierung (PORT-6)

Werkzeuge für das **macOS-Bundle** von *Prime Photos*. Windows/Linux sind hier nur als
Stichpunkte dokumentiert (keine echten Installer). Die System-Voraussetzungen stehen in
`docs/dev/BUILD.md` (§3 macOS, §4 Windows, §5 Linux).

Dateien:

| Datei | Zweck |
|---|---|
| `Info.plist` | Vorlage für `Contents/Info.plist` des Bundles; `__VERSION__` / `__BUILD__` werden von `make-app.sh` ersetzt |
| `make-app.sh` | baut `cargo build --release`, assembliert `target/dist/Prime Photos.app`, signiert und verifiziert |
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

Das erzeugt `target/dist/Prime Photos.app` und signiert sie **ad-hoc** (`codesign --sign -`). Starten:

```sh
open "target/dist/Prime Photos.app"
```

Das Bundle trägt ein **App-Icon** (Dock-/Taskbar-Icon): `build.rs` rastert die SVG-Quelle
`assets/logo.svg` in die zehn iconset-Größen nach `target/pp-appicon.iconset/`, und `make-app.sh`
bündelt dieses Verzeichnis mit `iconutil -c icns` zu `Contents/Resources/AppIcon.icns`. Darauf
verweist `CFBundleIconFile = AppIcon` in `packaging/Info.plist`. Das Icon entsteht also **im Build
und ist nicht eingecheckt** — im Repo liegt keine `.icns`/PNG-Datei, sie liegt unter dem
ignorierten `target/`.

### Umgebungsvariablen

| Variable | Wirkung | Standard |
|---|---|---|
| `SKIP_BUILD=1` | überspringt `cargo build --release` (nutzt das vorhandene Binary) | aus |
| `VERSION=x.y.z` | `CFBundleShortVersionString` | `0.1.0` |
| `BUILD=n` | `CFBundleVersion` | `1` |
| `CODESIGN_IDENTITY="<Name>"` | Signatur-Identität statt ad-hoc | `-` (ad-hoc) |
| `DIST_DIR=<dir>` | Ausgabeordner des Bundles | `target/dist` |

Beispiel:

```sh
SKIP_BUILD=1 VERSION=0.2.0 BUILD=2 ./packaging/make-app.sh
```

### Ad-hoc vs. selbstsignierte Identität

- **Ad-hoc** (Standard, `CODESIGN_IDENTITY=-`): keine Identität nötig, kein Schlüsselbund-Eintrag.
  Reicht für den lokalen Test. Die Signatur ist *nicht* stabil über Rebuilds und nicht für eine
  Verteilung/Notarisierung geeignet.
- **Selbstsignierte Identität** (empfohlen, wenn die Signatur stabil bleiben soll — z. B. damit
  der Bundle-Identifier und Berechtigungen dauerhaft an dieselbe App gebunden sind, was laut
  `AGENTS.md` für Benachrichtigungen relevant ist): eine eigene Identität im Schlüsselbund
  anlegen und ihren Namen dem Skript übergeben.

**Selbstsigniertes Zertifikat im Schlüsselbund anlegen (nur Beschreibung — dieses Repo legt
kein Zertifikat an und fasst den Schlüsselbund nicht an):**

1. *Schlüsselbundverwaltung* öffnen (Programme → Dienstprogramme → Schlüsselbundverwaltung).
2. Menü *Schlüsselbundverwaltung* → *Zertifikatassistent* → *Zertifikat erstellen …*.
3. **Name** z. B. `Prime Photos Codesign` frei wählen.
4. **Identitätstyp**: *Code-Signierung* („Code Signing").
5. **Zertifikatstyp**: *Selbstsigniertes Zertifikat* („Self Signed Root").
6. Erstellen und ablegen im **Anmelde-Schlüsselbund (login)**. Den eigenen Schlüsselbund
   autorisiert man selbst; für das Signieren kann macOS einmalig nach der Freigabe fragen.

Danach den **exakten Namen** der Identität ermitteln:

```sh
security find-identity -v -p codesigning
```

Der Name aus der Ausgabe (z. B. `Prime Photos Codesign`) wird übergeben:

```sh
CODESIGN_IDENTITY="Prime Photos Codesign" ./packaging/make-app.sh
```

Das Skript ruft `codesign --force --options runtime --sign "<Name>"` auf und prüft mit
`codesign --verify --verbose=2`. Zum erneuten Signieren nur der Signatur-Schritt reicht
`SKIP_BUILD=1` (bzw. ein erneuter vollständiger Lauf — das Bundle wird dabei sauber neu gebaut); mit
`SKIP_BUILD=1` muss das vom Build erzeugte `target/pp-appicon.iconset` allerdings schon aus einem
früheren Build vorliegen, sonst bricht das Skript beim Icon-Schritt ab.

## Windows (nur Hinweis)

Bauen mit dem MSVC-Target (`cargo build --release`, `docs/dev/BUILD.md` §4). Für die **Verteilung**
wäre zusätzlich eine Authenticode-Signatur (SignTool) nötig — **hier nicht umgesetzt**, es gibt
keinen Installer im Repo.

## Linux (nur Hinweis)

Kein Codesigning nötig; es würde reichen, das Binary (`target/release/prime-photos`) bzw. ein
Archiv mitzugeben. **AppImage/Flatpak/.deb sind nicht Teil dieses Skripts** (out of scope).

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
  **Nicht** verifiziert ist der Pfad mit einer selbstsignierten Identität (`CODESIGN_IDENTITY=<Name>`),
  da hier bewusst kein Zertifikat angelegt wird.
