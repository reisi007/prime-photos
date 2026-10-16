# Tray icon and menu

The menu-bar icon appears **only while the user is signed in**. Without a
session there is no icon — and therefore also no close-to-tray behaviour:
closing the window quits the app.

## How it works

- Glyphs follow the state: idle, active, paused, done, error.
- On macOS the glyph is a template image and follows the system appearance;
  elsewhere the icon is chosen by theme, falling back to the coloured logo
  when the theme is unknown.
- The menu offers "Fenster anzeigen", "Alle anhalten"/"Alle fortsetzen"
  and "Beenden", in the current UI language.
- The tray item has a **dynamic tooltip** (`tray_tooltip`):
  "Prime Photos" (idle, brand only), "Prime Photos – Angehalten",
  "Prime Photos – In Bearbeitung", "Prime Photos – Mit Fehlern abgeschlossen",
  "Prime Photos – Abgeschlossen". The pause entry is labelled "**Alle
  anhalten**" ↔ "**Alle fortsetzen**" (original wording). The **App label**
  remains unchanged ("Alle Sicherungsvorgänge anhalten" ↔ "… fortsetzen").
- User assets live in **`assets/`** — as **SVG sources** (one source of
  truth). In the build, `build.rs` rasterises them with **`resvg`** (pure
  Rust library, `default-features = false` ⇒ 53 crates, **only**
  `[build-dependencies]`, no runtime dependency, binary stays small) to
  `OUT_DIR`, embedded per `include_bytes!(concat!(env!("OUT_DIR"), …))` and
  decoded with the existing `image` crate. **Why resvg:** `sips`/`qlmanage`
  deliver good quality on the Mac but only there — our **CI and the release
  matrix are Linux/Windows**; ImageMagick's internal renderer (`rsvg-convert`
  is missing) visibly does not paint the strokes. resvg quality was reviewed
  on exactly these SVGs (all five tray states + both logos, black/white,
  gradient correct). **Theme without new dependency:**
  `egui::Context::system_theme()` (available in 0.31) — dark ⇒ white tray
  variants, light ⇒ black; `None` (unknown) ⇒ the coloured logo, which is
  visible on every background. macOS keeps the black template set.
- **No fallback:** if an SVG is missing, `build.rs` breaks the build
  (compile error) — no `Option`, no `exists()` check, no fallback to a drawn
  variant. Decode errors at runtime = fail fast.

## Decisions

- **Assets: Tray-Icons + Logo (User 2026-10-14) — implementiert** (Befund
  2025-10-15: `build.rs` rastert die SVGs bereits, `src/tray.rs` bettet sie
  ein; der frühere Stand „geplant, nicht implementiert" war veraltet).

  Eigene Assets des Users liegen in **`assets/`** — als **SVG-Quellen** (eine
  Quelle der Wahrheit). **User-Entscheidung 2026-10-14: im Build entstehen
  daraus PNGs, die mit ausgeliefert werden.** Umsetzung: `build.rs` rastert
  sie mit **`resvg`** (reine Rust-Bibliothek, `default-features = false` ⇒ 53
  Crates, **nur** `[build-dependencies]`, keine Laufzeit-Abhängigkeit, Binary
  bleibt unverändert klein) nach `OUT_DIR`, eingebettet per
  `include_bytes!(concat!(env!("OUT_DIR"), …))` und dekodiert mit der
  vorhandenen `image`-Crate. **Warum resvg:** `sips`/`qlmanage` liefern auf
  dem Mac gute Qualität, bauen aber nur dort — unser **CI und die
  Release-Matrix sind Linux/Windows**; ImageMagicks interner Renderer
  (`rsvg-convert` fehlt) malt die Striche nachweislich nicht. resvg-Qualität
  wurde an genau diesen SVGs gesichtet (alle fünf Tray-Zustände + beide Logos,
  schwarz/weiß, Farbverlauf korrekt). **Theme ohne neue Abhängigkeit:**
  `egui::Context::system_theme()` (in 0.31 vorhanden) — dunkel ⇒ weiße
  Tray-Varianten, hell ⇒ schwarze; `None` (unbekannt) ⇒ das farbige Logo, das
  auf jedem Hintergrund sichtbar ist. macOS bleibt beim schwarzen
  Template-Satz. **Kein Fallback (User 2026-10-14):** fehlt ein SVG, bricht
  `build.rs` den Build (Compile-Fehler) — kein `Option`, keine
  `exists()`-Prüfung, kein Rückfall auf eine gezeichnete Variante.
  Dekodier-Fehler zur Laufzeit = fail fast.

- **UI-Icons: Bibliothek statt selbst gezeichnet (Welle 3c, 2026-10-16).**
  `src/icons.rs` enthält nach Welle 3c **keine** selbst gezeichneten
  Navigations-Glyphen mehr. Die beiden Sidebar-Icons sind **Material
  Symbols**-SVGs (Apache-2.0, Google LLC) über **dieselbe** Pipeline:
  `assets/icon-cloud.svg` („Sicherung") und `assets/icon-settings.svg`
  („Einstellungen") — **gefüllte** Variante (`…_fill1_24px.svg`), 64-px-Raster,
  zur Laufzeit auf `TEAL` (ausgewählt/gehover) bzw. `MUTED` getönt, im
  festen 18 × 18-Rect der Icon-Spalte. Motiv **und** Stil folgen dem
  Original-Bundle (gesichtet: `navmenu-backup@3x.png` = massiv gefüllte
  Wolke, `navmenu-settings@3x.png` = gefülltes Zahnrad mit Loch) —
  Projektregel Original-Parität. **Gefüllt gilt nur für die Navigation:** die
  Zeilen-Icons (Kebab, Refresh) bleiben als **Linie** (Outlined), weil sie im
  Original ebenfalls Linien sind. **Selbst gezeichnet bleiben:** die
  Marktplatz-Flaggen und unser Logo (`upload_logo`); der Status-Punkt ist ein
  Kreis, kein Glyph. **Verworfen:** `backup` (Wolke mit Hochpfeil) + `tune`
  (drei Schieberegler) — nicht die Motive des Originals; die
  **Outlined**-Variante für die Navigation — weicht sichtbar vom Original ab.

- **Tray: Statuszeile + Pause-Labels (User-Entscheidung 2026-10-16) — überholt durch Welle N
  (2026-10-16).** Geplant war eine **nicht klickbare Statuszeile** („Bereit" / „Upload läuft: x/y" /
  „Angehalten" / „Letzter Upload: x/y Erfolgreich") und die Labels „Alle Sicherungen anhalten" ↔
  „Sicherung starten". **Gewählt wurde stattdessen** (Welle N) ein **dynamischer Tooltip**
  („Prime Photos – {Status}", siehe „How it works") und die Labels **„Alle anhalten"** ↔ **„Alle
  fortsetzen"** (Original-Wortlaut). Das **App-Label** bleibt unverändert („Alle Sicherungsvorgänge
  anhalten" ↔ „… fortsetzen"). Anlass: „Es ist alles abgeschlossen, da macht ‚Anhalten' keinen
  Sinn."

## Original reference

- `../original/SPEC_OS_INTEGRATION.md` — OS integration (tray, autostart, menu).
- `../dev/THIRD_PARTY.md` — Third-party icon attributions.
