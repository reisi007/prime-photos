# UI-Review-Checkliste

Prüfliste und Befehle für die visuelle Abnahme von Fenster-Layout, Abständen
und Textabbrüchen. Alle synthetischen Aufnahmen kommen aus der
Rune-Live-View-Fixture ([`UI_FIXTURES.md`](UI_FIXTURES.md)): Sie rendert die
**echte Produktionsansicht** `live_view` mit Fake-Daten. Es wird **nichts** am
echten Konto, an `config.toml`, Keychain, SQLite oder im Netzwerk angefasst.

## Bauen

```bash
cargo build --locked
```

Das Release-Bundle kann separat für eine **Retina-Sichtprüfung der normalen
UI** gebaut werden ([`DEV_NOTES.md`](DEV_NOTES.md) §7). Es enthält keinen
Fixture-Harness; Fixture-Screenshots müssen aus der Debug-Binary kommen.

```bash
packaging/make-app.sh
```

## Rune-native Headless-Review-Matrix

Das Skript setzt die Sprache und Skala fest, wählt jede Szene per
Rune-`--scene` und schreibt PNG-Screenshots sowie View-Trees. Es nutzt
ausschließlich `--rune-backup-live-fixture` und rendert damit die echte
Produktionsansicht inklusive echter `target_card`. Es ruft **nicht**
`--rune-backup` auf und ändert den Produktions-Launcher nicht.

```bash
cargo build --locked --features rune-ui-spike
scripts/capture-rune-ui-review.sh
```

| Fixture | Szenen | Ansichten |
|---|---|---|
| Live-View-Fixture | `mixed`, `empty`, `settings`, `about`, `licenses` | alle bei 1400×900 in Hell/Dunkel; `mixed` und `empty` zusätzlich bei 1000×700 und 760×560 in Hell/Dunkel; View-Tree je Aufnahme |

`mixed` zeigt fünf synthetische Zielordner mit den Zuständen Upload läuft,
Abgeschlossen, Ordner nicht gefunden, Fehler und deaktiviert sowie gefakte
Zähler, Fortschritt und Warteschlange. Die erzeugten Dateien liegen
standardmäßig in `target/rune-ui-review/` und heißen
`live-<szene>-<theme>-<size>.png` bzw. `.json`. `RUNE_UI_BIN` und
`RUNE_UI_REVIEW_OUT` überschreiben Binary und Zielordner. Die
Fixture-Sicherheitsgrenzen stehen in [`UI_FIXTURES.md`](UI_FIXTURES.md).

Die Bilder sind das primäre Artefakt der Fixture-Sichtprüfung. Sie sind **kein
Produktions-Paritätsnachweis** und keine native macOS/AppKit/Wry-
Runtime-Validierung. Produktionsdialoge sowie Integrations-/Native-Runtime-
Validierung bleiben offen; ein Live-Screenshot der interaktiven Route kann die
Fixture-Bilder ergänzen, ist aber nicht Teil der Matrix.

Für einen manuellen macOS-Lauf kann in GitHub unter **Actions → Rune headless UI
review → Run workflow** der Workflow für den gewünschten Branch gestartet und
das Artefakt `rune-ui-review` heruntergeladen werden. Er prüft zuerst Build und
Tests der Produktionsgrenze (Release-Binary wird nicht gestartet), dann
Debug-Build und -Tests mit Standard-Features sowie die Fixtures, die das
Capture-Skript aus der Debug-Binary rendert. `--rune-backup` wird nicht
übergeben. Der Upload wird auch nach einem fehlgeschlagenen Capture versucht
und kann dann Teilaufnahmen enthalten; ohne erzeugte Dateien gibt es kein
Artefakt. Alles bleibt headless: kein natives Fenster, AppKit-/Wry- oder
Produktionskonto-Runtime-Smoke.

## Prüfliste

- [ ] **Seitenrand**: kein Inhalt berührt den Fensterrand; die gemeinsame rechte
  Kante von Tabelle, Fortschritt und Karten ist eingehalten.
- [ ] **Sidebar und Inhalt**: Logo und erste Inhaltszeile der Seite liegen auf
  derselben Höhe; die Navigation ist per Tastatur erreichbar.
- [ ] **Tabelle**: Kopfzeile und Werte stehen exakt übereinander; die Statusspalte
  läuft nie in die nächste Spalte.
- [ ] **Ordnerkarte**: Status und Aktionssymbole enden vor dem Kartenrand; kein
  Text läuft unter die Symbole. Der Label↔Toggle-Abstand bleibt 8 pt, die
  Gruppe ist rechtsbündig, und deaktivierte Controls zeigen keinen Hover.
- [ ] **Kleine Fenster (760×560)**: Listen scrollen, statt Inhalte zu überdecken;
  der Kontoblock bleibt vollständig sichtbar.
- [ ] **Kontrast**: normaler Text einschließlich gedämpfter und semantischer
  Statusfarben erreicht mindestens **4.5:1**; bedeutungsvolle Control-Ränder,
  Slider-Schienen und Fokusindikatoren mindestens **3:1**. Die Palette-Tests
  berechnen die WCAG-Kontrastwerte für Hell und Dunkel.
- [ ] **Slider**: die 1–8 Upload-Parallelität lässt sich per Maus ziehen, aktive
  und inaktive Schiene sind klar unterscheidbar, und der exakte ganzzahlige Wert
  bleibt sichtbar.
- [ ] **Beide Themes**: Backup, Einstellungen, About, Lizenzen und
  Statusanzeigen haben passende dunkle/helle Flächen; keine alten hellen Karten
  oder dunklen Texte auf dunklen Oberflächen.

## Automatisierte Randprüfung

Ein kurzes Skript (Python + Pillow) beantwortet die harten Fragen aus der
Matrix. Die erwarteten Kanten werden aus der jeweiligen Rune-View-Tree-Aufnahme
abgeleitet; das Skript ersetzt keine Sichtprüfung der Fixture-Szenen.
