# Handover — Rune-Migration (Issue #49)

Stand: 2026-11-08. Dieser Stand ist auf `main` gepusht.

## Ziel

Prime Photos vollständig von eframe/egui auf Rune migrieren, ohne Funktionsverlust. Danach: UI-Verifikation mit synthetischen Daten und README-Aktualisierung. Der Default-Start bleibt bis zur belegten Parität bei eframe.

## Schnellstart per HTTPS

```sh
git clone https://github.com/reisi007/prime-photos.git
cd prime-photos
git checkout main
```

## Prompt für die nächste Session

Diesen Block als Auftrag an den nächsten Agenten übergeben:

```text
Du übernimmst die Rune-Migration von Prime Photos (GitHub Issue #49).

Repository per HTTPS klonen:
  git clone https://github.com/reisi007/prime-photos.git
  cd prime-photos
  git checkout main

Lies zuerst:
  - AGENTS.md (verbindliche Regeln)
  - Handover.md (Stand, Blocker, Befehle)
  - docs/features/rune-migration.md (Paritätsstatus)
  - bei Verhaltensfragen die Clean-Room-Specs in docs/original/

Ziel: eframe/egui vollständig durch Rune ersetzen, Funktionalität beibehalten,
UI mit synthetischen Daten verifizieren, danach README aktualisieren. Der
Default-Start bleibt bis zur belegten Parität bei eframe.

Aktueller Stand: Die explizite Route --rune-backup hat Session-Restore,
Refresh/Invalidierung, Tray-Menüaktionen, Close-to-tray, Reopen, About/Lizenzen,
Einstellungen und die lokalisierte Backup-UI. Alle Tests, cargo check,
ratchet check und diff check sind grün; ein unabhängiger Review war APPROVED.
Die Migration ist NICHT abgeschlossen: eframe ist noch Default, README nennt
weiterhin egui/eframe.

Nächste Aufgabe in dieser Reihenfolge:
  1. Offene Blocker aus Handover.md abarbeiten, insbesondere Tray-Done/Error-
     und Theme-Parität, Autostart-Abgleich beim Normalstart, Drag-&-Drop und
     vollständige synthetische UI-Verifikation.
  2. Jede Verhaltensänderung mit synthetischen Tests absichern; keine echten
     Konten, Cookies, Keychain-, Autostart- oder Netzwerkzugriffe in Tests.
  3. Erst wenn Parität belegt ist: Default-Launcher auf Rune umstellen,
     eframe/egui entfernen, README aktualisieren.
  4. Danach alle Gates erneut ausführen.

Verifikation:
  cargo test --locked
  cargo test --locked --features rune-ui-spike
  cargo check --locked --features rune-ui-spike
  ratchet check
  git diff --check

Regeln:
  - Keine neuen #[allow]/#[expect]; jede Warnung ist ein Fehler.
  - Ratchet-Baseline nicht von Hand ändern.
  - macOS-Ergebnisse nicht als Windows-/Linux-Nachweis darstellen.
  - Keine Plattform-Laufzeit behaupten, die nicht tatsächlich geprüft wurde.
  - Commits mit Refs #49 und Verify:-Footer; Build-Verify-Skill befolgen.
```

## Was in diesem Lauf erledigt wurde

- Rune-Backup-Route `--rune-backup` mit Session-Lifecycle:
  - Restore des Refresh-Tokens nach dem Single-Instance-Guard,
  - gemeinsamer Refresh-Worker mit UI-neutralem Wake-Callback,
  - Cookie-Übernahme nur in den Speicher,
  - Invalidierungsbereinigung inklusive Token-Store, Upload-Abbruch und Rückkehr zur Anmeldung.
- Tray-Menüaktionen im expliziten Rune-Backup-Host: Fenster anzeigen, Pause/Fortsetzen, About, Beenden.
- Single-Instance-Tests in `src/single_instance/tests.rs` ausgelagert, damit der 500-Zeilen-Ratchet wieder eingehalten wird (Baseline unverändert).
- Zwei formatierte deutsche UI-Texte lokalisiert:
  - `src/rune_backup_live/view.rs`: `tr2("{} von {} Dateien abgeschlossen", …)`
  - `src/rune_backup_live/targets.rs`: `tr3("{} / {} Dateien · {}", …)`
  - Englische Fassungen in `src/i18n.rs`, Regressionstest deckt DE und EN ab.

## Belegte Prüfungen in diesem Lauf

- `i18n::tests::rune_backup_view_and_target_strings_are_translated`: 1 bestanden.
- `rune_backup_live::lifecycle_route`: 4 bestanden.
- `rune_backup_live`: 22 bestanden.
- Rune-Gesamtsuite: 403 + 2 Integrationstests bestanden.
- Default-Gesamtsuite: 342 + 2 Integrationstests bestanden.
- `cargo check --locked --features rune-ui-spike`: grün.
- `ratchet check`: 8/8 Gates, `oversized-rust-file 11 ≤ 11`.
- `git diff --check`: ohne Befund.
- Unabhängiger Lifecycle-Review: **APPROVED**, keine Findings. Geprüft wurden Close-to-tray, Quit-Abgrenzung, Show/Reopen, Dock-Icon und die synthetischen Tests.
- Keine Runtime-Verifikation für Tray, Fenster-Lifecycle, Windows oder Linux. macOS-Tests sind kein Nachweis für andere Plattformen.

Dieser Stand wird als ein Commit auf `main` gepusht. Die Migration bleibt bewusst hinter dem Default-Cutover zurück.

## Close-to-tray/Reopen (fertig)

`src/rune_backup_live/lifecycle_route.rs` (271 Zeilen) ergänzt für `--rune-backup`:

- `keep_running` plus `on_close`/`on_reopen`,
- Schließen mit aktivem Tray versteckt das Fenster und lässt Session/Upload laufen,
- Schließen ohne Tray beendet die App (kein unerreichbarer Zustand),
- Tray- und Single-Instance-Show stellen das Dock-Icon wieder her und öffnen das Fenster,
- macOS-Reopen öffnet nur, wenn kein Fenster sichtbar ist,
- vier synthetische Entscheidungs-Tests ohne echtes Fenster/Tray/AppHandle.

Grenze: Quellparität, keine Laufzeitverifikation auf macOS/Windows/Linux.

Ein Worktree-Snapshot vor dem Cutoff liegt unter:

```
/private/var/folders/vq/db6hdqsx32b1xg4z2mnzt8nm0000gn/T/opencode/prime-photos-pre-cutoff.tgz
```

## Offene Blocker bis zum Default-Cutover

1. Close-to-tray/Reopen und macOS-Aktivierung vollständig verifizieren (Quellparität reicht nicht).
2. Tray-Glyphen für Done/Error und dynamisches Erscheinungsbild; `Theme::Unknown` nutzt auf macOS Template-Glyphen, sonst Logo-Fallback.
3. Autostart-Abgleich beim Normalstart (nicht nur explizite Aktion in den Einstellungen).
4. Einmalige Drag-&-Drop-Sicherung in Rune.
5. Vollständige synthetische UI-Verifikation: Login-Chrome, Einstellungen, Backup-Szenen, About/Lizenzen in Light/Dark und mehreren Viewports.
6. Default-Launcher auf Rune umstellen, eframe/egui aus `Cargo.toml` und `src/main.rs` entfernen.
7. `README.md` von „Rust + egui/eframe“ auf Rune aktualisieren.
8. Ratchet, vollständige Default-/Rune-Suiten und Release-Build nach dem Cutover erneut ausführen.

## Nächste Schritte in Reihenfolge

1. `git pull --ff-only` und Stand prüfen: `git status --short`, `cargo check --locked --features rune-ui-spike`, `ratchet check`, `git diff --check`.
2. Die verbleibenden Blocker abarbeiten und UI synthetisch verifizieren.
3. Unabhängigen Review über den gesamten geänderten Bereich durchführen.
4. Erst dann Cutover, eframe-Entfernung und README-Update.

## Wichtige Dateien

- `src/rune_backup_live.rs` — Produktionsroute, Setup/Poll.
- `src/rune_backup_live/lifecycle_route.rs` — Close-to-tray/Reopen.
- `src/rune_backup_live/session_route.rs` — Session-Restore/Refresh/Invalidierung.
- `src/rune_backup_live/tray_route.rs` — Tray-Menüaktionen.
- `src/login.rs`, `src/auth.rs` — gemeinsame Session-/Token-Logik.
- `src/single_instance.rs`, `src/single_instance/tests.rs` — Instanz-Guard und Show-Request.
- `docs/features/rune-migration.md` — Paritätsstatus und Blocker.
- `src/main.rs`, `Cargo.toml`, `README.md` — noch eframe-basiert, bewusst unverändert.

## Arbeitshinweise

- Der gepushte Stand ist sauber; der lokale Snapshot ist nur ein Fallback und nicht Teil des Repos.
- Keine echten Konten, Cookies, Keychain- oder Netzwerkzugriffe in Tests.
- Jede Warnung ist ein Fehler; keine neuen `#[allow]`/`#[expect]`.
- Ratchet-Baseline nicht von Hand ändern.
- Commit-Footer `Verify:` nicht vergessen, sonst steigt der Ratchet-Zähler.
