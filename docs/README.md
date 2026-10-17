# Dokumentation

Die Dokumentation ist in vier Bereiche gegliedert:

## Original / Recon — [`original/`](original/)

Clean-room-Specs zum Amazon-Photos-Desktop-Client (REA der .NET-Assembly + Ressourcen). Diese Dokumente beschreiben das **Original**, nicht unser Code. Sie sind die Referenz für Feature-Parität und UI-Abhängigkeit.

- [SPEC_LOGIN.md](original/SPEC_LOGIN.md) — Login-/Auth-Flow des Originals (Geräteweg, PKCE, Token).
- [SPEC_UPLOAD.md](original/SPEC_UPLOAD.md) — Upload-Protokoll des Originals (Endpunkte, Header, Dedup).
- [SPEC_DEDUP.md](original/SPEC_DEDUP.md) — Dedup-Mechanik (MD5-Identität, UploadType, Konflikt-Policy).
- [SPEC_DUPLICATES.md](original/SPEC_DUPLICATES.md) — Duplikat-Erkennung und 409-Verhalten.
- [SPEC_CONFLICT_OPTIONS.md](original/SPEC_CONFLICT_OPTIONS.md) — Konflikt-Optionen (IGNORIEREN/RENAME/OVERWRITE/ASK).
- [SPEC_FILENAME_ENCODING.md](original/SPEC_FILENAME_ENCODING.md) — Dateinamens-Kodierung (NFC/NFD, Server-Verhalten).
- [SPEC_CAPTURE_DATE.md](original/SPEC_CAPTURE_DATE.md) — Aufnahmedatum (EXIF, Server-Ableitung, Wandlung).
- [SPEC_UPLOAD_PROGRESS.md](original/SPEC_UPLOAD_PROGRESS.md) — Fortschritts-UI und ETA-Berechnung.
- [SPEC_ONE_TIME_UPLOAD.md](original/SPEC_ONE_TIME_UPLOAD.md) — Einmal-Upload (Drag & Drop, Queue, Persistenz).
- [SPEC_BACKGROUND_SYNC.md](original/SPEC_BACKGROUND_SYNC.md) — Hintergrund-Sync (FSEvents, Cadence, Pause).
- [SPEC_CHANGE_DETECTION.md](original/SPEC_CHANGE_DETECTION.md) — Änderungserkennung (Events, Stat-Check, Wasserzeichen).
- [SPEC_NOTIFICATIONS.md](original/SPEC_NOTIFICATIONS.md) — Benachrichtigungs-System des Originals.
- [SPEC_PREFERENCES.md](original/SPEC_PREFERENCES.md) — Einstellungen/Preferences des Originals.
- [SPEC_ACCOUNT_BILLING.md](original/SPEC_ACCOUNT_BILLING.md) — Konto- und Billing-Bereich.
- [SPEC_OS_INTEGRATION.md](original/SPEC_OS_INTEGRATION.md) — OS-Integration (Tray, Autostart, Menü).
- [SPEC_ABOUT.md](original/SPEC_ABOUT.md) — About-Fenster und Versionshinweise.
- [SPEC_RELEASE_NOTES.md](original/SPEC_RELEASE_NOTES.md) — Release Notes nach einem Update (Auslöser, Merker, Fenster).
- [SPEC_FILE_IDENTITY.md](original/SPEC_FILE_IDENTITY.md) — Datei-Identität (MD5, Stat-Check, Pfad-Bindung).
- [SPEC_FOLDERS.md](original/SPEC_FOLDERS.md) — Ordner-Scan und Filter-Logik.
- [SPEC_UPLOAD_DESTINATION.md](original/SPEC_UPLOAD_DESTINATION.md) — Upload-Ziel und Zielauflösung.
- [SPEC_STATUS_DETAIL.md](original/SPEC_STATUS_DETAIL.md) — Status-Ansicht und Detail-Panel.
- [SPEC_SYNC.md](original/SPEC_SYNC.md) — Sync-Engine (bidirektional, Download, Trash).
- [SPEC_MEDIA_IMPORT.md](original/SPEC_MEDIA_IMPORT.md) — Medien-Import (Apple-Photos, Aperture).
- [SPEC_CONTENT_FILTER_DESTINATION.md](original/SPEC_CONTENT_FILTER_DESTINATION.md) — Inhaltsfilter und Ziel.
- [SPEC_MISSING_SOURCE.md](original/SPEC_MISSING_SOURCE.md) — Fehlerfälle bei fehlenden Quelldateien.
- [SPEC_LIBRARY.md](original/SPEC_LIBRARY.md) — Bibliothek (Alben, Favoriten, Papierkorb).
- [SPEC_SCOPE_INVENTORY.md](original/SPEC_SCOPE_INVENTORY.md) — Feature-Inventar und Gap-Tabelle.
- [UI_FLOWS.md](original/UI_FLOWS.md) — UI-Flows des Originals (Screens, Navigation, Dialoge).
- [UI_STATES.md](original/UI_STATES.md) — UI-Zustände (Zeilenmodell, Statuswortschatz, Bitmasks).

## Protokoll — [`protocol/`](protocol/)

Wire-Protokoll unserer Upload-Engine (cookie-basierter Weg).

- [PROTOCOL.md](protocol/PROTOCOL.md) — Endpunkte, Header, Upload-Antworten, 409-Verhalten.
- [openapi.yaml](protocol/openapi.yaml) — OpenAPI-Spec des Geräte-Flows (device auth, REST).

## Entwicklung — [`dev/`](dev/)

Dokumentation unserer Codebase und Entwicklungsprozesse.

- [BUILD.md](dev/BUILD.md) — Build & Run (Plattformen, Abhängigkeiten, Konfigurationspfade).
- [SETUP.md](dev/SETUP.md) — Werkzeuge auf der Entwicklermaschine (PATH, ilspycmd, macOS-/Bild-Tools).
- [RELEASE.md](dev/RELEASE.md) — Release-Prozess, Artefakte, Signierung, Verifikation.
- [I18N.md](dev/I18N.md) — Internationalisierung (Sprachschicht, Locale-Default).
- [DEV_NOTES.md](dev/DEV_NOTES.md) — Fallstricke und Codebase-Kenntnisse.
- [THIRD_PARTY.md](dev/THIRD_PARTY.md) — Drittanbieter-Attribution (Icons, Material-Symbols).

## Features — [`features/`](features/)

Feature-spezifische Dokumentation (eine Datei pro Feature) und Live-Befunde.

- [FEATURES.md](features/FEATURES.md) — Index über alle Feature-Dateien.
- [LIVE_ACCEPTANCE.md](features/LIVE_ACCEPTANCE.md) — Live-Abnahme-Befunde (echter Account, Cookie-Flow).

### Session
- [login.md](features/login.md) — Sign-in (Gerätefluss/PKCE, Webview-Wipe, Marktplatz, Start-Gate)
- [session-refresh.md](features/session-refresh.md) — Token-Auffrischung (80 %, Worker, Cookies neu münzen)
- [sign-out.md](features/sign-out.md) — Abmelden + Abbruch eines laufenden Uploads

### Backup
- [backup-targets.md](features/backup-targets.md) — Sicherungsziele (Ordnerdialog, Inhaltsfilter)
- [destination.md](features/destination.md) — Upload-Ziel / Spiegelung
- [change-detection.md](features/change-detection.md) — Änderungserkennung (Events, Stat-Check, Wasserzeichen)
- [duplicates-conflicts.md](features/duplicates-conflicts.md) — Dedup-Index, Inhalts-Identität, vier Konflikt-Optionen

### Upload
- [upload.md](features/upload.md) — Upload-Engine (Wire-Parameter, Aufnahmedatum, NFC-Dateinamen)
- [queue-progress.md](features/queue-progress.md) — Warteschlange, Concurrency, Fortschritt/ETA, Ergebnis-Aufschlüsselung
- [pause.md](features/pause.md) — Globale Pause
- [one-time-upload.md](features/one-time-upload.md) — Drag & Drop / Einmal-Queue

### Desktop
- [autostart.md](features/autostart.md) — Login-Item (Default an)
- [tray.md](features/tray.md) — Tray-Icon / -Menü / Zustände
- [notifications.md](features/notifications.md) — OS-Benachrichtigungen
- [settings.md](features/settings.md) — Einstellungs-Seite

### UI & App
- [window-layout.md](features/window-layout.md) — Panel-Reihenfolge, virtuelle Liste, Höhen, Ordner-Karten
- [error-handling.md](features/error-handling.md) — Fehlerbehandlung im UI
- [languages.md](features/languages.md) — Internationalisierung (i18n)
- [branding.md](features/branding.md) — Name / Branding / Logo-Assets
- [about-licenses.md](features/about-licenses.md) — About / Drittanbieter-Lizenzen
- [platform.md](features/platform.md) — Cross-Platform-Portierung
- [scope.md](features/scope.md) — Scope-Grenzen und Roadmap

## Startpunkt

Für neue Leser: [`README.md`](../README.md) im Repo-Wurzelverzeichnis.
