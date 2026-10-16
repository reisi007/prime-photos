# Autostart

The "Nach dem Anmelden starten" login item is **on by default** at first use,
because without it no watcher runs after a reboot and change detection would
sleep.

## How it works

- The login item is registered via the `auto-launch` crate
  (cross-platform: macOS / Windows / Linux).
- `Config.autostart_enabled: Option<bool>`:
  - `None` = never decided ⇒ register once + store `Some(true)`.
  - `Some(false)` = veto, never enable; a hand-made entry is not fought.
  - `Some(true)` ⇒ a missing login item is repaired.
- The OS state remains the operative truth; `Config` stores only the intent.
- The original also enables autostart during onboarding
  (`../original/SPEC_OS_INTEGRATION.md` §3.6).

## Decisions

- **Autostart (User-Entscheidung 2026-10-14) — Default an:** Der
  Login-Item-Schalter „Nach dem Anmelden starten" ist bei erster Nutzung
  **ein** (PORT-4 baute nur den Schalter, Default aus). Begründung: ohne
  Login-Item läuft nach Neustart/Ausloggen **kein Watcher** — die
  Event-Erkennung würde schlafen (Wasserscan ist nur Sicherheitsnetz). Das
  Original schaltet Autostart beim Onboarding ebenfalls selbst ein
  (`../original/SPEC_OS_INTEGRATION.md` §3.6).
  `Config.autostart_enabled: Option<bool>` (`None` = nie entschieden ⇒ einmal
  registrieren + `Some(true)` speichern; `Some(false)` = Veto, nie
  einschalten, ein von Hand angelegter Eintrag wird nicht bekämpft;
  `Some(true)` ⇒ fehlendes Login-Item wird repariert). Der OS-Zustand bleibt
  die operative Wahrheit, `Config` speichert nur die Absicht. Details:
  `AGENTS.todo.md` → „Zurückgestellt" #2.

## Original reference

- `../original/SPEC_OS_INTEGRATION.md` §3.6 — Autostart behavior of the original.
