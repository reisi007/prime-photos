# About and third-party licenses

The About window shows the app logo, version, and a button to open the
scrollable license list. The license list covers **all** dependencies from
`Cargo.lock`, with full license texts.

## How it works

- The About window is deliberately built **without** Amazon references
  (branding rule): no terms-of-service link, no privacy link, no
  third-party web link, no device serial number, no "send logs" button, no
  version notices.
- **Own license: `MIT OR Apache-2.0`** — `LICENSE-MIT` + `LICENSE-APACHE`
  in the repo, `license = "MIT OR Apache-2.0"` in `Cargo.toml`. Previously
  the repo had **no** license: for a public release that was an open gap.
- **License list = all dependencies from `Cargo.lock`**, deliberately
  **over-complete** — including transitive and Windows-/Linux-only crates
  that do not land in the macOS binary. Generated per
  `cargo metadata --format-version 1 --locked` from
  **`packaging/gen-third-party-licenses.sh`** to
  **`assets/third-party-licenses.txt`**; the script is **deterministic**
  (no timestamp) and runs **offline**. `cargo-license`/`cargo-about` are
  **not** installed and will **not** be added.
- **Full license texts are included** (Apache-2.0 §4 and MIT require
  **attaching** the license). The texts are collected from the **local
  crate copies** (via `manifest_path` of the metadata), identical texts are
  deduplicated by sha256; if a text is missing, it is **reported** instead of
  invented. No network in the generator.
- **Display:** a dedicated, **scrollable license window**, reachable via a
  button in the **About** window (at ~760 components the only readable form).
  Two sources are shown: the generated list **and** the curated icon
  attribution from `../dev/THIRD_PARTY.md` (which says: "The About/license
  page (Welle 4) reads from this file" — a source of truth).
- **Trigger: tray menu + macOS app menu**, entry "**Über Prime Photos**",
  **without** keyboard shortcut — exactly the two paths of the original
  (`clouddrive_desktop_contextmenu_support_about`), only with our name. In
  the app menu it is the **first** entry (like in the original). **New
  mechanism for this:** a tiny ObjC target object (`objc2::define_class!`,
  objc2 0.6 is already a dependency) sets a static flag and wakes the UI —
  the AppKit menu items have `nil` target and run via the responder chain,
  so a custom selector needs its own target. No `muda` rebuild of the main
  menu (that would risk the edit group running via the responder chain).
- **Copyright line: "© <year> Prime Photos"**, year at runtime from the
  system time (like the original puts its current year). The Amazon wording
  is **not** adopted.
- **Version:** `CARGO_PKG_VERSION`; the commit ID only if available at build
  time — otherwise omit, **invent nothing** (the original shows
  `AssemblyVersion.CommitId`).
- **Deliberately not reproduced** (branding rule / Post-MVP):
  terms-of-service and privacy link (point at Amazon pages in the original),
  the third-party **web** link, the **device serial number**, "send logs"
  and the version notices (`../original/SPEC_ABOUT.md` §9).

## Decisions

- **About + Drittanbieter-Lizenzen (Welle 4, User-Entscheidungen 2026-10-16):**
  - **Eigene Lizenz: `MIT OR Apache-2.0`** (User-Entscheidung 2026-10-16) —
    `LICENSE-MIT` + `LICENSE-APACHE` im Repo, `license = "MIT OR Apache-2.0"`
    in `Cargo.toml`. Vorher hatte das Repo **gar keine** Lizenz: für ein
    öffentliches Release war das eine offene Lücke.
  - **Lizenzliste = alle Abhängigkeiten aus `Cargo.lock`**
    (User-Entscheidung: „alle Deps"), bewusst **übervollständig** — auch
    transitiv und auch Windows-/Linux-only-Crates, die nicht im macOS-Binary
    landen. Erzeugt per `cargo metadata --format-version 1 --locked` aus
    **`packaging/gen-third-party-licenses.sh`** nach
    **`assets/third-party-licenses.txt`**; das Skript ist **deterministisch**
    (kein Zeitstempel) und läuft **offline**. `cargo-license`/`cargo-about`
    sind **nicht** installiert und werden **nicht** aufgenommen.
  - **Vollständige Lizenztexte gehören mit hinein** (User-Entscheidung):
    Apache-2.0 §4 und MIT verlangen das **Beilegen** der Lizenz. Die Texte
    werden aus den **lokalen Crate-Kopien** gesammelt (über `manifest_path`
    der Metadaten), identische Texte per sha256 **dedupliziert**; fehlt ein
    Text, wird das **ausgewiesen** statt erfunden. Kein Netzwerk im
    Generator.
  - **Darstellung:** eigenes, **scrollbares Lizenz-Fenster**, erreichbar
    über einen Knopf im **About**-Fenster (bei ~760 Komponenten die einzige
    lesbare Form). Zwei Quellen werden gezeigt: die generierte Liste **und**
    die kuratierte Icon-Attribution aus `../dev/THIRD_PARTY.md` (dort steht:
    „Die About-/Lizenzseite (Welle 4) liest aus dieser Datei" — eine Quelle
    der Wahrheit).
  - **Auslöser: Tray-Menü + macOS-App-Menü**, Eintrag „**Über Prime
    Photos**", **ohne** Tastenkürzel — genau die zwei Pfade des Originals
    (`clouddrive_desktop_contextmenu_support_about`), nur mit unserem Namen.
    Im App-Menü ist er der **erste** Eintrag (wie im Original). **Neue
    Mechanik dafür:** ein winziges ObjC-Zielobjekt (`objc2::define_class!`,
    objc2 0.6 ist bereits Dependency) setzt ein statisches Flag und weckt die
    UI — die AppKit-Menüpunkte haben `nil`-Target und laufen über die
    Responder-Chain, ein eigener Selektor braucht deshalb ein eigenes
    Target. Kein `muda`-Umbau des Hauptmenüs (das würde die per
    Responder-Chain funktionierende Bearbeiten-Gruppe riskieren).
  - **Copyright-Zeile: „© <Jahr> Prime Photos"**, Jahr zur Laufzeit aus der
    Systemzeit (wie das Original sein laufendes Jahr einsetzt). Die
    Amazon-Formulierung wird **nicht** übernommen.
  - **Version:** `CARGO_PKG_VERSION`; die Commit-ID nur, wenn sie zur
    Build-Zeit verfügbar ist — sonst weglassen, **nichts erfinden** (das
    Original zeigt `AssemblyVersion.CommitId`).
  - **Bewusst nicht nachgebaut** (Branding-Regel bzw. Post-MVP):
    Nutzungsbedingungen- und Datenschutz-Link (zeigen im Original auf
    Amazon-Seiten), der Drittanbieter-**Web**-Link, die
    **Geräte-Seriennummer**, „Protokolle senden" und die Versionshinweise
    (`../original/SPEC_ABOUT.md` §9).
  - **Verworfen:** nur die **direkten** Abhängigkeiten listen (erfüllt die
    Attributionspflicht für transitive Crates nicht); die Liste als
    **Textdatei ins Bundle** legen statt sie zu rendern; About nur im
    **App-Menü**; **ohne** Copyright-Zeile; nur Komponentenliste **ohne**
    Lizenztexte; die eigene Lizenz später zu klären.

## Original reference

- `../original/SPEC_ABOUT.md` — About window and version notices of the original.
- `../dev/THIRD_PARTY.md` — Curated third-party icon attribution.
- `../dev/RELEASE.md` §11 — License generation.
