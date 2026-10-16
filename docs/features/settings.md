# Settings

The settings page provides general settings (language, autostart,
notifications), upload settings (concurrency, slow mode, duplicate default,
name-conflict default), and the account block. Sign-in is automatic and is
**not** a settings entry; signing out lives in the sidebar account block.

## How it works

- General settings: language, autostart, notifications.
- Upload settings: concurrency, slow mode, duplicate default, name-conflict
  default.
- The account block: profile picture, "Angemeldet als:", "Abmelden".
- **No save button:** every change is persisted **immediately** (pattern
  `if changed { save_config() }`). The "Erweitert (Konto)" section (config
  path, marketplace, zone, user-agent) is **removed**.
- **Concurrency/Rate in `Config`:** `concurrency`/`limit_enabled`/`limit_kbps`
  are config fields (serde defaults, old `config.toml` loads); clamping
  **1..=8** stays.
- `slow_mode` remains deliberately **transient**.
- Marketplace is chosen only at sign-in, not in settings.

## Decisions

- **Einstellungen ohne Speichern-Knopf (F5, Welle 5.6).** Die Sektion
  „Erweitert (Konto)" (Konfig-Pfad, Marktplatz, Zone, User-Agent) ist
  **entfernt**; jede Änderung wird **sofort** persistiert (Muster
  `if changed { save_config() }`). Folge: den Marktplatz wählt man nur noch
  beim Anmelden. `slow_mode` bleibt bewusst **transient**.

- **Concurrency/Rate in `Config` (F6, Welle 5.6).** `concurrency`/`limit_enabled`/`limit_kbps`
  als Config-Felder (serde-Defaults, alte `config.toml` lädt weiter);
  Klammerung **1..=8** bleibt.

- **Kein „Wöchentlich aktiviert um …" (Welle 5.15).** Der Block im Dialog
  „Datensicherungseinstellungen" ist **entfernt** (samt der i18n-Keys
  „Wöchentlich aktiviert" und „um") — wir unterstützen kein wöchentliches
  Hochladen (User-Wortlaut: „wöchentlich um XX hochladen unterstützen wir
  nicht, entfernen").

## Original reference

- `../original/SPEC_PREFERENCES.md` — Settings/preferences of the original.
