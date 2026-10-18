# OS Integration — Clean-Room Spec (reverse-engineered)

Behavioural specification of the **original** Amazon Photos desktop client's operating-system
integration on macOS: the Finder Services entry, the crash-relaunch LaunchAgent, the process /
Dock / window lifecycle of an agent app, the autostart mechanism, and the remaining
integration-relevant `Info.plist` keys — plus the mapping onto this port.

The original behaviour below is **reverse-engineered from the shipped managed assembly, the
shipped plists/scripts and the German string tables**, not inferred from string tables alone.
Nothing in this document identifies the original product with this port; this port is a
third-party tool ("Prime Photos").

This document is **clean-room**: it records *observed behaviour* in our own words and tables.
It contains **no verbatim decompiled code** and **no personal data**. German user-facing labels
are quoted from the shipped `de.lproj/*.strings` (product strings, not code); type and member
names are cited only to the extent needed to describe the mechanism.

**Confidence convention.**
- **belegt** = read directly out of the reverse-engineered `Amazon Photos.dll` (member/branch),
  the shipped `Info.plist` / script / strings files, the entitlements display, or our committed
  code.
- *inferred* = reasoned from adjacent evidence, not read off a line.
- **UNKNOWN** = could not be determined from the inspected sources.

---

## 1. Purpose & scope

This doc owns everything where the original app talks to the **operating system**, not to the
Amazon servers:

| Area | Covered here |
|---|---|
| Finder/context-menu service "Bei Amazon Photos hochladen" | §3.2 (`NSServices`, `ServicesMenu.strings`, handler, pipeline, Windows sibling) |
| Crash-relaunch LaunchAgent (`WorkerMaxRestarts*`) | §3.3 (plist, paths, conditions, restart mechanics, power events) |
| Menu-bar status item as a **process/app model** (`LSUIElement`, activation policy) | §3.4 |
| Window lifecycle (close without quit, reopen, quit vs. kill) | §3.5 |
| Autostart/login item | §3.6 |
| Other integration keys (URL scheme, app groups, Dock) | §3.7 |

Explicitly **not** covered here (cross-references only, §7): the status-popup and notification
surfaces that the status item drives (`SPEC_NOTIFICATIONS.md`), the upload pipeline the
service and the CLI share (`SPEC_ONE_TIME_UPLOAD.md` §6), background scheduling
(`SPEC_BACKGROUND_SYNC.md`).

A deliberate **overlap boundary** with `SPEC_NOTIFICATIONS.md` §2 exists: that doc owns
*what* the menu bar shows (icons, tooltip, badge, click phases) and the LaunchAgent plist
*template*; this doc owns *how the entry points are registered and what they trigger*, the
crash-restart policy, and the process/window lifecycle. Where both state a fact, the wording here
was re-derived from the sources listed in §2.

---

## 2. Reverse-engineering method (belegt)

- Tooling: `ilspycmd` **is installed** at `~/.dotnet/tools/ilspycmd`, run with
  `PATH` extended and `DOTNET_ROOT=/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec`. No tool was
  installed for this task.
- Target (read-only, **never launched**): `/Applications/Amazon Photos.app` →
  `Contents/MonoBundle/Amazon Photos.dll` (managed assembly, 4.6 MB), full type listing
  (`ilspycmd -l c`) plus a full decompilation dump grepped for call sites.
- Plists/scripts read with `plutil -p` / plain reads: `Contents/Info.plist`,
  `Contents/Resources/restart_app.plist`, `Contents/Resources/restart_app.scpt`,
  `Contents/Resources/quit_and_restart_app.scpt` (compiled AppleScript — readable strings only).
- Entitlements displayed read-only via `codesign -d --entitlements -`.
- Strings: `Contents/Resources/de.lproj/Localizable.strings` and
  `Contents/Resources/de.lproj/ServicesMenu.strings`.
- `Contents/Library/` does **not exist** in this bundle (verified by `ls`): no `LoginItems/`,
  no `LaunchServices/` subdirectory, no helper app.
- Classes used as evidence: `Amazon.CloudDrive.{AddBackupFromMacContextMenu, AppDelegate, App,
  AppStartup, AppRestart, AppShutdown, ContextMenuPaths, ContextMenuHelper}`,
  `Amazon.CloudDrive.Infrastructure.{PhotosRequest, UrlRouter, Url.Handlers.InstallerUrlHandler,
  CrashFrequencyHelper}`, `Amazon.CloudDrive.Workers` (`OnWorkerCrashed` path),
  `Amazon.CloudDrive.UI.{MenuBarController, MenuBarStatusItemViewController, WindowManager}`,
  `Amazon.CloudDrive.StartupTasks.{ProcessContextMenuPathsTask, UploadArgStartupTask,
  AddBackupFromWindowsContextMenuTask, AppIconTask.StartAppIcon, FinishOnboardingTask}`,
  `Amazon.CloudDrive.PreferencesActionIntentHandler`, `Constants`, `AppSettings`,
  `StartupContext`, `AppStartupFlags`.
- Our side: reading the committed `src/` files (`tray.rs`, `notify.rs`, `app_menu.rs`,
  `platform.rs`, `app.rs`, `settings.rs`, `main.rs`), `Cargo.toml`, `packaging/Info.plist`,
  `packaging/make-app.sh`.

---

## 3. Mechanics (original)

### 3.1 `Info.plist` — the integration surface (belegt)

`Contents/Info.plist` of version 11.3.0 (`CFBundleVersion` 36, identifier
`com.amazon.clouddrive.mac`, executable boots a Mono/CoreCLR runtime via
`MonoBundleExecutable`). Integration-relevant keys:

| Key | Value | Relevance |
|---|---|---|
| `LSUIElement` | `true` | agent app: no Dock icon, no app menu by default |
| `NSServices` | one entry, see §3.2 | Finder right-click → "Services" entry |
| `CFBundleURLTypes` | scheme `amazonphotos` (name = bundle id) | deep-link entry, §3.7 |
| `LSMultipleInstancesProhibited` | `true` | single instance only |
| `LSMinimumSystemVersion` | `10.15` | Catalina floor |
| `NSMainNibFile` | `MainMenu` | the menu/nib that hosts the OS menu bar |
| `NSUserNotificationAlertStyle` | `alert` | notifications surface — see `SPEC_NOTIFICATIONS.md` |
| `NSPhotoLibraryUsageDescription` | Click \"OK\" to allow back up of your Apple Photos Library. | Apple-Photos import consent text |
| `NSRequiresAquaSystemAppearance` | `true` | forces light appearance |
| `NSSupportsAutomaticGraphicsSwitching` | `true` | GPU switching allowed |
| `ATSApplicationFontsPath` | `Fonts` | bundled fonts |
| `LSApplicationCategoryType` | `public.app-category.productivity` | App Store category |

Absent from the plist (observed): any `SMLoginItem*` key, `SMPrivilegedExecutables`, `NSSupports*`
sandbox keys, app-group keys — app groups live in the **entitlements** instead (§3.7), not in
`Info.plist`.

### 3.2 The Finder service "Bei Amazon Photos hochladen" (belegt unless noted)

**Registration.** The bundle declares exactly **one** `NSServices` entry:

| Plist key | Value |
|---|---|
| `NSMenuItem.default` | "Upload to Amazon Photos" (English base) |
| `NSMessage` | `addBackupFromMacContextMenu` |
| `NSPortName` | `Finder` |
| `NSRequiredContext` | `{ NSTextContent = FilePath }` |
| `NSSendFileTypes` | `public.directory` |

The menu label is localised through the standard mechanism: the German
`de.lproj/ServicesMenu.strings` maps the base string to **"Bei Amazon Photos hochladen"** — the
label this port's brief references. The German general strings file additionally carries the
same text under `clouddrive_desktop_context_menu_upload`, which the managed code uses for the
**Windows** sibling entry (below).

At runtime the service is not bound through a nib: in `AppDelegate.DidFinishLaunching` the app
sets `NSApplication.SharedApplication.ServicesProvider` to the DI instance of the
`AddBackupFromMacContextMenu` class. macOS resolves the `NSServices` message through that
provider; the exported selector is `addBackupFromMacContextMenu:userData:error:`. → **belegt**
(the binding of `NSPortName = Finder` to the dispatch is **UNKNOWN** in detail — see §6).

**Accepted input.** Per the plist the service is offered for a **directory** selection only
(`NSSendFileTypes`, `NSRequiredContext = FilePath`). The handler itself checks only that the
pasteboard carries `public.file-url` and otherwise returns silently; it then reads the
`NSFilenamesType` property list (an array of path strings). Each non-empty path is pushed into
the shared `IContextMenuPaths` queue; empty strings are skipped. Every invocation records a
health metric (`ContextMenu.FoldersSelected` = number of selected paths) and logs
"Saving folder selected in Finder … for backup" per path. Any exception is swallowed and logged
via the app logger — a malformed service call never surfaces a dialog. → **belegt** (finder-side
filtering *inferred*: the handler has no re-check of "is directory").

**Pipeline (what the service actually triggers).** `ContextMenuPaths` is a singleton wrapping a
replay-subject of strings. The `ProcessContextMenuPathsTask` startup task subscribes to it and
forwards each path to `ContextMenuHelper.AddBackup(path)`, which:

1. asks `IAllowedScheduleFolderStrategy.GetScheduleFolderWarning(path)`; a non-empty warning
   (e.g. a disallowed folder) aborts this path with a log line;
2. otherwise builds a `ScheduledBackupItem` seeded from the **current customer defaults**
   (`ContentFilter`, `BackupItemInterval`, the photos/videos deduplication flag; external-drive
   flag via `IExternalDriveService`);
3. fires a `CreateDefaultBackupActionIntent`, whose handler persists the schedule record, adds a
   filesystem watcher for the path, records business metrics, and requests a scan with
   `ForceScan = true` unless the record is paused (uploads are deferred unless the interval is
   the "immediate" option).

Net effect: **the Services entry creates one scheduled backup per selected folder, with the
user's default settings — not a one-time upload.** This is the key behavioural fact for the port
and the intended divergence point (§5). The contrast case is the *argument* path
(`UploadArgStartupTask`): dragging a folder onto the app icon / `--upload=<path>` starts a
**one-time** queue instead (`SPEC_ONE_TIME_UPLOAD.md` §6) — belegt, different pipeline.

**Windows sibling (for our cross-platform mapping).** The managed code also registers a shell
context-menu entry via a registry key under `Software\Classes\directory\shell\amz_addBackup`
with the same German label and the command `<exe> --folderToBackup="%V"`; when the app is
launched that way it routes the path into the *same* `IContextMenuPaths` queue. → **belegt**.
So the "create default backup from a folder hand-off" behaviour is the cross-platform contract;
only the plumbing differs per OS.

### 3.3 Crash relaunch — LaunchAgent + watchdog (belegt unless noted)

The app **does not** ship a helper bundle (`Contents/Library/` does not exist). Instead it
re-arms a per-user LaunchAgent at every launch:

**Agent plist.** The template `Contents/Resources/restart_app.plist` (read via `plutil -p`):

| Key | Value |
|---|---|
| `Label` | `com.amazon.clouddrive` |
| `KeepAlive` | `true` |
| `ProcessType` | `Interactive` |
| `ProgramArguments` | `/usr/bin/osascript`, `${SCRIPTPATH}`, `${APPPATH}`, `${APPID}` |

**Registration / removal** (both from `App`, skipped when a debugger is attached):
- ensure `Constants.ScriptsPath` exists = **`~/Library/Application Scripts/Amazon Cloud Drive`**;
- `launchctl unload` the agent, then copy `restart_app.plist` + `restart_app.scpt` from the
  bundle's `ResourcePath` into that folder;
- substitute the placeholders `${SCRIPTPATH}` (the copied script), `${APPPATH}` (bundle path),
  `${APPID}` (bundle identifier) in the copied plist and `launchctl load` it back.
- Removal is `launchctl unload` of the same file; call sites are the shutdown kill path and the
  "crashed too frequently" path (§3.3 guard).

**What the script does.** The compiled `restart_app.scpt` carries variables for app path
(`/Applications/Amazon Photos.app`), bundle id (`com.amazon.clouddrive.mac`) and installer id
(`com.amazon.clouddrive.mac.installer`), an inline comment that this "should eventually be
replaced with an application helper", then waits until **both** app and installer processes are
no longer running and relaunches with `open -a … --args --source-restart`. → read from the
compiled script's readable strings; the control flow beyond the visible strings is *inferred*.

**Effect of `KeepAlive=true`:** launchd relaunches the app when the process dies — this is a
**recovery** agent, not a background uploader (the relaunched process is the full GUI app in
accessory mode). Same conclusion as `SPEC_NOTIFICATIONS.md` §2.2, independently derived here
from the plist + registration code.

**Worker-level watchdog (`WorkerMaxRestarts*`).** `AppSettings` (persisted in
`~/Library/Application Support/Amazon Cloud Drive/app-settings.json`) exposes
`WorkerMaxRestarts` (default **5**) and `WorkerMaxRestartsWindowInSeconds` (default **15**).
On a worker crash the state machine reports the crash with that look-back window; if the crash
count reaches `WorkerMaxRestarts` it logs a Fatal, increments the app crash-frequency counter,
writes a crash log and calls `IAppRestart.Restart()` — otherwise it merely restarts the single
worker (state `ForceStopped`). → **belegt**.

**App-level crash-frequency guard.** A counter file lives at
`~/Library/Logs/Amazon Cloud Drive/Crashes/crash-frequency`; the limits are **10 crashes within
5 minutes**. The counter is deleted on ordinary startup sources (not for "restart"), checked at
launch; when exceeded the app shows a warning dialog (German strings in §4), **unregisters the
relaunch agent** and continues — i.e. it stops the crash loop instead of restarting. The counter
is also incremented from the unhandled-exception path. → **belegt**. CLI overrides for crash
behaviour exist: `--crash-show` / `--crash-hide` (production hides the dialog) and
`--crash-exit` / `--crash-restart` (production restarts). → **belegt**.

**Self-restart mechanics.** `AppRestart.Restart(additionalArgs)`: set the shutdown flag, stop all
workers (500 ms), shut down metrics, filter the original startup args, prepend
`--source-restart` (unless an AutoStart argument is already present), append the additional
args, and run `Contents/Resources/quit_and_restart_app.scpt` through `osascript` with bundle
path, bundle id and the argument string. The compiled script quits the app, waits for it (and any
running installer) to exit, and reopens it with the passed `--args`. → **belegt**.

**Power events** are wired through `NSWorkspace`: going to sleep stops all workers; power-off
quits; **wake restarts the app** with `--source-autostart` (so a wake means a cold relaunch, not
a resume). → **belegt**. Whether the relaunch agent survives a user logout / reboot depends on
launchd semantics and was not traced → **UNKNOWN**.

### 3.4 Process model: agent app and Dock lifecycle (belegt)

- **Agent by default.** `Info.plist` has `LSUIElement = true`, and `AppDelegate` additionally
  sets `NSApplicationActivationPolicy = Accessory` at startup (in the same startup-task
  preparation step that disables the system's own relaunch-on-login via
  `DisableRelaunchOnLogin()`). → **belegt**.
- **Dynamic Dock icon.** A singleton `WindowManager` tracks opened windows by identifier:
  a `WindowOpenEvent` adds the window and calls `ShowDockIcon()` → `ActivationPolicy = Regular`
  + `ActivateIgnoringOtherApps(true)`; a `WindowCloseEvent` removes it and calls `HideDockIcon()`
  → back to `Accessory`, **only when no window remains open**. `CloseAll()` closes every window
  service (used by the "Fenster schließen" menu item, §3.5). → **belegt**.
- **Status item** is created once via `NSStatusBar.SystemStatusBar.CreateStatusItem(...)`
  (square length) with a state-dependent template image and the app menu attached; the
  menu/click/popover behaviour itself is owned by `SPEC_NOTIFICATIONS.md` §2.4–2.5
  (two-phase: preloaded minimal menu → full popover). The status item is started through a
  dedicated startup task (`AppIconTask.StartAppIcon`), with a second task switching it to the
  full menu after startup. → **belegt**.

So the original is an app that **lives in the menu bar** and only borrows a Dock presence while
a window is open — the opposite default of an ordinary windowed app.

### 3.5 Window lifecycle & menu surfaces (belegt)

| Behaviour | Original implementation |
|---|---|
| Close window ≠ quit | `ApplicationShouldTerminateAfterLastWindowClosed` returns **false**; the status item keeps the app alive |
| Refuse stray terminations | `ApplicationShouldTerminate` returns `Now` only when the shutdown flag is set or the quit Apple event (`QuitApplication`) arrives; otherwise **Cancel** |
| OS reopen (window click while running) | `ApplicationShouldHandleReopen`: does nothing while the startup tasks have not run; otherwise re-enters startup with `Reopen = true`, classifying the source from the Apple event (`TrayActive` / `TrayInactive` / `None`) |
| "Fenster schließen" (⇧⌘W) | OS main menu item → `WindowManager.CloseAll()` |
| "Minimieren" (⌘M) | standard `performMiniaturize:` |
| Quit (⌘Q, two menu sites) | `ShutdownApplication()` → `AppShutdown.Quit()`: set shutdown flag, metric, stop workers (500 ms), shutdown metrics, `NSApplication.Terminate` on the main thread. No confirmation dialog is part of this path → *inferred* "quits immediately" (no modal observed in the code) |
| Kill (installer / kill-switch) | `AppShutdown.Kill()`: shutdown flag, **unregister relaunch agent**, `Process.Kill()` |
| Start-at-login detection | `DidFinishLaunching` inspects the current Apple event: an `OpenApplication` event whose `prdt` parameter is the login-item code (`lgit`) is classified as `AppStartupFlags.AutoStart` — the app *knows* it was launched as a login item |
| Power sleep | stops all workers (no quit), so an upload in flight is not corrupted |

Menu surfaces (labels §4): a real application menu bar is installed despite the accessory policy —
the app menu carries About / Preferences (⌘,) / Help / Feedback / Release notes / "Fenster
schließen" / Quit, an Edit menu carries the standard cut/copy/paste selectors, and a Window menu
carries Minimize. The status item's own menu (opened via the item) is the pause/resume toggle
plus the overflow entries (Preferences, photos link, storage, help, release notes, feedback,
about, quit).

### 3.6 Autostart / login item — only the belegte facts (belegt)

- The preference **"Amazon Photos beim Starten öffnen"**
  (`clouddrive_desktop_preferences_auto_start_application`) is implemented **without a
  LoginItem bundle and without `SMAppService`**: the app shells out to `/usr/bin/osascript` to
  make / delete / list login items via the `System Events` application (created with
  `hidden:false`), and reads the current state the same way. The entitlement
  `com.apple.security.automation.apple-events` is what makes this possible.
- It is **enabled automatically at the end of onboarding** (`FinishOnboardingTask` sets the
  startup status to true) and **disabled when the session is dead**: the invalid-refresh-token
  handler removes the login item before restarting the app.
- The preferences dialog resynchronises the checkbox from the actual OS state whenever the
  setter fails. `Info.plist` contains **no** `SMLoginItemEnabled` key; the bundle contains
  **no** `Contents/Library/LoginItems/` (the directory does not exist).
- Whether the login item is re-created after an update, or if the user is ever prompted, is
  **UNKNOWN** (§6).

### 3.7 Other integration keys (belegt)

- **URL scheme.** `CFBundleURLTypes` registers `amazonphotos`. `DidFinishLaunching` subscribes
  to the `GURL` Apple event and routes the URL through `IUrlRouter`: the URL must have the
  `amazonphotos` scheme and exactly **two** segments (`amazonphotos://<handler>/<event>`);
  handlers are looked up by name. In this build exactly one handler is present in the managed
  assembly — `InstallerUrlHandler` (name "installer"; `installer/quit` shuts the app down).
  Whether more handlers exist outside the managed assembly is **UNKNOWN**.
- **Entitlements** (displayed read-only): app groups `group.com.amazon.clouddrive.mac` and
  `group.amazon.photos.desktop`; `com.apple.security.automation.apple-events` (login items,
  §3.6); `com.apple.security.personal-information.photos-library` (Apple Photos import); plus
  the Mono runtime's usual JIT/memory relaxations. **No sandbox entitlement** → the app runs
  unsandboxed (that is why the login-item AppleScript and the LaunchAgent writes work).
- **Dock behaviour.** No Dock icon while only background windows exist (§3.4); no Dock badge and
  no attention/bounce was found in the managed code; the only badge is drawn on the status-bar
  image (see `SPEC_NOTIFICATIONS.md` §2.4).
- **`NSMultipleInstancesProhibited = true`** — launching a second instance focuses/relaunches the
  running one rather than starting a parallel process (the reopen path §3.5).

---

## 4. String evidence

Keys cited in this doc, verbatim from the shipped `de.lproj` files (German values) with the
referencing base string where applicable. "Referenced?" = grep for the managed call site.

| Key / source | Base string | German | Referenced? |
|---|---|---|---|
| `ServicesMenu.strings` | "Upload to Amazon Photos" | "Bei Amazon Photos hochladen" | yes — the `NSServices` menu label (`NSServices` → `NSMenuItem.default`) |
| `…_context_menu_upload` | "Bei Amazon Photos hochladen" | yes (`AddBackupFromWindowsContextMenuTask`) |
| `…_preferences_auto_start_application` | "Amazon Photos beim Starten öffnen" | yes (preferences checkbox, autostart §3.6) |
| `…_crashfrequency_kill_title` | "Beim Ausführen der Amazon ​Photos-App ist ein Fehler aufgetreten." | yes (crash-frequency guard §3.3) |
| `…_crashfrequency_kill_message` | "Bitte wenden Sie sich an den Kundenservice, um weitere Unterstützung zu erhalten." | yes (same dialog) |
| `…_mac_quit` | "Amazon Photos verlassen" | yes (status menu + OS menu, ⌘Q) |
| `…_menu_closewindow` | "Fenster schließen" | yes (OS menu, ⇧⌘W → `CloseAll`) |
| `…_menu_window` | "Fenster" | yes (OS menu title) |
| `…_menu_minimize` | "Minimieren" | yes (OS menu, ⌘M) |
| `…_pause` / `…_resume` | "Alle anhalten" / "Alle fortsetzen" | yes (status menu toggle, state-dependent) |
| `…_contextmenu_statuswindow` | "Status anzeigen" | **no** — defined but unreferenced in this build (*inferred* dead/legacy label) |
| `…_contextmenu_hover_no_status_message` / `…_hover_status_message` | "Amazon Photos" / "Amazon Photos - {0}" | status-item tooltip (see `SPEC_NOTIFICATIONS.md` §2.4) |

Not re-quoted here (owned by other docs): the status-popup titles
(`…_contextmenu_manualupload` "Upload und Status", the `…_contextmenu_message_*` state texts,
`…_contextmenu_uploaderrored`) → `SPEC_NOTIFICATIONS.md` §2.4–3.

---

## 5. Gap analysis vs. our app (committed state)

| Capability | Original | Our app | File reference |
|---|---|---|---|
| Finder service "Bei Amazon Photos hochladen" | one `NSServices` entry (folders only) → default scheduled backup | **none** — `packaging/Info.plist` has no `NSServices`, no service handler exists | `packaging/Info.plist` |
| Windows shell sibling | registry `directory\shell\amz_addBackup` → same queue | **none** | — |
| URL scheme | `amazonphotos://<handler>/<event>` → `IUrlRouter` | **none** — no `CFBundleURLTypes` | `packaging/Info.plist` |
| Agent / `LSUIElement` | agent app, Dock icon only while a window is open | **parity** — a dynamic Dock icon like the original: `Regular` (Dock icon + app menu) while a window is open, `Accessory` (neither) once the window is hidden; a **silent login-item start** (no window, no Dock icon, tray only) is classified from the same Apple event as the original (`kAEOpenApplication` / `lgit`); closing the window only hides it, the app keeps running; **single instance** via `LSMultipleInstancesProhibited` in the bundle plus a socket/TCP handshake guard. Only deliberate deviation: our bundle does **not** set `LSUIElement` — the policy is switched at runtime instead | `src/app_activation.rs` (`launched_as_log_in_item`, `become_accessory`/`become_regular`, activation observer), `src/app.rs` (`should_start_hidden`, `CloseRequested`/`show_main_window`), `src/single_instance.rs`, `packaging/Info.plist` (`LSMultipleInstancesProhibited`) |
| Close ≠ quit | `ApplicationShouldTerminateAfterLastWindowClosed` false; tray keeps running | **yes (equivalent)** — close-to-tray: close is cancelled, window hidden | `src/app.rs` (`CloseRequested` handling in `update`) |
| "Fenster anzeigen" | reopen via status item/OS reopen | tray item "Fenster anzeigen" → show + focus | `src/tray.rs`, `src/app.rs` |
| "Beenden" / ⌘Q | two ⌘Q sites → stop workers → terminate | tray "Beenden" → viewport close; ⌘Q only via minimal app menu "Prime Photos beenden" | `src/tray.rs`, `src/app_menu.rs`, `src/app.rs` |
| OS main menu | App + Edit + Window menus (About, Preferences, Help, "Fenster schließen" ⇧⌘W, Quit, Minimize) | **partial** — app menu with Quit + full Edit menu (for ⌘X/⌘C/⌘V in the login webview); no Window menu, no ⇧⌘W | `src/app_menu.rs` |
| Crash relaunch | LaunchAgent (`KeepAlive`, `restart_app.scpt`, `launchctl load/unload`) + worker watchdog (5/15 s) + crash-frequency guard (10/5 min) | **none** — no relaunch mechanism, no crash policy | — |
| Self-restart | `AppRestart` via `quit_and_restart_app.scpt`, `--source-restart`; wake → `--source-autostart` | **none** | — |
| Autostart / login item | preference via `osascript` System Events; auto-enabled at onboarding | **partial** — `auto-launch` crate in **`MacOSLaunchMode::SMAppService`** (no AppleScript; `SMAppService.mainApp.register()` → visible in the **Anmeldeobjekte**, System Settings → General → Login Items; requires macOS 13+ — below that the crate returns `Err(UnsupportedOS)` and the app stays on the existing "unsupported" path, persisting nothing); a **deliberate deviation** from the original's `osascript`/System Events (§3.6); settings checkbox "Nach dem Anmelden starten"; *inferred* not auto-enabled at first sign-in (no onboarding default found) | `Cargo.toml` (`auto-launch`), `src/app.rs` (`Autostart`, `build_autostart`), `src/settings.rs` |
| Notifications | `NSUserNotification` + status item | `notify-rust` run-finished toast → cross-ref | `src/notify.rs`, `SPEC_NOTIFICATIONS.md` |
| Apple Photos import | `NSPhotoLibraryUsageDescription` + entitlement | **out of scope** (MVP excludes the import) | — |
| Bundle packaging | full `Info.plist` (URL, services, LSUIElement…) | template with identifier/version/category only; `make-app.sh` signs ad-hoc; **app icon copied and `CFBundleIconFile = AppIcon`** (observed) — `Contents/Resources/AppIcon.icns`, built from `assets/logo.svg` (`build.rs` → `target/pp-appicon.iconset` → `iconutil` in `make-app.sh`), **not checked in** | `packaging/Info.plist`, `packaging/make-app.sh` |

---

## 6. UNKNOWN (explicitly not determined)

- **Finder-side filtering of the service.** The plist restricts the service to directories; the
  handler does not re-check. Whether a file selection can ever reach `AddBackup` (e.g. via a
  stale pasteboard or another sender) is **UNKNOWN**.
- **`NSPortName = "Finder"` semantics.** The value is read verbatim from the plist; how the
  service message is routed to this app (and why the Finder port name is used) was not traced.
  → **UNKNOWN**.
- **What the service shows.** The code path creates a schedule and requests a scan; no window /
  toast was observed on this path. Whether the main window or a toast appears (and whether the
  user must be signed in — `ProcessContextMenuPathsTask` ordering vs. login was not traced) is
  **UNKNOWN**.
- **`restart_app.scpt` / `quit_and_restart_app.scpt` details** beyond the readable strings
  (retry counts, timeouts, installer-wait loops): compiled AppleScript; only strings were read.
- **LaunchAgent lifetime across logout/reboot** (launchd reload semantics, re-arm after update):
  not traced.
- **Additional `IUrlHandler`s** outside the managed assembly (native launcher or another dll):
  only `InstallerUrlHandler` is present in the decompilation.
- **Whether the login item is re-created by the updater** and whether the user is ever asked:
  **UNKNOWN**. What *is* belegt: osascript-based, auto-enabled at onboarding, disabled on dead
  session.
- **Crash-dialog visibility in this build**: production defaults (`--crash-hide`, `--crash-restart`)
  are belegt; whether the crash-frequency dialog (§4) can ever be *seen* by a user depends on
  those flags → **UNKNOWN**.
- **Dock-tile behaviour on a second instance launch** (`LSMultipleInstancesProhibited`): focus
  vs. terminate-first was not traced.
- **`…_contextmenu_statuswindow` ("Status anzeigen")** render site — defined but unreferenced;
  possibly a legacy label.

---

## 7. Cross-references

- `SPEC_NOTIFICATIONS.md` §2.1–2.6 — bundle/process model, LaunchAgent plist template,
  login item (osascript), status item icon/tooltip/badge, click phases, close-keeps-running;
  §3 — notification surfaces (not duplicated here).
- `SPEC_STATUS_DETAIL.md` — the status-popup detail (planned at the time of writing; the
  popup content is only cross-referenced here, not specified).
- `SPEC_ONE_TIME_UPLOAD.md` §6 — the one-time pipeline the `--upload` argument path shares;
  §8 — its string table (a drop/service conflict policy is *not* re-specified here).
- `SPEC_BACKGROUND_SYNC.md` — scheduled cadence/pause that a service-created backup obeys.
- `UI_FLOWS.md` §2 — the login window chrome (context for the app menu / Edit menu need),
  §13 — status-item click question (resolved in `SPEC_NOTIFICATIONS.md` §2.5).
- `../dev/RELEASE.md` — our packaging/signing context (ad-hoc macOS signature).
- Our code: `src/tray.rs` (tray items "Fenster anzeigen" / pause / "Beenden"),
  `src/app_menu.rs` (macOS app + Edit menu), `src/app.rs` (`Autostart`, close-to-tray,
  tray event dispatch), `src/settings.rs` (autostart checkbox), `src/notify.rs`
  (run-finished notification), `src/platform.rs` (reveal/open helpers),
  `packaging/Info.plist` + `packaging/make-app.sh` (bundle), `Cargo.toml` (`auto-launch`,
  `tray-icon`, `notify-rust`, `objc2-app-kit`).
