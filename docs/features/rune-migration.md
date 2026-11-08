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
- Four explicit interactive developer routes are available only in debug
  builds with `rune-ui-spike`: `--rune-login`, `--rune-wry-smoke`,
  `--rune-backup`, and `--rune-backup-smoke`. The production login route reads
  the existing config only after the explicit route is selected, and it does
  not access the keychain or make auth requests until the user clicks its
  production sign-in action. It uses the existing `WebviewLogin` navigation
  handler, PKCE portal URL, authorization-code redemption, token/keyring write,
  and in-memory session install. Closing or cancelling releases its Wry child.
- `--rune-wry-smoke` is a separate local-only Wry route. Its page is embedded
  HTML with synthetic text, no external assets or HTTP(S) URLs, and it does not
  load app config/keychain state or perform authentication/network requests.
  The production login and backup routes share the child-WebView owner; the Wry
  smoke route verifies that host's Rune logical bounds and teardown path. Rune's
  standard native Edit menu supplies Copy/Paste where the platform supports
  that menu path.
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
- `--rune-backup` is an interactive developer route for the persisted backup
  page. It loads the existing config and backup-target store only after this
  route is explicitly selected. After the single-instance guard succeeds, it
  restores only the refresh token through the existing keyring abstraction; a
  read error is logged and leaves the route signed out. The shared session
  worker refreshes restored tokens immediately, keeps the existing scheduled
  refresh and cookie-retry policies, and wakes Rune to apply renewed cookies to
  memory only. A token being restored is distinct from cookies being upload
  ready: the route shows a restoring status and does not enable uploads or start
  background sync until cookies arrive. Invalidated sessions clear the token,
  cookies, and active-upload state and return to the sign-in surface. After
  successful sign-in or cookie renewal, the existing background supervisor
  starts when enabled and configured; its initial pass scans enabled targets
  and may upload before the filesystem watcher takes over. Polling observes
  state and updates the view. The route reuses the
  shared production planner, upload runner, queue/progress, pause, retry, target
  options and conflict-answer handling. Target creation uses the native folder
  picker; an existing target's source path remains read-only while its dedupe
  and conflict options can be edited. Its General settings include an explicit
  autostart action: the route seeds the switch from OS readback and changes the
  login item only after a user click, recording intent after a successful OS
  operation. Normal-start intent reconciliation remains unported. One-time
  drag-and-drop uploads remain unported. The explicit route's lifecycle parity
  and remaining blockers are detailed below; this is not a cutover.
- `--rune-backup-smoke` is an interactive, synthetic-only visual check of the
  backup layout, including target progress, queue rows and local-only controls.
  It does not load app/account state, scan folders, authenticate, or upload.
  Headless `--snap` captures support `--scene` values
  `empty|populated|alert|about|licenses`, `--theme light|dark`, and
  `--size WxH`. Initial and narrower viewport captures were inspected for
  every original backup-scene/theme pairing under
  `target/rune-backup-smoke/`; the interactive layout test also checks the
  initial, resized, and minimum viewports. It is separate from the deterministic
  headless `--rune-backup-fixture`, which remains unchanged.

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
3. General and Bandwidth settings integrated with the production `Config`, with
   immediate persistence, live appearance and language changes, plus
   tray/native-menu label updates. The separate settings-only Rune fixture is
   initialized from `Config::default()` with no persistence
   path; downloads are not implemented. The interactive backup route now ports
   the explicit autostart setting action, but normal-start autostart intent
   reconciliation remains eframe-owned. Album selection is persisted in the
   shared `Config` and synchronized by both hosts.
4. The About and third-party-license UI now exists on the Rune backup route.
   The explicit `--rune-backup` route now shares the single-instance guard and
   show-request behavior, and reuses the native tray for show, pause/resume,
   About, and quit when a signed-in session is pending. Its tray glyph derives
   idle/active/paused state. With the current `Theme::Unknown`, macOS selects its
   monochrome per-state icons while non-macOS uses the logo fallback. The
   explicit route now shares close-to-tray, tray-show restoration, and macOS
   reopen/activation behavior. This is source-level parity, not runtime
   verification on macOS, Windows, or Linux. Tray done/error and dynamic
   appearance parity, normal-start autostart reconciliation, full UI/platform
   evidence, the default launcher, and eframe removal remain blockers.
5. The existing debug fixtures represented by real Rune view paths, with
   interaction tests and screenshots covering at least signed-out/login chrome,
   backup empty/uploading/failure, and both settings tabs. Existing eframe demo
   screenshots are not Rune parity evidence.

These are deliberate blockers for this backup slice, not implied ports:

- `main.rs` still starts eframe by default. `app.rs` remains responsible for
  the backup-configuration, About, and licenses dialogs. Rune's explicit
  backup route has the close-to-tray and activation lifecycle slice described
  above, but it is not runtime-verified across platforms.
- Rune's built-in tray backend does not provide macOS support in the pinned
  checkout (`53c7f60`): it supports Linux with D-Bus and Windows, while macOS
  returns `TrayError::Unsupported`. The explicit backup route reuses Prime
  Photos' existing `tray-icon` integration instead; this does not imply full
  tray lifecycle or glyph/appearance parity.
- Rune session restoration, access-token refresh, cookie renewal, and
  invalidation cleanup are implemented by the explicit backup route through the
  shared session worker. Source-level close-to-tray/reopen and macOS activation
  parity is also integrated; tray done/error and dynamic appearance parity,
  normal-start autostart reconciliation, full UI/platform evidence, the default
  launcher, and eframe removal remain blockers.
- Rune's native folder-dialog support already covers macOS/Windows and the
  Linux XDG portal. This slice uses that facility; it is not a reason to
  reimplement dialogs. The existing app-owned backup/About/licenses dialogs
  remain separate cutover blockers.
- The interactive backup route does not port one-time drag-and-drop uploads;
  after sign-in it reuses the existing background supervisor when enabled and
  configured, as noted above. The smoke route is synthetic-only and never reads
  real configuration or starts authentication, scans, or uploads.

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
a `--rune-settings` flag or another production-like interactive mode; these
interactive routes remain developer-only verification tools.
