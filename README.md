# Prime Photos

A cross-platform desktop app (Rust + egui/eframe) that backs up photos from local folders to your own Amazon Photos account.

## Disclaimer

Prime Photos is **not** an official Amazon product and is **not** affiliated with, endorsed by, or supported by Amazon. It is a third-party tool that works with **your own** Amazon Photos account, using your own credentials. It never presents itself as an Amazon app — no Amazon logos or wordmarks are used as its identity. Only the functionally required protocol details (real hosts/URLs, header and cookie names, and sign-in wording such as "Mit Amazon anmelden" where a sign-in genuinely happens) are used.

## Status

- **Pre-release / MVP.**
- **macOS (Apple Silicon) is the primary target and the only platform that has been built and run-tested.**
- **Windows and Linux are declared, equal targets**, but are **not run-tested**. The intended proof is a cross-compile check (`cargo check --target …`), which has **not been performed yet**.
- No release has actually been triggered; the CI workflows are only syntactically checked.

See [`docs/BUILD.md`](docs/BUILD.md) §1 for the full test boundary.

## Features

- **One-time sign-in** through an embedded login web view (device/PKCE flow); only the **refresh token** is stored, in the OS keychain (macOS Keychain / Windows Credential Manager / Linux Secret Service). Access token and session cookies stay in memory.
- **Persistent backup targets**: pair a local folder with an upload policy (content filter, interval, dedup, name-conflict policy).
- **Content-based deduplication**: a persistent local MD5 index in SQLite — the same bytes are never uploaded twice, even under a different path/name.
- **Name-conflict handling**: ask, ignore, save as a new file (server renames), or replace — with safe replace that only overwrites the node a local path is bound to.
- **Event-driven change detection**: one OS file-system watch per backup folder (FSEvents / ReadDirectoryChangesW / inotify), with a start-up watermark as a safety net.
- **Per-file progress** (byte-accurate) with ETA, plus a result breakdown into newly uploaded / already in the account / unchanged / errors.
- **Pause/resume** (globally, from the backup header and the tray).
- **One-off queue via drag & drop** (persisted so a restart resumes it).
- **Tray icon**, **desktop notifications**, and an **About window with third-party licenses**.
- **UI language: German** (German/English i18n layer; the OS locale is the default).

## Build & run

Requires Rust (edition 2024 ⇒ rustc ≥ 1.85). No CLI flags.

```sh
cargo build              # debug binary: target/debug/prime-photos
cargo run                # build (if needed) and start
cargo build --release    # release binary: target/release/prime-photos
```

Per-platform prerequisites (macOS / Windows / Linux system packages) are in **[`docs/BUILD.md`](docs/BUILD.md)**.

macOS `.app` bundle (ad-hoc signed by default):

```sh
./packaging/make-app.sh  # -> target/dist/Prime Photos.app
```

See [`packaging/README.md`](packaging/README.md) for details.

## Configuration paths

All files live under the `prime_photos` config folder (`dirs::config_dir()/prime_photos/`):

- `config.toml` — app settings
- `folders.toml` — backup targets
- `dedup.sqlite3` — persistent dedup index

Exact paths per platform: [`docs/BUILD.md`](docs/BUILD.md) §6.

## Documentation

- [`docs/BUILD.md`](docs/BUILD.md) — build & run per platform, prerequisites, config paths.
- [`docs/RELEASE.md`](docs/RELEASE.md) — release process, artifacts, signing, verification.
- [`docs/PROTOCOL.md`](docs/PROTOCOL.md) — the wire protocol notes.
- [`docs/I18N.md`](docs/I18N.md) — the internationalization layer.
- [`docs/DEV_NOTES.md`](docs/DEV_NOTES.md) — codebase pitfalls (developer knowledge).
- [`docs/THIRD_PARTY.md`](docs/THIRD_PARTY.md) — third-party component attribution.
- [`AGENTS.md`](AGENTS.md) — durable project decisions (agent/developer-oriented, not an end-user guide).

## License

Licensed under **MIT OR Apache-2.0**, at your option.

- [`LICENSE-MIT`](LICENSE-MIT)
- [`LICENSE-APACHE`](LICENSE-APACHE)
