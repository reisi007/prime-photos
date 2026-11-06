# Settings

The settings page provides general settings (language, appearance, autostart,
notifications), upload settings (concurrency, slow mode, duplicate default,
name-conflict default), and the account block. Sign-in is automatic and is
**not** a settings entry; signing out lives in the sidebar account block.

## How it works

- General settings: language, appearance, autostart, notifications.
- Upload settings: concurrency, slow mode, duplicate default, name-conflict
  default.
- The account block: profile picture, "Angemeldet als:", "Abmelden".
- **No save button:** every change is persisted **immediately** (pattern
  `if changed { save_config() }`). The "Erweitert (Konto)" section (config
  path, marketplace, zone, user-agent) is **removed**.
- **Concurrency/Rate in `Config`:** `concurrency`/`limit_enabled`/`limit_kbps`
  are config fields (serde defaults, old `config.toml` loads); clamping
  **1..=8** stays.
- **Appearance in `Config`:** `appearance` stores `system`, `light` or `dark`.
  Missing keys in older `config.toml` files default to `system`. System follows
  the native appearance reported to egui by eframe/winit; Light and Dark
  explicitly override it. A selection is saved immediately and takes effect
  without restarting the app.
- **Concurrency is a mouse-operable native egui slider:** it keeps egui's
  built-in `Slider` behavior, displays the exact integer value beside the
  track, and stays limited to **1..=8**.
- `slow_mode` remains deliberately **transient** (never persisted), but it
  takes effect **immediately**, including on a running upload.
- Marketplace is chosen only at sign-in, not in settings.
- Appearance is new Prime Photos behavior; `docs/original/SPEC_PREFERENCES.md`
  does not specify an appearance preference.

## Decisions

- **Einstellungen ohne Speichern-Knopf (F5, Welle 5.6).** Die Sektion
  „Erweitert (Konto)" (Konfig-Pfad, Marktplatz, Zone, User-Agent) ist
  **entfernt**; jede Änderung wird **sofort** persistiert (Muster
  `if changed { save_config() }`). Folge: den Marktplatz wählt man nur noch
  beim Anmelden. `slow_mode` bleibt bewusst **transient** (wird nie
  gespeichert), wirkt aber **sofort** — auch auf einen laufenden Lauf.

- **Concurrency/Rate in `Config` (F6, Welle 5.6).** `concurrency`/`limit_enabled`/`limit_kbps`
  als Config-Felder (serde-Defaults, alte `config.toml` lädt weiter);
  Klammerung **1..=8** bleibt.

- **Erscheinungsbild (Issue #46).** Neue, sofort gespeicherte Prime-Photos-
  Einstellung `appearance = "system" | "light" | "dark"`. Alte Konfigurationen
  ohne Schlüssel bleiben kompatibel und folgen dem System. `System` nutzt die
  native Theme-Meldung von eframe/egui; explizites Hell/Dunkel gilt sofort und
  überschreibt sie. Die Original-Spezifikation der Einstellungen enthält keine
  Darstellungsvorgabe — dies ist eine neue Prime-Photos-Funktion.

- **Kein „Wöchentlich aktiviert um …" (Welle 5.15).** Der Block im Dialog
  „Datensicherungseinstellungen" ist **entfernt** (samt der i18n-Keys
  „Wöchentlich aktiviert" und „um") — wir unterstützen kein wöchentliches
  Hochladen (User-Wortlaut: „wöchentlich um XX hochladen unterstützen wir
  nicht, entfernen").

## Original reference

- `../original/SPEC_PREFERENCES.md` — Settings/preferences of the original.
