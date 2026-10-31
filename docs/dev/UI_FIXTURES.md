# UI-Fixtures und Screenshot-Harness (Issue #29)

Die Demo-Datenquelle rendert neun ausgewählte UI-Zustände ohne echte
Amazon-Sitzung und ohne echte Fotordner. Sie ist ausschließlich für Entwicklung
und Sichtprüfung gedacht; die normale Anwendung ändert sich nicht.

## Zweck

- Sichtprüfung von Zuständen, die sonst nur mit Konto, laufendem Upload oder
  abgezogener Festplatte entstehen.
- Reproduzierbare Screenshots für Reviews und Bugreports: dieselben Fixtures,
  dieselbe Fenstergröße, dieselbe Sprache (Deutsch).
- Die Demo rendert über die **bestehenden** Pfade (`PrimePhotoApp::update`,
  `backup_ui`, `settings_ui`, `login_ui`). Es gibt keinen zweiten
  Rendering-Weg — ein Screenshot zeigt, was ein echter Frame zeigt.

**Nicht abgedeckt** (bewusst noch kein Szenario): der Anlegen-/Bearbeiten-Dialog
(„Datensicherungseinstellungen"), der Namenskonflikt-Dialog, der Tab
„Bandbreite", eine laufende Anmeldung mit Webview, die abgelaufene Sitzung, das
Drag-&-Drop-Overlay sowie das Über-/Lizenzfenster.

## Benutzung

```bash
# Interaktive Demo, Szenario "mixed", 1200×800
target/release/prime-photos --demo

# Interaktive Demo eines bestimmten Szenarios
target/release/prime-photos --demo=uploading
target/release/prime-photos --demo all-done

# Alle Szenarien mit ID und Beschreibung ausgeben (Exit 0)
target/release/prime-photos --demo-list

# Screenshot: fester Frame-Ablauf, PNG schreiben, selbst beenden
target/release/prime-photos --screenshot=/tmp/prime-demo-mixed.png
target/release/prime-photos --demo=failures --screenshot=/tmp/prime-demo-failures.png
```

Der Screenshot-Pfad **muss absolut** sein; ein relativer Pfad wird mit einer
Fehlermeldung und Exit-Code 2 abgelehnt. Ebenso ein unbekanntes Szenario, ein
unbekanntes Flag aus unserem Namensraum (`--demo…`/`--screenshot…`) oder ein
weiteres Argument neben `--demo-list`.

Argumente, die **nicht** zu Prime Photos gehören, werden beim normalen Start
ignoriert — macOS injiziert z. B. `-psn_0_…` beim ersten Bundle-Start, IDEs
schicken eigene Flags. Sobald aber ein Demo-Lauf angefordert ist, gilt ein
solches Argument als Tippfehler und wird gemeldet.

## Szenarien

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

- **Automatisierte Screenshots** (Reviews, CI, dieser Harness) laufen mit der
  Release-Binary `target/release/prime-photos`. Das Bild entsteht im Renderer
  über egui (`ViewportCommand::Screenshot`) und hat auf einem 1×-Bildschirm
  exakt die Fenstergröße in Pixeln.
- **Visuelle Retina-Prüfungen** gehen über das gebaute `.app`-Bundle
  (`packaging/make-app.sh`), siehe `docs/dev/DEV_NOTES.md` §7 — der nackten
  Binary fehlt `Info.plist`/`NSHighResolutionCapable`, das Bild ist dort unscharf.

```bash
# Automatisiert (1×): 1400×900 PNG
target/release/prime-photos --demo=mixed --screenshot=/tmp/prime-demo-mixed.png

# Retina-Sichtprüfung über das Bundle (2× → 2800×1800 PNG)
packaging/make-app.sh
"target/dist/Prime Photos.app/Contents/MacOS/prime-photos" \
    --demo=mixed --screenshot=/tmp/prime-demo-mixed@2x.png
```

Die Fenstergröße wird in **Punkten** angefordert: 1400×900 im Screenshot-Modus,
1200×800 interaktiv (Minimum 760×520). Die PNG-Datei enthält **physische
Pixel**; auf einem 2×-Display bzw. im Bundle ist sie daher 2800×1800. Beide
Demo-Modi behalten ihre Größe (keine bildschirmabhängige Anpassung), damit das
Bild reproduzierbar bleibt.

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

`cargo test` deckt die Demo ohne Fenster ab (`src/demo/tests.rs`):

- Argumente: alle Formen (`--demo`, `--demo=<id>`, `--demo <id>`,
  `--screenshot <Pfad>`, `--screenshot=<Pfad>`, Kombination, Reihenfolge),
  Off ohne Argumente, `--demo-list` (eigener Modus; zusätzliche oder
  kombinierte Argumente sind ein Fehler), Injektionen wie `-psn_0_…`
  (Normalstart bleibt möglich), relative Screenshot-Pfade, unbekannte
  Argumente und Szenarien (Fehler enthalten die Verwendung).
- Szenarien: `parse(id())` ist für jedes Szenario ein Roundtrip, IDs sind
  eindeutig und vollständig.
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
