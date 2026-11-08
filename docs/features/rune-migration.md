# Rune UI migration (Issue #49)

The goal is a **direct replacement** of eframe/egui with Rune. There must not
be two end-user UI modes. The normal app therefore stays on its existing eframe
path until the Rune app preserves the established functionality; the
settings-only Rune fixture is not an app cutover.

## Current slice

- Rune is pinned to `53c7f60f51ce12bd5da53145d9d86ce1552324e4`.
- Rune is an optional dependency behind the opt-in `rune-backup` feature;
  `rune-ui-spike` enables that feature plus debug-only fixtures and verification
  routes. The production backup modules are available in release builds with
  `rune-backup`, but routing still requires the explicit `--rune-backup` flag.
  Bare launch remains eframe. Headless fixtures start with fixture defaults and
  do not read or write the user's config, keychain, folders, or account data.
- The default launch is still the existing eframe Prime Photos app, including
  login, backup, tray, single-instance, and activation paths. No cutover or
  second end-user UI mode has been made.
- The legacy host is isolated behind the default-enabled `legacy-eframe` Cargo
  feature. The default host remains unchanged; the explicit Rune production
  release route can be built without eframe/egui:
- Default eframe build and launch:
  ```sh
  cargo build
  cargo run
  ```
- Rune-only production release build and launch:
  ```sh
  cargo build --release --no-default-features --features rune-backup
  cargo run --release --no-default-features --features rune-backup -- --rune-backup
  ```
  The Rune route remains explicit; a bare launch in a build without
  `legacy-eframe` does not switch to Rune.
- Four explicit interactive routes are available in debug builds with
  `rune-ui-spike`: `--rune-login`, `--rune-wry-smoke`, `--rune-backup`, and
  `--rune-backup-smoke`. The production backup route is also available in a
  release build with `--features rune-backup`; it remains explicit and does not
  change the default launcher. The production login route reads
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
- A separate login fixture is available only for headless rendering and
  interaction tests through `--rune-login-fixture` plus `--snap`, `--tree`, or
  `--script`. Its `signed-out` scene shows Prime Photos identity, marketplace
  selection, and a sign-in CTA; `error` shows a deterministic fixture-only
  error. The CTA performs no authentication and the fixture does not construct
  PKCE, create Wry/WKWebView, access account/config/keychain data, or make
  network requests. This fixture does not implement authentication or alter
  normal launch: eframe remains the normal app UI.
- The login fixture's `signed-out` and `error` scenes are test evidence for Rune
  view structure and interactions only; they do not satisfy the production
  login/WebView cutover blocker below.
- A separate backup-page fixture is available only for headless rendering and
  interaction tests through `--rune-backup-fixture` plus `--snap`, `--tree`, or
  `--script`. Its deterministic `empty`, `uploading`, and `failure` scenes use
  synthetic target/file labels and file counts. Its buttons are local
  demonstrations only: adding a target does not open a folder dialog, pausing
  changes only fixture state, and retry does not enqueue an upload or access the
  network. The fixture does not implement actual backups or change launch
  behavior; normal debug/release start remains the existing eframe app.
- `--rune-backup` is an explicit interactive production route for the persisted
  backup page. It is available in debug builds with `rune-ui-spike` and in
  release builds with `--features rune-backup`. It loads the existing config and
  backup-target store only after this route is explicitly selected. After the
  single-instance guard succeeds, it
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
  picker; an existing target's source path remains read-only while its dedupe,
  conflict, and content-filter options can be edited. The Rune form defaults new
  targets to `Photos`, preserves an existing target's saved filter on edit, and
  exposes the accessible "Fotos sichern" toggle; video uploads remain unsupported
  and no video control is offered. The existing scan path remains photos-only.
  Its General settings include an explicit
  autostart action: the route seeds the switch from OS readback and changes the
  login item only after a user click, recording intent after a successful OS
  operation. On route initialization it also reconciles normal-start intent with
  the same enable/repair/veto policy as the eframe host; a failed config load
  still prevents saving the fallback. The production Backup page also accepts
  one-time photo drops: direct files are checked with the shared
  `folders::is_supported_image` predicate, dropped directories are scanned with
  `folders::scan_images`, and the resolved photo list is persisted
  before the shared upload run. A transient row reports progress without
  creating a permanent target. Uploads continue to use the account root without
  a destination picker. Failed or cancelled runs retain the queue; a cleanly
  completed run clears it. On restart, the queue resumes only when background
  sync is enabled, the session is usable, no upload is active, and the app is
  unpaused. This retains the app's existing path+MD5 dedup behavior, which
  deliberately differs from the original one-time upload semantics. Video
  uploads are not supported and remain future/backlog work. The
  explicit route's lifecycle parity
  and remaining blockers are detailed below; this is not a cutover.
- `--rune-backup-smoke` is an interactive, synthetic-only visual check of the
  backup layout, including target progress, queue rows and local-only controls.
  It does not load app/account state, scan folders, touch SQLite, authenticate,
  or upload.
  Headless `--snap` captures support `--scene` values
  `mixed|uploading|paused|all-done|failures|empty|many-folders|about|licenses`,
  `--theme light|dark`, and `--size WxH`; the old `populated` and `alert` scene
  IDs no longer exist. The Rune review matrix also uses the login fixture's
  `signed-out` and `error` scenes and the existing settings fixture's General
  and Bandwidth tabs. This is separate from the deterministic headless
  `--rune-backup-fixture`, whose fixture scenes are unchanged.
- Rune backup-smoke interaction/layout tests and the review-capture script are
  now present. The script writes PNGs for all nine backup-smoke scenes at
  1400×900 in both themes, plus `mixed`, `failures`, and `many-folders` at
  760×560 and 1000×700 in both themes; it also writes a 1400×900 view tree for
  each backup-smoke scene in the light theme. Login (`signed-out`, `error`) and
  settings (General, Bandwidth) captures use 760×560 and 1000×700 in both
  themes. Captures use German and scale 1; the default output is
  `target/rune-ui-review/`. The successful headless capture artifact from
  [workflow run #42512063274](https://github.com/prime-photos/prime-photos/actions/runs/42512063274)
  (commit `ff3665a503248e6e3d8253bf6c913140f65f67f4`) was visually inspected
  on 2026-11-08 from `/tmp/rune-ui-review-42512063274`. Sampled backup, login,
  settings, About, and licensing views across light/dark themes and sizes looked
  correctly rendered, with no clipping in the inspected views. The red
  `Fixture-Ladefehler: Einstellungen konnten nicht geladen werden.` in the
  failures scene is intentional fixture content, hard-coded in
  `src/rune_backup_live/smoke/scenes.rs:67`. This completes only Rune headless
  screenshot generation/inspection: it is fixture-only visual review, not
  production parity or native macOS/AppKit/Wry runtime validation. Production
  dialogs and integration/native-runtime validation remain open, as do the
  other cutover blockers below. Legacy eframe visual checks remain separate and
  open.

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
   the explicit autostart setting action and normal-start intent reconciliation.
   Album selection is persisted in the
   shared `Config` and synchronized by both hosts.
4. The About and third-party-license UI now exists on the Rune backup route.
   The explicit `--rune-backup` route now shares the single-instance guard and
   show-request behavior, and reuses the native tray for show, pause/resume,
   About, and quit when a signed-in session is pending. Its tray glyph derives
   state with the eframe priority: paused, active, sticky last failure, recent
   success, then idle. A clean completion shows Done for five seconds and
   schedules one wake at expiry; a cancelled run preserves the previous
   outcome. The pinned Rune `Cx::system_dark()` value is observed in the view
   and safely forwarded to tray reconciliation, which changes the glyph only
   when state or appearance changes. macOS uses its black template glyphs
   regardless of appearance; Windows/Linux use white glyphs in dark mode, black
   in light mode, and the colored logo only while appearance is unknown. The
   explicit route also shares close-to-tray, tray-show restoration, and macOS
   reopen/activation behavior. This is source-level parity, not runtime
   verification on macOS, Windows, or Linux. Its macOS silent-start policy
   requires a Login Item launch, a restored refresh-token session, and a
   successfully built tray. Manual and signed-out starts, and starts on other
   platforms, remain visible. Because AppKit reports Login Item classification
   only after launch policy selection, the Rune production builder starts as an
   accessory unconditionally on macOS and creates its first native window with
   `visible: false`. Setup then validates the Login Item, restored session, and
   tray: a valid silent start calls `set_dock_icon(false)` and leaves the window
   hidden; every other start calls `set_dock_icon(true)` before queuing
   `app.open()`, promoting to Regular before the window appears. Tray show and
   macOS reopen likewise restore Regular before opening. The initial Login Item
   activation is consumed without opening the silent window. This startup
   behavior is source-level only and has not been runtime-verified on macOS
   here. Full UI/platform evidence, the default launcher, and eframe removal
   remain blockers.
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
  Photos' existing `tray-icon` integration instead. Glyph state and appearance
  now follow the source-level policy above; this does not imply full tray
  lifecycle parity or cross-platform runtime verification.
- Rune session restoration, access-token refresh, cookie renewal, and
  invalidation cleanup are implemented by the explicit backup route through the
  shared session worker. Source-level close-to-tray/reopen and macOS activation
  parity is also integrated. Full UI/platform evidence, the default launcher,
  and eframe removal remain blockers.
- Rune's native folder-dialog support already covers macOS/Windows and the
  Linux XDG portal. This slice uses that facility; it is not a reason to
  reimplement dialogs. The existing app-owned backup/About/licenses dialogs
  remain separate cutover blockers.
- The production `--rune-backup` route handles one-time photo drops and
  persisted-queue resume; this does not change the default launcher. The pinned
  Rune drop API documents macOS, Windows and X11 file drops; it does not provide
  file drags on Wayland. This is API support, not runtime verification of those
  platforms. Fixtures and the smoke route remain synthetic-only and never read
  real configuration, scan files, touch SQLite, authenticate, or upload.

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

The `rune-backup` feature stays opt-in until the full direct replacement is
ready; enabling it only makes the explicitly selected backup route available
and does not change bare launch. Do not add a `--rune-settings` flag or another
production-like interactive mode. The login/Wry routes, smoke route, fixtures,
and headless tooling remain debug-only verification tools.
