# Rune UI migration (Issue #49)

The goal is a **direct replacement** of eframe/egui with Rune. There must not
be two end-user UI modes. The normal app therefore stays on its existing eframe
path until the Rune app preserves the established functionality; the
settings-only Rune fixture is not an app cutover.

## Current slice

- Rune is pinned to `53c7f60f51ce12bd5da53145d9d86ce1552324e4`.
- Rune is an optional dependency behind `rune-ui-spike` and is not enabled by
  default. The source module and argument routing are debug-only. Headless
  fixtures start with fixture defaults and do not read or write the user's
  config, keychain, folders, or account data.
- The default launch is still the existing eframe Prime Photos app, including
  login, backup, tray, single-instance, and activation paths. No cutover or
  second end-user UI mode has been made.
- Two explicit interactive developer routes are available only in debug builds
  with `rune-ui-spike`: `--rune-login` and `--rune-wry-smoke`. The production
  login route reads the existing config only after the explicit route is
  selected, and it does not access the keychain or make auth requests until the
  user clicks its production sign-in action. It uses the existing
  `WebviewLogin` navigation handler, PKCE portal URL, authorization-code
  redemption, token/keyring write, and in-memory session install. Closing or
  cancelling releases its Wry child.
- `--rune-wry-smoke` is a separate local-only Wry route. Its page is embedded
  HTML with synthetic text, no external assets or HTTP(S) URLs, and it does not
  load app config/keychain state or perform authentication/network requests.
  Both Rune routes use the same child-WebView owner, Rune logical bounds and
  teardown path. Rune's standard native Edit menu supplies Copy/Paste where the
  platform supports that menu path.
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
- A separate backup-page fixture is available only for headless rendering and
  interaction tests through `--rune-backup-fixture` plus `--snap`, `--tree`, or
  `--script`. Its deterministic `empty`, `uploading`, and `failure` scenes use
  synthetic target/file labels and file counts. Its buttons are local
  demonstrations only: adding a target does not open a folder dialog, pausing
  changes only fixture state, and retry does not enqueue an upload or access the
  network. The fixture does not implement actual backups or change launch
  behavior; normal debug/release start remains the existing eframe app.

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

The integrated local-only `--rune-wry-smoke` route was manually verified on
macOS: the Rune window rendered its embedded page at 960×680 and after resizing
to 1050×760; focus and keyboard input were visible, and the native Edit menu's
Copy/Paste worked with the page's synthetic sample. The screenshot evidence is
local under `target/rune-wry-integration-*.png`. No production sign-in action
was clicked and no authentication request or real account data was used. This
verifies the shared Rune/Wry child host, not the production login portal or
full auth lifecycle. The earlier standalone probe's bounds change from
720×472 to 900×632 likewise does not establish production-login or
cross-platform parity. Rune's macOS tray is not used here; retain the app's
existing `tray-icon` integration and prove it against Rune's app lifecycle
before cutover. Do not claim Windows or Linux runtime parity from macOS-only
testing.

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
a `--rune-settings` flag or another production-like interactive mode; the two
routes above remain developer-only verification tools.
