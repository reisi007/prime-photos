# Error handling in the UI

End-user error messages are **understandable** — technical details never
appear in the visible UI. They go to the log (`log::warn!`) instead.

## How it works

- In the visible UI (especially the login header) only **understandable**
  messages appear; technical details (`IO error: …`, wry errors, URLs, error
  codes) go to the log, **never** to the surface.
- The header rest (logo + "Prime Photos" + subtitle) is also cleanly
  vertically centred.
- `config_error` is made **understandable**: the technical detail goes to
  the log, a plain sentence is shown in the UI — and it is **also shown on
  the sign-in screen** (after signing out it would otherwise be invisible).
- **No cancel button in the UI** for upload abort (not commissioned) — the
  abort is tied solely to signing out (see [sign-out.md](sign-out.md)).
- A click on empty space triggers **no action** — no folder picker, no
  Finder, no visible feedback. The **only** allowed interaction with "empty"
  space is **drag & drop** of images.

## 401 / expired session

- **Soll-Vertrag (Issue #6):** use the stored **device-flow refresh token** for
  session renewal ([session-refresh.md](session-refresh.md)); the background
  worker renews access and mints the upload cookies from that token. The
  refresh token is in the OS keychain; access tokens and cookies are
  memory-only.
- If the session cannot be renewed and is declared invalid, the app clears its
  session and returns to sign-in. The user signs in again through the app's
  device-flow/PKCE path ([login.md](login.md)); no app restart is required.
- Do **not** recommend `config_update.py` or copying browser cookies as a
  recovery procedure. Those manual-cookie instructions do not match the
  current session model and are not a supported way to restore it; the existing
  401 log copy that mentions the script is legacy guidance. This contract does
  not promise that every individual upload `401` is synchronously repaired or
  specify a retry count.

## Decisions

- **Keine technischen Fehlermeldungen im Endnutzer-UI (User-Regel 2026-10-15):**
  Im sichtbaren UI (v. a. im Login-Header) erscheinen nur **verständliche**
  Meldungen; technische Details (`IO error: …`, wry-Fehler, URLs,
  Fehlercodes) gehen ins Log (`log::warn!`), **nie** in die Oberfläche.
  Anlass: „Cookies konnten nicht gelesen werden: IO error: timed out waiting
  for cookies response" stand im Login-Header. Auch der Header-Rest (Logo +
  „Prime Photos" + Untertitel) wird vertikal sauber zentriert.

- **`config_error` verständlich machen** (User-Regel „keine technischen
  Fehlermeldungen im Endnutzer-UI"): das technische Detail geht ins Log, im
  UI steht ein verständlicher Satz — und er wird **auch auf dem Login-Screen**
  angezeigt (nach dem Abmelden ist er sonst unsichtbar).
