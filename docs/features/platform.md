# Platform and portability

Prime Photos targets **macOS (Apple Silicon), Windows, and Linux** as
equal-grade platforms. New features use cross-platform crates, not
macOS-specific APIs.

## How it works

- **Target: fully cross-platform.** macOS (Apple Silicon), Windows and
  Linux are equal targets. Reversal of the earlier "native macOS" goal.
- **New features via cross-platform crates** (not via macOS APIs):
  menu bar/tray → `tray-icon`, notifications → `notify-rust`, autostart →
  `auto-launch`. All three work on macOS/Windows/Linux.
- **Deliberately macOS-specific** (with `#[cfg(target_os = "macos")]` or
  no-op fallback on other OS): the native app menu (`src/app_menu.rs`, so
  that ⌘X/⌘C/⌘V work in the web view — on Windows/Linux the web view does it
  itself) and the main-queue scheduling of the cookie probe in
  `src/login.rs` (the run-loop re-entry is a macOS problem; other OS probe
  directly).
- The embedded login web view remains a **`wry` child view** — `wry` is
  cross-platform (macOS WKWebView / Windows WebView2 / Linux WebKitGTK) and
  is attached to the eframe window via `raw-window-handle`; no change to the
  fundamental architecture is needed.
- **Signing is only relevant on macOS.** There a (self-signed) code
  signature is required so that notifications/bundle identifier work
  cleanly; Windows/Linux need none. The macOS `.app` bundling + signature
  is delivered as a separate package.
- **Verification boundary:** On this machine only the **macOS build is
  runnable and testable**. Windows/Linux are covered by `cfg` guards and
  cross-platform crates and are **compiled** in the release workflow
  (Windows x64 on `windows-latest`, Linux x64 on `ubuntu-latest`; both legs
  green so far) — **not** run-tested. A cross-compile check on the reference
  machine (`cargo check --target …`) is still **not performed** (only
  `aarch64-apple-darwin` and `wasm32-unknown-unknown` are installed). This is
  a known, accepted limitation.
- **Marketplace URLs central (project rule, user decision 2026-10-14):**
  Every URL that depends on the **marketplace** (or the zone) is built
  **exclusively in `src/urls.rs`**. Outside this file there must be **no**
  `format!("https://www.amazon.{}…")`, **no** `thumbnails-photos.amazon.`
  literal and **no** `content-{zone}` literal (tests belong in the util).
  Reason: the profile-picture implementation had hardcoded the thumbnail
  host to `.de` — this was only caught in verification, because URLs were
  scattered across multiple files. The country→domain mapping
  (`amazon_domain`, today `src/login.rs`) moves into the util; `login.rs`,
  `prime_photos.rs` and `app.rs` get it from there. **Every new route:** util
  function + test that checks at least `de` **and** a non-DE marketplace.
  Marketplace-neutral means: no literal TLD in the code, the derivation
  happens only in the util.

## Decisions

- **Plattform / Portierung (User-Entscheidung 2026-10-13):**
  - **Ziel: voll cross-platform.** macOS (Apple Silicon), Windows und Linux
    sind gleichwertige Ziele. Kehrtwende vom früheren „Native macOS"-Ziel.
  - **Neue Features über cross-platform Crates** (nicht über macOS-APIs):
    Menüleiste/Tray → `tray-icon`, Benachrichtigungen → `notify-rust`,
    Autostart → `auto-launch`. Alle drei funktionieren auf
    macOS/Windows/Linux.
  - **Bewusst macOS-spezifisch bleiben** (mit
    `#[cfg(target_os = "macos")]` bzw. No-op-Fallback auf anderen OS): das
    native App-Menü (`src/app_menu.rs`, damit ⌘X/⌘C/⌘V im WebView gehen —
    auf Windows/Linux erledigt das die WebView selbst) und die
    Main-Queue-Scheduling der Cookie-Probe in `src/login.rs` (der
    Run-Loop-Reentry ist ein macOS-Problem; andere OS proben direkt).
  - Das eingebettete Login-WebView bleibt ein **`wry`-Kind-View** — `wry`
    ist cross-platform (macOS WKWebView / Windows WebView2 / Linux WebKitGTK)
    und wird über `raw-window-handle` an das eframe-Fenster gehängt; keine
    Änderung an der Grundarchitektur nötig.
  - **Signierung ist nur auf macOS relevant.** Dort ist eine
    (selbstsignierte) Codesignatur nötig, damit
    Benachrichtigungen/Bundle-Identifier sauber funktionieren;
    Windows/Linux brauchen keine. Das macOS-`.app`-Bundling + Signatur wird
    als eigenes Paket geliefert.
  - **Verifikationsgrenze:** Auf diesem Rechner ist nur der **macOS-Build
    lauffähig und testbar**. Windows/Linux werden durch `cfg`-Guards und
    cross-platform Crates abgedeckt und im Release-Workflow **kompiliert**
    (Windows x64 auf `windows-latest`, Linux x64 auf `ubuntu-latest`; beide
    Beine bisher grün) — **laufgetestet** sind sie damit nicht. Ein
    Cross-Compile-Check auf dem Referenzrechner (`cargo check --target …`) ist
    weiterhin **nicht durchgeführt** (dort sind nur `aarch64-apple-darwin` und
    `wasm32-unknown-unknown` installiert). Das ist eine bekannte, akzeptierte
    Einschränkung.
  - **Verworfen:** „selbstsigniertes `.app` nur für macOS, sonst nichts" als
    Antwort auf den Benachrichtigungs-/Menüleisten-Wunsch (war die erste
    Teilantwort des Users, ersetzt durch „voll cross-platform").

- **Marktplatz-URLs zentral (Projektregel, User-Entscheidung 2026-10-14):**
  Jede vom **Marktplatz** (oder der Zone) abhängige URL wird **ausschließlich
  in `src/urls.rs`** gebuilt. Außerhalb dieser Datei darf **kein**
  `format!("https://www.amazon.{}…")`, **kein** `thumbnails-photos.amazon.`-
  Literal und **kein** `content-{zone}`-Literal stehen (Tests gehören in die
  Util). Anlass: die Umsetzung des Profilbilds hatte den Thumbnail-Host auf
  `.de` hartkodiert — das fiel erst in der Verifikation auf, weil URLs über
  mehrere Dateien verstreut waren. Die Land→Domain-Zuordnung
  (`amazon_domain`, heute `src/login.rs`) zieht in die Util um; `login.rs`,
  `prime_photos.rs` und `app.rs` beziehen sie von dort. **Jede neue Route:**
  Util-Funktion + Test, der mindestens `de` **und** einen Nicht-DE-Marktplatz
  prüft. Marktplatzneutral heißt: kein Literal-TLD im Code, die Ableitung
  passiert nur in der Util.

- **Win/Linux: In-App-„Beenden" fehlt (Residuum aus PORT-3) — Rückweg jetzt auch dort
  (User-Entscheidung 2026-10-17).** Ohne sichtbares Tray-Icon gibt es dort keinen
  In-App-„Beenden"-Knopf (macOS ist über ⌘Q safe); der **Rückweg** aus dem versteckten Zustand
  existiert aber jetzt plattformübergreifend: der Tray-Eintrag „Fenster anzeigen", ein
  **App-Neustart** (unter macOS aktiviert Launch Services die laufende Instanz; ein Dock-/Taskbar-
  Klick ist ein weiterer Weg, sofern ein Icon vorhanden ist) — immer über denselben
  `show_main_window`-Pfad (Fenster zeigen + fokussieren + Dock-Icon zurück,
  `src/app_activation.rs`/`src/app.rs`). Der **Einzelinstanz-Schutz** greift plattformabhängig: auf
  macOS lehnt Launch Services den zweiten Start über `LSMultipleInstancesProhibited` ab, sonst
  fängt `src/single_instance.rs` ihn über den Socket-/TCP-Handshake ab. Die offene Arbeit
  (In-App-„Beenden" unter Win/Linux) bleibt im Ledger.
