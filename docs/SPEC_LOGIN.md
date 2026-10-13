# SPEC_LOGIN — Login / Sign-in

Clean-room specification of the **login feature** as it exists in two places:

1. **Original:** the official Amazon Photos macOS desktop client
   (`/Applications/Amazon Photos.app`, bundle `com.amazon.clouddrive.mac`,
   version `11.3.0` / build `36`).
2. **Ours:** this repository, restricted to the **committed** `HEAD` state
   (the working tree is dirty with an unrelated pause fix; statements about our
   code are taken from `git show HEAD:…` only).

This document describes **behaviour**, not implementation mechanics. It contains
no verbatim decompiled code and no personal data. German UI labels are quoted
from the shipped `Localizable.strings` and are fine to reproduce.

**Confidence markers:** *observed* = read directly from the binary / interface
files / committed code. *inferred* = reasoned. *unknown* = could not be
determined and is recorded as such rather than guessed. Values the task asks for
but that could not be verified are listed in § 7.

---

## 1. Purpose & MVP scope

Login is part of the **MVP scope** (see `AGENTS.todo.md`): the user signs in to
Amazon Photos before any backup target or upload can be created. Both the
original and our app surface a sign-in step; they differ in flow (device OAuth
vs. browser-cookie), which is the core of § 5.

Out of scope for this spec: the post-login device registration / token chain
(`POST /auth/register`, `POST /auth/token`, endpoint discovery) which is
already documented in `docs/PROTOCOL.md` §§ 2.1–2.6, and upload protocol
details (`docs/SPEC_UPLOAD.md`).

---

## 2. Sources

| Source | Used for |
|---|---|
| `Contents/MonoBundle/Amazon Photos.dll` (decompiled via `ilspycmd`) | `LoginConstants`, `LoginWindowController`, `LoginWebPolicyDelegate`, `MacLoginTask`, `DomainProvider`, `Localization`, `WebCookieManager` |
| `Contents/MonoBundle/Amazon.Auth.Map.dll` (decompiled) | `AuthPortalUrl` base class, `AmazonCloudDriveAuthPortalUrl` subclass, `ClientIdProvider` |
| `Contents/Resources/de.lproj/Localizable.strings` | German labels (keys cited inline) |
| `docs/UI_FLOWS.md`, `docs/UI_STATES.md`, `docs/PROTOCOL.md` | window geometry, navigation vocabulary, device-flow endpoints |
| `git show HEAD:src/login.rs`, `HEAD:src/config.rs`, login parts of `HEAD:src/main.rs`, `HEAD:src/amzn_photo.rs` | our committed implementation |

---

## 3. Original app — sign-in (observed)

### 3.1 Window shell

Separate top-level window, created on demand from `LoginWindowController`
(`MacLoginTask.ShowAuthPortal`). *observed:*

| Property | Value | Source |
|---|---|---|
| Size | 880 × 530 pt | `LoginConstants.AuthPortalWindowSize` |
| Style mask | Titled \| Closable \| Miniaturizable | `InitLoginViews` |
| Identifier | `LoginView` | `InitLoginViews` |
| Title | "Amazon Photos" (`clouddrive_desktop_title`) | `InitLoginViews` |
| Release on close | yes (`ReleasedWhenClosed = true`) | `InitLoginViews` |
| Close behaviour | closing calls `_appShutdown.Quit()` (the whole app exits) | `HandleShouldClose` |
| Inner auth container | 380 × 528 at (500, 0) inside the window | `AuthPortalContainerLocation` / `…Size` |
| Welcome panel | 500 × 528 (left side) | `AuthPortalWelcomeViewSize` |

Layout is two-column: a **welcome/marketing panel** (500 pt) on the left and an
**auth portal container** (380 pt) on the right, separated by a 1 pt border
colour `Secondary4`. The window is not resizable. *observed (frames)*.

The welcome panel shows:

- marketing header "Laden Sie Fotos und Videos von Ihrem Computer, Ihrer
  Kamera und Ihrer Speicherkarte hoch" (`clouddrive_desktop_login_marketing_header`);
- sub-headers "Einfach speichern" (`…_marketing_header1`),
  "Sichern Sie Dateien, Dokumente und Fotos von Ihrem Computer."
  (`…_marketing_header2`),
  "Amazon Photos speichert alles sicher an einem Ort und bietet einfachen
  Zugriff." (`…_marketing_header3`);
- an illustration (`backup-illustration-login`) shown at (81, 125) with a
  320 × 406 container.

### 3.2 Loading and no-network states

Before the auth page is usable the controller shows two overlays inside the
380 × 528 container *observed*:

- **Loading view**: a centred spinning `NSProgressIndicator`, hidden when the page
  finishes loading.
- **No-network view**: an alert icon (`signin-alert`, 30 × 25), heading
  "Keine Internetverbindung" (`clouddrive_desktop_login_ui_no_network_heading`),
  description "Überprüfen Sie Ihre Internetverbindung und versuchen Sie erneut,
  sich bei Amazon Photos anzumelden."
  (`clouddrive_desktop_login_ui_no_network_description`), and a
  "Wiederholen" button (`clouddrive_desktop_login_ui_retry_button`) that
  re-issues the auth-portal request when connectivity returns.

A `NetworkDetector` drives these. Implementation detail of the detector is
*unknown*.

### 3.3 Auth portal request

The sign-in page is a normal web page loaded in a `WKWebView`/legacy
`WebView`. Cleanup happens first: `WebCookieManager.DeleteAllCookies()` wipes
the shared `NSHttpCookieStorage` (1 s timeout) before any request is issued.
*observed.*

The URL is assembled from two pieces:

- Base: `https://www.<domain>/ap/signin?` where `<domain>` comes from the
  `DomainProvider` (§ 3.6).
- Parameters (base `AuthPortalUrl` + `AmazonCloudDriveAuthPortalUrl`
  subclass) *observed:*

| Parameter | Value |
|---|---|
| `openid.ns` | `http://specs.openid.net/auth/2.0` |
| `openid.claimed_id` | `http://specs.openid.net/auth/2.0/identifier_select` |
| `openid.identity` | `http://specs.openid.net/auth/2.0/identifier_select` |
| `openid.mode` | `checkid_setup` |
| `openid.oa2.scope` | `device_auth_access` |
| `openid.ns.oa2` | `http://www.amazon.com/ap/ext/oauth/2` |
| `openid.oa2.response_type` | `code` |
| `openid.oa2.code_challenge_method` | `S256` |
| `openid.oa2.client_id` | `device:<hex>` — hex of `"<deviceSerial>#<deviceTypeId>"`, lowercased, with a `device:` prefix (see `ClientIdProvider`) |
| `language` | sanitised culture string (exact sanitisation *unknown*) |
| `marketPlaceId` | marketplace id from § 3.6 |
| `openid.return_to` | `https://www.<domain>/ap/maplanding` |
| `openid.pape.max_auth_age` | `0` |
| `forceMobileLayout` | `true` |
| `openid.assoc_handle` | `amzn_photos_desktop_<region>` (§ 3.6 normalisation) |
| `pageId` | `amzn_photos_desktop_<amazonRegion>` (§ 3.6 normalisation) |
| `openid.oa2.code_challenge` | S256 digest of a fresh 32-byte code verifier (injected when `FullUrlForAuthCode` is read) |

Two additional details *observed*:

- The webview's `CustomUserAgent` is a mobile Android UA
  (`Mozilla/5.0 (Linux; Android 4.4; Nexus 5 Build/_BuildID_) AppleWebKit/… Chrome/30.0.0.0 Mobile Safari/537.36`)
  so Amazon serves the mobile sign-in layout that `forceMobileLayout` also asks for.
- The back panel (40 pt, background `FAB`, 1 px `Secondary4` top border) with a
  back-arrow image and the label "Zurück zur Anmeldung"
  (`clouddrive_desktop_login_back_button`) is **hidden** while the URL contains
  `/ap/signin` and **shown** for other `/ap/` pages or other hosts. Tapping it
  invokes `BackButtonClickedHandler` (exact navigation *unknown*).

### 3.4 Navigation policy

`LoginWebPolicyDelegate.DecidePolicyForNavigation` (*observed*):

- If the URL contains `/ap/`:
  - extract `openid.oa2.authorization_code` from the query string. If present,
    store it (with the `client_id` and PKCE code verifier) as the login result
    and **ignore** the navigation — this is the sign-in completion signal.
  - record metrics when the URL contains `register` or `forgotpassword`.
  - otherwise **use** the navigation.
- If the URL contains `/gp/`: open it in the default browser (`NSWorkspace`) and
  **ignore** the navigation.
- otherwise **use** the navigation.
- `DecidePolicyForNewWindow`: if the URL contains the terms-of-use node id
  `201376540`, record a "TermOfUse" metric; the URL is always handed to
  `NSWorkspace` (new window → external browser).

Failed provisional / finished load counters and health metrics are recorded on
the corresponding webview events; an actual load failure is logged as fatal.
On finished load the back button visibility is toggled and the loading overlay
hidden. *observed*.

### 3.5 Result consumption

`MacLoginTask.ShowAuthPortal` (*observed*):

1. creates or reuses a `LoginWindowController`, publishes `WindowOpenEvent`,
   shows and centres the window;
2. awaits `LoginCompletion()` — i.e. the `TaskCompletionSource<AuthPortalResult>`
   that `LoginWebPolicyDelegate` completes with the authorization code, client id
   and code verifier;
3. sets `_authenticated = true`, publishes `WindowCloseEvent`;
4. validates the account status via `AccountStatusValidator`;
5. if valid, closes and drops the login window on the main queue.

On close-without-success, `HandleShouldClose` calls `_appShutdown.Quit()`, so the
original **cannot** be dismissed without quitting the app. *observed*.

The captured authorization code is then consumed by the device-registration flow
(`POST /auth/register` with PKCE verifier → refresh token → `POST /auth/token`
→ `GET /drive/v2/account/endpoint`). See `docs/PROTOCOL.md` §§ 2.1–2.4.

### 3.6 Marketplace → domain mapping

`DomainProvider` is driven by `ILocalization`: `CloudDriveRegion` selects the
domain and marketplace id; `AmazonRegion` and `Culture` are used for `pageId` and
`language` respectively. Fallback for any unsupported region is `US` →
`amazon.com` / `ATVPDKIKX0DER`. *observed:*

| `CloudDriveRegion` (supported, non-deprecated) | Domain | Marketplace id |
|---|---|---|
| `AU` | `amazon.com.au` | `A39IBJ37TRP1C6` |
| `BR` | `amazon.com.br` | `A2Q3Y263D00KWC` (deprecated → falls back to US) |
| `CA` | `amazon.ca` | `A2EUQ1WTGCTBG2` |
| `CN` | `amazon.cn` | `AAHKV2X7AFYLW` (deprecated → falls back to US) |
| `DE` | `amazon.de` | `A1PA6795UKMFR9` |
| `ES` | `amazon.es` | `A1RKKUPIHCS9HS` |
| `FR` | `amazon.fr` | `A13V1IB3VIYZZH` |
| `GB` | `amazon.co.uk` | `A1F83G8C2ARO7P` |
| `IT` | `amazon.it` | `APJ6JRA9NG5V4` |
| `JP` | `amazon.co.jp` | `A1VC38T7YXB528` |
| `US` | `amazon.com` | `ATVPDKIKX0DER` |
| anything else | `amazon.com` | `ATVPDKIKX0DER` (treated as `US`) |

Region inputs come from the macOS locale:

- `Region` (`ILocalization`) = `NSLocale.CurrentLocale.CountryCode ?? "US"`.
- `AmazonRegion` = `Region` if it is in the supported set, else `US`.
- `CloudDriveRegion` = `CustomerRegion` if present in the supported set and not
  deprecated, else `US`.
- `CustomerRegion` = stored customer region, or `AmazonRegion` as fallback.

Association-handle normalisation (from `AmazonCloudDriveAuthPortalUrl`) is
`gb` → `uk` before building `amzn_photos_desktop_<region>` (both for the
cloud-drive region used in `assoc_handle` and the Amazon region used in
`pageId`).

### 3.7 Authenticated session — device flow

The original does **not** use browser cookies for uploads; its sign-in produces
an OAuth authorization code that is exchanged for a refresh token and then an
access token (`Atna|…`), see `docs/PROTOCOL.md`. `WebCookieManager` exists for
a secondary purpose (writing website cookies obtained from Panda into
`NSHttpCookieStorage`), and login starts with a full cookie wipe; the relevant
session cookie names for *our* web flow (`session-id`, `at-acb*`, `x-acb*`,
`ubid-acb*`) are **not** referenced in the original binary. *observed: the*
*original never names these cookie names; our cookie names come from the web*
*upload reference.* Therefore the mapping of those names to "authenticated" is
defined by our implementation (§ 4.3), not by the original.

---

## 4. Our implementation (committed `HEAD`)

### 4.1 Architecture (committed)

- Embedded `wry` `WKWebView` as a **child view of the main eframe window**.
  No separate window, no external browser, no Chrome DevTools Protocol.
  (The helper-process variant `ppu-login` was tried and rejected; see
  `AGENTS.todo.md` "Verworfen".) *observed in committed code.*
- Cookies are **not** read from egui's `update`: `wry::WebView::cookies` is
  synchronous and spins the run loop, which re-enters winit's event handler and
  panics. Probes are instead dispatched with `DispatchQueue::main().exec_async`
  and the result is collected on the next frame. *observed.*
- UI language German, matching the original's labels where quoted.

### 4.2 Entry point / main-window behaviour

`PrimePhotoApp::update` (committed) routes to the login flow whenever
`login.running || !is_logged_in()`, and sets the window title to
"Login – Prime Photo Uploader" in that state, otherwise
"Prime Photo Uploader". The window is sized once on first frame to
`clamp(monitor × 0.82, 900..1500)` × `clamp(monitor × 0.85, 600..1000)`,
i.e. it does **not** use the original's fixed 1130 × 770 / 880 × 530 frames
(benchmark values are retained for the chrome, but our sizing is elastic). *observed.*

`is_logged_in()` returns `!session_id.is_empty() && !cookie_x_acb.is_empty()`.
*observed.*

### 4.3 Sign-in URL construction (committed)

`login::amazon_domain(country)` and `login::login_url(country)` *observed in*
*committed `HEAD:src/login.rs`:*

| Input country | Amazon domain | `cc` used in `assoc_handle` / `return_to` |
|---|---|---|
| `us` | `com` | `us` |
| `uk`, `gb` | `co.uk` | `uk`, `gb` respectively |
| `at`, `ch` | `de` | `de` (both map to `de`) |
| `jp` | `co.jp` | `jp` |
| `mx` | `com.mx` | `mx` |
| `br` | `com.br` | `br` |
| `au` | `com.au` | `au` |
| `cn` | `cn` | `cn` |
| `in` | `in` | `in` |
| other (e.g. `de`, `fr`, `it`, `es`, `ca`) | same as the code | same as the code |

Full URL parameters produced *observed*:

| Parameter | Value |
|---|---|
| `openid.pape.max_auth_age` | `3600` |
| `openid.return_to` | `https://www.amazon.<domain>/photos?sf=1&ref_=APH_D_PA1_<CC>` (percent-encoded; `<CC>` = `cc` uppercased) |
| `openid.identity` | `http://specs.openid.net/auth/2.0/identifier_select` |
| `openid.assoc_handle` | `amzn_photos_web_<cc>` |
| `openid.mode` | `checkid_setup` |
| `openid.claimed_id` | `http://specs.openid.net/auth/2.0/identifier_select` |
| `openid.ns` | `http://specs.openid.net/auth/2.0` |

Not present in our URL (deliberately, because this is the **web** flow):
`marketPlaceId`, `language`, OAuth client id / code challenge / scope, `pageId`,
`forceMobileLayout`. Our flow does not register a device. *observed.*

`return_to` carries the country uppercased as a path component
(`APH_D_PA1_DE` etc.), which is an organisation-specific Amazon referrer token;
its exact handling by Amazon is *unknown*.

### 4.4 Country / marketplace selection UI (committed)

The login screen (shown **before** opening Amazon's page) lets the user pick a
marketplace from `COUNTRIES` *observed*:

| Code | Label (as shown) |
|---|---|
| `de` | Deutschland · amazon.de |
| `at` | Österreich · amazon.de |
| `ch` | Schweiz · amazon.de |
| `us` | USA · amazon.com |
| `uk` | Vereinigtes Königreich · amazon.co.uk |
| `fr` | Frankreich · amazon.fr |
| `it` | Italien · amazon.it |
| `es` | Spanien · amazon.es |
| `ca` | Kanada · amazon.ca |
| `jp` | Japan · amazon.co.jp |
| `in` | Indien · amazon.in |
| `au` | Australien · amazon.com.au |
| `br` | Brasilien · amazon.com.br |
| `mx` | Mexiko · amazon.com.mx |

- On **first run** (`config.toml` absent) the country is preselected from the
  macOS locale via `system_country()`: tries `defaults read -g AppleLocale`,
  splits on `_` or `-`, takes the second field, lowercases, keeps it only if it
  is one of the codes above; falls back to the `LANG` environment variable with
  the same extraction. If neither matches, the default `"us"` stands. *observed.*
- The same combobox is available in Settings ("Allgemein") after login, along
  with a free-text `zone` field. *observed.*
- The same `login_url` is reachable from a "Amazon Photos im Browser öffnen"
  link that opens the URL externally. *observed.*

### 4.5 Chrome / branded header deviation (committed)

While Amazon's page is open, a slim branded header sits above the webview
(our documented deviation from the original's § 3.1 two-column layout):

- upload logo (30 pt), app name "Prime Photo Uploader", subtitle
  "Bei Amazon anmelden";
- a spinner while no error, otherwise the error text in red;
- an "Abbrechen" button on the right that calls `WebviewLogin::cancel`.
- Window title forced to "Login – Prime Photo Uploader" (§ 4.2).

Rationale documented in `AGENTS.md`: this is a third-party tool, so its
identity and a cancel path must stay reachable. Window *sizes* from the original
remain the benchmark even though our implementation sizes elastically. *observed.*

### 4.6 Webview lifecycle (committed)

`WebviewLogin` state machine (`login.rs`) *observed:*

- `start(window, url, bounds)`: builds the webview as a child of the host
  window with `.with_url(url).with_bounds(bounds)`. Errors are stored in
  `error`. `running` becomes true; probe state reset.
- `set_bounds(rect)`: forwarded to the webview each frame.
- `poll()`: if not running, nothing. Otherwise:
  1. take any completed `outcome` (cookie string) or `error`;
  2. if an outcome was found: store it, set `running=false`, drop the webview,
     return (login complete);
  3. if an error was found: store it (webview stays open);
  4. if no probe in flight and the last probe is at least
     `PROBE_INTERVAL` (500 ms) old (or never probed), mark it in flight with
     `last = now` and schedule a probe.
- `cancel()`: drop the webview, `running=false`. No outcome is produced.

Deferred start: `begin_login` is not called directly from the button click
because creating the webview needs the window handle; the click sets
`pending_login`, and `update` consumes it (after saving config) at the end of
the frame. *observed.*

### 4.7 Asynchronous cookie probe (committed)

`schedule_probe` is dispatched via
`DispatchQueue::main().exec_async`. Inside it:

1. Borrow the thread-local webview; if present, call `wv.cookies()`.
2. Apply `has_login(&cookies)`. If true, format the cookie string into
   `probe.outcome`.
3. On `wry` error, store the first error text.
4. Clear `in_flight = false`.

`has_login` (*observed*):

```
has_session && has_at && has_x
where has_session = any cookie named exactly "session-id"
      has_at     = any cookie name starting with "at-acb"
      has_x      = any cookie name starting with "x-acb"
```

Notably `ubid-acb*` is **not** part of the authentication predicate, but is
still harvested later. The predicate matches anonymous session-less pages
negatively and only fires once Amazon's redirect chain has set the auth
cookies. *observed.*

`cookie_string` joins every cookie in the webview store as `name=value; ` — so
the captured string contains **all** cookies, not only the auth subset. *observed.*

Probe failures (wry error) are surfaced in red but do not stop probing. *observed.*

### 4.8 Cookie parsing and persistence (committed)

`parse_cookie_string` in `main.rs` splits on `;`, matches by name *observed*:

| Cookie name pattern | Field | Notes |
|---|---|---|
| `session-id` | `session_id` | exact match |
| name starting `x-acb` (length > 5) | `cookie_x_acb` (first only) | suffix = detected country (!) |
| name starting `at-acb` | `cookie_at_acb` (first only) | suffix used for country if not yet set |
| name starting `ubid-acb` | `cookie_ubid_acb` (first only) | value unquoted |
| name starting `sess-at-acb` | `cookie_x_amz_access_token` (first only) | access-token-style cookie, not used by `has_login` |
| anything else | ignored | |

Country detection: the first `x-acb*` or `at-acb*` cookie's suffix (after the
prefix) is lowercased and stored as `parsed.country`, which `apply_cookie_string`
writes into `config.country` (overriding the user's picker selection). Because
an Austria login on amazon.de yields `x-acbde`, this **collapses `at`/`ch` into
`de`** — a known, accepted imprecision. *observed (and worth flagging).*

`apply_cookie_string` then calls `Config::save()`; on error the error is stored
in `config_error`. A successful import logs "Login erfolgreich – Cookies
übernommen."

Config schema (`Config`, committed) *observed:*

| Field | Default | Meaning |
|---|---|---|
| `country` | `"us"` | marketplace code driving domain/cookies |
| `zone` | `"eu"` | upload content-zone prefix (`content-<zone>.drive.amazonaws.com`) |
| `user_agent` | Safari 18.2 string | `User-Agent` for upload calls |
| `session_id` | `""` | `session-id` cookie |
| `cookie_x_acb` | `""` | `x-acb<cc>` value |
| `cookie_at_acb` | `""` | `at-acb<cc>` value |
| `cookie_ubid_acb` | `""` | `ubid-acb<cc>` value |
| `cookie_x_amz_access_token` | `""` | `sess-at-acb*` value |

Storage path:
`~/Library/Application Support/amzn_photos_uploader/config.toml` (`config_dir()`).

### 4.9 Web (cookie) flow after sign-in (committed)

`AmznPhoto::build_common_headers` and `renew_auth_token` (committed) build the
upload-cookie header as *observed*:

```
Cookie: session-id=<session_id>; x-acb<country>=<cookie_x_acb>;
       at-acb<country>=<cookie_at_acb>; ubid-acb<country>=<cookie_ubid_acb>
```

plus `x-amzn-SessionId: <session_id>` and the configured `User-Agent`. Uploads go
to `https://content-<zone>.drive.amazonaws.com/v2/upload?…` (see
`SPEC_UPLOAD`). *observed.*

Token refresh happens against
`https://www.amazon.<country>/photos/auth/token?_=<epoch_ms>` with the same
cookie header, returning an `access_token` that is stored in
`cookie_x_amz_access_token`. How (or whether) that token is then used for
uploads is *not visible* in the committed code's header builder (it stores it
but `build_common_headers` does not send `x-amz-access-token`); treat as
*unverified/legacy* — do not change behaviour without a recon. *observed.*

### 4.10 Zone and account details

- `zone` defaults to `"eu"` and is editable in Settings. There is no endpoint
  discovery (no call to `/drive/v2/account/endpoint`), so the content zone is
  **not** derived from sign-in. The desktop client's response there gives
  `contentUrl: https://content-eu.drive.amazonaws.com/` (see `PROTOCOL.md` § 2.4);
  our default matches that one observed region. Other regions: *unknown*.
- No avatar/profile picture, account name, or quota is fetched in the committed
  code; **HEAD** renders only the green/red "● Angemeldet"/"● Nicht angemeldet" status
  line in the sidebar, with no avatar or "Angemeldet als:" text. The account block
  + "Abmelden" are a P2 TODO item and exist only in the (as yet uncommitted)
  working-tree pass.
  P2 item.

### 4.11 Leaving the login flow

- `cancel()` drops the webview and leaves `running=false`; `outcome` stays `None`,
  so `poll_login` does nothing and the user returns to the pre-login screen.
  *observed.*
- Errors remain visible on both the pre-login screen and the in-page header
  until cleared by a new `start`. *observed.*
- Sign-out itself (clearing config) is *not* implemented in the committed code
  (no "Abmelden" handler in the HEAD version consulted); see
  `AGENTS.todo.md` P2 "An-/Abmelden-Icon". *observed gap.*

---

## 5. Mapping: original vs. ours

| Concern | Original (Amazon Photos 11.3) | Ours (committed `HEAD`) |
|---|---|---|
| Sign-in window | Separate `NSWindow`, 880 × 530, titled/closable/miniaturizable | Same eframe window, elastic sizing, header 16/8 margin (deviation) |
| Window title | "Amazon Photos" (`clouddrive_desktop_title`) | "Login – Prime Photo Uploader" (deviation, documented) |
| Closable while signing in | Closing quits the whole app (`HandleShouldClose → Quit`) | "Abbrechen" cancels only the webview; returns to picker |
| Layout | Two columns: 500 pt welcome/marketing panel + 380 × 528 auth container | One column: branded 60 pt header (`HEADER_H`) + webview; our pre-login screen shows logo + marketplace picker |
| Auth page | `https://www.<domain>/ap/signin?…` with device OAuth params, `assoc_handle=amzn_photos_desktop_<region>`, `pageId=amzn_photos_desktop_<amazonRegion>` | Same host/path but minimal param set, `assoc_handle=amzn_photos_web_<cc>` |
| Marketplace handling | Derived from `NSLocale` → `DomainProvider` (11 regions, marketplace id table, deprecated → US) | User picker (14 codes); default from `system_country()` mapping `AppleLocale`/`LANG` to a supported code; domain table 9 explicit arms (11 codes) + passthrough (de/fr/it/es/ca) |
| Locale/region derived values | `language` param, `pageId` | none sent |
| User agent | Mobile Android (Chrome 30 / Android 4.4), to get mobile sign-in | WKWebView default (desktop Safari-like) |
| Back button | 40 pt panel, shown on non-`/ap/signin` pages, label "Zurück zur Anmeldung" | none — the webview carries Amazon's own navigation; cancel is the header's "Abbrechen" |
| No-network handling | Overlay in the auth container, retry button, `NetworkDetector` | `WebviewLogin::error` surfaced in red; page simply fails to load (*no* dedicated no-network overlay) |
| Cleanup before load | `WebCookieManager.DeleteAllCookies()` wipes `NSHttpCookieStorage` | none — wry store starts empty anyway |
| Completion signal | `openid.oa2.authorization_code` query param on a `/ap/` navigation | `session-id` + `at-acb*` + `x-acb*` cookies appearing in the store |
| Post-login consumption | Authorization code → device registration → refresh/access token → `/drive/v2/account/endpoint` | Cookie string parsed into `Config` fields; no device registration; uploads use Cookie header (`session-id`, `x-acb`, `at-acb`, `ubid-acb`) + `x-amzn-SessionId` |
| Sign-in outcome detectivity | Success = auth code captured | Success = auth cookie predicate |
| Profile / avatar | not in the sign-in window itself (account info comes later) | not implemented in HEAD; P2 item |

### 5.1 Flow distinction (important)

The two are **different authentication systems**:

- **Original = device flow.** The auth portal is an OAuth 2.0 + PKCE entry point
  (`device_auth_access` scope, `authorization_code`, code verifier). The desktop
  app registers itself as a device, receives a refresh token, exchanges it for
  access tokens, and talks to `drive.amazonaws.com` / `upload-photos.amazon.de`
  with bearer tokens. See `PROTOCOL.md`.
- **Ours = web (cookie) flow.** We never register a device or handle a refresh
  token. We capture the browser session cookies set by `amazon.<tld>` during a
  normal sign-in and replay them against `content-<zone>.drive.amazonaws.com`.
  This matches the entourloop reference implementation, not the desktop client.

Consequences: features the original derives from sign-in (per-account endpoint
URLs, source id, quota, region) are **not** available to us without extra work;
where we approximate them (fixed `zone = eu`) we must not present it as
auth-derived. *inferred from the comparison above.*

---

## 6. German UI labels quoted (original)

| Key | Value |
|---|---|
| `clouddrive_desktop_title` | "Amazon Photos" |
| `clouddrive_desktop_login_marketing_header` | "Laden Sie Fotos und Videos von Ihrem Computer, Ihrer Kamera und Ihrer Speicherkarte hoch" |
| `clouddrive_desktop_login_marketing_header1` | "Einfach speichern" |
| `clouddrive_desktop_login_marketing_header2` | "Sichern Sie Dateien, Dokumente und Fotos von Ihrem Computer." |
| `clouddrive_desktop_login_marketing_header3` | "Amazon Photos speichert alles sicher an einem Ort und bietet einfachen Zugriff." |
| `clouddrive_desktop_login_back_button` | "Zurück zur Anmeldung" |
| `clouddrive_desktop_login_ui_no_network_heading` | "Keine Internetverbindung" |
| `clouddrive_desktop_login_ui_no_network_description` | "Überprüfen Sie Ihre Internetverbindung und versuchen Sie erneut, sich bei Amazon Photos anzumelden." |
| `clouddrive_desktop_login_ui_retry_button` | "Wiederholen" |

Our own labels (committed UI) — *observed*:

| Where | Text |
|---|---|
| Webview header subtitle | "Bei Amazon anmelden" |
| Pre-login primary button | "Mit Amazon anmelden" |
| Combobox caption | "Land / Amazon-Marktplatz:" |
| Cancel button | "Abbrechen" |
| External-browser link | "Amazon Photos im Browser öffnen" |
| Success log | "Login erfolgreich – Cookies übernommen." |
| Webview error prefix (page) | "Anmeldeseite konnte nicht geöffnet werden: {e}" |
| Cookie error prefix | "Cookies konnten nicht gelesen werden: {e}" |
| Window title (login state) | "Login – Prime Photo Uploader" |

---

## 7. Open questions / unverifiable points

Marked *unknown* rather than guessed:

1. **`language` sanitisation** in the original (`Validation.SanitizeLanguage`) —
   exact algorithm not decompiled here.
2. **`NetworkDetector`** implementation (how availability is observed).
3. **Back-button destination** (`BackButtonClickedHandler`) — handler body not
   captured in this pass.
4. **`sess-at-acb` cookie semantics** — our parser treats it as an access
   token, but we have not confirmed against live traffic whether Amazon sets a
   cookie by that name for the web flow, or whether `cookie_x_amz_access_token`
   is ever needed for uploads. Header builder does not send it. *unverified.*
5. **Content-zone derivation:** whether any region other than EU needs a
   different `zone` (`content-<zone>.drive.amazonaws.com`); observed only
   `content-eu`. Our code never derives the zone from sign-in.
6. **Validity of `amzn_photos_web_<cc>`** for all 14 codes in our picker —
   Amazon may reject or collapse some (e.g. `at`/`ch` → `de`). Not empirically
   verified beyond the fact that the page loads.
7. **Whether `at-acb*`/`x-acb*` cookies always appear before we probe.** The
   predicate requires both, so a session that yields only one of them will
   never complete login. Empirical timing not measured.
8. **Webview first-run cookie state:** wry's store behaviour across app
   restarts (whether Amazon's session persists) is not pinned down here.
9. **Original's handling of expired sessions during sign-in** (auto-refresh,
   account-status invalid retry) — partially covered by § 3.5 but the invalid-
   account path's UI is *unknown*.
10. **Our sign-out** is not implemented in committed `HEAD` (no handler wiring
    for "Abmelden"); the flow exists only as a TODO (P2).
11. **Main-window sizing deviation:** original uses fixed 1130 × 770; ours
    sizes elastically. Whether Amazon's UI breaks under other sizes is not part
    of this spec (see `SPEC_UPLOAD` / UI specs).

## 8. Files of interest (committed `HEAD`)

- `src/login.rs` — `amazon_domain`, `login_url`, `WebviewLogin`, `has_login`,
  `PROBE_INTERVAL`.
- `src/config.rs` — `Config` fields and defaults.
- `src/main.rs` — `is_logged_in`, `begin_login`, `poll_login`,
  `apply_cookie_string`, `parse_cookie_string`, `login_ui`, `login_screen_ui`,
  `system_country`, `COUNTRIES`, country-comboboxes, window-title logic,
  `pending_login` deferral.
- `src/amzn_photo.rs` — `build_common_headers`, `renew_auth_token` (cookie
  header, zone interpolation).

## 9. Change checklist (if editing this feature)

- Keep cookie predicate (`session-id` + `at-acb*` + `x-acb*`) in sync with
  § 4.7 if the auth set is revised; add a migration note in `AGENTS.todo.md`.
- Any new country codes must be added in three places: `COUNTRIES` (UI),
  `amazon_domain` (URL), and `system_country` coverage; document in
  `AGENTS.md` Decisions.
- Probe timing (`PROBE_INTERVAL`) must remain off the egui `update` thread.
- Login-window title and branded header are **documented deviations**; do not
  silently revert to the original's two-column layout.
- Never implement device flow by accident: this spec intentionally scopes `ours`
  to the web (cookie) flow; the device flow lives in `PROTOCOL.md`.

---

*Clean-room note: tables and wording above are original authorship; German
strings and enum/location values are quoted facts from the shipped app and its
committed source. No decompiled code was reproduced verbatim.*
