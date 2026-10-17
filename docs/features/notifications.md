# OS notifications

Desktop notifications are sent via the cross-platform `notify-rust` crate.
Notifications inform the user about upload results and errors without
requiring the app window to be focused.

## How it works

- `notify-rust` 4.18.2 is used for desktop notifications.
- On macOS, `notify-rust` uses `mac-notification-sys` (synchronous, pumps
  the run loop) — notifications are dispatched to the main dispatch queue
  and the result is picked up in the next frame (`src/notify.rs`).
- On Windows, `tauri-winrt-notification` is used.
- On Linux, D-Bus via `zbus` (pure Rust) with a notification daemon.
- **macOS: `set_application` must run before the first notification.** The
  `notify-rust` macOS backend has to know which application a notification
  belongs to; otherwise it asks the OS for an application named „use_default",
  and the OS opens the system panel „Choose Application — Where is use_default?"
  at the end of an upload run. `init()` (`src/notify.rs`) calls
  `set_application` once at startup with our bundle id
  (`de.reisinger.prime-photos`); an `Err` is harmless — the backend's `Once` is
  completed either way and the lookup never runs again.
- The original client has a documented notification system
  (`../original/SPEC_NOTIFICATIONS.md`) which serves as the parity
  reference.

## Decisions

- **No explicit user decisions in `AGENTS.md` beyond the crate choice**
  (cross-platform via `notify-rust`, see [platform.md](platform.md)).
- The notification behavior follows the original's notification system
  (`../original/SPEC_NOTIFICATIONS.md`).

- **Verworfen: der Fehlschlag „es bleibt `com.apple.Finder` stehen" als Ursache
  des `use_default`-Panels (2026-10-15).** Er trifft nicht zu — der Dialog
  entsteht **vor** dem Fallback.

- **Verworfen: die Vermutung, der User habe das LaunchServices-Panel für den
  Finder gehalten (2026-10-15).** Ausdrücklich vom User korrigiert — es geht
  **kein** Finder auf. Daraus **keine** Klick-Logik ableiten.

## Original reference

- `../original/SPEC_NOTIFICATIONS.md` — Notification system of the original.
- `../dev/BUILD.md` §2.1 — `notify-rust` platform backends.
