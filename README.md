# Prime Photos

A cross-platform desktop app (Rust + egui/eframe) that backs up photos from local folders to your own Amazon Photos account.

## Disclaimer

Prime Photos is **not** an official Amazon product and is **not** affiliated with, endorsed by, or supported by Amazon. It is a third-party tool that works with **your own** Amazon Photos account, using your own credentials. It never presents itself as an Amazon app — no Amazon logos or wordmarks are used as its identity. Only the functionally required protocol details (real hosts/URLs, header and cookie names, and sign-in wording such as "Mit Amazon anmelden" where a sign-in genuinely happens) are used.

## Status

- **Early stage / MVP.**
- **macOS (Apple Silicon) is the primary target and the only platform that has been built *and* run-tested.** The macOS bundle is built locally, not in CI.
- **Windows and Linux are declared, equal targets.** The release workflow **builds** them (Windows x64 on `windows-latest`, Linux x64 on `ubuntu-latest`), and both legs have been green on every release so far — they are **compiled in CI but not run-tested**.
- Windows binaries are **unsigned** (the Authenticode steps are skipped when the certificate secrets are absent); macOS bundles are **ad-hoc signed** and **not notarized**.

See [`docs/dev/BUILD.md`](docs/dev/BUILD.md) §1 for the full test boundary.

## Download

Current builds are attached to the [GitHub Releases](https://github.com/reisi007/prime-photos/releases) page. macOS is built locally and uploaded by hand; Windows/Linux come from the release workflow.

| Asset | Platform | Install |
|---|---|---|
| `Prime-Photos-macos-arm64.dmg` | macOS (Apple Silicon) | Open the DMG, drag **Prime Photos** into `/Applications`. The bundle is ad-hoc signed and not notarized, so Gatekeeper blocks the first start — see [`docs/dev/RELEASE.md`](docs/dev/RELEASE.md) §5. |
| `Prime-Photos-windows-x64.zip` | Windows x64 | Unzip, then run `prime-photos.exe`. |
| `Prime-Photos-linux-x64.tar.gz` | Linux x64 | `tar -xzf Prime-Photos-linux-x64.tar.gz`, then run `./prime-photos`. |

Per-platform verification steps: [`docs/dev/RELEASE.md`](docs/dev/RELEASE.md) §7.

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
- **Background app**: closing the window hides it and keeps the app running, and the Dock icon follows the window — there while a window is open, gone while the app sits in the tray. Started at login it runs silently (no window, tray only). Bring the window back from the tray, by relaunching the app, or by a second launch. Only one instance runs at a time — a second launch shows the running window instead of opening a new one.
- **UI language: German** (German/English i18n layer; the OS locale is the default).

## Build & run

Requires Rust (edition 2024 ⇒ rustc ≥ 1.85). No CLI flags.

```sh
cargo build              # debug binary: target/debug/prime-photos
cargo run                # build (if needed) and start
cargo build --release    # release binary: target/release/prime-photos
```

Per-platform prerequisites (macOS / Windows / Linux system packages) are in **[`docs/dev/BUILD.md`](docs/dev/BUILD.md)**.

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

Exact paths per platform: [`docs/dev/BUILD.md`](docs/dev/BUILD.md) §6.

## Documentation

- [`docs/features/FEATURES.md`](docs/features/FEATURES.md) — feature documentation (one file per feature).
- [`docs/dev/BUILD.md`](docs/dev/BUILD.md) — build & run per platform, prerequisites, config paths.
- [`docs/dev/RELEASE.md`](docs/dev/RELEASE.md) — release process, artifacts, signing, verification.
- [`docs/protocol/PROTOCOL.md`](docs/protocol/PROTOCOL.md) — the wire protocol notes.
- [`docs/dev/I18N.md`](docs/dev/I18N.md) — the internationalization layer.
- [`docs/dev/DEV_NOTES.md`](docs/dev/DEV_NOTES.md) — codebase pitfalls (developer knowledge).
- [`docs/dev/THIRD_PARTY.md`](docs/dev/THIRD_PARTY.md) — third-party component attribution.
- [`AGENTS.md`](AGENTS.md) — durable agent rules (agent/developer-oriented, not an end-user guide).

## License

Licensed under **MIT OR Apache-2.0**, at your option.

- [`LICENSE-MIT`](LICENSE-MIT)
- [`LICENSE-APACHE`](LICENSE-APACHE)
