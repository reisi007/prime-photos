# Agentenregeln

## Zweck und Ablage

- **Diese Datei enthält ausschließlich dauerhafte, handlungsleitende Regeln** für Agenten: Invarianten, Grenzen, verbindliche Arbeitsabläufe und Gates. Keine Projektbeschreibung, allgemeine Doku-Navigation, Feature-Spezifikationen, Statusberichte oder offenen TODOs.
- `AGENTS.todo.md` enthält ausschließlich offene Arbeit. Verifizierte erledigte TODOs werden gelöscht, nicht abgehakt.
- Änderungen an plattformabhängigem Code müssen macOS (Apple Silicon), Windows und Linux berücksichtigen. Build und Tests werden in diesem Projekt nur auf macOS ausgeführt; keine Verifikation für andere Plattformen behaupten.

## Invarianten

- **Branding:** Projekt, Repo und App heißen **„Prime Photos"**; nichts darf wie eine offizielle Amazon-App aussehen (keine Amazon-Logos/-Identität im eigenen Fenster). Funktional Notwendiges (echte Hosts/URLs, `x-amzn-*`-Header, „Bei Amazon anmelden") bleibt erlaubt.
- **Marktplatzabhängige URLs** ausschließlich in `src/urls.rs` definieren; keine TLD-Literale anderswo. Jede neue Route erhält dort eine Util-Funktion und Tests für `de` sowie mindestens einen Nicht-DE-Marktplatz.
- **Assets:** Änderungen müssen die Pipeline `assets/*.svg` → `build.rs` (resvg) → `OUT_DIR` (Iconset-Raster via `packaging/make-app.sh`) → `include_bytes!` erhalten.
- **README versionsfrei halten** (User 2026-10-17): keine Release-Anzahl oder Versionsnummer in `README.md`; auf den Release-Seiten-Link verweisen.
- Subagenten dürfen nichts außerhalb des Auftrags einführen. Unklarheiten zurückmelden, statt sie eigenmächtig zu entscheiden.

## Arbeitsweise

- **Build-Verify-Loop, strikt:** Umsetzer-Subagent (frische Session, Ziel-Dateien + vollständige Spezifikation) → `cargo build` grün → **unabhängiger** Verifier-Subagent (andere Session: Diff gegen Auftrag, Invarianten, Blast Radius) → **Commit nach jeder** Verify-Runde, unabhängig vom Verdikt; bei „CHANGES REQUIRED": fixen → erneut verifizieren (bereits gepushte Commits: **neuer** Commit statt `--amend`).
- **Der Orchestrator editiert keine `src/`-Datei** — nur `AGENTS.md`, `AGENTS.todo.md`, `AGENTS.skills.md`; alles andere machen Subagenten. Lesen/Untersuchen ist frei.
- **Spec-first:** vor Implementierungen die Clean-Room-Specs in `docs/original/` bzw. das Original-Bundle (`/Applications/Amazon Photos.app`, nur lesend; Decompile via `ilspycmd`) heranziehen.
- **Feature-Verhalten und fachliche Entscheidungen** in `docs/features/` dokumentieren; vor Verhaltensänderungen die einschlägige Original-Spezifikation in `docs/original/` lesen.
- **Live-Werte bleiben beim Hauptagent** (Cookies, Konto-Parameter); abgebrochene Subagent-Session: auf dem vorhandenen Diff fortsetzen, nicht neu beginnen.

## Gates

- **Jede Warnung ist ein Fehler** (`[lints.rust] warnings = "deny"`); keine neuen `#[allow]`/`#[expect]`.
- `cargo fmt --check` ist **kein** Gate — die einmalige Formatierung des Altbestands steht als erster Punkt im Ledger; danach gilt er als Gate.
