# Prime Photos — Toolchain & local tooling (SETUP.md)

What this machine is expected to have on `PATH` to work on this repo — build, packaging, clean-room
recon, UI checks and the agent tooling around it. **Only tools that were really verified are listed**;
every entry names the command it was checked with. Things that are deliberately *not* used are in
§7 (not silently missing).

Scope note: this is the **developer machine** setup (macOS, Apple Silicon), not the CI runner. CI
installs its own dependencies (`.github/workflows/ci.yml`, `docs/dev/BUILD.md` §5).

## How this was verified

Commands run in a **non-interactive, non-login shell** (`/bin/bash -c …`, no `~/.zshrc`), i.e. the
same class of shell an agent/subagent gets:

```sh
command -v <tool>      # existence + resolved path
<tool> --version       # version where the tool supports it
```

Two shells were checked on purpose:

| Shell | `cargo` on `PATH`? | Meaning |
|---|---|---|
| The agent shell of this session | **yes** — `~/.cargo/bin` is already in `PATH` | nothing to do |
| `env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin /bin/bash -c …` | **no** — `command -v cargo` ⇒ not found | the raw, profile-less shell |

**Consequence:** never assume `cargo`/`rustc` are reachable. In any shell that does not source a
profile, prefix the call:

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo build
```

## 1. Rust / Cargo

| Tool | Resolved path | Version | Purpose |
|---|---|---|---|
| `cargo` | `~/.cargo/bin/cargo` | `cargo 1.98.0 (797e8a9bc 2026-08-05)` | build, test, `cargo metadata` |
| `rustc` | `~/.cargo/bin/rustc` | `rustc 1.98.0 (88d9e12ae 2026-08-18)` | edition 2024 ⇒ **≥ 1.85** required (`Cargo.toml`) |
| `rustup` | `~/.cargo/bin/rustup` | present | toolchain management (no `rust-toolchain.toml` in the repo) |
| `rustfmt` | `~/.cargo/bin/rustfmt` | via `cargo fmt` | **not** a gate — the repo is not fmt-clean (`docs/dev/DEV_NOTES.md` §8) |

`cargo-clippy` is installed too, but clippy is **not** a gate (neither here nor in CI).

**The `PATH` trap:** `~/.cargo/bin` is in the interactive `PATH`, but a profile-less non-interactive
shell does not see it (see above). Always prefix `PATH="$HOME/.cargo/bin:$PATH"` when calling cargo
from a script, a hook or a subagent.

**C compiler:** required because `rusqlite` has the `bundled` feature and compiles SQLite from C
sources (`docs/dev/BUILD.md` §3). Present: `cc` → `/usr/bin/cc`, `clang` → `/usr/bin/clang`
(Apple clang version 21.0.0). The SDK comes from Xcode; `xcode-select -p` ⇒
`/Applications/Xcode.app/Contents/Developer` (full Xcode, not only the Command Line Tools).

## 2. Reverse-engineering the original bundle (`docs/original/`, spec-first)

| Tool | Resolved path | Version | Purpose |
|---|---|---|---|
| `ilspycmd` | `~/.local/bin/ilspycmd` (**wrapper**, not the real binary) | `ilspycmd: 11.1.0.9782`, `ICSharpCode.Decompiler: 11.1.0.9782` | decompile the original .NET assembly |
| `dotnet` | `/opt/homebrew/bin/dotnet` | `10.0.401` | runtime the decompiler needs |

The original bundle is read **read-only** and never modified:

```
/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll   (4 591 616 bytes)
```

### Why `~/.local/bin/ilspycmd` is a wrapper

The real binary is `~/.dotnet/tools/ilspycmd` — an **arm64 .NET apphost**. Without `DOTNET_ROOT` it
only looks at `DOTNET_ROOT_ARM64` and aborts with *"You must install .NET to run this application."*,
even though Homebrew's `dotnet` is installed. The wrapper in `~/.local/bin` (which is on `PATH`)
sets both variables and then `exec`s the real binary:

```sh
DOTNET_ROOT="${DOTNET_ROOT:-/opt/homebrew/opt/dotnet/libexec}"
export DOTNET_ROOT
export DOTNET_ROOT_ARM64="${DOTNET_ROOT_ARM64:-$DOTNET_ROOT}"
exec "$HOME/.dotnet/tools/ilspycmd" "$@"
```

Honest note: **`DOTNET_ROOT_ARM64` is unset in the environment** (`echo "${DOTNET_ROOT_ARM64:-<unset>}"`
⇒ `<unset>`). The wrapper is what makes it work — calling `~/.dotnet/tools/ilspycmd` directly from a
profile-less shell is the case that fails. `/opt/homebrew/opt/dotnet/libexec` is the stable symlink
(survives Homebrew version bumps).

Typical calls (as documented inside the wrapper):

```sh
ilspycmd --version
ilspycmd -l c "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"   # list classes
ilspycmd -t <Full.Type.Name> "<dll>"                                                     # one type
ilspycmd -o <dir> "<dll>"                                                                # everything
```

## 3. macOS system tools

All of these are preinstalled system binaries; none needs installation.

| Tool | Path | Used for | Referenced in the repo |
|---|---|---|---|
| `codesign` | `/usr/bin/codesign` | ad-hoc / identity signing + verification of the `.app` | `packaging/make-app.sh`, `docs/dev/RELEASE.md` |
| `iconutil` | `/usr/bin/iconutil` | `target/pp-appicon.iconset` ⇒ `AppIcon.icns` | `packaging/make-app.sh`, `build.rs` |
| `security` | `/usr/bin/security` | `security find-identity -v -p codesigning` (signing identity) | `packaging/README.md` |
| `xattr` | `/usr/bin/xattr` | drop `com.apple.quarantine` after installing a build | `docs/dev/RELEASE.md` |
| `screencapture` | `/usr/sbin/screencapture` | UI screenshots for visual checks | `AGENTS.todo.md` (UX review) |
| `open` | `/usr/bin/open` | launch the built bundle | `packaging/README.md`, `make-app.sh` |
| `osascript` | `/usr/bin/osascript` | AppleScript / automation | only original-recon specs |
| `swift` | `/usr/bin/swift` | Swift driver present (Xcode) | not used by this repo |
| `hdiutil` | `/usr/bin/hdiutil` | DMG creation | **no script in the repo** |

Notes on the non-obvious rows:

- `codesign` and `iconutil` have **no `--version`** flag (`codesign --version` ⇒
  `unrecognized option`); existence was checked with `command -v`.
- `swift` resolves to `/usr/bin/swift` — `swift-driver version: 1.168.6 Apple Swift version 6.4 …
  Target: arm64-apple-macosx27.0.0`. It comes with Xcode and is **not** used anywhere in this repo
  (checked by grep) — the build is pure Rust (`build.rs` + `resvg`).
- `hdiutil` exists but is referenced by **no** file in the repo (grep over the whole tree): there is
  no DMG script, only the `.app` bundle from `packaging/make-app.sh`. Any DMG work is manual.

## 4. Image tools

| Tool | Path | Version | Notes |
|---|---|---|---|
| `magick` | `/opt/homebrew/bin/magick` | `ImageMagick 7.1.2-32 Q16-HDRI aarch64` | the ImageMagick 7 entry point |
| `convert` | `/opt/homebrew/bin/convert` | same install | **deprecated** in IM7 — it warns on every call; prefer `magick` |
| `exiftool` | `/opt/homebrew/bin/exiftool` | `13.55` | read/write EXIF (capture dates, orientation) |
| `sips` | `/usr/bin/sips` | `sips-316` | macOS-native image conversion |

Honest notes:

- None of these four is called by a script in this repo (grep over the tree: `magick`/`exiftool`:
  **zero** hits; `sips`: only a comment in `build.rs`). They are available for **manual** work —
  preparing screenshots, inspecting EXIF of test photos, image checks.
- `sips` is **deliberately not used by the build**: `build.rs` explains why — `sips`/`qlmanage` are
  macOS-only, while the release matrix also builds Windows and Linux, so `resvg` (pure Rust) does the
  SVG→PNG rasterising.

## 5. General dev tools

| Tool | Path | Version | Purpose |
|---|---|---|---|
| `git` | `/usr/bin/git` | `2.54.0 (Apple Git-157)` | commits, tags, hooks (`.githooks/pre-commit` → CodeGraph sync) |
| `gh` | `/opt/homebrew/bin/gh` | `2.101.0 (2026-09-15)` | PRs, issues, release/CI observation |
| `python3` | `/opt/homebrew/bin/python3` | `3.14.7` | helper scripts (`config_setup.py`, `config_update.py`, `packaging/gen-third-party-licenses.sh`) |
| `node` | `~/.nvm/versions/node/v26.8.1/bin/node` | `v26.8.1` | host for `npx`-based tooling |
| `npx` | `~/.nvm/versions/node/v26.8.1/bin/npx` | `11.19.0` | launches the `rea` MCP server |
| `codegraph` | `~/.local/bin/codegraph` → `~/.codegraph/versions/v1.6.2/bin/codegraph` | `1.6.2` | the code-intelligence index of this repo (`.codegraph/`) |

- **`gh`** is *not* part of this repo's release flow — releases run through `git tag` +
  `.github/workflows/release.yml`. `gh` is for PRs/issues and for watching runs.
- **`rea` MCP server** (reverse-engineering tools) is started by the agent harness, not by hand:
  `npx -y --prefer-offline rea-agents@6.4.0 mcp` (`~/.config/opencode/opencode.jsonc`). So `node`/
  `npx` must be on `PATH` for REA to work.
- **`codegraph`**: `codegraph serve --mcp` is the MCP server; `.githooks/pre-commit` refreshes the
  index with `codegraph sync -q` and **fails open** (a missing `codegraph` only prints a warning).

## 6. Third-party licence generation — what is actually used

`packaging/gen-third-party-licenses.sh` produces `assets/third-party-licenses.txt` for the
About/licence window. Its own header names its dependencies: **`cargo` and `python3` only**. It runs
`cargo metadata --format-version 1 --locked` and reads the licence files from the local crate copies
with an inline Python script — offline and deterministic (no timestamp).

## 7. Deliberately not needed

| Tool | Status | Why it is not needed |
|---|---|---|
| `cargo-license` | **not installed** (`~/.cargo/bin` contains only cargo/rustc/rustup/clippy/rustfmt/trunk/wasm-* tooling) | licence attribution is generated by `gen-third-party-licenses.sh` via `cargo metadata` + `python3` |
| `cargo-about` | **not installed** | same — no `about.toml`/`deny.toml` in the repo |

Do not add either as a build prerequisite: the existing script covers the licence obligation and is
the one referenced by the About window.
