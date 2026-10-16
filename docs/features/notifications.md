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
- The original client has a documented notification system
  (`../original/SPEC_NOTIFICATIONS.md`) which serves as the parity
  reference.

## Decisions

- **No explicit user decisions in `AGENTS.md` beyond the crate choice**
  (cross-platform via `notify-rust`, see [platform.md](platform.md)).
- The notification behavior follows the original's notification system
  (`../original/SPEC_NOTIFICATIONS.md`).

## Original reference

- `../original/SPEC_NOTIFICATIONS.md` — Notification system of the original.
- `../dev/BUILD.md` §2.1 — `notify-rust` platform backends.
