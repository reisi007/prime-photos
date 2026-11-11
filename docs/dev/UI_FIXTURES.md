# Rune UI-Fixtures und Screenshot-Harness (Debug-only)

Die Fixtures sind eine **separate Headless-Ansicht**, kein alternativer
Produktionsstart. Sie sind nur in Debug-Builds mit `--features rune-ui-spike`
verfügbar; Release-Builds und der normale Start enthalten keine
Debug-Fixture-Routen. Die Prüflisten stehen in [`UI_REVIEW.md`](UI_REVIEW.md).

## Zweck

- Sichtprüfung von Zuständen, die sonst nur mit Konto, laufendem Upload oder
  abgezogener Festplatte entstehen.
- Reproduzierbare Screenshots für Reviews und Bugreports: dieselben Fixtures,
  dieselbe Fenstergröße, dieselbe Sprache und dieselbe Render-Skala.

`--rune-backup-smoke` zeigt die Backup-Szenen `mixed`, `uploading`, `paused`,
`all-done`, `failures`, `empty` und `many-folders` sowie die vorhandenen About-
und Lizenz-Routen. Die Anmeldung (`signed-out` und `error`) bleibt in
`--rune-login-fixture`; die Tabs Allgemeines und Bandbreite bleiben im
Rune-Einstellungs-Fixture. Dieses Fixture startet aus `Config::default()` ohne
Speicherpfad. Die Fenster und Screenshots greifen nicht auf Konten,
Konfiguration, Keychain, SQLite, Nutzdateien, Netzwerk, Dateiauswahl oder native
OS-Aktionen zu. Das Aufnahmeskript schreibt nur die explizit angeforderten
PNG-/JSON-Artefakte in sein Zielverzeichnis. Backup-Smoke-Schaltflächen ändern
ausschließlich den lokalen Fixture-Zustand.

## Benutzung

Nach einem Debug-Build mit dem Debug-Feature erzeugt das Skript eine feste
Matrix mit `--snap`-PNG-Dateien und `--tree`-JSON-Dateien:

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

## Tests

`cargo test --locked --features rune-ui-spike` deckt die Rune-Fixtures ohne Fenster ab, unter
anderem:

- die Rune-Routen- und Argumentprüfung (`src/rune_fixture/route_tests.rs`),
- die Fixture-Szenen und ihre Navigation
  (`src/rune_fixture/rune_interactive_routing_tests.rs`),
- die Login-, Backup-, About- und Einstellungs-Fixture-Zustände in den jeweiligen
  `tests`-Modulen.

Die Tests brauchen weder Netzwerk noch Dateisystem und dürfen keine
Fixture-Pfade anlegen.

## Aufnahmestatus

Der Workflow
[#42512063274](https://github.com/prime-photos/prime-photos/actions/runs/42512063274)
erzeugte am 2026-11-08 das erfolgreiche Rune-Artefakt für Commit
`ff3665a503248e6e3d8253bf6c913140f65f67f4`; es wurde unter
`/tmp/rune-ui-review-42512063274` abgelegt. Stichproben der Fixture-Ansichten
wurden visuell geprüft; Details siehe [`UI_REVIEW.md`](UI_REVIEW.md). Die
vollständige Matrix ist nicht erschöpfend geprüft; die native
macOS/AppKit/Wry-Runtime-Validierung bleibt offen.
