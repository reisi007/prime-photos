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

Das erzeugt `target/dist/Prime Photos.app` und signiert sie mit der lokalen Codesigning-Identität
`Prime Photos Local Signer`, sofern die im Schlüsselbund liegt und `CODESIGN_IDENTITY` nicht gesetzt
oder leer ist; ist `CODESIGN_IDENTITY` nicht leer (z. B. `-`), gewinnt dieser Wert. Starten:

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
| `VERSION=x.y.z` | `CFBundleShortVersionString` | `0.1.6` |
| `BUILD=n` | `CFBundleVersion` | `6` |
| `CODESIGN_IDENTITY="<Name>"` | Signatur-Identität statt des Standards (gewinnt immer) | lokale Identität `Prime Photos Local Signer`, sonst `-` (ad-hoc) |
| `DIST_DIR=<dir>` | Ausgabeordner des Bundles | `target/dist` |

Beispiel:

```sh
SKIP_BUILD=1 VERSION=0.2.0 BUILD=2 ./packaging/make-app.sh
```

### Signatur: lokale Identität vs. ad-hoc

- **Standard (lokale Identität, falls vorhanden):** liegt im Schlüsselbund eine Codesigning-Identität
  mit dem exakten Namen `Prime Photos Local Signer`, nimmt das Skript sie automatisch. Grund: eine
  Zertifikats-Identität hat ein über Rebuilds **stabiles** Designated Requirement (Identifier +
  Zertifikat). Erwartete Folge: die Keychain-ACL des gespeicherten Refresh-Tokens bleibt bei selbst
  gebauten Updates gültig, und der Schlüsselbund-Dialog beim Start entfällt — **belegt ist das
  nicht**, siehe „Nicht verifiziert" am Ende dieser Datei.
- **Ad-hoc (`CODESIGN_IDENTITY=-` oder keine Identität):** keine Identität nötig, kein
  Schlüsselbund-Eintrag; greift auch, wenn `security` fehlt oder die Identität nicht gefunden wird.
  Reicht für den lokalen Test, ist aber *nicht* stabil über Rebuilds: bei ad-hoc ist das Designated
  Requirement der cdhash, der sich mit jedem Build ändert — die Keychain-ACL des Tokens passt danach
  nicht mehr, und macOS fragt beim Start nach dem Passwort für den Schlüsselbund „Anmeldung".
  Für eine Verteilung/Notarisierung ist ad-hoc ohnehin nicht geeignet.
- **Andere Identität:** `CODESIGN_IDENTITY="<Name>"` gewinnt immer, auch gegen die automatische
  Erkennung — der Name muss exakt dem Eintrag aus `security find-identity -v -p codesigning`
  entsprechen.

Die Reihenfolge ist damit: explizite Umgebungsvariable, sonst `Prime Photos Local Signer`, sonst
ad-hoc; das Skript meldet die gewählte Identität in der Zeile `==> Signatur: …`.

Scheitert `codesign` mit der gefundenen Identität (gesperrter Schlüssel, fehlende Freigabe), bricht
das Skript ab; in diesem Fall bewusst `CODESIGN_IDENTITY=-` setzen (ad-hoc, ohne den
Keychain-ACL-Vorteil).

**Selbstsigniertes Zertifikat im Schlüsselbund anlegen (nur Beschreibung — dieses Repo legt
kein Zertifikat an und fasst den Schlüsselbund nicht an):**

1. *Schlüsselbundverwaltung* öffnen (Programme → Dienstprogramme → Schlüsselbundverwaltung).
2. Menü *Schlüsselbundverwaltung* → *Zertifikatassistent* → *Zertifikat erstellen …*.
3. **Name**: für die automatische Erkennung exakt `Prime Photos Local Signer`; jeder andere Name
   geht auch, muss dann aber per `CODESIGN_IDENTITY` übergeben werden.
4. **Identitätstyp**: *Code-Signierung* („Code Signing").
5. **Zertifikatstyp**: *Selbstsigniertes Zertifikat* („Self Signed Root").
6. Erstellen und ablegen im **Anmelde-Schlüsselbund (login)**. Den eigenen Schlüsselbund
   autorisiert man selbst; für das Signieren kann macOS einmalig nach der Freigabe fragen.

Danach den **exakten Namen** der Identität ermitteln:

```sh
security find-identity -v -p codesigning
```

Der Name aus der Ausgabe wird übergeben — nur nötig, wenn er von `Prime Photos Local Signer`
abweicht (im Beispiel die frei gewählte Identität `Prime Photos Codesign`):

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
  Der Pfad mit einer selbstsignierten Identität (`CODESIGN_IDENTITY=<Name>`) wurde am 2026-10-26
  für die lokale Identität `Prime Photos Local Signer` auf diesem Rechner mit `SKIP_BUILD=1`
  geprüft: `codesign -dv --verbose=2` meldet `Authority=Prime Photos Local Signer`,
  `codesign --verify --strict` läuft fehlerfrei durch. Mit einem **anderen** Namen ist der Pfad
  nicht geprüft. **Nicht** belegt ist zudem, ob der Schlüsselbund-Dialog damit dauerhaft ausbleibt;
  erwartet wird es, weil das Designated Requirement über Rebuilds gleich bleibt — über mehrere
  aufeinanderfolgende Builds ist das noch nicht abgenommen.
