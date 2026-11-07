# Rune UI migration (Issue #49)

The goal is a **direct replacement** of eframe/egui with Rune. There must not
be two end-user UI modes. The normal app therefore stays on its existing eframe
path until the Rune app preserves the established functionality; the
settings-only Rune fixture is not an app cutover.

## Current slice

- Rune is pinned to `53c7f60f51ce12bd5da53145d9d86ce1552324e4`.
- Rune is an optional dependency behind `rune-ui-spike` and is not enabled by
  default. The source module and argument routing are debug-only; Rune runs
  only when headless `--snap`, `--tree`, or `--script` tooling is requested. It
  starts with fixture defaults and does not read or write the user's config,
  keychain, folders, or account data.
- No Rune interactive window is exposed. A normal debug or release launch
  remains the existing Prime Photos app, including the login, backup, tray,
  single-instance, and activation paths.
- The settings fixture covers only General and Bandwidth settings. It is
  useful for component experiments and is not production persistence or UI
  parity.
- A separate signed-out login fixture is available only for headless rendering
  and interaction tests through `--rune-login-fixture` plus `--snap`, `--tree`,
  or `--script`. It shows Prime Photos identity, marketplace selection, a
  sign-in CTA, and a deterministic fixture-only error. The CTA performs no
  authentication and the fixture does not construct PKCE, create Wry/WKWebView,
  access account/config/keychain data, or make network requests. This fixture
  does not implement authentication or alter normal launch: eframe remains the
  normal app UI.
- The signed-out login fixture is test evidence for Rune view structure and
  interactions only; it does not satisfy the production login/WebView cutover
  blocker below.

## Cutover blockers

Do not change the default launcher until Rune has all of the following in the
same app:

1. Signed-out login chrome, marketplace choice, cancel/error states, and the
   existing Wry device-login WebView as a visible, resizeable, focusable child
   view in the same Rune window. The WebView data wipe and copy/paste shortcuts
   must still work.
2. The signed-in backup page: empty and populated targets, target add/edit and
   conflict dialogs, queue actions, pause/resume, active progress, retry and
   failure states, and the existing background upload/sync code.
3. General and Bandwidth settings with immediate persistence, live appearance
   and language changes, plus tray/native-menu label updates.
4. About and third-party-licenses surfaces, plus the tray actions, close/reopen,
   activation, and single-instance behavior.
5. The existing debug fixtures represented by real Rune view paths, with
   interaction tests and screenshots covering at least signed-out/login chrome,
   backup empty/uploading/failure, and both settings tabs. Existing eframe demo
   screenshots are not Rune parity evidence.

The current macOS Wry child-view spike proves only that a child can be created;
visible rendering, resize/DPI, focus, keyboard, clipboard and lifecycle remain
unverified. Rune's macOS tray is not used here; retain the app's existing
`tray-icon` integration and prove it against Rune's app lifecycle before
cutover. Do not claim Windows or Linux runtime parity from macOS-only testing.

## Rune feature choices and tradeoffs

The optional dependency disables Rune's defaults and enables only the needs of
the intended desktop replacement / integration checks:

- `native-file-dialog` provides macOS/Windows dialogs and the Linux XDG
  FileChooser portal. It requires D-Bus on Linux; folder selection can fail on
  desktops without a portal. `dbus` also enables Rune's Linux desktop-theme
  integration. Prime Photos continues to own tray behavior through
  `tray-icon`, not Rune's tray.
- `metal-backend` selects Rune's native Metal path on macOS and has no effect
  on Windows/Linux. It adds Apple-specific dependencies and a renderer path
  that needs macOS validation; other platforms continue to use Rune's wgpu
  backend.
- `accessibility` enables the native AccessKit adapters for UI Automation,
  NSAccessibility, and AT-SPI. It is included for accessibility parity, but is
  not evidence of conformance: Rune's macOS accessibility re-entrancy issue
  (#150) and the absence of an independent cross-platform audit remain release
  risks.
- `embedded-fonts` is omitted (desktop builds bundle Inter independently of
  that web-oriented feature); `svg` is omitted because Prime Photos already
  rasterizes SVG assets with `build.rs`; audio features are not needed.

The feature stays opt-in until the full direct replacement is ready. Do not add
a `--rune-settings` flag or another interactive preview mode.
