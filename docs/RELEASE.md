# Prime Photos — Release & Verteilung

Wie ein Release dieses Projekts entsteht und wie ein heruntergeladener Build geprüft wird.
System-Voraussetzungen zum Bauen: `docs/BUILD.md`. macOS-Bundle lokal: `packaging/README.md`.
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

---

## 2. Was das CI-Release erzeugt

Der `build`-Job ist eine Matrix (`fail-fast: false`). Der `release`-Job hat `needs: [build]` und
läuft damit **nur, wenn beide Beine grün sind** — sonst entsteht kein Release.

| Bein | Runner | Artefakt |
|---|---|---|
| Windows x64 | `windows-latest` | `Prime-Photos-windows-x64.zip` (enthält `prime-photos.exe`) |
| Linux x64 | `ubuntu-latest` | `Prime-Photos-linux-x64.tar.gz` (enthält `prime-photos`) |

- Beide bauen mit `cargo build --release --locked`.
- Der Linux-Job installiert die System-Abhängigkeiten aus `docs/BUILD.md` §5:
  `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev libssl-dev pkg-config build-essential`.
- **Es werden keine SHA256-Prüfsummen erzeugt** (nicht implementiert).

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
- Kein `codesign --deep` (von Apple abgeraten). Details: `packaging/README.md`.
- Das fertige Bundle wird bei Bedarf **manuell** an ein bestehendes GitHub Release gehängt
  (ein `.zip` per `ditto -c -k --keepParent "Prime Photos.app" Prime-Photos-macos-arm64.zip`
  erhält die Bundle-Struktur). Das ist **nicht** automatisierter Teil des Workflows.

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

```sh
ditto -x -k Prime-Photos-macos-arm64.zip .            # oder Doppelklick im Finder
codesign --verify --verbose=2 "Prime Photos.app"      # gültige Signatur?
codesign -dv "Prime Photos.app"                        # Authority / "Signature=adhoc"
spctl -a -vvv --type execute "Prime Photos.app"        # Gatekeeper-Bewertung
```

Bei einem ad-hoc-/selbstsignierten Bundle meldet `spctl` „rejected" → Workaround aus §5.

---

## 8. Fork-PRs und Secrets

Secrets werden **nicht** an Läufe aus Forks weitergereicht (GitHub-Verhalten). Ein `pull_request`
aus einem Fork sieht die Signatur-Secrets nicht; Build und Test laufen ohne Secrets. Release-Läufe
(`push` eines Tags) entstehen im Haupt-Repo und erhalten die Secrets.

---

## 9. Nicht verifiziert / offen

- **Kein Release wurde tatsächlich ausgelöst.** Die Workflows sind nur syntaktisch geprüft
  (YAML-`safe_load`), kein CI-Lauf fand statt.
- Windows-Authenticode-Signatur (§3) nicht ausgeführt; `signtool`-Verfügbarkeit nicht geprüft.
- macOS-Notarisierung/Stapling nicht eingerichtet und nicht geprüft.
- Keine SHA256-Prüfsummen im Release (nicht implementiert).
- Ob `softprops/action-gh-release@v2` + `actions/download-artifact@v4` in dieser Kombination das
  gewünschte Release erzeugen, ist nicht durch einen echten Lauf belegt.

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

## 11. Referenzen

- `docs/BUILD.md` — System-Voraussetzungen macOS/Windows/Linux.
- `packaging/README.md` — `make-app.sh`, `Info.plist`, ad-hoc vs. eigene Identität.
- `AGENTS.md` „CI/Release" — die getroffenen Entscheidungen.
- `.github/workflows/ci.yml`, `.github/workflows/release.yml` — die Workflows selbst.
