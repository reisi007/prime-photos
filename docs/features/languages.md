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
- **Latin languages only** (de, en, fr, it, es, pt). **No CJK** (egui's
  default font is Latin-only → would need a bundled Noto-CJK font, ~16 MB,
  or an OS-loaded font) and **no RTL** (egui has no bidi layout; Arabic /
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
    **Kein CJK** (egui's Default-Font ist Latin-only → bräuchte einen
    Noto-CJK-Font, gebündelt ~16 MB oder pro OS geladen) und **kein RTL** (egui
    hat kein Bidi-Layout; Arabisch/Hebräisch nur in der Webview, nicht in
    unserem Chrome).
  - **Webview-Sprache:** **Marktplatz-Default belassen** — Amazon entscheidet
    die Seitensprache aus TLD + Cookies/`Accept-Language`. Wir erzwingen
    **nichts** (`lc-`-Cookie/`language`-Parameter wären unbelegt und könnten
    die Login-Seite verändern). Folge: deutsches UI + `amazon.com` ⇒
    Loginseite englisch — bewusst akzeptiert.
  - **Regel:** Kein hartkodierter UI-Text außerhalb von `src/i18n.rs`. Neue
    Strings kommen direkt zweisprachig dazu. Die Marktplatz-Ländernamen in
    `countries.rs` sind ebenfalls UI-Text (→ i18n).

- **Dateinamen-Kodierung: NFC (User-Entscheidung 2026-10-14):** die
  Upload-Namens-Kodierung (NFC) betrifft auch den Anzeigetext — Details in
  [upload.md](upload.md).

## Original reference

- `../dev/I18N.md` — i18n implementation details (API, rules, recipes).
