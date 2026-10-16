# Sign-out

Signing out deregisters the device with Amazon and deletes our session
material. A sign-out also **aborts any running upload** — the upload worker
checks the abort flag at its wait points and returns cleanly.

## How it works

- Signing out calls `POST https://api.amazon.com/auth/deregister` with the
  access token, then deletes the tokens (keychain + in-memory).
- A **cancel flag in `UploadShared`** is set by `logout()`. The upload worker
  checks this flag at its **wait points** (pause-park,
  `src/upload.rs:393`) **and** at the conflict dialog, then returns.
- A running HTTP request is allowed to finish; the flag only affects the
  worker's decision loop.
- `run_upload` resets the flag at start.
- An aborted run does **not** count as finished: watermark is **not** advanced
  (otherwise the folder would be considered "unchanged" at next start and the
  remainder would never be caught up), `FolderStatus`/counters are **not**
  updated (today the row would claim "done" with half the count), one-time
  queue is kept (already correct: `backup.rs:1460` deletes only when
  `done + failed >= total`). Already-uploaded files stay in the dedup index
  and are skipped via the stat path on the next run.
- **No cancel button in the UI** (not commissioned) — the abort is tied solely
  to signing out.
- `config_error` is made **understandable**: the technical detail goes to the
  log, a plain sentence is shown in the UI — and it is **also shown on the
  sign-in screen** (after signing out it would otherwise be invisible).

## Decisions

- **Abmelden:** `POST https://api.amazon.com/auth/deregister` mit dem
  Access-Token, danach die Tokens löschen.

- **Abmelden bei laufendem Upload (User-Entscheidung 2026-10-16):**
  - **Ein Abbruch-Mechanismus wird gebaut** (User-Wahl aus drei Optionen).
    Anlass: `logout()` beendet den Session-Worker bereits (Generation-Bump)
    und löscht Token/Keychain — aber **nicht** einen laufenden Upload: der
    Worker hält einen eigenen `Config`-Klon mit den **alten** Cookies und lief
    mit widerrufenen Zugangsdaten in Fehler. **Die dritte Residue der alten
    Notiz ist hinfällig:** der Login-Screen *ersetzt* die Haupt-UI
    (`src/app.rs:950`), der Abmelden-Knopf ist bei offener Login-WebView also
    gar nicht erreichbar.
  - Umsetzung: **Abbruch-Flag in `UploadShared`** (kein neuer Kanal — die App
    hält `self.upload` bereits, `logout()` erreicht es ohne neues Plumbing).
    `logout()` setzt es; der Worker prüft es an seiner **Wartestelle**
    (Pause-Park, `src/upload.rs:393`) **und** beim Konflikt-Dialog und kehrt
    zurück. Ein laufender HTTP-Request läuft zu Ende. `run_upload` setzt das
    Flag beim Start zurück.
  - **Ein abgebrochener Lauf gilt nicht als erledigt:** Wassermarke **nicht**
    vorschieben (sonst gilt der Ordner beim nächsten Start als „unverändert"
    und die Reste werden nie nachgeholt), `FolderStatus`/Zähler **nicht**
    verbuchen (heute würde die Zeile „fertig" mit halber Zahl behaupten),
    Einmal-Queue behalten — das war schon korrekt (`backup.rs:1460` löscht nur
    bei `done + failed >= total`). Bereits hochgeladene Dateien bleiben im
    Dedup-Index und werden beim nächsten Lauf über den Stat-Pfad übersprungen.
  - **Verworfen:** nur dokumentieren; den Lauf über die vorhandene Pause
    **parken** (er nimmt nach erneutem Anmelden mit den **alten** Cookies wieder
    auf — nicht sauber); `conn: None`-Analogie als Vorbild (der Abbruch
    braucht keinen neuen Kanal).
  - **Kein Abbruch-Knopf in der UI** (nicht beauftragt) — der Abbruch hängt
    allein am Abmelden.
  - **`config_error` verständlich machen** (User-Regel „keine technischen
    Fehlermeldungen im Endnutzer-UI"): das technische Detail geht ins Log, im
    UI steht ein verständlicher Satz — und er wird **auch auf dem Login-Screen**
    angezeigt (nach dem Abmelden ist er sonst unsichtbar).

## Original reference

- `../original/SPEC_LOGIN.md` — Login/Auth flow of the original (device flow, PKCE, token).
