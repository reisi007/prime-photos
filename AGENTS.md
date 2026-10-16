# Prime Photos

Cross-platform Rust desktop app (egui) for uploading photos to Amazon Photos. Ziele sind macOS (Apple Silicon), Windows und Linux; gebaut/getestet wird derzeit nur macOS.

**Fachliche Entscheidungen und Feature-Verhalten stehen in `docs/` — nicht hier**; diese Datei ist nur das Betriebshandbuch für Agenten.

## Wo was steht

- `docs/features/` — **eine Datei pro Feature**: Verhalten + alle dauerhaften Entscheidungen (inkl. „Verworfen"). **Hier sucht man, wenn es ums Produkt geht.** Index: `docs/features/FEATURES.md`.
- `docs/original/` (Clean-Room-Recon) · `docs/protocol/` (Wire-Protokoll) · `docs/dev/` — `BUILD.md`, `RELEASE.md`, `SETUP.md` (Werkzeuge), `DEV_NOTES.md` (Fallstricke), `I18N.md`, `THIRD_PARTY.md` · `docs/README.md` (Index).
- `AGENTS.todo.md` — **nur** die offene Arbeit (Checkliste, keine Doku).

## Code

- **Marktplatzabhängige URLs nur in `src/urls.rs`** — kein TLD-Literal anderswo; neue Route ⇒ Util-Funktion + Test für `de` **und** einen Nicht-DE-Marktplatz.
- Assets `assets/*.svg` → `build.rs` (resvg) → `OUT_DIR` (+ iconset-Raster, `packaging/make-app.sh`) → `include_bytes!`

## Regeln

- **Branding:** Projekt, Repo und App heißen **„Prime Photos"**; nichts darf wie eine offizielle Amazon-App aussehen (keine Amazon-Logos/-Identität im eigenen Fenster) — funktional Notwendiges (echte Hosts/URLs, `x-amzn-*`-Header, „Bei Amazon anmelden") bleibt erlaubt.
- **Subagenten führen nichts Neues ein:** jeder Auftrag bleibt strikt auf die beauftragte Änderung begrenzt; Unklarheiten zurückmelden statt eigenmächtig lösen.
- **Ledger-Hygiene:** umsetzte **und** verifizierte Einträge werden aus `AGENTS.todo.md` **gelöscht** (Historie = Git); „Verworfen" wird **nie** gelöscht.

## Arbeitsweise

- **Build-Verify-Loop, strikt:** Umsetzer-Subagent (frische Session, Ziel-Dateien + vollständige Spezifikation) → `cargo build` grün → **unabhängiger** Verifier-Subagent (andere Session: Diff gegen Auftrag, Invarianten, Blast Radius) → **Commit nach jeder** Verify-Runde, unabhängig vom Verdikt; bei „CHANGES REQUIRED": fixen → erneut verifizieren → `--amend`.
- **Der Orchestrator editiert keine `src/`-Datei** — er schreibt nur `AGENTS.md`, `AGENTS.todo.md` und `AGENTS.skills.md`; alles andere machen Subagenten. Lesen/Untersuchen ist frei.
- **Spec-first:** vor Implementierungen die Clean-Room-Specs in `docs/original/` bzw. das Original-Bundle (`/Applications/Amazon Photos.app`, nur lesend; Decompile via `ilspycmd`) heranziehen.
- **Live-Werte bleiben beim Hauptagent** (Cookies, Konto-Parameter); abgebrochene Subagent-Session: auf dem vorhandenen Diff fortsetzen, nicht neu beginnen.

## Gates

- **Jede Warnung ist ein Fehler** (`[lints.rust] warnings = "deny"`); keine neuen `#[allow]`/`#[expect]`.
- `cargo fmt --check` ist **kein** Gate — die einmalige Formatierung des Altbestands steht als erster Punkt im Ledger (`AGENTS.todo.md`); danach gilt er als Gate.
