# Autostart

The "Nach dem Anmelden starten" login item is **on by default** at first use,
because without it no watcher runs after a reboot and change detection would
sleep.

## How it works

- The login item is registered via the `auto-launch` crate
  (cross-platform: macOS / Windows / Linux). On macOS it uses
  `MacOSLaunchMode::SMAppService`: `SMAppService.mainApp.register()` enters the
  app into the **Anmeldeobjekte** (System Settings → General → Login Items).
  Requires macOS 13+; a freshly registered entry may first be `RequiresApproval`
  until the user approves it in that list. Below macOS 13 the crate returns
  `Err(UnsupportedOS)`, so the feature degrades into the existing "unsupported"
  path and nothing is persisted.
- `Config.autostart_enabled: Option<bool>`:
  - `None` = never decided ⇒ register once + store `Some(true)`.
  - `Some(false)` = veto, never enable; a hand-made entry is not fought.
  - `Some(true)` ⇒ a missing login item is repaired.
- The OS state remains the operative truth; `Config` stores only the intent.
- The original also enables autostart during onboarding
  (`../original/SPEC_OS_INTEGRATION.md` §3.6).

## Decisions

- **SMAppService statt LaunchAgent (User-Entscheidung 2026-10-17):** Auf macOS
  registriert `build_autostart()` per `MacOSLaunchMode::SMAppService`
  (`SMAppService.mainApp.register()`), nicht mehr als LaunchAgent (Plist unter
  `~/Library/LaunchAgents`). Grund: ein LaunchAgent erscheint in macOS **nicht**
  unter „Anmeldeobjekte" (allenfalls unter „Erlauben im Hintergrund") — der User
  sah den Eintrag deshalb nicht, obwohl Autostart gewünscht war. SMAppService ist
  der moderne Apple-Weg (macOS 13+) und macht den Eintrag sichtbar; ein frisch
  registrierter Eintrag kann zunächst `RequiresApproval` sein, bis der User ihn
  freigibt. **Abweichung vom Original:** Das Original schaltet Autostart per
  `osascript` / System Events (`../original/SPEC_OS_INTEGRATION.md` §3.6,
  Vergleichstabelle Z. 357); wir weichen davon bewusst ab, weil SMAppService der
  stabile und in den Anmeldeobjekten sichtbare Weg ist.
- **Veto-Falle: Absicht speichern, OS-Zustand nur anzeigen (User-Entscheidung
  2026-10-17):** Nach einem erfolgreichen `enable()`/`disable()` persistiert
  `set_autostart` die **Absicht des Users** (`autostart_intent_to_record(enabled)
  = Some(enabled)`), **nicht** den von der OS zurückgelesenen Zustand. Die
  Checkbox spiegelt weiterhin die OS-Wahrheit (`self.autostart_enabled =
  self.autostart.is_enabled().unwrap_or(false)`), damit sie nichts behauptet, was
  das OS nicht tut — aber daraus darf nie ein Veto werden. Grund:
  `is_smappservice_enabled()` ist `status() == Enabled`; ein frisch registrierter
  Eintrag ist aber erst `RequiresApproval` ⇒ der Read-back wäre `false`. Würde
  dieser Read-back gespeichert, entstünde still `Some(false)` = Veto, und
  `autostart_action` liefert danach für immer `Nothing` — die App registriert nie
  wieder etwas. `Some(false)` schreibt daher **nur** ein ausdrückliches
  Ausschalten des Users. Der `Err`-Zweig bleibt unverändert: nichts speichern,
  nur loggen.
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
