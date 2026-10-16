# Session refresh

The access token is renewed in the background at **80 % of its remaining
lifetime**, and the web cookies are re-minted at start-up and after every
successful renewal. While offline, nothing is given up: a connection error
does not consume an attempt, and the app simply tries again on the next tick.

## How it works

- A background worker on the existing Tokio runtime polls every 3 seconds
  (pattern: `TickTockTimer`), up to **5 attempts** spaced **15 seconds** apart
  (the `AppSettings` values). The result is delivered via a shared slot that
  the UI polls per frame.
- The worker refreshes the access token at 80 % of its remaining lifetime
  (`expires_in` − 60 seconds).
- After every successful refresh, the four web cookies
  (`session-id`, `x-acb*`, `at-acb*`, `ubid-acb*`) are re-minted from the
  refresh token via `POST https://api.amazon.com/auth/token` with
  `requested_token_type=auth_cookies` und fed into the in-memory cookie jar.
- The upload engine stays cookie-based; the live cookie jar remains valid
  across token refreshes.
- Offline does not count as a failure: a connection error
  (`AuthError::Network`) does not consume an attempt, the schedule stays,
  the next tick retries — **the app never gives up while offline**.
- A fatal outcome (e.g. invalid refresh token) marks the session as invalid,
  clears the tokens and returns to the sign-in screen.

## Decisions

- **Auffrischen:** bei **80 % der Restlaufzeit** (der Ablauf ist `expires_in` −
  60 s) durch einen **Hintergrund-Worker** auf der bestehenden Tokio-Runtime
  (3-s-Tick wie `TickTockTimer`, bis zu **5** Versuche im **15-s**-Abstand —
  die belegten `AppSettings`-Werte). **Offline zählt nicht** (bewusste
  Abweichung vom Original): ein Verbindungsfehler (`AuthError::Network`)
  verbraucht **keinen** Versuch, der Termin bleibt, der nächste Tick probiert
  erneut — **offline meldet nie ab**. Fataler Ausgang = Sitzung ungültig.

- **Web-Cookies erneuern:** `POST https://api.amazon.com/auth/token?source_token=<refresh>&source_token_type=refresh_token&requested_token_type=auth_cookies&domain=.<tld>&app_name=…&app_version=…`
  — **beim Start und nach jeder erfolgreichen Auffrischung** aus dem
  Refresh-Token gemintet und in die bestehende Cookie-Kette (im Speicher)
  eingespeist; die **Upload-Engine bleibt cookie-basiert**, die live belegte
  Kette aus Welle 2 bleibt damit gültig.

- **S-C-Umsetzung:** der Auffrischungs-Worker läuft als **ein** langlebiger
  Task auf der bestehenden Tokio-Runtime (Muster wie `avatar_fetch`),
  Ergebnis über einen geteilten Slot, den die UI pro Frame pollt.
  **Folge der Speicher-only-Regel:** der reine Cookie-Rückfall (Geräteweg
  abgelehnt) gilt nur für die **laufende** Sitzung, nicht über einen Neustart
  — der Geräteweg ist der persistente Pfad.

- **`expires_in` kommt als JSON-String** (`"3600"`): der Parser akzeptiert
  String **und** Zahl (Register **und** Refresh).

- **Desktop auth chain (observed):** `POST api.amazon.com/auth/token`
  (`source_token_type=refresh_token` -> `access_token`) ->
  `GET drive.amazonaws.com/drive/v2/account/endpoint` ->
  `POST cdws.../drive/v1/account/source`.

- **Flow note:** this is the **web** flow (cookie-based). The desktop client's
  **device** flow (`amzn_photos_desktop_*` signin -> `/auth/register` ->
  refresh token -> `/auth/token` ->
  `upload-photos.amazon.de/v2/upload`) is documented in
  `../protocol/openapi.yaml` but not used.

## Original reference

- `../original/SPEC_LOGIN.md` — Login/Auth flow of the original (device flow, PKCE, token).
- `../protocol/PROTOCOL.md` — Wire protocol (endpoints, headers, upload responses).
