# Scope and roadmap

Deliberate in/out boundaries for the MVP, plus the planned
implementation waves.

## How it works

- The MVP covers: login, upload-target creation ("Upload-Ziel erstellen":
  backup configuration with destination, content filter, interval, dedup,
  conflict handling), upload flow, settings dialog. The download flow is
  relevant but explicitly **not** part of the MVP.
- **Out (deliberately, not MVP):** bidirectional sync,
  downloads/restore, file management (albums/favourites/trash/sharing),
  Apple Photos / iPhoto / Aperture import, autoplay/media player,
  upsell/trial/over-quota/outage banners, auto-update, proxy and "hidden
  files" settings, crash-relaunch launch agent, iOS dedup prompt.
- **For a public release relevant:** About / third-party licenses
  (attribution obligation).
- **Concurrency cap "8":** not a technical limit but the upper bound of the
  customer/server-delivered setting
  (`CustomerSettingsMaxSupportedUploadQueues = 8`); the **device default**
  is smaller (`UploadMaxQueues = 4`, `MaxNumberOfQueuesPerProcessorCore =
  1.0` ⇒ practically `min(4, Cores)`). A technical justification for the 8
  is **not** documented in the original.
- **"Not implemented" means:** documented and deliberately out — not
  "automatically later".
- **Storage-usage bar ("Speichernutzung"):** intentionally omitted for now:
  it only applies to Prime members and needs a real quota fetch; no
  placeholder is shown.
- **MVP gap closed (correction 2026-10-14):** the interactive file-name
  conflict dialog is **implemented** (`81cd277`, verified in `a3a3625`):
  on 409 `Ask` prompts per file (ignore / save as new file / replace) with
  "don't ask again" as a run-wide override and 60-second timeout ⇒ ignore.
  The earlier note "the option 'Nachfragen' is without it ineffective" is
  thus superseded.

## Planned implementation waves

- **Welle 1 — echter Lauf** (kein Code) — Belege im Ledger.
- **Welle 2 — Konflikt-/Duplikat-Kette abschließen:** 409-Ursache trennen
  (2.1), „Ersetzen" via `PUT /v2/upload/<nodeId>` (2.2), „Nicht noch einmal
  fragen" pro Ziel persistieren (2.3), Rest nach
  `../original/SPEC_CONFLICT_OPTIONS.md` (2.4). **Läuft ab 2026-10-15.**
- **Welle 3 — Progress-/UI-Umbau** (Detail in den Hauptbereich, Klapp-Panel
  weg, Live-Status) **plus die Layout-Punkte aus
  `../original/SPEC_UPLOAD_PROGRESS.md` § 7.8** — feste Spaltenköpfe, Kürzen +
  Tooltip, Liste füllt die Höhe, die 300-Zeilen-Kappe entfällt, leerer Zustand
  zentriert (User-Entscheidung 2026-10-15: „3.1–3.4 **plus** §7.8-Layout"
  als **ein** Paket).
- **Welle 4 — About / Third-Party-Lizenzen** (Voraussetzung für ein
  öffentliches Release).
- **Welle 5 — Robustheit & Politur** (`rfd::FileDialog` → `AsyncFileDialog`,
  P2-Reste, Branding).
- **Welle 6 — Post-MVP / bewusst später** (dokumentiert, **nicht** eingeplant).

Frühere Auswahl von 2026-10-14 („P2+P3", „Echter Name + Profilbild", „About"):
die beiden ersten sind umgesetzt, das dritte ist jetzt **Welle 4** — die
alte Nummerierung ist damit abgelöst.

Weitere offene Punkte (Queue-Paginierung, Fehlerfenster, Pause pro Ordner,
Sortierung/Filter, Tray-Status-Popup, Pro-Ziel-Limit, Drosselung, Logout im
Upload) sind im Ledger dokumentiert, aber **nicht** eingeplant. Regel bleibt:
„Anlegen bleibt Pflicht — Abarbeiten nicht"; ein Listeneintrag allein löst
keinen Umsetzungs- oder Verify-Lauf aus.

## Decisions

- **MVP scope (user decision 2026-10-13):** login, upload-target creation
  ("Upload-Ziel erstellen": backup configuration with destination, content
  filter, interval, dedup, conflict handling), upload flow, settings dialog.
  The download flow is relevant but explicitly NOT part of the MVP.

- **Out (bewusst, nicht MVP):** bidirektionale Synchronisation,
  Downloads/Restore, Datei-Management (Albums/Favoriten/Papierkorb/Teilen),
  Apple-Photos-/iPhoto-/Aperture-Import, Autoplay/Media-Player,
  Upsell-/Trial-/Overquota-/Outage-Banner, Auto-Update, Proxy- und
  „ausgeblendete Dateien"-Einstellungen, Crash-Relaunch-LaunchAgent,
  iOS-Dedupe-Prompt.

- **Für ein öffentliches Release relevant:** About / Third-Party-Lizenzen
  (Hinweispflicht).

- **Concurrency-Cap „8":** kein technisches Limit, sondern die Obergrenze der
  kunden-/server-gelieferten Einstellung
  (`CustomerSettingsMaxSupportedUploadQueues = 8`); der **Geräte-Default** ist
  kleiner (`UploadMaxQueues = 4`, `MaxNumberOfQueuesPerProcessorCore = 1.0`
  ⇒ praktisch `min(4, Cores)`). Eine sachliche Begründung der 8 ist im
  Original **nicht** belegt.

- **Spec-Abgleich 2026-10-17 — zwei Backlog-Begriffe sind im Original nicht
  belegt:** (a) ein **Netz-/Kosten-Gate** — das Original kennt nur die
  **Bandbreiten-Drosselung** (`../original/SPEC_PREFERENCES.md` §3.3: KB/s,
  clamp 1..99999, **0 = unbegrenzt**, **keine** Netztyp-Bedingung); (b) ein
  **„Pro-Ziel-Limit"** — belegt ist nur die **globale** Concurrency
  (`../original/SPEC_SCOPE_INVENTORY.md` #24: `UploadMaxQueues = 4`, Cap 8).
  Beides wäre damit eine **Neuerung**, keine Parität.

- **Oberziel des Users (2026-10-13):** „nur neue Dateien hochladen, keine
  Bandbreite verschwenden."

- **„Nicht umsetzen" heißt hier:** dokumentiert und bewusst draußen — nicht
  „später automatisch".

- **MVP-Lücke geschlossen (Korrektur 2026-10-14):** der interaktive
  Dateinamens-Konflikt-Dialog ist **implementiert** (`81cd277`, verifiziert
  in `a3a3625`): bei 409 fragt `Ask` pro Datei (Ignorieren / Als neue Datei
  speichern / Ersetzen) mit „Nicht noch einmal fragen" als run-weitem
  Override und 60-s-Timeout ⇒ Ignore. Die frühere Notiz „die Option
  ‚Nachfragen' ist ohne ihn wirkungslos" ist damit überholt.

- **"Speichernutzung"/storage bar is intentionally omitted for now:** it only
  applies to Prime members and needs a real quota fetch; no placeholder is
  shown (inventory #7, **Out**; the open work stays in the ledger).

- **„Startseite"/Übersichtsseite (User-Entscheidung 2026-10-17): verworfen.**
  Der Recon beantwortet die frühere Backlog-Frage: `…_navigationMenu_home`
  („Startseite") und `…_navigationMenu_smartscan` („Intelligenter Scan")
  existieren als **Strings**, haben im Original aber **keinen** Menüpunkt und
  keinen Screen (`../original/UI_STATES.md:61`,
  `../original/SPEC_LIBRARY.md:535` — Spalte „no",
  `../original/UI_FLOWS.md:754`). Ein eigener Screen wäre damit eine Erfindung
  statt Parität; die ungenutzten Strings im Build bleiben liegen (**nicht**
  „aufräumend" entfernen und **nicht** doch noch bauen).

## Original reference

- `../original/SPEC_SCOPE_INVENTORY.md` — Feature inventory and gap table.
