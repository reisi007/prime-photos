# Languages (i18n)

German and English, with the OS locale as the default and an override in the
settings. The embedded sign-in page keeps the marketplace's own language —
Amazon decides it from the TLD and cookies, and forcing it would be unproven
and could alter the sign-in page.

## How it works

- The UI language is stored in `Config.language` (`Option<String>`, serde
  default = system locale).
- Default = OS locale; override in settings.
- **Detection order** (only when `Config.language` is `None`, i.e. the
  "Automatic (system)" default): the POSIX environment variables come first, in
  their precedence order `LC_ALL`, `LC_MESSAGES`, `LANG` — the first one that is
  set and non-empty decides, so an explicit POSIX override keeps winning. A GUI
  app launched from **Finder**, however, inherits none of them, so on **macOS**
  the real OS locale is read via `NSLocale::currentLocale().localeIdentifier()`
  as a fallback. On **Windows/Linux** there is no such API — the environment is
  all there is.
- **Classification:** the language prefix `de*` (case-insensitive; `de_AT`,
  `de-DE`, `de`) ⇒ German, everything else (`en_US`, `fr_FR`, `C`, `POSIX`,
  unset) ⇒ English. On the reference machine (`AppleLocale = de_AT`) this makes
  the app start in German.
- The embedded sign-in page keeps the marketplace's own language — Amazon
  decides it from the TLD + cookies/`Accept-Language`. We force **nothing**.
  Consequence: German UI + `amazon.com` ⇒ English sign-in page — deliberately
  accepted.
- **Latin languages only** (de, en, fr, it, es, pt). **No CJK** (the bundled UI
  font is Latin-only; a CJK font would add ~16 MB) and **no RTL** (Arabic /
  Hebrew only in the web view, not in our own chrome).
- Country/marketplace names in `countries.rs` are UI text (→ i18n): the
  table holds only the German name key, the label is built via `country_label`
  using `tr` + literal domain.

## Decisions

- **Internationalisierung (i18n) — User-Entscheidung 2026-10-13:**
  - **Jetzt einführen, minimal:** direkt **nach REF-2** (dann sind die
    UI-Strings frisch pro Feature-Modul sortiert) eine zentrale String-Schicht
    `src/i18n.rs` mit `Lang`-Enum und einer Tabelle. Zuerst **de + en**
    vollständig; jede weitere Sprache ist danach **additiv** (kein Umbau).
  - **Sprachwahl:** Default = OS-Locale, **Override in den Einstellungen**;
    im `Config` persistiert (`language`, serde-Default aus der OS-Locale,
    damit alte `config.toml` weiter lädt).
  - **Umfang mittelfristig: lateinische Sprachen** (de, en, fr, it, es, pt).
    **Kein CJK** (der gebündelte UI-Font ist Latin-only; ein Noto-CJK-Font
    bräuchte ~16 MB zusätzlich) und **kein RTL** (Arabisch/Hebräisch nur in der
    Webview, nicht in unserem Chrome).
  - **Webview-Sprache:** **Marktplatz-Default belassen** — Amazon entscheidet
    die Seitensprache aus TLD + Cookies/`Accept-Language`. Wir erzwingen
    **nichts** (`lc-`-Cookie/`language`-Parameter wären unbelegt und könnten
    die Login-Seite verändern). Folge: deutsches UI + `amazon.com` ⇒
    Loginseite englisch — bewusst akzeptiert.
  - **Regel:** Kein hartkodierter UI-Text außerhalb von `src/i18n.rs`. Neue
    Strings kommen direkt zweisprachig dazu. Die Marktplatz-Ländernamen in
    `countries.rs` sind ebenfalls UI-Text (→ i18n).

- **macOS-Sprachquelle live bestätigt (2026-10-17):** Ein Finder-/Doppelklick-Start
  erbt kein `LANG`/`LC_ALL`/`LC_MESSAGES`; seit 5.21 liest macOS deshalb zusätzlich
  `NSLocale::currentLocale().localeIdentifier()` als Zusatzquelle (die
  Env-Variablen bleiben vorrangig). Live geprüft am installierten Bundle
  (Version 0.1.0): der Start war **deutsch** (Systemsprache `de_AT`) — ohne diesen
  macOS-Zweig war die UI englisch. Die Einstellung „Automatisch (Systemsprache)"
  greift damit auch ohne POSIX-Umgebung; der Override in den Einstellungen bleibt
  unverändert.

- **Native macOS-Menüs (Prime-Photos-Vertrag, Issue #8 umgesetzt):** Das
  App-Menü („Prime Photos" als Titel, „Über Prime Photos", „Prime Photos
  beenden", Ausblenden/Einblenden/Dienste), das Edit-Menü (Titel sowie
  Rückgängig, Wiederholen, Ausschneiden, Kopieren, Einfügen, Alles auswählen)
  und das Window-Menü (Titel sowie Minimieren, Schließen) zeigen die aktuelle
  UI-Sprache. Rune installiert seine Standardmenüs selbst und zwar mit festen
  englischen Titeln; `app_menu::rune_standard` findet die vorhandenen Einträge
  über ihre stabilen AppKit-Selektoren und benennt sie **in place** um
  (idempotent, keine Struktur-/Key-Änderung, No-op solange ein Eintrag fehlt).
  Das passiert beim Start (`install_rune_about`/`ensure_rune_about_installed`)
  und nach einem Sprachwechsel über `app_menu::refresh_labels`, das direkt nach
  `i18n::set_lang` läuft. Das Tray folgt derselben Sprachumstellung über
  `LiveState::set_language` → `Effect::RefreshTray` →
  `tray_route::request_label_refresh()` → `Tray::set_labels` (`src/tray.rs`).
  Das ist der Vertrag für **unser** macOS-Menü;
  er sagt nichts darüber aus, ob oder wie die Original-App ihre Menüs
  lokalisiert — die Menübefunde in `docs/original/SPEC_OS_INTEGRATION.md` sind
  reine Verhaltensbeobachtung. Für Windows/Linux wird kein entsprechendes
  natives Menüverhalten zugesagt; dort ist der Refresh ein No-op.

- **Dateinamen-Kodierung: NFC (User-Entscheidung 2026-10-14):** die
  Upload-Namens-Kodierung (NFC) betrifft auch den Anzeigetext — Details in
  [upload.md](upload.md).

## Original reference

- `../dev/I18N.md` — i18n implementation details (API, rules, recipes).
