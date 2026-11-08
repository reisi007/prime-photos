# UI-Fixtures und Screenshot-Harness (Debug-only, Issue #29)

Es gibt zwei getrennte Review-Wege, keinen gemeinsamen Renderer:

- Der **Legacy-eframe-Fixture-Harness** weiter unten rendert zehn ausgewählte
  Demo-Zustände über `PrimePhotoApp::update` und die bestehenden eframe-Seiten.
- Die **Rune-Review-Matrix** am Ende rendert die entsprechenden synthetischen
  Szenen über die dedizierte Rune-Smoke-Ansicht und die bestehenden Rune-Login-
  und Rune-Einstellungs-Fixtures.

Beide Wege sind für Entwicklung und Sichtprüfung gedacht und nur in Debug-Builds
verfügbar. Rune-Smoke-Aufnahmen belegen weder eframe-Parität noch die Freigabe
der normalen App-UI; `--rune-backup` bleibt der separate interaktive
Entwicklerpfad. Release-Builds enthalten keine eframe-Demo-Szenarien oder
Debug-Fixture-Routen. Die Migrations-Gates stehen in
[`rune-migration.md`](../features/rune-migration.md), die Prüflisten in
[`UI_REVIEW.md`](UI_REVIEW.md).

## Zweck

- Sichtprüfung von Zuständen, die sonst nur mit Konto, laufendem Upload oder
  abgezogener Festplatte entstehen.
- Reproduzierbare Screenshots für Reviews und Bugreports: dieselben Fixtures,
  dieselbe Fenstergröße und dieselbe Sprache (standardmäßig Deutsch; optional
  Englisch mit `--demo-language=en`).
- Die Demo rendert über die **bestehenden eframe**-Pfade
  (`PrimePhotoApp::update`, `backup_ui`, `settings_ui`, `login_ui`). Es gibt
  keinen zweiten eframe-Rendering-Weg — ein Screenshot zeigt, was ein echter
  Frame zeigt. Dies testet weder das Rune-Fixture noch weist es Rune-Parität
  nach.

**Nicht abgedeckt** (bewusst noch kein Szenario): der Anlegen-/Bearbeiten-Dialog
(„Datensicherungseinstellungen"), der Namenskonflikt-Dialog, eine laufende
Anmeldung mit Webview, die abgelaufene Sitzung, das Drag-&-Drop-Overlay sowie
das Über-/Lizenzfenster.

## Benutzung

```bash
# Debug-Binary bauen; Demo-Flags funktionieren nicht in Release-Builds.
cargo build --locked

# Interaktive Demo, Szenario "mixed", 1200×800 (Standard: Deutsch)
target/debug/prime-photos --demo

# Reproduzierbar hell (Demo- und Screenshot-Standard, unabhängig vom Host-Theme)
target/debug/prime-photos --demo --demo-theme=light

# Interaktive Demo eines bestimmten Szenarios
target/debug/prime-photos --demo=uploading
target/debug/prime-photos --demo all-done

# Alle Szenarien mit ID und Beschreibung ausgeben (Exit 0)
target/debug/prime-photos --demo-list

# Screenshot: fester Frame-Ablauf, PNG schreiben, selbst beenden
target/debug/prime-photos --demo-language=en \
    --screenshot="$PWD/target/prime-demo-mixed.png"
target/debug/prime-photos --demo=failures --demo-language=en \
    --screenshot="$PWD/target/prime-demo-failures.png"

# Dark-mode screenshot; `--demo-theme=system` uses native eframe/winit input
target/debug/prime-photos --demo=settings --demo-language=en --demo-theme=dark \
    --screenshot="$PWD/target/prime-demo-settings-dark.png"

# Screenshot in einer bestimmten Fenstergröße (Punkte, Minimum 760×520)
target/debug/prime-photos --demo=mixed --demo-language=en --size 760x520 \
    --screenshot="$PWD/target/prime-demo-mixed-760.png"
```

`--demo-language=en` (oder `de`) betrifft nur diesen Debug-Lauf; ohne Angabe
bleibt das bisherige deutsche Demo-Default erhalten. `--demo-theme=light|dark|system`
setzt das Erscheinungsbild nur für diesen Lauf. Ohne Angabe ist das Demo-Theme
Light, damit Screenshots unabhängig vom Entwicklerrechner reproduzierbar sind;
`system` folgt dem nativen Theme aus eframe/winit. Kein Theme-Wert wird aus
`config.toml` gelesen oder dorthin geschrieben. Release-Binaries weisen
jede Option aus `--demo*`, `--screenshot*` oder `--size*` vor dem Fensterstart
mit einer lokalisierten Meldung und Exit-Code 2 zurück.

`--size WxH` gilt nur für Demo-/Screenshot-Läufe und akzeptiert `760x520` wie
`760X520`; alles unter dem App-Minimum **760×520**, über **8192×8192** oder in
einem anderen Format (z. B. `760`, `760x`, `760,520`) wird mit Exit-Code 2
abgelehnt. Ohne `--size` gilt 1400×900 für Screenshots und 1200×800 für die
interaktive Demo.

Der Screenshot-Pfad **muss absolut** sein; ein relativer Pfad wird mit einer
Fehlermeldung und Exit-Code 2 abgelehnt. Ebenso ein unbekanntes Szenario, ein
unbekanntes Flag aus unserem Namensraum (`--demo…`/`--screenshot…`/`--size…`)
oder ein weiteres Argument neben `--demo-list`.

Argumente, die **nicht** zu Prime Photos gehören, werden beim normalen Start
ignoriert — macOS injiziert z. B. `-psn_0_…` beim ersten Bundle-Start, IDEs
schicken eigene Flags. Sobald aber ein Demo-Lauf angefordert ist, gilt ein
solches Argument als Tippfehler und wird gemeldet.

## Legacy-eframe-Szenarien

| ID | Beschreibung |
|---|---|
| `mixed` | Referenzbild: 6 Ordner — 3 vorhandene sind Teil des laufenden Uploads (Karten „Upload-Vorgang läuft" mit 63 / 62 / 62 Dateien, davon 13 / 12 / 12 fertig), 3 fehlende zeigen „Ordner nicht gefunden". 187 Queue-Einträge: 37 fertig, 9 in Bearbeitung (12 / 100 / 21 / 8 / 64 / 37 / 3 / 45 / 72 %), 141 wartend; Gesamtbalken „37 von 187". |
| `uploading` | Laufender Upload, 53 Zeilen (5 fertig, 8 in Bearbeitung, 40 wartend) auf 3 Ordner verteilt (18 / 18 / 17 Zeilen, davon 2 / 2 / 1 fertig); Karten live. |
| `paused` | Wie `uploading`, aber globale Pause aktiv: Zeilen bleiben sichtbar, Schaltfläche bietet „Alle Sicherungsvorgänge fortsetzen". |
| `all-done` | 3 Ordner vollständig, leere Warteschlange, kein Lauf. Die Gesamt-Fortschrittsleiste über der Ordnerliste zeigt „Alles aktuell"; die Log-Leiste unten meldet „Bereit". |
| `failures` | Ein Ordner mit Status „Fehler" (3 Fehler, 317 von 320), ein fertiger Ordner, 7 Queue-Zeilen (4 fertig, 3 fehlgeschlagen) und eine Ladefehler-Zeile in der Seitenleiste. |
| `empty` | Angemeldet, aber keine Sicherungsziele und keine Warteschlange (beide Leerzustände). |
| `many-folders` | 12 Ordner: 0–5 nehmen am Lauf teil („Upload-Vorgang läuft"), 6 „Warten", 7 „Fehler", 8 nie gescannt (kein Status), 9–11 „Ordner nicht gefunden" — prüft das Scrollen der Liste. |
| `settings` | Einstellungsseite, Tab „Allgemeines". |
| `settings-bandwidth` | Einstellungsseite, Tab „Bandbreite" (Slider, Rate, Standardwerte für neue Ziele). |
| `signed-out` | Anmeldebildschirm. Unsere eigene Seite rendert ohne Webview. In der **Demo** ist „Mit Amazon anmelden" wirkungslos (kein Webview, kein Netzwerk); in der normalen App öffnet derselbe Knopf die Anmeldung. |

Neue Szenarien gehören in `src/demo/scenario.rs` (ID, Beschreibung, `ALL`) und
in `src/demo/fixture.rs` (Dispatch) mit einem Builder in
`src/demo/live_scenarios.rs` (mit aktivem Lauf) bzw.
`src/demo/settled_scenarios.rs` (ohne). Dabei gilt:

- `source_exists` hat **pro Ordner** genau einen Eintrag; `false` simuliert
  „Ordner nicht gefunden", ohne dass ein Verzeichnis existieren muss. Im Demo
  antwortet ein Index ohne Fixture-Eintrag `false` (fail closed), damit die UI
  nie einen echten Pfad prüft, den die Fixture nicht beschreibt.
- `folder_paths`, `folder_total`, `folder_done`, `folder_new`, `folder_failed`
  werden **gemeinsam aus der Queue abgeleitet** (`fixture::published_run` bzw.
  `settled_run`). Nur Ordner mit Queue-Zeilen bekommen Einträge; fehlende
  Quellen bleiben außen vor. Ein Ordner mit Zeilen zeigt dadurch zwingend
  „Upload-Vorgang läuft" mit genau den Zahlen seiner Zeilen — der Zustand
  „fertige Karte neben laufenden Zeilen" ist im echten Modell unmöglich.
- `upload.total == upload.queue.len()`, `upload.done` == Anzahl der `Done`-Zeilen,
  `upload.failed` == Anzahl der `Failed`-Zeilen; die Summen der Ordnermaps
  gleich den Lauf-Zählern.
- Namen, Größen und Prozente sind aus dem Index abgeleitet (kein Zufall), damit
  die Daten reproduzierbar sind. Zwei PNGs desselben Szenarios sind trotzdem
  **nicht byte-identisch**: der Fortschritts-Spinner animiert und kann in
  unterschiedlichen Drehstellungen erfasst werden (ETA und Zahlen sind fix).

## Sicherheitsgarantien

Ein Demo-Lauf ist **nicht** destruktiv und hat keine Außenwirkung:

| Bereich | Verhalten |
|---|---|
| `config.toml` | wird weder gelesen noch geschrieben (`save_config`, Anmelde-Start und Logout sind No-Ops). |
| Theme-Präferenz | der Demo-Override kommt nur aus `--demo-theme`; die Präferenz der echten Installation wird weder gelesen noch geändert. |
| `folders.toml` | wird nicht geladen; die Fixtures ersetzen den Store, ein Reload im `update` ist abgeschaltet. |
| Keychain | kein Lesezugriff, kein Löschen (`poll_session`/`logout` sind gesperrt). |
| SQLite (`dedup.sqlite3`) | wird nicht geöffnet — auch nicht für eine wiederhergestellte Einmal-Sicherung (`resume_one_time` gesperrt). |
| Netzwerk | keine Requests: kein Kontodaten-Abruf, keine Sitzungs-Aktualisierung, kein Upload, keine Anmeldung. |
| Benachrichtigungen | keine (`notify::init` läuft im Demo-Modus nicht, der Abschluss-Aufruf ist zusätzlich gesperrt). |
| Einzelinstanz | `single_instance::acquire` läuft nicht — eine laufende echte Instanz schluckt die Demo nicht. |
| Tray / Login-Item | kein Tray (auch der Tray-Aufbau ist gesperrt), keine Autostart-Registrierung (`Autostart` bleibt leer, `set_autostart` ist ein No-Op). |
| Datei-Dialoge / Finder | nicht erreichbar (`add_backup_folder`, `apply_action`, der Formular-Handler sind No-Ops). |
| Hintergrund-Sync / Finalisierung | kein Supervisor, keine Filesystem-Watcher, kein Store-Schreiben (`finalize_upload` gesperrt). |
| Logdatei | **wird geschrieben**: `logging::init()` öffnet wie im Normalbetrieb `<Config-Verzeichnis>/prime_photos/prime-photos.log` im Append-Modus und protokolliert den Start. Das ist die einzige Datei außer dem Screenshot, die ein Demo-Lauf anfasst. |

Die Pause-Schaltfläche darf in der Demo umschalten (rein im Speicher), damit der
Pausenzustand sichtbar ist.

## Screenshots

Zwei Wege, bewusst getrennt:

- **Automatisierte Screenshots** (Reviews und dieser Harness) laufen mit der
  Debug-Binary `target/debug/prime-photos`; nur dort ist der Harness enthalten.
  Das Bild entsteht im Renderer über egui (`ViewportCommand::Screenshot`) und
  hat auf einem 1×-Bildschirm exakt die Fenstergröße in Pixeln.
- Das Release-`.app` aus `packaging/make-app.sh` enthält absichtlich keinen
  Harness; Demo-Flags werden auch dort abgelehnt. Retina-Prüfungen am Release
  prüfen die normale UI, nicht Fixture-Screenshots. Die Debug-Binary hat keine
  `Info.plist`/`NSHighResolutionCapable`; Details siehe `docs/dev/DEV_NOTES.md` §7.

```bash
# Automatisiert aus dem Debug-Binary (1×): 1400×900 PNG
cargo build --locked
target/debug/prime-photos --demo=mixed --demo-language=en \
    --screenshot="$PWD/target/prime-demo-mixed.png"

# Minimum-Fenster für die Rand-/Clip-Prüfung (Issue #30)
target/debug/prime-photos --demo=mixed --demo-language=en --size 760x520 \
    --screenshot="$PWD/target/prime-demo-mixed-760.png"
```

Die Fenstergröße wird in **Punkten** angefordert: 1400×900 im Screenshot-Modus,
1200×800 interaktiv, per `--size` frei wählbar (Minimum 760×520). Die PNG-Datei
enthält **physische Pixel**; auf einem 2×-Display bzw. im Bundle ist sie daher
2800×1800, bei `--size 760x520` also 1520×1040. Beide Demo-Modi behalten ihre
Größe (keine bildschirmabhängige Anpassung), damit das Bild reproduzierbar
bleibt.

Das Fenster wird sichtbar und fokussiert gestartet und nach dem Schreiben
geschlossen; der Prozess endet von selbst. Angefordert wird nach drei
eingeschwungenen Frames (Texturen und Schriften sind dann geladen). Zwei
Sicherheitsnetze verhindern ein Hängen: eine Frame-Grenze (3000 Frames ohne
Antwort) und ein Watchdog-Thread mit 15 s Wanduhr-Deadline, der den Prozess mit
Exit-Code 1 beendet, wenn keine Datei geschrieben wurde. Ein fehlgeschlagenes
Schreiben beendet den Lauf ebenfalls mit Exit-Code 1 und einer Meldung auf
stderr und im Log; ein erfolgreicher Lauf setzt vor dem Schließen ein Latch,
damit ein späterer Frame den Erfolg nicht in einen Timeout umdeutet.

## Tests

`cargo test --locked` deckt die Demo ohne Fenster ab (`src/demo/tests.rs`):

- Argumente: alle Formen (`--demo`, `--demo=<id>`, `--demo <id>`,
  `--screenshot <Pfad>`, `--screenshot=<Pfad>`, `--size WxH`/
  `--size=WxH`, Kombination, Reihenfolge), optionale Demo-Sprache
  (`--demo-language=de|en`), Demo-Theme (`--demo-theme=system|light|dark`),
  Off ohne Argumente, `--demo-list`
  (eigener Modus; zusätzliche oder kombinierte Argumente sind ein Fehler),
  Injektionen wie `-psn_0_…` (Normalstart bleibt möglich), relative
  Screenshot-Pfade, unbekannte Argumente und Szenarien (Fehler enthalten die
  Verwendung). `--size` wird auf Format und den Bereich 760×520…8192×8192
  geprüft und ist ohne `--demo`/`--screenshot` ein Fehler.
- Szenarien: `parse(id())` ist für jedes Szenario ein Roundtrip, IDs sind
  eindeutig und vollständig (zehn, inklusive `settings-bandwidth`).
- Fixtures: ein `source_exists`-Eintrag pro Ordner; `mixed` hat exakt 187
  Zeilen (37/9/141), 150 offene Zeilen, die Prozentwerte 12/100/21/8 und
  Live-Zähler 63/13, 62/12, 62/12 auf den vorhandenen Ordnern; `all-done` hat
  keine offenen Zeilen; `mixed` markiert drei fehlende Quellen; Sitzungsfelder
  sind in allen angemeldeten Szenarien nicht leer und in `signed-out` leer.
- Konsistenz: für **jedes** Szenario gilt pro Queue-Ordner
  `folder_total` == Anzahl seiner Zeilen, `folder_done`/`folder_new` == Anzahl
  seiner `Done`-Zeilen, `folder_failed` == Anzahl seiner `Failed`-Zeilen,
  `folder_paths` == Ordnerpfad; jeder `folder_paths`-Eintrag hat einen
  `folder_total`-Eintrag, Ordner ohne Zeilen haben keinen, und jeder
  Queue-Ordner ist im Store vorhanden und als vorhanden markiert.

Die Tests brauchen weder Netzwerk noch Dateisystem und dürfen keine der
Fixture-Pfade anlegen.

## Rune-Review-Matrix

Die Rune-Matrix ist eine **separate Headless-Ansicht**, kein alternativer
Produktionsstart und kein Ersatz für die eframe-Demo. `--rune-backup-smoke`
zeigt die Backup-Szenen `mixed`, `uploading`, `paused`, `all-done`, `failures`,
`empty` und `many-folders` sowie die vorhandenen About- und Lizenz-Routen. Die
Anmeldung (`signed-out` und `error`) bleibt in `--rune-login-fixture`; die Tabs
Allgemeines und Bandbreite bleiben im Rune-Einstellungs-Fixture. Dieses Fixture
startet aus `Config::default()` ohne Speicherpfad. Die Fenster und Screenshots
greifen nicht auf Konten, Konfiguration, Keychain, SQLite, Nutzdateien, Netzwerk,
Dateiauswahl oder native OS-Aktionen zu. Das Aufnahmeskript schreibt nur die
explizit angeforderten PNG-/JSON-Artefakte in sein Zielverzeichnis.
Backup-Smoke-Schaltflächen ändern ausschließlich den lokalen Fixture-Zustand.

Nach einem Debug-Build mit dem optionalen Rune-Feature erzeugt das Skript eine
feste Matrix mit `--snap`-PNG-Dateien und `--tree`-JSON-Dateien:

```bash
cargo build --locked --features rune-ui-spike
scripts/capture-rune-ui-review.sh
```

Die Standardausgabe liegt unter `target/rune-ui-review/`. `RUNE_UI_BIN` und
`RUNE_UI_REVIEW_OUT` können Binary bzw. Zielverzeichnis überschreiben. Alle
Rune-Backup-Szenen entstehen bei 1400×900 in Hell und Dunkel; `mixed`, `failures`
und `many-folders` zusätzlich bei 760×560 (Fixture-Minimum) und 1000×700 in
beiden Themes. Für jede der neun Backup-Szenen wird außerdem bei 1400×900 ein
Rune-View-Tree gespeichert. Signed-out, Login-Fehler, Einstellungen-Allgemeines
und Bandbreite werden jeweils bei 760×560 und 1000×700 in beiden Themes
aufgenommen. Sprache und Render-Skala sind fest auf Deutsch bzw. 1× gesetzt.

**Aufnahmestatus dieser Änderung:** Workflow
[#42512063274](https://github.com/prime-photos/prime-photos/actions/runs/42512063274)
erzeugte am 2026-11-08 das erfolgreiche Rune-Artefakt für Commit
`ff3665a503248e6e3d8253bf6c913140f65f67f4`; es wurde unter
`/tmp/rune-ui-review-42512063274` abgelegt. Stichproben der Fixture-Ansichten
wurden visuell geprüft; Details siehe [`UI_REVIEW.md`](UI_REVIEW.md). Die
vollständige Matrix ist nicht erschöpfend geprüft. Die Legacy-eframe-Matrix
wurde nicht aufgenommen; ihr Screenshot-Review, Produktionsparität und die
native macOS/AppKit/Wry-Runtime-Validierung bleiben offen.
