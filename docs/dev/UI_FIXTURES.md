# Rune Live-View-Fixture und Screenshot-Harness (Debug-only)

Die Fixture ist **kein alternativer Produktionsstart**: Sie rendert die echte
Produktionsansicht `view::live_view` mit einem synthetisch gefüllten
`LiveState`, einschließlich der echten `target_card` aus
`src/rune_backup_live/targets.rs`. Sie ist nur in Debug-Builds mit
`--features rune-ui-spike` verfügbar; Release-Builds und der normale Start
enthalten keine Debug-Fixture-Route. Die Prüflisten stehen in
[`UI_REVIEW.md`](UI_REVIEW.md).

## Zweck

- Sichtprüfung von Zuständen der echten Ansicht, die sonst nur mit Konto,
  laufendem Upload oder abgezogener Festplatte entstehen.
- Reproduzierbare Screenshots für Reviews und Bugreports: dieselbe Fixture,
  dieselbe Fenstergröße, dieselbe Sprache und dieselbe Render-Skala.

Die Route `--rune-backup-live-fixture` baut den `LiveState` aus
`test_support::signed_out_state()` mit klar synthetischen Werten
(`/synthetic/...`-Pfade, `IMG_2043.HEIC`, „SYNTH …"-Ordnernamen) und startet
`App::try_run()` – kein `keep_running()`, kein natives Fenster, keine echten
Config-, Keychain-, SQLite-, Dateiauswahl- oder Netzwerkzugriffe
(`persist_config`/`persist_store` bleiben false). Die Sprache wird fest auf
Deutsch gesetzt (`i18n::set_lang(Lang::De)`), das Theme über
`state.config.appearance = Appearance::Light/Dark` aus `--theme`.

## Flag und Szenen

`--rune-backup-live-fixture` ist headless-only und akzeptiert `--scene NAME`,
`--theme light|dark`, `--size WxH` sowie genau eine Rune-Headless-Aktion
(`--snap FILE`, `--tree FILE` oder `--script FILE`). Andere Rune-Routen oder
Headless-Aktionen dürfen nicht kombiniert werden.

| Szene | Gerenderter Zustand |
|---|---|
| `mixed` | `LivePage::Backup` mit fünf Fake-Ordnern: Upload läuft, abgeschlossen, Ordner nicht gefunden, Fehler, deaktiviert. Zähler/Fortschritt/Warteschlange sind gefaked; `upload.active = true` und `folder_paths` lassen die echte `target_card` ihren Spinner zeichnen. |
| `empty` | `LivePage::Backup` ohne Sicherungsziele (Leerzustand aus der Produktionsansicht). |
| `settings` | `LivePage::Settings` mit denselben synthetischen Werten. |
| `about` | `LivePage::About`. |
| `licenses` | `LivePage::Licenses`. |

## Benutzung

Nach einem Debug-Build mit dem Debug-Feature erzeugt das Skript eine feste
Matrix mit `--snap`-PNG-Dateien und `--tree`-JSON-Dateien:

```bash
cargo build --locked --features rune-ui-spike
scripts/capture-rune-ui-review.sh
```

Alle fünf Szenen werden bei 1400×900 in Hell und Dunkel aufgenommen;
`mixed` und `empty` zusätzlich bei 1000×700 und 760×560 (dem Produktions-
Fenster-Minimum). Für jede Aufnahme gibt es einen View-Tree. Die
Standardausgabe liegt unter `target/rune-ui-review/`. `RUNE_UI_BIN` und
`RUNE_UI_REVIEW_OUT` können Binary bzw. Zielverzeichnis überschreiben. Vor dem
Capture löscht das Skript dort alle vorhandenen PNG-/JSON-Artefakte; diese
Dateien sind generiert und nicht im Git getrackt.

Beispiel für eine einzelne Aufnahme:

```bash
target/debug/prime-photos --rune-backup-live-fixture \
  --scene mixed --theme light --size 1400x900 --snap /tmp/mixed.png
```

## Tests

`cargo test --locked --features rune-ui-spike` deckt die Fixture ohne Fenster ab,
unter anderem:

- die Routen- und Argumentprüfung (`src/rune_fixture/route_tests.rs`,
  `src/rune_fixture/rune_interactive_routing_tests.rs`),
- die Szenen der Live-View-Fixture
  (`src/rune_backup_live/live_fixture.rs`), insbesondere dass die echte
  `live-targets`-Karten mit „Ordner nicht gefunden" und Spinner gerendert
  werden,
- die Geometrie-/Hover-Regressionen der echten `target_card`
  (`src/rune_backup_live/targets/tests.rs`).

Die Tests brauchen weder Netzwerk noch Dateisystem und legen keine Fixture-Pfade
an.

## Live-Screenshot als Ergänzung

Die synthetischen Bilder sind das primäre UI-Review-Artefakt. Ergänzend kann
ein manueller Live-Screenshot der interaktiven Produktionsroute (`--rune-backup`)
dieselbe Ansicht mit echtem Konto zeigen; er ist kein Bestandteil des
Headless-Captures und ersetzt weder die Fixture-Matrix noch eine native
Runtime-Validierung.
