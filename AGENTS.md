# Agentenregeln

## Zweck und Ablage

- **Nur Regeln, keine Entscheidungen:** Diese Datei enthält ausschließlich Regeln — Anweisungen, die für jede Arbeit in diesem Repo gelten; keine Projektbeschreibung, Doku-Navigation oder Feature-Spezifikation. Dauerhafte Entscheidungen, Begründungen, Messwerte und Historie gehören nicht hierher, sondern in `docs/features/` bzw. die Projekt-Doku; offene Punkte nach `AGENTS.todo.md`; Erledigtes in die Git-Historie.
- `AGENTS.todo.md` enthält ausschließlich offene Arbeit. Verifizierte erledigte TODOs werden gelöscht, nicht abgehakt.
- Änderungen an plattformabhängigem Code müssen macOS (Apple Silicon), Windows und Linux berücksichtigen. Build und Tests werden in diesem Projekt nur auf macOS ausgeführt; keine Verifikation für andere Plattformen behaupten.

## Invarianten

- **Branding:** Projekt, Repo und App heißen **„Prime Photos"**; nichts darf wie eine offizielle Amazon-App aussehen (keine Amazon-Logos/-Identität im eigenen Fenster). Funktional Notwendiges (echte Hosts/URLs, `x-amzn-*`-Header, „Bei Amazon anmelden") bleibt erlaubt.
- **Marktplatzabhängige URLs** ausschließlich in `src/urls.rs` definieren; keine TLD-Literale anderswo. Jede neue Route erhält dort eine Util-Funktion und Tests für `de` sowie mindestens einen Nicht-DE-Marktplatz.
- **Assets:** Änderungen müssen die Pipeline `assets/*.svg` → `build.rs` (resvg) → `OUT_DIR` (Iconset-Raster via `packaging/make-app.sh`) → `include_bytes!` erhalten.
- **README versionsfrei halten:** keine Release-Anzahl oder Versionsnummer in `README.md`; Releases dort nur über den Link auf die Release-Seite sichtbar machen.
- Subagenten dürfen nichts außerhalb des Auftrags einführen. Unklarheiten zurückmelden, statt sie eigenmächtig zu entscheiden.

## Arbeitsweise

- **Build-Verify-Loop, strikt:** Umsetzer-Subagent (frische Session, Ziel-Dateien + vollständige Spezifikation) → `cargo build` grün → **unabhängiger** Verifier-Subagent (andere Session: Diff gegen Auftrag, Invarianten, Blast Radius) → **Commit nach jeder** Verify-Runde, unabhängig vom Verdikt; bei „CHANGES REQUIRED": fixen → erneut verifizieren (bereits gepushte Commits: **neuer** Commit statt `--amend`).
- **Der Orchestrator editiert keine `src/`-Datei** — nur `AGENTS.md`, `AGENTS.todo.md`, `AGENTS.skills.md`; alles andere machen Subagenten. Lesen/Untersuchen ist frei.
- **Spec-first:** vor Implementierungen die Clean-Room-Specs in `docs/original/` bzw. das Original-Bundle (`/Applications/Amazon Photos.app`, nur lesend; Decompile via `ilspycmd`) heranziehen.
- **Feature-Verhalten und fachliche Entscheidungen** in `docs/features/` dokumentieren; vor Verhaltensänderungen die einschlägige Original-Spezifikation in `docs/original/` lesen.
- **Live-Werte bleiben beim Hauptagent** (Cookies, Konto-Parameter); abgebrochene Subagent-Session: auf dem vorhandenen Diff fortsetzen, nicht neu beginnen.

## Gates

- **Jede Warnung ist ein Fehler** (`[lints.rust] warnings = "deny"`); keine neuen `#[allow]`/`#[expect]`.
- **Ratchet-Gates sind Pflicht:** `ratchet check` muss lokal und in CI grün sein. `ratchet.config.jsonc`, `ratchet.baseline.json` und `scripts/ratchet-count.sh` gehören zusammen und werden nur gemeinsam geändert; die Baseline bewegt ausschließlich `ratchet baseline update` (nur in Richtung) bzw. `ratchet baseline raise` mit Begründung — nie per Hand. Ein neuer Zähler braucht eine Entscheidung, keinen beiläufigen Zusatz.
- `cargo fmt --check` ist **kein** Gate, solange das Repo nicht fmt-clean ist; nach der einmaligen Formatierung des Altbestands gilt er als Gate (Begründung: `docs/dev/DEV_NOTES.md` §8).

## Gates

Regeln dieser Ebene. Was nur ein Verzeichnis betrifft, gehört in die
`AGENTS.md` **dieses** Verzeichnisses — sie wird geladen, wenn eine Datei
daraus gelesen wird.

Ein Plan in `docs/plans/` beginnt mit `**Status:** Plan | In Arbeit | Done |
Empfehlung, nicht implementiert`.

Die kanonischen Agent-Regeln laufen über `ratchet` (Preset `agent-rules`):
`ratchet check` vor jedem Commit und in der CI.
