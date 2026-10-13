# Amazon Photos macOS Desktop App — UI Screens & Navigation (Behavioural Spec)

Clean-room description of the **user-visible** behaviour of the official Amazon Photos
desktop client for macOS (`Amazon Photos.app`, bundle `com.amazon.clouddrive.mac`,
version `11.3.0` / build `36`, `LSUIElement = true`).

This document describes **what the app does and shows**, not how it is coded. Everything
here was reconstructed from four sources:

| Source | What it gives |
|---|---|
| `Contents/MonoBundle/Amazon Photos.dll` (decompiled) | view-models, state machines, navigation intents, control wiring, programmatic window frames |
| `Contents/Resources/de.lproj/Localizable.strings` (986 entries) | the exact **German labels**; every label below cites its key |
| `Contents/Resources/*.nib` + `*.storyboardc` (interface files) | real window/view frames, control sizes, class names, base (English) strings |
| `~/Library/Application Support/Amazon Cloud Drive/*.db` + `*.json` | the persisted fields that back each state |

> **Confidence convention.** Statements marked **observed** come directly from the sources
> above (interface-file frames or programmatic frames in the shipped binary). Statements
> marked *inferred* are reasoned rather than read off a source. Where a value could not be
> determined at all it is called out under "Open questions".
>
> **How the interface files were read.** `ibtool` (Xcode 27) **refuses compiled nibs**
> (`Interface Builder cannot open compiled nibs`) and `--convert` is a *class-rename* flag,
> not a format converter, so it cannot decompile them. The two on-disk formats were handled
> directly instead:
> - **flat `NIBArchive`** files (`DriveThemedWindow.nib`, `SelectiveSyncView.nib`,
>   `UpsellView.nib`, all three `Diagnostics.storyboardc/*.nib`) — parsed with the
>   open-source `nibarchive` parser (`MAGIC "NIBArchive"`, header
>   `formatVersion=1, coderVersion=10`, then object/key/value/class-name tables).
> - **`NSKeyedArchiver` plists** inside the `Onboarding.storyboardc/*.nib` directories —
>   read with `plutil`/`Property List` (they store frames as plain strings such as
>   `{{1800, -26.5}, {880, 550}}`).
>
> **Redaction.** Account identifiers, user names, device serial numbers and absolute
> file paths are shown as placeholders (`<account-id>`, `<user-name>`, `<path>`).
> None of the interface files contain personal data.

---

## 1. App shell

- **No Dock presence by default.** The app is a menu-bar application (`LSUIElement`).
  It runs as a status item with a popover/menu; windows are opened on demand. *observed*
- **Two window families:**
  1. the **main window** (`DriveWindow`, title `clouddrive_desktop_title` → "Amazon Photos"),
     which hosts the Backup / Download / Sync tabs;
  2. a set of **separate utility windows** (Preferences, About, Activity, Onboarding,
     FTUE, SelectiveSync, Upsell, AutoConnect, Release Notes, Error, …).
- **Window title** on the main window and several dialogs is `clouddrive_desktop_title`
  ("Amazon Photos"); the Preferences window uses `clouddrive_desktop_preferences_window_title`
  ("Voreinstellungen"). *observed*
- Everything is driven by an **event/intent bus**: menu items, status-popup rows and tab
  buttons publish intents such as *show DriveWindow on screen X*, *show backup
  configuration*, *pause all backups*, *retry errored uploads*. Navigation therefore has
  many redundant entry points into the same screens. *observed*

### 1.1 Main window vs. status popup

The same three transfer domains (backup, download, sync) appear in two places:

- the **Status Popup** (menu-bar) — a compact 3-row summary plus a notifications list;
- the **main DriveWindow** — the full tabbed working surface.

Clicking a status-popup row navigates the main window to the matching tab, and the row's
call-to-action can be *start backup* or *retry errored transfers*. *observed*

---

## 2. Screen inventory

| # | Screen / window | Purpose | Primary entry points |
|---|---|---|---|
| 1 | **Main window** `DriveWindow` | Tabbed working surface | Status item, status-popup row, nav-menu item, dock icon, drag-drop, autoplay, upsell modal, file-name-conflict notification |
| 2 | **Backup tab** (`Screen.Backup`) | Scheduled + one-time backups, progress, pause | Nav menu "Datensicherung", header "Hochladen", status-popup backup row, FTUE completion, upsell |
| 3 | **Download tab** (`Screen.Restore`) | Download cloud folders/albums without sync | Nav menu "Herunterladen", header download button, status-popup download row |
| 4 | **Sync tab** (`Screen.Sync`) | Classic two-way sync status (legacy) | Nav menu "Synchronisieren" (only when sync is enabled) |
| 5 | **Folder picker** (`Screen.FolderPicker`) | Choose cloud folders to download/upload | Download tab "Ordner auswählen…", FTUE bulk-download |
| 6 | **Backup configuration** window/sheet | Create/edit a scheduled backup (source, destination, filter, dedupe, conflict rule) | "Datensicherung hinzufügen …", "Details anzeigen"→Edit, AutoConnect |
| 7 | **Preferences** window | 6–7 preference tabs | Nav menu "Einstellungen", app menu "Voreinstellungen" |
| 8 | **Onboarding** window | Legacy post-login education + sync setup | First run after login, `OnboardingVersion < 2` |
| 9 | **FTUE** window | First-time "get started" wizard | First run, "Loslegen"/Get started, upsell trigger |
| 10 | **Upsell** modal | Storage upsell (out of space) | Quota events, FTUE page, home tab "your plan" |
| 11 | **Status popup** | Menu-bar summary + notifications | Menu-bar status item |
| 12 | **Activity** window | "Uploads in progress" list | Status item during active uploads |
| 13 | **Selective Sync** window | Which cloud folders sync locally | Onboarding, Preferences sync tab |
| 14 | **AutoConnect** prompt | Offer backup for a newly attached device | Device arrival, when AutoConnect is on |
| 15 | **About** window | Version, links, send logs | App menu "Mehr dazu" |
| 16 | **Release Notes** window | What's new | App menu "Versionshinweise" |
| 17 | **Error** window | Per-item upload errors | Error banner "Fehler beim Hochladen anzeigen" |
| 18 | **Failed Items** window | Retry failed sync items | Sync tab |
| 19 | **File-name conflict** overlay/sheet | Resolve name collisions | Upload conflict, notification |
| 20 | **Startup / splash** | "App wird geladen" + greeting | App launch |
| 21 | **Campaign / notification** window | In-app account messages | Server push |
| 22 | **Diagnostics** window | Support diagnostics | Support menu |
| 23 | **Sign-in / login** window | Amazon sign-in web flow | First run, sign-out |
| 24 | **Installer stub** window | Download/install/update of the real app | Running the downloaded installer |

Screens 12–24 are auxiliary; the four "main" surfaces are #2, #3, #4 and #7.

### 2.1 Observed window dimensions

Every window frame below is read either from a shipped interface file or from the
window-construction site in the binary. All values are in points (width × height).
*observed*

| Window | Size | Where the value comes from |
|---|---|---|
| **Main window** (`DriveWindow`) | **1130 × 770** | programmatic `WindowFrame`; `StyleMask = Titled│Closable│Miniaturizable│Resizable` |
| Login / auth portal | 880 × 530 | programmatic (`AuthPortalWindowSize`); inner container 380 × 528 at (500, 0) |
| **Onboarding** (storyboard window) | **880 × 550** | `Onboarding.storyboardc/NSWindowController-…nib` → `NSWindowRect = {{1800, -26.5}, {880, 550}}` |
| **Diagnostics** (storyboard window) | **800 × 600** | `Diagnostics.storyboardc/NSWindowController-…nib` (window and its view nib are both 800 × 600) |
| Preferences | 700 × 600 | programmatic; `StyleMask = Titled│Closable` |
| FTUE | 1000 × 700 (service default) → **650 × 502** on the Upsell page | programmatic (`FtueWindowWidth/Height`, then a resize) |
| Error window | 600 × 400 | programmatic `WindowFrame` |
| Upsell modal | 650 × 433 | programmatic, borderless `NSPanel`, corner radius 10 |
| Activity | 550 × 391 | programmatic `WindowFrame` |
| AutoConnect | 540 × 360 | programmatic `WindowFrame` |
| Campaign / notification | 850 × 595 | programmatic |
| About | 504 × 410 | programmatic |
| Startup / splash | 640 × 360 | programmatic |
| Release Notes | 370 × 382 | programmatic |
| Selective Sync | view **660 × 396**; code sets an initial 600 × 300 | see `UI_STATES.md` §5.6 |
| "New cloud folder" dialog | 540 × 240 | programmatic |
| Main-window *theme* nib | **0 × 0** | `DriveThemedWindow.nib` `NSWindowRect = {{640, 389}, {0, 0}}` — the nib defines only an empty themed shell; the real size is applied at runtime |

Main-window chrome constants (programmatic): header **64 pt** tall, growing to **104 pt**
while a large-scale-event banner is shown (banner itself 40 pt); footer **60 pt**; outer
margin **8 pt**; window corner radius **16 pt**. The left navigation menu spans from the
header's bottom edge to the footer's top edge. *observed*

**Selective Sync — nib vs. window mismatch.** `SelectiveSyncView.nib` lays the view out at
**660 × 396 pt** (content `{{0, 55}, {660, 278}}`, outline/table `{{1, 1}, {658, 276}}`,
bottom bar `{{1, 433}, {658, 16}}`), while the window service first sets a
**600 × 300 pt** frame. Both numbers are *observed*; which one wins (assigning the content
view can resize the window to fit, which would make the effective size 660 × 396) is
*inferred* and not resolved here.

**Diagnostics.** Both the window nib (800 × 600) and its view nib contain the same
**800 × 600** frame; the table has the columns **Timestamp, Name, Value, ReceivedEvents**,
bound to `self.viewModel.eventsArray`. *observed*

---

## 3. Main window (`DriveWindow`)

### 3.1 Layout (top → bottom) — geometry *observed*, labels *observed*

The main window is **1130 × 770 pt**. Its `DriveThemedWindow.nib` contains **no geometry at
all** (window rect `{{640, 389}, {0, 0}}`), so the shell — header, nav menu, content, footer
— is laid out programmatically. Measured constants from that code:

1. **Header bar** (`HeaderView`, **64 pt** tall; **104 pt** while a banner is shown)
   - Amazon Photos logo (left, **60 × 32 pt**).
   - Primary nav buttons (centre-left), each **144 pt** wide, 15 pt label, 2 pt apart,
     40 pt after the logo: **"Hochladen"** (`clouddrive_desktop_clouddrivepicker_uploadbutton`) → Backup tab; **"Herunterladen"** (`clouddrive_desktop_contextmenu_opendownloadstab`) → Download tab; **"Synchronisieren"** (`clouddrive_desktop_contextmenu_opensynctab`) → Sync tab, **hidden unless sync is enabled**.
   - **"Fotos im Web öffnen"** (`clouddrive_desktop_menu_open_photos_on_web_button`), then a 1 × 16 pt divider (16 pt gap) and the account/avatar button on the right (16 pt margins).
   - A **4 pt** accent-coloured bar marks the active header button.
2. **Left navigation menu** (`MenuViewModel`, collapsible) — spans from the header's bottom
   edge to the footer's top edge, left-aligned to the window.
   - Items: **"Datensicherung"** (`clouddrive_desktop_navigationMenu_backup`), **"Herunterladen"** (`clouddrive_desktop_navigationMenu_download`), **"Einstellungen"** (`clouddrive_desktop_navigationMenu_setttings`), plus **"Synchronisieren"** (`clouddrive_desktop_navigationMenu_sync`) inserted only when sync is enabled.
   - The menu can be collapsed/expanded (collapsed = icon-only, ≥ a minimum width); the
     collapsed state is persisted per user (`ShouldCollapseNavigationMenu`). *observed*
   - The strings file also contains **"Startseite"** (`clouddrive_desktop_navigationMenu_home`)
     and **"Intelligenter Scan"** (`clouddrive_desktop_navigationMenu_smartscan`); no menu
     item is constructed from them in this build → *inferred legacy / unused*.
3. **Content area** — the active tab's view.
4. **Optional banners** stacked above content, each **40 pt** tall: Large-Scale-Event outage
   banner (`clouddrive_desktop_outage_banner_text`), Sync-deprecation banner
   (`clouddrive_desktop_syncdeprecationbanner_header` / `…_description`), Sync→Backup
   migration banner (`clouddrive_desktop_sync_to_backup_migration_banner_*`). *observed*
5. **Footer** (`FooterView`, **60 pt** tall)
   - **Storage usage** block (left, **337 × 60 pt**): "Ihr Speicherplatz" (`…_youraccount_yourstoragetitle`),
     value "{1} von {2} belegt" (`…_youraccount_usageformat`), and an **"Upgrade jetzt vornehmen"**
     hyperlink (`…_youraccount_upgradenow_buttontitle`) → opens storage management in the browser. *observed*
   - **Remaining-files + time-remaining** text (`UploadsView`, **78 pt** tall, ≥ 150 pt wide, e.g. `…_backup_remainingcount`
     "Elemente werden gesichert, ({0:n0} verbleibend)" and `…_backup_remainingtimeandbytes`). *observed*
   - **Global "Alle Sicherungsvorgänge anhalten" / "… fortsetzen" button** (**40 pt** tall,
     15 pt label), 15 pt left of the uploads text — the
     global backup pause toggle. **Hidden when there are no scheduled backups with items.** *observed*

### 3.2 Tab semantics

The main window has an active "screen" with four values: **Backup**, **Restore**
(the Download tab), **Sync**, **FolderPicker**. `FolderPicker` is not a real tab: selecting
it activates the Download tab and opens the folder picker on top. *observed*

Only one tab view-model is "selected" at a time; leaving a tab suspends its transfer-item
loading, returning resumes it. *observed*

### 3.3 Nav menu collapse

The nav menu has an expand/collapse command; collapsing hides the text and shows icons
only (`MenuItemViewModel.Collapsed`). There is also a legacy overflow/context menu
(see §9) with the commands **"Datensicherung"**, **"Herunterladen"**, **"Synchronisieren"**,
**"Uploads"**, **"Startseite"**, **"Voreinstellungen"**, **"Status anzeigen"**, **"Hilfe"**,
**"Mehr dazu"**, **"Versionshinweise"**, **"Ihre Fotos"**, **"Meine Amazon Photos"** and
**"Amazon Photos beenden"** (`clouddrive_desktop_contextmenu_*`). *observed labels*

---

## 4. Backup tab — flows

### 4.1 Purpose

Show all backups (scheduled "Automatisch gesicherte Ordner" + a single one-time backup
bucket), each with live status, progress, pause/resume, "Scan now", and a details view.

### 4.2 Entry points

Nav menu "Datensicherung"; header "Hochladen"; the status-popup backup row; the nav-menu
overflow item; FTUE completion; the upsell modal; drag-and-drop onto the window; default
screen of the main window. *observed*

### 4.3 Structure

- **Left panel** (`BackupsPanelView`): header **"Cloud-Backups"**
  (`clouddrive_desktop_cloud_backups_title`), a **list** of backup items, and a
  **drag-and-drop drop zone** ("Ordner per Drag-and-drop verschieben oder
  Backup hinzufügen" `/clouddrive_desktop_drag_and_drop_*`). *observed*
- **Right side**: the selected backup's **detail** view.
- **Empty state** (`EmptyStateView`, when no backups exist):
  **"Verlieren Sie nie wieder ein Foto oder eine Datei"**
  (`clouddrive_desktop_statuswindow_backup_emptytab_title`), subtitle
  **"Einen Ordner zur automatischen Datensicherung festlegen"**
  (`…_backup_emptytab_subtitle`), button **"Ordner auswählen …"**
  (`…_backup_emptytab_choosefolderbuttontitle`) and the alternative row
  **"oder"** + **"Hochladen..."** (`…_backup_emptytab_ortitle`, `…_statuswindow_browsetouploadbuttontitle`). *observed*

### 4.4 Adding a backup ("Welche Art von Sicherung?")

A split-button/segmented choice (`clouddrive_desktop_statuswindow_splitbutton_*`):

- **"Einmalig"** (`…_splitbutton_backuponce`) → one-time backup: files/folders are
  added to an ad-hoc queue (drag-and-drop or folder/file picker); appears as a **single**
  row titled **"Einmalige Sicherung"** (`clouddrive_desktop_backup_onetimebackupitem_title`).
- **"Plan"** (`…_splitbutton_scheduledbackup`) → opens the **Backup configuration** window.

The tab header has **"Datensicherung hinzufügen ..."**
(`clouddrive_desktop_statuswindow_scheduletab_mainview_addschedulebuttontitle`) and, when
relevant, a global **"Alle Sicherungsvorgänge anhalten"**
(`clouddrive_desktop_backupscreen_pauseallbackups`) toggle. *observed*

### 4.5 Scheduled-backup configuration window

Title **"Datensicherungseinstellungen"** (`clouddrive_desktop_backupconfiguration_windowtitle`).
Fields, top → bottom (**observed**):

| Control | German label (key) |
|---|---|
| Header | "Datensicherung" style header (`…_preferences_backup_title`) / mode-dependent |
| Source folder | "Gesicherter Ordner:" (`…_backupconfiguration_backupfolderpathtitle`) + "Ändern..." (`…_backupconfiguration_selectbackupfolder_button`) |
| Destination | "Ziel für Datensicherung:" (`…_backupconfiguration_backupdestinationtitle`) + "Ändern..." (`…_backupconfiguration_selectbackupdestination_button`) |
| Content | "Was soll gesichert werden:" (`…_backupconfiguration_selectcontentfiltertitle`) — segmented **Alles / Fotos und Videos / Fotos** |
| Upload cadence | "Änderungen hochladen:" (`…_backupconfiguration_selectintervaltitle`) |
| Weekly schedule | "… aktiviert" (`…_selectinterval_dayofweek_title`) + "um" (`…_selectinterval_time_title`) |
| Dedupe | "Duplikate vermeiden: Keine Fotos und Videos sichern, die sich bereits in meinem Konto befinden" (`…_statuswindow_scheduleconfiguration_dedupeoption`) |
| Name conflicts | "Umgang mit Dateinamenkonflikten:" (`…_backupconfiguration_selectfilenameconflictresolutiontitle`) — radio group |
| Footer | "Speichern" (`clouddrive_desktop_statuswindow_scheduleconfiguration_savebuttontitle`) and "Abbrechen" (`clouddrive_desktop_cancel`) |

Facts about this window (**observed**):

- The **source** field is read-only when editing an existing backup; it is editable only
  when creating a new one (`CanChangeSource`). The destination is always changeable.
- The window can be opened in three modes: **Create**, **Edit**, **AutoConnect**.
- **Validation before save:** missing local path ("Lokaler Pfad ist nicht verfügbar"),
  missing destination ("Zielknoten ist nicht verfügbar"), name conflict with a root folder
  ("Namenskonflikt mit Stammordner") or an already-existing schedule for the same local
  path ("Datensicherung für lokalen Pfad existiert bereits"). *observed*
- When the **upload interval changes while items are queued**, a confirmation dialog
  appears ("Elemente in der Warteschlange",
  `…_backupintervalchanged_dialogtitle`) offering **"Jetzt hochladen"**
  (`…_uploadimmediately`) or **"Später"** (`…_uploadlater`), with the message
  "Nach dem neuen Plan werden Elemente in der Warteschlange um {0} auf {1} hochgeladen."
  (`…_dialogmessage_nextuploadtime`). *observed*
- Folder-selection guards: overlapping with an existing backup
  (`…_schedules_overlappingfolder_warningmessage`), restricted path
  (`…_schedules_restrictedfolder_warningmessage`), root folder
  (`…_schedules_rootfolder_warningmessage`), sync folder
  (`…_schedules_syncfolder_warningmessage`), or child/parent of an existing backup
  (`clouddrive_desktop_ftue_child_folder_warning`, `…_ftue_parent_folder_warning_*`). *observed*

### 4.6 Backup item row (list)

A scheduled row shows (**observed**):

- folder icon (different icon when the interval is "Sofort"),
- folder **name** (the last path component; for an external drive, the drive's display name),
- a **status line** (see §11),
- an optional **"Zuletzt aktualisiert: {0}"** line
  (`clouddrive_desktop_scheduledbackupitem_lastupdatedtime`),
- an inline **pause/resume** icon button and a **refresh / "Jetzt scannen und hochladen"**
  icon,
- a **context menu** (`ScheduledBackupTooltipView` + menu):

| Menu item | German label (key) |
|---|---|
| Scan now | "Jetzt scannen und hochladen" (`…_scheduletab_scannowbutton`) |
| Pause | "Datensicherung anhalten" (`…_scheduletab_pauseschedule`) |
| Resume | "Datensicherung fortsetzen" (`…_scheduletab_resumeschedule`) |
| Retry failed | "Fehler erneut versuchen" (retry group, `clouddrive_desktop_retryall`) |
| Reveal | "Im Finder anzeigen" (`…_scheduletab_revealinfinder`) |
| Details | "Details anzeigen" (`…_scheduletab_viewdetails_button`) |
| Edit | "Bearbeiten" (`…_scheduletab_editbutton`) |
| Remove | "Datensicherung stoppen" (`…_scheduletab_removebutton`) |

Hover tooltips: "Zurück zu {path}"-style (`…_schedules_item_tooltip_backupto`),
"{0} von {1} Elemente hochgeladen..." (`…_schedules_item_tooltip_items_uploaded`),
"Der automatische Upload ist aktiviert/deaktiviert"
(`…_schedules_item_tooltip_autoupload_on` / `…_off`). *observed*

**Removing** a backup opens a confirmation: title
"Soll die Sicherung dieses Ordners gestoppt werden?"
(`…_scheduletab_removebuttonwarningtitle`), body
"Alle zuvor hochgeladenen Inhalte bleiben sicher in Amazon Photos gespeichert."
(`…_scheduletab_removebuttonwarningmessage`). *observed*

### 4.7 Details view (nested vs flat)

Selecting a backup shows its transfer items. Two display modes, toggled by two icon
buttons in the details header:

- **Nested / drill-down** — tooltip "Verschachtelte Anzeige"
  (`clouddrive_desktop_backupdetailsview_drilldown`), with a breadcrumb navigation stack
  (folder path, parent-nav caret, overflow button).
- **Flat list** — tooltip "Flache Listenanzeige"
  (`clouddrive_desktop_backupdetailsview_flatlist`).

Above the item table sits an **Auto-Upload toggle** ("Neue Dateien automatisch
hochladen", `clouddrive_desktop_auto_upload_toggle_text`) with sub-text
"Zukünftige Dateien oder Dateiänderungen werden automatisch als neue Dateien in der Cloud
hochgeladen" (`…_auto_upload_toggle_subtitle_text`) and a help tooltip
(`…_auto_upload_toggle_tooltip_text`). *observed*

Details footer: an **error banner** (`…_errorbannerview_uploaderror` /
`…_errorbannerview_uploaderrorplural`) and a **"Wiederholen"** button
(`clouddrive_desktop_login_ui_retry_button`) that is enabled only when retries are possible. *observed*

Item table columns (from `ScheduledBackupTooltipView`/table code and strings):
**"Name"** (`…_statuswindow_filenamecolumntitle`), **"Status"**
(`…_statuswindow_statuscolumntitle`), **"Größe"** (`…_statuswindow_sizecolumntitle`),
**"Letzte Aktualisierung"** (`…_statuswindow_lastmodifiedcolumntitle`). *observed*

### 4.8 Drag-and-drop flow

Dragging files/folders onto the window shows an overlay
("FOTOS PER DRAG & DROP VERSCHIEBEN, UM SIE HOCHZULADEN",
`clouddrive_desktop_drivewindow_dragdropoverlay`; "Zur einmaligen Sicherung mit der Maus
hierher ziehen", `…_statuswindow_draganddropoverlay_message`) and then a scanning state
("Dateien in Bearbeitung ...", `clouddrive_desktop_dragdrop_scanningfiles`). Dropped items
go to the **one-time backup** queue by default; a destination picker can choose a cloud
folder ("Cloud-Pfad: /All/{0}", `clouddrive_desktop_dragdrop_defaultfolder_path`). *observed*

---

## 5. Download tab (`Screen.Restore`)

### 5.1 Purpose

Download cloud folders/albums to local disk **without** turning on two-way sync.

### 5.2 Entry points

Nav menu "Herunterladen"; header download button; status-popup download row; FTUE
"Ordner herunterladen". *observed*

### 5.3 Controls (all **observed**)

- Empty state: **"Alle Ihre Amazon Photos herunterladen ({0})"**
  (`…_statuswindow_emptystate_downloadmessage`) or `…_downloadmessage_nosize`, plus
  **"Ordner auswählen..."** (`…_emptystate_selectfoldersbuttontitle`) and
  **"Herunterladen..."** (`…_emptystate_downloadbuttontitle`).
- **Folder/album picker** (`FolderPickerViewController` / `AlbumPickerViewController`):
  - header "Wählen Sie Ordner oder Alben zum Herunterladen ohne Synchronisierung aus."
    (`clouddrive_desktop_restore_header`) and
    "Wählen Sie Ordner zum Herunterladen ohne Synchronisierung aus."
    (`…_clouddrivepicker_download_selectfoldermessage`);
  - root folder shown as **"Alle"** (`…_clouddrivepicker_rootfoldername`);
  - actions **"Auswählen"** (`…_selectbutton`), **"Herunterladen nach..."**
    (`…_downloadbutton`), **"Neuen Ordner erstellen"** (`…_newfolderbutton`),
    **"Ordner aktualisieren"** (`…_refreshbutton`);
  - album-only extras: **"Nach Album suchen"** (`…_restore_albumsearch`),
    **"Album herunterladen(s)"** (`…_restore_downloadalbum`);
  - a **"Nur Dateien herunterladen, die keine Fotos und keine Videos sind"** checkbox
    (`…_clouddrivepicker_onlynonmedia`).
- Destination picker: **"Download-Ziel auswählen"** (`…_selectdownloadlocation`).
- Download list rows use the same status vocabulary as uploads (Warten / In Bearbeitung /
  Abgeschlossen / Fehlgeschlagen / Abgebrochen / Verworfen / Wird heruntergeladen). *observed*
- Guards: downloading into the sync folder is refused
  ("Bitte wählen Sie einen anderen Ordner aus." / "Das Herunterladen von Inhalten in
  deinen Synchronisierungsordner …", `…_downloadnotallowedinsyncfolderheader` / `…body`);
  insufficient disk space warns ("Nicht genügend Speicherplatz",
  `…_downloadnotenoughspacewarning_title`). *observed*
- Batch history can be cleared: "Verlauf löschen?" (`…_clearcompletedconfirmationheader`),
  "Dadurch wird die Liste der vergangenen Downloads gelöscht."
  (`…_downloads_clearcompletedconfirmationtextmessage`). *observed*

---

## 6. Sync tab (`Screen.Sync`) — legacy

Present only when sync is enabled for the account (the app is migrating users from Sync to
Backup; see the deprecation/migration banners). *observed*

- Shows sync progress ("Synchronisierung läuft ({0} Elemente verbleibend)",
  `…_syncwindow_syncingitems_plural`), a **"Synchronisierung angehalten/fortsetzen"**
  toggle (`…_syncwindow_pausesync` / `…_resumesync`), **"Einstellungen"**
  (`clouddrive_desktop_syncwindow_settings`) and **"Kürzlich geändert"**
  (`…_syncwindow_recentchanged`). *observed*
- Special messages: "Keine Internetverbindung." (`…_syncwindow_nonetwork`), "Der Speicher
  Ihres Kontos ist aufgebraucht." (`…_syncwindow_specialmessage_overquota`), service
  unavailable (`…_syncwindow_specialmessage_serviceunavailable`). *observed*
- Failed items open the **"Fehlgeschlagene Elemente"** window
  (`clouddrive_desktop_failedsyncitemswindow_title`) with **"Wiederholen"**
  (`…_failedsyncitemswindow_retry`). *observed*

---

## 7. Preferences window

Title **"Voreinstellungen"** (`clouddrive_desktop_preferences_window_title`). The window is
**modal-ish with an explicit Apply/OK**: closing with unsaved changes warns
("Voreinstellungen schließen?" / "Alle ungespeicherte Änderungen gehen verloren.",
`…_preferences_window_close_warning_dialog_title` / `…_message`). Footer buttons
**"Übernehmen"** (`…_preferences_applyButton`) and **"OK"** (`…_preferences_okButton`). *observed*

Tabs observed (labels from `clouddrive_desktop_preferences_*`):

| Tab | German label (key) | Contents (see `UI_STATES.md` §5) |
|---|---|---|
| General | "Allgemeines" (`…_general_title`) | auto-start, notifications, notification badges, hidden files |
| Account | "Konto" (`…_account_title`) | "Angemeldet als:" (`…_signed_in_title`), "Abmelden" (`…_signout`), "Speicher verwalten" (`…_manageStorage`) |
| Backup | "Datensicherung" (`…_backup_title`) | AutoConnect toggle, "Datensicherung zurücksetzen" (`…_backup_reset_button_text`), dedupe options, iOS-duplicate filter |
| Bandwidth | "Bandbreite" (`…_bandwidth_title`) | concurrency sliders + per-app rate limits |
| Sync | "Synchronisieren" (`…_sync_title`) | sync enable/disable, selective-sync folder picker, "new top-level cloud folders" behaviour |
| Proxy | "Proxy" (`…_proxy_title`) | No-proxy / Auto-detect / Manual radio + server/port |
| Advanced / Diagnostics | "Erweitert" (`…_advanced_title`) | "Diagnosen:" (`…_diagnostics_section_title`), "Protokolle senden" (`…_send_logs_button_text`) |

Radio/checkbox labels (**observed**): "Nicht begrenzen" (`…_bandwidth_dont_limit_title`),
"Begrenzen auf" (`…_bandwidth_limit_to_title`), "KB/Sekunde" (`…_bandwidth_rate`),
"Gleichzeitige Downloads:" (`…_bandwidth_downloads_slider_title`), "Gleichzeitige Uploads:"
(`…_bandwidth_uploads_slider_title`), "Download-Rate pro Datei" (`…_download_rate_title`),
"Upload-Rate pro Datei" (`…_upload_rate_title`), "Automatische Erkennung"
(`…_proxy_autodetect`), "Manuell" (`…_proxy_manual`), "Kein Proxy" (`…_proxy_noproxy`),
"Server"/"Port" (`…_proxy_serverlabel` / `…_proxy_portlabel`), "Amazon Photos beim Starten
öffnen" (`…_auto_start_application`), "Benachrichtigungen anzeigen"
(`…_enable_notifications`), "Aktivieren Sie Badges für Benachrichtigungen"
(`…_enablenotificationbadges`), "Keine ausgeblendeten Dateien hochladen"
(`…_excludehiddenfiles`).

Sign-out flow: "Wollen Sie sich von diesem Gerät abmelden?"
(`…_signout_dialog_title`) with body "Der Inhalt bleibt auf diesem Computer und wird nicht
mehr mit Amazon synchronisiert." (`clouddrive_desktop_signout_message`). *observed*

Reset flow: "Datensicherung zurücksetzen?" (`…_backup_resetconfirmationheader`) /
"Sind Sie sicher, dass Sie alle Datensicherungseinstellungen löschen wollen?"
(`…_backup_resetconfirmationtextmessage`). *observed*

---

## 8. Onboarding & FTUE

There are **two independent first-run systems** in this build. *observed*

### 8.1 Legacy Onboarding window (workflow-based)

States: **Education → SetupSync → SetupComplete → Completed**. *observed*

**Geometry (observed from `Onboarding.storyboardc`).** The storyboard's window controller
sets `NSWindowRect = {{1800, -26.5}, {880, 550}}` → the Onboarding window is
**880 × 550 pt** (design-time screen `{{1280, -280}, {1920, 1057}}`). The scenes are
`SetupSync` and `SetupComplete`; the content view controller is
`OnboardingSetUpSyncViewController`; the storyboard carries outlets `window`, `delegate`
and `contentViewController`. The window title is **empty in the storyboard** and is set at
runtime (`clouddrive_desktop_onboarding_setupcomplete_title`). The two view nibs:

- SetupSync view (`JUH-K4-qEi-view-0UK-Yj-1Em`), classes `NSView` / `NSTextField` /
  `NSButton` (`OnboardingButton` extends `DriveButton`). Sub-controls at
  `{{122, 450}, {637, 36}}`, `{{366, 504}, {148, 21}}`, `{{263, 225}, {355, 100}}`,
  `{{172, 57}, {220, 23}}`, `{{53, 25}, {111, 16}}`, `{{26, 60}, {138, 16}}`,
  `{{169, 19}, {157, 25}}`, `{{418, 30}, {44, 35}}`.
- SetupComplete view (`t2L-1P-099-view-Cbs-jj-AZK`), root **880 × 550**, classes `NSView` /
  `NSTextField` / `NSButton` / `NSImageView`. A **panel `{{90, 161}, {700, 228}}`** and a
  **two-column list of four 249 × 17 pt rows** each, at x = 396 (`y = 42, 77, 135, 187`) and
  x = 475 (`y = 11, 71, 129, 187`) — i.e. the "what to do next" list is rendered as two
  columns of four rows. A `{{0, -34}, {382, 274}}` element sits above.

The storyboard stores **English base strings** (e.g. "Open your sync folder", "Preferences
and other options", "Open your Amazon Drive online", "Backup settings", "Backup and sync
settings"), which are replaced at runtime from `Localizable.strings`. *observed*

- **Education** — "Zwei Möglichkeiten zur Nutzung von Amazon Photos"
  (`clouddrive_desktop_onboarding_education_title`) explaining **"Datensicherungen planen"**
  (`…_education_schedules_title`; "Beim Löschen von Inhalten in der Cloud werden diese NICHT
  lokal gelöscht und umgekehrt", `…_education_schedules_info`) vs. **"Ordner synchronisieren"**
  (`…_education_sync_title`; "Beim Löschen von Inhalten in der Cloud werden diese lokal
  gelöscht und umgekehrt", `…_education_sync_info`). Buttons **"Weiter"**
  (`clouddrive_desktop_onboarding_next_button`) and **"Nicht jetzt"**
  (`…_onboarding_skip_button`).
- **SetupSync** — "Ordner synchronisieren"
  (`clouddrive_desktop_onboarding_setupsync_title`): checkbox **"Synchronisation für
  Amazon Photos aktivieren"** (`…_setupsync_enablesync_checkbox_title`), local sync folder
  ("Lokaler Synchronisierungsordner", `…_choose_sync_folder_title`) and cloud folders
  ("Amazon Photos-Ordner:", `…_choose_amazondrive_folders_title`), plus
  **"Synchronisierung starten"** (`clouddrive_desktop_onboarding_startsync`).
  Guard dialogs: sync folder inside another account
  (`…_folderconflict_warningheader`), outside the home directory
  (`…_invalidsyncfolder_header`), overlapping with a backup
  (`…_syncfolder_overlappingwithautosave_header`), unsupported location
  (`…_foldernotallowed_warningmessage`).
- **SetupComplete** — "Einrichtung abgeschlossen"
  (`clouddrive_desktop_onboarding_setupcomplete_title`): a short list of what to do next,
  labelled by mode:
  - backup mode: "Einstellungen für Datensicherung und Synchronisierung"
    (`…_backupmode_setupcomplete_backupsettings`), "Amazon Photos online öffnen"
    (`…_backupmode_setupcomplete_openamazondriveweb`), "Präferenzen und andere Optionen"
    (`…_backupmode_setupcomplete_otheroptions`);
  - all mode: "Datensicherungseinstellungen" / "Ihren Synchronisierungsordner öffnen" /
    "Amazon Photos online öffnen" / "Präferenzen und andere Optionen"
    (`…_allmode_setupcomplete_*`).

Versioning: the onboarding is stored as `OnboardingVersion` in the per-account
`settings.json`; the current onboarding version is **2**, so accounts already at 2 skip
this flow. *observed*

### 8.2 FTUE window ("get started")

Pages: **Welcome → ConfigureAnyFolders → (Upsell) / BulkDownload**. *observed*

- **Welcome** (`clouddrive_desktop_ftuewindow_welcomepage_header` → "Willkommen bei
  Amazon Photos"): buttons **"Weiter"** (`…_welcomepage_next`)
  and **"Einrichtung überspringen"** (`…_welcomepage_skipsetup`). A secondary link
  **"Erfahre mehr über das Herunterladen von Fotoordnern"**
  (`…_welcomepage_bulkdownload`) leads to the bulk-download page.
- **ConfigureAnyFolders** — "Wähle die Ordner aus, die du bei Amazon Photos hochladen
  möchtest" (`…_configureanyfolderspage_header`); suggested folders are "Bilder" and
  "Filme" on macOS (`…_configureanyfolderspage_subheader_mac`); **"Weitere Ordner
  hinzufügen"** (`…_addmorefolders`) and **"Keine Ordner ausgewählt"**
  (`…_nofoldersselected`). Primary action is **"Weiter"** when the account has no plan
  (then → Upsell page) or **"Fertig"** (`…_configuredefaultfolderspage_done`) otherwise.
- **BulkDownload** — "Deine Amazon Photos-Ordner herunterladen"
  (`…_ftuewindow_bulkdownload_header`); primary **"Ordner herunterladen"**
  (`…_download_folders`) → closes FTUE and opens the **folder picker**; secondary
  **"Mit dem Upload fortfahren"** (`…_continue_to_upload`) → back to Welcome.
- **Upsell** — see §10.

The other FTUE strings ("Kurzeinführung", "Schützen Sie Ihre Fotos", "Erstellen und
drucken Sie Fotos", "Zeigen Sie Ihre Fotos auf dem Smartphone an", …,
`clouddrive_desktop_ftuewindow_*`) belong to an alternate/legacy tour layout; *inferred*.

Versioning: `FtueVersion` in the per-account `settings.json`. *observed*

---

## 9. Menu-bar status popup & overflow menu

### 9.1 Status popup

Opened from the menu-bar status item. Header shows the Amazon Photos logo, a home/backup
shortcut button and the account avatar (which opens the app/overflow menu). *observed*

Content:

- **Three state rows** (Backup / Download / Sync), each with an icon, a status text and a
  call-to-action link:
  - Backup: "Sicherung wird durchgeführt" (`clouddrive_desktop_statuspopup_backingup`),
    "Sicherung angehalten" (`…_backuppaused`), "Datensicherung abgeschlossen"
    (`…_backup_complete`);
  - Download: "Wird heruntergeladen" (`…_statuspopup_downloading`),
    "Download abgeschlossen" (`…_statuspopup_downloadcomplete`);
  - Sync: "Wird synchronisiert" (`…_statuspopup_syncing`),
    "Synchronisierung abgeschlossen" (`…_statuspopup_synccomplete`).
  - Stats suffix: "({0:n0} verbleibend)" (`…_statuspopup_stats_remaining`),
    "({0:n0} Fehler)" (`…_statuspopup_stats_errors`),
    "({0:n0} verbleibende und {1:n0} Fehler)" (`…_statuspopup_stats_remaininganderrors`).
  - Call-to-action: **"Sicherung starten"** when there is nothing to transfer (opens the
    backup configuration) or **retry errored transfers** when there are failures.
- **Notifications** list ("Benachrichtigungen", `…_statuspopup_notifications`;
  empty state "Keine neuen Benachrichtigungen", `…_statuspopup_nonotifications`).
  Notification kinds observed: near/over quota
  (`…_notification_nearquota` / `…_notification_overquota`), update available
  (`…_notification_updateavailable`, "Jetzt aktualisieren"), "Erinnerungen von …"
  this-day (`…_notification_thisdaytext`), campaign message
  (`clouddrive_desktop_notification_campaignwindow_*`), sync mass-delete
  (`…_notification_syncmassdeletes_*`), drag-drop started, media import.
- Hover status strings: "Amazon Photos - {0}" (`…_contextmenu_hover_status_message`),
  with variants "Angehalten" (`…_message_uploadpaused`), "Upload-Vorgang läuft ({0} Dateien
  verbleibend)" (`…_message_uploadremaining`), "Upload abgeschlossen"
  (`…_message_uptodate`), "Netzwerk nicht verfügbar" (`…_message_nonetwork`),
  "Kein Speicherplatz mehr" (`…_message_overquota`),
  "Ungelesene Benachrichtigungen" (`…_message_unreadnotifications`),
  "Konfiguration wird durchgeführt" (`…_message_getchanges`),
  "{0} Dateien konnten nicht hochgeladen werden" (`…_uploaderrored`). *observed*

- **First close** shows an explanatory notification: "Auf Amazon Photos zugreifen" /
  "Du kannst die App jederzeit über dieses Symbol in der Menüleiste aufrufen."
  (`…_mac_notification_statuswindowclosedfirsttime_title` / `…_message`). *observed*

### 9.2 Overflow / app menu

The avatar button pops the app menu, which contains the items listed in §3.3 plus
**"Hilfe" → "Help Center" / "Feedback senden"**
(`…_contextmenu_support_help`, `…_contextmenu_support_sendfeedback`), **"Synchronisierung
aktivieren/deaktivieren..."** (`…_enablesync` / `…_disablesync`) and
**"Amazon Photos beenden"/"… verlassen"** (`…_contextmenu_exit_windows` /
`clouddrive_desktop_mac_quit`). *observed*

---

## 10. Upsell

Triggered by storage pressure (`UpsellType`): **OversaturatedUploadQueue, NearQuota,
OverQuotaUpload, HomeTab**. *observed*

- **Modal** (borderless `NSPanel`, **650 × 433 pt**, corner radius 10): title
  "Dir geht der Speicherplatz aus" (`clouddrive_desktop_upsell_modal_title`), body
  "Dateien, die dein Speicherkontingent überschreiten, werden nicht hochgeladen."
  (`…_upsell_modal_description`), buttons **"Mehr Speicherplatz kaufen"**
  (`…_upsell_buy_button`) and **"Nicht jetzt"** (`…_upsell_cancel_button`). *observed*
  `UpsellView.nib` itself carries **no controls** — just one `UpsellView` instance with a
  placeholder size of 438 × 255; the modal geometry is set programmatically. *observed*
- **Inline/FTUE variant**: "Schalte mehr Speicherplatz frei!"
  (`clouddrive_desktop_upsell_title`) with "Du kannst deinen Videospeicher mit unseren
  Tarifen ab 1,99 $/Monat aufrüsten." (`…_upsell_description`). *observed*
- The buy action opens **storage management** in the browser with the session cookies
  injected. *observed*

---

## 11. Backup state machine (core)

Every backup item derives its label from a small set of inputs: the aggregated transfer
state, the global worker state (paused?), the cold-boot (startup) state, and any schedule
error. *observed*

### 11.1 Inputs

- `AggregatedTransferState`: Unknown, Preparing, InProgress, Completed, Failed,
  InProgressWithFailures (= InProgress|Failed), CompletedWithFailures (= Completed|Failed).
- `AggregatedWorkerState`: Transitioning, Running, Paused, Stopped, ForceStopped.
- `ColdBootState`: Unknown, InProgress, Complete.
- `ScheduleError`: None, LocalFolderNotFound, DestinationNodeNotAvailable,
  ExternalDriveMissing, NameConflict.
- Per-item stats: total / remaining / completed / failed counts.

### 11.2 Transition table (label shown per row) — **observed**

| Condition (precedence order, top wins) | Status text | Alert level |
|---|---|---|
| schedule error (see §11.3) | error header + sub-text | Error |
| paused **and** Failed | "ANGEHALTEN" + "{n} Fehler" | (neutral) |
| paused (global or per-backup) | **"ANGEHALTEN"** (`…_scheduletab_paused`) | Normal |
| (InProgress \| InProgressWithFailures \| Unknown) **and** cold-boot in progress | **"Warten"** (`…_statuswindow_statuswaiting`), tooltip "Das Hochladen wird vorbereitet. Dieser Vorgang kann bis zu 10 Minuten dauern." (`…_statuswaiting_upload_tooltip`) | Info |
| Completed | **"Datensicherung abgeschlossen"** (`…_statuspopup_backupcomplete`) | Success |
| CompletedWithFailures or Failed | "{n} Fehler" (`…_singular_errorsformat` / `…_plural_errorsformat`) | Error |
| InProgress | **"Upload-Vorgang läuft"** (`…_upload_statuswindow_statusuploading`) | Info |
| InProgressWithFailures | "Upload-Vorgang läuft" + "{n} Fehler" | Info |
| Preparing | **"Konfiguration wird durchgeführt"** (`…_contextmenu_message_getchanges`) | Info |
| Unknown | (empty) | Normal |

(The *one-time* backup item uses the same table but ignores the per-backup paused flag —
only the global worker pause applies.)

### 11.3 Schedule-error sub-states

| `ScheduleError` | Header | Sub-text |
|---|---|---|
| LocalFolderNotFound | "Ordnerfehler" (`…_scheduleerror_foldermissing_header`) | "Lokaler Ordner fehlt" (`…_scheduleerror_foldermissing_text`) |
| DestinationNodeNotAvailable | "Stammfehler" (`…_scheduleerror_destinationmissing_header`) | "Ziel nicht verfügbar" (`…_scheduleerror_destinationmissing_text`) |
| ExternalDriveMissing | **"GETRENNT"** (`…_backupitem_disconnected`) | "ANGEHALTEN" (`…_scheduletab_paused`) |
| NameConflict | "Benennungsfehler" (`…_scheduleerror_nameconflict_header`) | "Namenskonflikt mit Ordner" (`…_scheduleerror_nameconflict_text`) |

Additional non-fatal row states (**observed**):
- a **"Einige Ordner wurden nicht gesichert"** warning
  (`clouddrive_desktop_schedules_warningmessage_folders_not_backedup`);
- a **"Zuletzt aktualisiert: {0}"** line (`…_scheduledbackupitem_lastupdatedtime`) that is
  shown only when the last enumeration is older than **3 minutes**; in the "CX
  simplification" variant the wording is **"Wurde aktualisiert: {0}"**
  (`…_scheduledbackupitem_updatedtime`). The relative age string refreshes on a decaying
  interval (5 s / 60 s / 300 s / 3600 s depending on age).

### 11.4 Global pause toggle

Two independent pause mechanisms, both surfaced in the UI (**observed**):

1. **Global "Alle Aktivitäten anhalten / fortsetzen"**
   (`clouddrive_desktop_pause_allactivity` / `…_resume_allactivity`). When toggled the app
   posts a toast **"Alle Aktivitäten wurden angehalten"**
   (`clouddrive_desktop_globaltoggle_all_activity_paused`) or
   **"…fortgesetzt"** (`…_all_activity_unpaused`). This pauses *all* workers.
2. **Backup-only pause** ("Alle Sicherungsvorgänge anhalten/fortsetzen",
   `clouddrive_desktop_backupscreen_pauseallbackups` / `…_resumeallbackups`), tracked as
   `ScheduledBackupsPaused` on the global backup state. The backup tab header and the
   window footer both toggle this; the footer button is **hidden when the total scheduled
   item count is 0**.

While the worker state is `Transitioning`, the pause/resume controls are disabled
(pauses/resumes are not instantaneous). *observed*

### 11.5 Completion / retry

- Finished uploads are "Vollständig" (`clouddrive_desktop_complete`) — the per-item
  progress text reaches this at 100 %.
- Items that fail are counted and offered for retry: **"Alle erneut versuchen"**
  (`clouddrive_desktop_retryall`) / **"Alle fehlgeschlagenen Dateien erneut versuchen"**
  (`…_statuswindow_retryfailedfiles_button`) / **"Erneut zur Warteschlange hinzufügen"**
  (`…_statuswindow_readdtoqueue`).
- **"Verlauf löschen"** (`…_statuswindow_clearcompletedbuttontitle`) removes completed
  entries after a confirmation ("Verlauf löschen? Dadurch wird die Liste der vergangenen
  Uploads gelöscht.", `…_clearcompletedconfirmationheader` / `…_uploads_clearcompletedconfirmationtextmessage`). *observed*

---

## 12. Dialogs (cross-cutting)

| Dialog | Title / body keys |
|---|---|
| Sign out | `…_preferences_signout_dialog_title`, `clouddrive_desktop_signout_message` |
| Disable sync | `clouddrive_desktop_disablesyncconfirmation`, `…_disablesync_explanation` |
| Enable sync | `clouddrive_desktop_enablesyncconfirmation`, `…_enablesyncconfirmationextrainfo` ("Hierfür muss die Anwendung neu gestartet werden") |
| Selective-sync confirmation | `clouddrive_desktop_selectivesyncwindow_confirmationheader` / `…_confirmationbody` |
| New cloud folder | `…_clouddrivepicker_newfolderdialog_title` + error variants (name too long, invalid chars, conflict, empty) |
| File-name conflict overlay | `…_filenameconflict_overlayview_title[_plural]`, buttons "Als neue Datei(en) speichern" / "Ersetzen" / "Ignorieren", checkbox "Nicht noch einmal nach {0}-Ordner fragen" |
| File-name conflict toast | `…_filenameconflict_toast_title` + "Öffnen" |
| Apple Photos library | `clouddrive_desktop_photoslibrary_dialogtitle` / `…_dialogmessage` / `…_dialog_learnmorebutton` |
| Unsupported OS | `clouddrive_desktop_unsupported_osversion_title` / `…_message` |
| Connection failed | `clouddrive_desktop_getendpointserror_title` / `…_message` |
| Locked account | `clouddrive_desktop_lockedaccount_title` / `…_subtitle` |
| DB failure | `…_sqlite3_error_dialog_title` / `…_message` |
| Update available | `clouddrive_desktop_update_available_title` + "Update installieren" / "Vielleicht später" |
| Send logs | `…_send_logs_success_dialog_title` / `…_send_logs_failure_dialog_title` |

Most dialogs carry a **"Diese Nachricht nicht erneut anzeigen"** /
"Nicht erneut fragen" suppression checkbox (`clouddrive_desktop_donotshowmessageagain`,
`…_autoconnectwindow_dontaskagain`, `…_dialog_suppression_message`). *observed*

---

## 13. Open questions / not determined

- **`ibtool` cannot decompile compiled nibs.** Xcode 27's `ibtool` answers
  `Interface Builder cannot open compiled nibs` for every file here, and its `--convert`
  flag renames *classes* (not formats). The interface files were therefore read with the
  `nibarchive` parser (flat `NIBArchive`) and `plutil` (`NSKeyedArchiver` plists inside
  `Onboarding.storyboardc`); the concrete frames are in §2.1, §3.1, §8.1 and §10 so they
  are *observed*, not inferred.
- **Autoresizing / Auto Layout rules.** The nibs are pre-Auto-Layout documents (they store
  `NSFrame`/`NSvFlags`, not constraints), so the tables above give *design-time* frames.
  How each frame responds to a window resize is *not determined*.
- **Selective Sync 660 × 396 vs 600 × 300.** See §2.1 — both values are observed, the
  resolution is *inferred*.
- **Which onboarding system runs for which user.** Both `OnboardingWindow` (workflow) and
  `FtueWindow` exist. The switching logic (`OnboardingVersion` / `FtueVersion` /
  `IsOnboardingComplete`) was only partially reconstructed → *not determined*.
- **Home tab / "Intelligenter Scan".** Strings exist; the corresponding screens are not
  reachable in `Screen` (Backup/Restore/Sync/FolderPicker) → likely dead code. *inferred*
- **Storage-usage numbers.** The footer storage block is backed by account info; the
  per-plan formatting (`limited_withplan`, `unlimitedphotos`, `unlimitedeverything`) is
  known, but which one applies to which entitlement was not fully traced → *not
  determined*.
- **Notifications content.** The full catalogue of notification types (this-day, story,
  campaign, quota, update, mass-delete) exists, but their individual screen layouts were
  not reconstructed in detail — none of them live in the shipped nibs.
- **Most screens have no nib at all.** Only the main-window *shell*, Selective Sync, Upsell,
  Onboarding and Diagnostics have interface files; the Backup/Download/Sync tabs, the
  status popup, the preferences panes and the backup-configuration window are built purely
  in code. Their geometry is taken from the construction constants (observed) but the
  resulting layout is otherwise *inferred*.
- **Menu-bar item click behaviour** (does a left-click open the popup directly, or only the
  status window?) was not fully traced.
- **Localised locale coverage.** Only `de.lproj` was inspected; other languages were not
  reviewed (labels are assumed identical in structure).
