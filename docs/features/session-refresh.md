# Session refresh

The access token is renewed in the background at **80 % of its remaining
lifetime**. Upload cookies are minted at start-up and after a successful
renewal. **Prime Photos contract (Issue #7, implemented):** a failed cookie mint
is retried on the next 3-second worker tick, independently of the next
access-token refresh appointment. This applies to cookie-mint failures during
login and after a successful token refresh. A cookie failure neither moves the
access-token appointment, invalidates the session, nor consumes an access-token
retry attempt. While offline, both kinds of work remain paced; there is no
busy loop.

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
- **Cookie minting is independent pending work (Prime Photos, Issue #7):** if
  minting fails during login or after a successful refresh, retry it on the next
  3-second worker tick; do not wait for the 80%-of-lifetime access-token
  appointment. Cookie-only retries do not refresh the access token, change its
  appointment, consume its retry budget, or invalidate the session. A successful
  retry is delivered through the existing in-memory cookie handoff to the UI.
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
  eingespeist; die **Upload-Engine bleibt cookie-basiert**. Schlägt das Minting
  fehl (auch während des Logins), wird es unabhängig vom 80-%-Termin beim
  nächsten 3-s-Worker-Tick erneut versucht; der Access-Token-Termin bleibt
  unverändert (Prime Photos contract, Issue #7).

- **S-C-Umsetzung:** der Auffrischungs-Worker läuft als **ein** langlebiger
  Task auf der bestehenden Tokio-Runtime (Muster wie `avatar_fetch`),
  Ergebnis über einen geteilten Slot, den die UI pro Frame pollt.
  **Persistenz:** der Refresh-Token ist der persistente Session-Pfad; nach einem
  Neustart werden Access-Token und Upload-Cookies daraus im Hintergrund erneuert.
  Einen Browser-Cookie-Login als Rückfall gibt es nicht.

- **`expires_in` kommt als JSON-String** (`"3600"`): der Parser akzeptiert
  String **und** Zahl (Register **und** Refresh).

- **Original desktop auth chain (observed; not this port's upload path):**
  `POST api.amazon.com/auth/token`
  (`source_token_type=refresh_token` -> `access_token`) ->
  `GET drive.amazonaws.com/drive/v2/account/endpoint` ->
  `POST cdws.../drive/v1/account/source`.

- **Original cookie behavior (observed in `SPEC_LOGIN.md` § 3.8):** the original
  also mints cookies from the refresh token for its embedded web views. The spec
  does **not** specify retries after a failed cookie mint; the next-tick retry
  above is a Prime Photos contract, not a claim about the original.

- **Flow distinction (current):** sign-in uses the device flow/PKCE and its
  stored refresh token. The upload engine remains cookie-based; its session
  cookies are minted from that refresh token and are not harvested from the
  login webview. Older notes describing browser-cookie sign-in as active are
  superseded (see [login.md](login.md)).

## Original reference

- `../original/SPEC_LOGIN.md` — Login/Auth flow of the original (device flow, PKCE, token).
- `../protocol/PROTOCOL.md` — Wire protocol (endpoints, headers, upload responses).
