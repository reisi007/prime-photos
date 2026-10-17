# Prime Photos — Clean-room Spec: background helper / menu-bar presence & notifications

Behavioural spec of the **original** Amazon Photos desktop client for macOS, focused on
(a) whether it has a background helper / login item / LaunchAgent / XPC service and how its
menu-bar presence is built, and (b) which notification APIs it uses and which events trigger
a user-facing notification. Written for the Prime Photos port as a *plan*, not as an
implementation order.

This document is **clean-room**: it records *observed behaviour* in our own words and tables.
It contains **no verbatim decompiled code** and **no personal data**. All account, user and
path values were redacted or omitted.

---

## 1. Scope & sources

| Item | Value |
|---|---|
| Target (read-only, never launched/modified) | `/Applications/Amazon Photos.app` |
| Bundle id | `com.amazon.clouddrive.mac` |
| Version / build | `11.3.0` (build `36`), per `Contents/Info.plist` |
| Managed code | `Contents/MonoBundle/Amazon Photos.dll` (.NET / CoreCLR, `Microsoft.macOS.dll` bindings) |
| Native launcher | `Contents/MacOS/Amazon Photos` |
| German labels | `Contents/Resources/de.lproj/Localizable.strings` (986 entries) |
| Existing notes reused | `UI_FLOWS.md` §9 (menu-bar status popup), §11.4; `UI_STATES.md` |

### 1.1 Exact commands used (all read-only)

```sh
# Bundle layout, plist, helper/login-item presence
ls -la "/Applications/Amazon Photos.app/Contents/"
ls -la "/Applications/Amazon Photos.app/Contents/MacOS" \
       "/Applications/Amazon Photos.app/Contents/Helpers" \
       "/Applications/Amazon Photos.app/Contents/Library" \
       "/Applications/Amazon Photos.app/Contents/Frameworks" 2>&1
find "/Applications/Amazon Photos.app" -iname '*login*' -o -iname '*helper*' \
     -o -iname '*.xpc' -o -iname '*agent*' -o -iname '*daemon*'
plutil -p "/Applications/Amazon Photos.app/Contents/Info.plist"
cat "/Applications/Amazon Photos.app/Contents/Resources/restart_app.plist"
ls ~/Library/LaunchAgents /Library/LaunchAgents

# Symbol-level search inside the managed DLL
strings -a "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll" \
  | grep -iE 'NSStatusItem|NSStatusBar|MenuBarExtra|LoginItem|LaunchAgent|SMAppService|ServiceManagement|SMLoginItem|NSUserNotification|UNUserNotification'
strings -a "/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll" \
  | grep -iE 'LaunchAgents|launchctl|osascript|kLSSharedFileList|DockTile|RequestUserAttention'

# Decompile (read-only). ilspycmd needs DOTNET_ROOT because the tool host is a Homebrew SDK.
export PATH="$PATH:$HOME/.dotnet/tools"
export DOTNET_ROOT="/opt/homebrew/Cellar/dotnet/10.0.401_1/libexec"
DLL="/Applications/Amazon Photos.app/Contents/MonoBundle/Amazon Photos.dll"
ilspycmd -l c "$DLL"                       # list classes
ilspycmd -t "Amazon.CloudDrive.Services.MenuBar.MenuBarController"        "$DLL"
ilspycmd -t "Amazon.CloudDrive.Services.MenuBar.MenuBarStatusItemViewController" "$DLL"
ilspycmd -t "Amazon.CloudDrive.Services.MenuBar.MenuBarAppIconService"    "$DLL"
ilspycmd -t "Amazon.CloudDrive.Services.MenuBar.PreloadedMenuBarController" "$DLL"
ilspycmd -t "Amazon.CloudDrive.Services.ToastNotificationService"         "$DLL"
ilspycmd -t "Amazon.CloudDrive.Services.PopupNotificationCenter.PopupNotificationCenter" "$DLL"
# plus targeted greps over a full `ilspycmd "$DLL"` dump for the event handlers and helpers

# German labels
grep -n -iE 'notification|statuspopup|statuswindowclosed|filenameconflict_toast' \
  "/Applications/Amazon Photos.app/Contents/Resources/de.lproj/Localizable.strings"
```

### 1.2 Confidence convention

* **observed** — read directly off a source above (plist, decompiled member, strings file).
* *inferred* — reasoned from adjacent evidence, not read off a line.
* **UNKNOWN** — could not be determined from the inspected sources.

---

## 2. Original behaviour — background helper / menu-bar presence

### 2.1 Bundle layout & process model — no separate helper app (observed)

`Contents/` contains only: `MacOS/`, `MonoBundle/`, `Resources/`, `_CodeSignature/`,
`Info.plist`, `PkgInfo`. There is **no** `Contents/Helpers/`, `Contents/Library/`,
`Contents/Library/LoginItems/`, `Contents/Frameworks/`, no `.xpc` bundle, and no second
executable. All logic lives in the single managed `Amazon Photos.dll`; the native launcher
only boots the CoreCLR runtime.

`Info.plist` (observed):

| Key | Value | Meaning |
|---|---|---|
| `LSUIElement` | `true` | Agent app: **no Dock icon, no app menu bar by default** |
| `NSUserNotificationAlertStyle` | `alert` | native notifications are presented as **alerts** (not banners) |
| `LSMultipleInstancesProhibited` | `true` | only one instance |
| `LSMinimumSystemVersion` | `10.15` | |
| `CFBundleURLTypes` | scheme `amazonphotos` | URL entry point (`handleGetURLEvent:`), not a helper |
| `NSMainNibFile` | `MainMenu` | |
| `NSServices` | `Upload to Amazon Photos` → `addBackupFromMacContextMenu` | Finder Services entry point |

Absent from `Info.plist`: `SMLoginItem*`, `SMPrivilegedExecutables`, any `*LoginItem*` key
(observed). At runtime the app additionally sets
`NSApplication.ActivationPolicy = Accessory` and calls `DisableRelaunchOnLogin()` (observed).

### 2.2 LaunchAgent — a restart/recovery agent, not an upload worker (observed)

The app does **not** ship a LoginItem, but at every launch it registers a **per-user
LaunchAgent** for crash recovery:

* Plist written to `~/Library/Application Scripts/Amazon Cloud Drive/restart_app.plist`
  (path from the app's `ScriptsPath` constant).
* Contents (shipped template `Contents/Resources/restart_app.plist`, placeholders
  `${SCRIPTPATH} ${APPPATH} ${APPID}` filled at runtime):

  | Key | Value |
  |---|---|
  | `Label` | `com.amazon.clouddrive` |
  | `ProcessType` | `Interactive` |
  | `KeepAlive` | `true` |
  | `ProgramArguments` | `/usr/bin/osascript` → `restart_app.scpt` → `<app path>` → `<bundle id>` |

* Registered via `/bin/launchctl load`, removed via `/bin/launchctl unload`.
* `KeepAlive=true` means launchd relaunches the app if it dies (recovery), **not** a hidden
  uploader: the relaunched process is the full GUI app in accessory mode.
* Unregistered on normal termination (`WillTerminate`) and on `Kill`/uninstall
  (observed: `UnregisterApplicationRecoveryAndRestart` call sites).

*No* `SMAppService`, `SMJobBless`, `SMLoginItem` or XPC usage was found in the managed code.
(`SMAppService` appears only as a type-name string inside the native launcher — a
`Microsoft.macOS`-bindings symbol, **not** a call site — so its use is **UNKNOWN**/not found.)

### 2.3 Login item (auto-start) — legacy `osascript` (observed)

The "Amazon Photos beim Starten öffnen" preference
(`clouddrive_desktop_preferences_auto_start_application`) is implemented without a
LoginItem/SMAppService: the app shells out to
`/usr/bin/osascript` → AppleScript `System Events` to
`make login item …` / `delete login item …` / `get the name of every login item`.
It is **enabled automatically** at the end of onboarding
(`FinishOnboardingTask` sets the app as a startup item). The LaunchServices
`kLSSharedFileList*` P/Invoke wrapper that also exists in the binary is used for
**Finder sidebar favourites**, not for login items (observed).

### 2.4 Menu-bar status item — `NSStatusBar` / `NSStatusItem` (observed)

* Created once: `NSStatusBar.SystemStatusBar.CreateStatusItem(NSStatusItemLength.Square)`
  (square, so the image size drives width).
* The status item's `Button` carries:
  * **Image** — a template (monochrome) image that changes with app state, or a coloured
    variant for unread notifications:

    | App state (`NotificationIcon`) | Image asset | Notes |
    |---|---|---|
    | `Idle` (signed out) | `idle_mac_blk` | template |
    | `Processing` (transferring / cold-boot / get-changes) | `sync_mac_blk` | template |
    | `Synced` (all done) | `complete_mac_blk` | template |
    | `Errored` (≥1 errored item) | `alert_mac_blk` | template |
    | `Paused` | `pause_mac_blk` | template |
    | `NoNetwork` (offline / service down) | `error2_mac_blk` | template |
    | `UnreadNotifications` | `unread_noti_mac_red_light` / `_dark` | **coloured**, theme-dependent |

  * **Tooltip** — `format("Amazon Photos - {0}", statusMessage)`, or the plain
    `"Amazon Photos"` (`clouddrive_desktop_contextmenu_hover_no_status_message`) when the
    message is empty.
  * **Red badge** — optional numeric badge drawn onto the status image
    (`BadgedIcon.DrawBadge(count, …)`) when the setting `EnableNotificationBadging` is on and
    there are unread in-app notifications (ThisDay/Story excluded).
* **Dark/light theme** — the app observes the distributed notification
  `AppleInterfaceThemeChangedNotification` to re-render the coloured (unread) image.
* **Sidebar/status state → icon + tooltip** is centralised in one mapping function
  (`AppStatusExtensions.GetNotificationMessage`), precedence top-to-bottom:

  | Condition | Icon | Tooltip text (key) |
  |---|---|---|
  | no network | NoNetwork | "Kein Netzwerk" (`…_syncwindow_nonetwork`) |
  | service unavailable | NoNetwork | "Verbindung kann nicht hergestellt werden." (`…_syncwindow_serviceunavailable`) |
  | cold boot (get changes) | Processing | "Konfiguration wird durchgeführt" (`…_contextmenu_message_getchanges`) |
  | signed in + unread + badging on | UnreadNotifications | "Ungelesene Benachrichtigungen" (`…_contextmenu_message_unreadnotifications`) |
  | global pause | Paused | "Angehalten" (`…_syncwindow_syncpaused`) |
  | transferring | Processing | "In Bearbeitung" (`…_statuswindow_statusprocessing`) |
  | ≥1 errored item | Errored | "Mit Fehlern abgeschlossen" (`…_statuswindow_errors`) |
  | otherwise (signed in) | Synced | "Abgeschlossen" (`…_contextmenu_message_transfercomplete`) |
  | not signed in | Idle | (empty) |

### 2.5 Menu-bar click behaviour — resolved (observed)

There are **two phases**, which answers the open question in `UI_FLOWS.md` §13:

1. **Pre-start phase** (before login/full startup, `PreloadedMenuBarController`): the status
   item's `Menu` is a minimal `NSMenu` with a hidden status item, **Info/About** and
   **Beenden/Quit** only. **Left-click opens that native menu.**
2. **Full phase** (`SwitchMode`, run by the `StartShowingFullMenu` startup task): the status
   item's `Menu` is set to `null` and the button's action is bound to `menuButtonClicked:`.
   **Left-click now opens the status popup** — an `NSPopover`
   (`StatusPopupWindowService.ShowRelativeTo(statusButton)`), i.e. the 3-row
   Backup/Download/Sync summary + notifications list described in `UI_FLOWS.md` §9.

The full-phase popover's avatar button opens the separate app/overflow `NSMenu`
(`MenuBarController`): Pause/Resume, Preferences (⌘,), separator, "Fotos im Web öffnen",
"Speicher verwalten", Hilfe, separator, Versionshinweise, Feedback senden, About, separator,
Beenden (⌘Q). *Right-click* has no distinct behaviour (single action). *observed*

The app also installs a real application menu bar (`NSMenu` as `MainMenu`) with
App/Edit/Window submenus, even though it is an accessory app. *observed*

### 2.6 Keeps running when the main window is closed (observed)

* `ApplicationShouldTerminateAfterLastWindowClosed` returns **false** → the app and the
  menu-bar item stay alive after the last window closes.
* `ApplicationShouldTerminate` returns **Now** only when the shutdown flag is set or the quit
  Apple event arrives (⌘Q from the app menu); otherwise it returns **Cancel** → closing a
  window never quits the app.
* "First close" explanatory tip: the string pair
  `clouddrive_desktop_notification_statuswindowclosedfirsttime_title` → "Auf Amazon Photos
  zugreifen" and `…_mac_notification_statuswindowclosedfirsttime_message` → "Du kannst die
  App jederzeit über dieses Symbol in der Menüleiste aufrufen." exists. In **this build** the
  message is shown as a custom **callout popover** (see §3.3), and only from
  `FinishOnboardingTask` (i.e. at the end of onboarding), **not** from a window-close handler.
  The title key is **defined but not referenced** in this build (the callout uses the string
  "Willkommen bei Amazon Photos" as its title). → the *literal* "on first window close" trigger
  is **UNKNOWN**; the tip is observed at onboarding completion. *observed / UNKNOWN*

There is **no Dock icon** (LSUIElement + Accessory) and **no Dock badge**; the only badge is
drawn on the menu-bar image. No `RequestUserAttention`/bounce was found. *observed*

---

## 3. Notifications

### 3.1 API used (observed)

| Aspect | Finding |
|---|---|
| Native toast API | **Deprecated `NSUserNotification` + `NSUserNotificationCenter`** (`window.Notification`/`UNUserNotificationCenter` is **not** used anywhere **in the managed assembly**; a `Microsoft.macOS` bindings symbol for it exists in the native launcher — cf. §4). Only the managed code was decompiled, so its use outside the assembly is **UNKNOWN** |
| Delegate | `ToastNotificationService : NSUserNotificationCenterDelegate`, set as the default center's delegate |
| Presentation | `ShouldPresentNotification` always returns `true` (presented even when the app is frontmost) |
| Sound | always the **default notification sound** |
| Alert style | `NSUserNotificationAlertStyle = alert` in `Info.plist` |
| Action button | optional per notification (`HasActionButton` + `ActionButtonTitle`) |
| Click handling | `DidActivateNotification` looks up the notification's GUID in an in-memory dictionary and runs its `Action()` callback, then removes it from the center |
| De-duplication | optional `DedupeId`; the service skips a toast if an identical `DedupeId` is already in `DeliveredNotifications` |
| Delivery time | `DeliveryDate = now` |
| Cleanup | on `WillTerminate` the app calls `RemoveAllDeliveredNotifications()` |
| In-app "notifications" | a **separate** `PopupNotificationCenter` feeding the status-popup list (§3.3) |
| Custom callout | a **separate** `NSPopover` "callout" anchored to the status button (`AppIconCalloutView`, auto-closing after a timeout, default 7000 ms) |
| Dialogs | `NSAlert` used for modal error/warning dialogs (e.g. crash-frequency, update failure) — not notifications |

Enum `NotificationType` for the **in-app** list only:
`OverQuota`, `NearQuota`, `UpdateAvailable`, `ThisDay`, `Story` (observed).

### 3.2 Event → notification / string table (observed)

"native toast" = `NSUserNotification` via `ToastNotificationService`. All labels quoted from
`de.lproj/Localizable.strings` (key → label).

| Trigger event | Channel | Title (key → label) | Body (key → label) | Button |
|---|---|---|---|---|
| **Upload started** (drag-drop) | native toast class `DragDropToastNotification` exists | `…_notification_dragdrop_title` → "Ihr Upload wurde gestartet" | `…_notification_dragdrop_message` → "Der Inhalt wird in Ihrem Amazon Photos-Konto gespeichert." | — |
| **Import started** (Apple Photos / media) | native toast | `…_notification_mediaimport_title` → "Ihr Upload wurde gestartet" | `…_notification_mediaimport_message` → "Die importierten Fotos und Videos werden in Ihrem Amazon Photos-Konto gespeichert." | `…_notification_mediaimport_button` → "Details" |
| **Upload finished (success)** | **no native toast.** Surfaced only via menu-bar icon `complete_mac_blk` + tooltip "Abgeschlossen"/"Upload abgeschlossen" (`…_contextmenu_message_transfercomplete` / `…_contextmenu_message_uptodate`) and the status-popup backup row "Datensicherung abgeschlossen" (`clouddrive_desktop_statuspopup_backupcomplete`) | — | — | — |
| **Upload / schedule error** | **no native toast.** Surfaced only via menu-bar icon `alert_mac_blk` + tooltip "Mit Fehlern abgeschlossen" (`…_statuswindow_errors`) and popup stats "({0:n0} Fehler)" (`…_statuspopup_stats_errors`) / "{0} Dateien konnten nicht hochgeladen werden" (`…_contextmenu_uploaderrored`) | — | — | — |
| **Over quota / subscription** | native toast (`AccountStatusNotificationHandler`) | free tier: `…_notification_overquota_freetier_title` → "Ihr Speicherplatz ist aufgebraucht"; unlimited-photos: `…_notification_overquota_unlimitedphotos_title` → "Kontolimit erreicht"; expired trial: `…_notification_trialexpired_title` → "Speicherplan wählen" | free tier: "Sie benötigen ein Upgrade für Ihr Abonnement, um mehr Speicherplatz zu bekommen."; unlimited-photos: "Video- und Dateispeicherplatz ist aufgebraucht. Foto-Uploads werden fortgesetzt."; trial: "Wechseln Sie zu \"Speicher verwalten\", um einen neuen Plan zu starten." | "Upgrade" / "Speicherplatz hinzufügen" / "Speicher verwalten" |
| **Sign-in required / expired** | **no toast found.** Token-refresh failure triggers an app **restart** (`AppRestartInvalidRefreshTokenHandler` → clears startup item), and the auth window is shown via restart (`AppRestartAuthPortalWindow`). No dedicated German notification string was found | — | — | **UNKNOWN** |
| **First window close / onboarding done** | custom **callout popover** (not `NSUserNotification`) | "Willkommen bei Amazon Photos" (`…_stub_installer_welcome_view_welcome`) | `…_mac_notification_statuswindowclosedfirsttime_message` → "Du kannst die App jederzeit über dieses Symbol in der Menüleiste aufrufen." | — |
| **Update available** | native toast (`UpdateAvailableNotificationHandler`) | normal: `…_notification_update_title` → "Aktualisierung in {0} Minuten"; sync variant: `…_notification_syncupdate_title` → "Update verfügbar" | normal: "Uploads werden nach der Aktualisierung fortgesetzt."; sync: "Eine neue Version von Amazon Photos ist verfügbar." | "Jetzt aktualisieren" / "Aktualisieren" (+ "Nicht jetzt") |
| **Update incompatible (.NET)** | **modal `NSAlert`** (`…_update_available_title`), not a toast | "Update verfügbar" | `…_dotnet_update_information` | prompt |
| **File-name conflict found** | native toast (`ResolveScheduleNameConflictsEventHandler`) | `…_filenameconflict_toast_title` → "Dateinamenkonflikte gefunden" | `…_filenameconflict_toast` → "Öffne die App, um Konflikte zu lösen." | `…_filenameconflict_toast_button` → "Öffnen" |
| **Sync mass-deletes** | native toast (`SyncMassDeletesNotificationHandler`) | `…_notification_syncmassdeletes_title` → "Neue Nachricht von Amazon Photos" | `…_notification_syncmassdeletes_shortmessage` → "Eine große Anzahl von Dateien wurde gelöscht." | `…_notification_campaignwindow_button` → "Details" |
| **Campaign / account message** | in-app window + status-popup list (not a toast) — *inferred: strings present, not wired to `PopupNotificationCenter` (no `Campaign` member in `NotificationType`)* | `…_notification_campaignwindow_title` → "Neue Nachricht von Amazon Photos" | `…_notification_campaignwindow_message` → "Sie haben eine neue Nachricht über Ihr Konto." | "Details" |
| **This-Day / Story (delighters)** | in-app status-popup list only | `…_statuspopup_notification_thisdaytext` → "Sie haben neue Fotos in „Dieser Tag“"; `…_statuspopup_notification_thisdaymemoriesfrom` → "Erinnerungen von" | — | — |
| **Near / over quota (in-app row)** | in-app status-popup list only | `…_statuspopup_notification_nearquota` → "Ihr Kontingent ist fast aufgebraucht"; `…_statuspopup_notification_overquota` → "Sie haben das Kontingent überschritten" | — | — |
| **Update available (in-app row)** | in-app status-popup list only | `…_statuspopup_notification_updateavailable` → "Es ist ein Update verfügbar" | — | `…_statuspopup_notification_updatenow` → "Jetzt aktualisieren" |

Throttles / gates actually consulted (observed): over-quota toast throttled by
`NotifyOverQuotaAfterTimeInSeconds` (default **86400 s**); file-conflict toast gated by
`FileConflictNotificationEnabled` (default **false**) and throttled by
`FileConflictNotificationThrottleHours` (default 0) and a `DedupeId`; mass-delete toast
window/threshold from `MassDeletesNotificationWindowInSeconds` (default 30 s) and a
cooldown (`WaitTimeBetweenMassDeletesNotificationsInSeconds`, default 3600 s).

> Note: the drag-drop toast class is defined but **has no observed publisher** in this build —
> the in-window drag-drop overlay is used instead. → *inferred dead / unused*.

### 3.3 The three "notification" surfaces (observed, kept distinct)

1. **Native toasts** (`NSUserNotificationCenter`, §3.1) — the OS-level banners/alerts for
   quota, updates, name conflicts, mass-deletes, media-import start.
2. **In-app status-popup list** (`PopupNotificationCenter`) — an in-memory list shown in the
   menu-bar popover under the header "Benachrichtigungen" (`…_statuspopup_notifications`),
   empty state "Keine neuen Benachrichtigungen" (`…_statuspopup_nonotifications`). Server-driven
   (This-Day, Story, quota, update). Unread count drives the red badge on the
   menu-bar image and the `PopupNotificationUnreadEvent`.
3. **Menu-bar callout popover** (`AppIconCalloutView`) — a custom floating tip anchored to the
   status item, auto-closing after a timeout (default 7000 ms). Used for the onboarding/first-run
   tip (§2.6).

### 3.4 User preferences / toggles (observed)

| Preference | Persisted key (default) | Wired to UI? | Effect |
|---|---|---|---|
| "Amazon Photos beim Starten öffnen" (`…_preferences_auto_start_application`) | a customer setting via `IAppStartup` | **yes** — `Preferences_StartOnStartup_CheckBox` | adds/removes the `System Events` login item (§2.3) |
| "Aktivieren Sie Badges für Benachrichtigungen" (`…_preferences_enablenotificationbadges`) | `EnableNotificationBadging` (default true) | **yes** — `Preferences_EnableNotificationBadging_Checkbox` | toggles the red unread badge on the menu-bar image |
| "Benachrichtigungen anzeigen" (`…_preferences_enable_notifications`) | `ShowNotifications` (default true) | **no control built in this build** (identifier not present among the preferences controls) | no consumer found — the toast service does **not** consult it | *inferred legacy / not effective (UNKNOWN)* |

No per-event notification toggles (e.g. "notify me when an upload finishes") were found — the
app has **no native "upload finished" notification at all** (see §3.2). *observed*

---

## 4. Confidence / UNKNOWN markers

* **observed** — plist values (`LSUIElement`, `NSUserNotificationAlertStyle`, absent
  `SM*`/LoginItem keys); the `NSStatusBar`/`NSStatusItem` construction, icon map, tooltip and
  badge; the two-phase menu click behaviour; `ApplicationShouldTerminateAfterLastWindowClosed
  = false`; the LaunchAgent (`restart_app.plist`, `KeepAlive`, `osascript`); the login item via
  `osascript`; the `NSUserNotification` toast service and every toast call site listed in §3.2;
  the German strings and keys.
* *inferred* — `DragDropToastNotification` is unused (class defined, no publisher found);
  `ShowNotifications`/`clouddrive_desktop_preferences_enable_notifications` is legacy/unwired;
  `SMAppService` in the native launcher is a binding symbol, not a call.
* **UNKNOWN** — the *literal* "first window close" trigger for the explanatory tip (observed
  only at onboarding completion; the title key is unreferenced); the sign-in-required/expired
  notification (none found; app restarts instead); whether `SMAppService`/newer
  `UNUserNotificationCenter` are used anywhere outside the managed assembly (only the managed
  code was decompiled).
* Not inspected: the obfuscated non-managed native launcher's full symbol set (only `strings`
  was used on it); other localisations besides `de.lproj`.

---

## 5. Mapping to our app (plan only — do NOT implement)

Our app is a Rust/egui+eframe native macOS app. The original's behaviour maps as follows.

| Original behaviour | Portable or Mac-only | Plan for Prime Photos |
|---|---|---|
| Menu-bar status item (`NSStatusBar`/`NSStatusItem`) | **Mac-only API**, but a cross-platform crate exists | Use a tray abstraction (`tray-icon`, which wraps `NSStatusItem` on macOS) or `objc2-app-kit` directly. Guard behind `#[cfg(target_os = "macos")]`; provide a no-op/portable stub elsewhere. |
| Status icon states (idle/uploading/done/error/paused/offline) | portable logic, Mac-only rendering | Map our uploader state to an icon; ship monochrome template images + a coloured unread variant. |
| Tooltip "Amazon Photos - {state}" | portable | Reuse our German status strings; one mapping function from worker state → (icon, tooltip). |
| Left-click opens the status popup vs a minimal pre-login menu | Mac-only (NSPopover) | Decide single behaviour for us: click → popup/menu. Simplest MVP: click opens the main window; later a popover. |
| Keep running with the window closed (accessory app, no Dock icon) | Mac-only (`LSUIElement`/ActivationPolicy/false-terminate) | In eframe/winit: intercept the window-close event and **hide** the window instead of exiting; ensure `quit` only via an explicit menu action. Unlike the original, our app stays a normal window app — its Dock icon remains while it runs (no `LSUIElement`/accessory activation policy). |
| Login item via `osascript System Events` | **Mac-only** (legacy) | Prefer modern `SMAppService.mainApp.register()` (macOS 13+) via `objc2`; fall back to the `osascript` approach only if needed. No launchd plist of our own is required for this. |
| Crash-recovery LaunchAgent (`KeepAlive`) | **Mac-only** | **Out of scope for the MVP** — the original's restart agent is a resilience nicety; skip unless we want crash-relaunch. |
| Native toasts on quota / update / name-conflict / mass-delete | Mac API is deprecated `NSUserNotification`; modern path is `UNUserNotificationCenter` | Use the modern `UserNotifications` framework via `objc2`/`objc2-user-notifications` (request authorization, then `UNMutableNotificationContent`), which needs our bundle id and a signed/bundled app to prompt. A lighter alternative is `osascript -e 'display notification …'`, but it lacks action buttons. |
| **Upload finished / upload error notification** | portable trigger, Mac-only delivery | The original has **none**; this is a gap we can *add*: on `done` show a success toast, on `error` a failure toast (our own strings). Keep it behind a single app-level "notifications" setting. |
| User toggle for notifications | portable | Persist a `notifications_enabled` flag in our `config.toml`; gate every toast (the original's `ShowNotifications` existed but was effectively unwired — we should wire ours properly). |
| Badge on the tray icon for unread/errors | portable decision, Mac-only rendering | Optional; can render a numeric badge onto the tray image from our error/unread count. |
| Dock badge / Dock icon | **Mac-only**, and the original has none | Skip; unlike the original, our app is a normal window app whose Dock icon stays while it runs (no `LSUIElement`/accessory mode). |

**Portable vs Mac-only summary:** the *decision logic* (which event fires a notification, the
visibility preference, the state→icon mapping, hide-on-close) is portable; the *delivery* APIs
(tray item, native notifications, login item, accessory activation policy) are all macOS-only
and must sit behind `cfg(target_os = "macos")` with portable stubs elsewhere.

---

## 6. Redaction note

This document contains **no personal data** and **no verbatim decompiled code**. Account
identifiers, user names, device/serial values and absolute user paths are omitted or shown as
placeholders. German user-facing labels are quoted (they are shipped UI strings, not code).
The target app was only read (plist/strings/read-only decompilation) and was never launched,
signed, or modified. `SPEC_NOTIFICATIONS.md` is the only file created or changed.
