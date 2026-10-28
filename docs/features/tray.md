# Tray icon and menu

The menu-bar icon appears **only while the user is signed in**. Without a
session there is no icon — and therefore also no close-to-tray behaviour:
closing the window quits the app.

## How it works

- Glyphs follow the state: idle, active, paused, done, error.
- **Error means failures remaining after the retry pass:** "Mit Fehlern
  abgeschlossen" is shown only when at least one file still failed after its
  one extra file-level retry (see [upload.md](upload.md)). A file that succeeds
  on the retry is a normal success and does not set the error glyph.
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
- **Close-to-tray (unchanged):** closing the window **hides** it (the close is
  cancelled) and the app keeps running in the background; only the tray's
  "Beenden" (or a real quit) exits.
- **Dock icon follows the window (macOS, user decision 2026-10-18):** while a
  window is open the app is a normal windowed app — Dock icon and app menu bar
  present (`Regular`). Once the window is hidden (close-to-tray) it switches to
  the **`Accessory`** activation policy — no Dock icon, no app menu — and back
  to `Regular` when the window is shown again. This is **original parity**: the
  original runs a dynamic Dock icon the same way (`Regular` while a window is
  open, `Accessory` when none is — `../original/SPEC_OS_INTEGRATION.md` §3.4).
  The bundle now sets `LSUIElement = true`, also matching the original; the
  runtime policy supplies the Dock icon while a window is open.
- **Silent login-item start (macOS, user decision 2026-10-18):** when the app is
  started **as a login item** (autostart) it runs **without a window and without
  a Dock icon** — tray only; a **manual** start (Finder, Dock, Spotlight,
  `open -a`, shell) shows the window as before. The login-item start is
  recognised the same way the original does it: a retained observer reads the
  current Apple event during `NSApplicationDidFinishLaunchingNotification`,
  caching whether `kAEOpenApplication` (`oapp`) has `prdt` property-data
  `kAELaunchedAsLogInItem` (`lgit`) (`../original/SPEC_OS_INTEGRATION.md` §3.5).
  The start decision is the pure, unit-tested
  `should_start_hidden(launched_as_log_in_item, has_tray)` — silent only for a
  login-item start **with** a possible tray; without one the window stays
  visible, so the app can never end up with no window *and* no menu-bar icon.
  On a silent start, the first `NSApplicationDidBecomeActiveNotification` is
  treated as launch-time activation, not a user request to show the window;
  later activations still bring it back. This was confirmed after a real
  sign-out/sign-in: the login item starts without a window.
- **Ways back — always the same path, and immediately:** the tray's "Fenster
  anzeigen", an app **relaunch** (Finder/Spotlight/`open -a` activate the already
  running instance) or a **second start** (the single-instance guard) all end in
  the same `show_main_window` path: show + focus the window. A **Dock click** is
  no longer among them while the window is hidden: the hidden app is `Accessory`,
  so there is no Dock icon to click. The activation arrives through an observer
  of `NSApplicationDidBecomeActiveNotification` (`src/app_activation.rs`). On
  close the app switches to `Accessory` and calls `deactivate()` so a later
  activation reaches the observer. `NSApplication::hide` is deliberately **not**
  called: the hidden state kept the Dock tile visible despite the correctly set
  `Accessory` policy (live finding, 2026-10-25). The return path sets `Regular`
  **first**, then calls `unhide` (if hidden, including by ⌘H) and `activate` in
  `show_main_window` — the policy must be `Regular` before `activate`.
- **Only one instance:** a second start does **not** open a second window — it
  asks the running instance to show its window and exits. On macOS the bundle's
  `LSMultipleInstancesProhibited` (`packaging/Info.plist`) already makes Launch
  Services refuse the second start and activate the running instance instead; for
  starts that bypass it (a second `./prime-photos` from a shell, Linux/Windows in
  general) `src/single_instance.rs` keeps a guard: a `UnixListener` on
  `…/prime_photos/instance.sock` (unix) or a loopback `TcpListener` on a fixed
  port (Windows), same one-line handshake (`prime-photos:show` →
  `prime-photos:ok`). **Unix stale-socket takeover (Soll-Vertrag, Issue #3):**
  concurrent starters must still leave exactly one socket owner. A contender
  that loses the takeover bind must not continue without the guard; it must
  reach the winner or stop. The original's `LSMultipleInstancesProhibited`
  (`../original/SPEC_OS_INTEGRATION.md` §3.1) is evidence for the original's
  single-instance contract, not for this port's socket-race mechanics.

## Decisions

- **Dock-Kachel bleibt trotz Accessory hängen (User-Entscheidung 2026-10-25):**
  Original-Parität — `LSUIElement` im Bundle + Policy-only ohne `hide`. Der
  Live-Befund zeigte: `Accessory` war korrekt gesetzt, aber der vorherige
  `NSApplication::hide`-Zustand ließ die Dock-Kachel sichtbar. Das Bundle setzt
  daher `LSUIElement = true`; beim Schließen wechselt die App zur
  `Accessory`-Policy und ruft `deactivate()` auf. Beim Zurückweg bleibt
  `unhide` als Schutz für den Fall erhalten, dass ⌘H die App versteckt hat.

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

- **Hintergrund-App: Close-to-Tray, Dock-Icon, Einzelinstanz (User-Entscheidung 2026-10-17) — überholt (2026-10-18).**
  Der User wollte als Rückweg aus dem versteckten Zustand **nur** Tray und Neustart — und (Korrektur
  vom selben Tag) das **Dock-Icon bleibt**; anders als beim Original ist das eine **bewusste
  Abweichung**. Umgesetzt: (1) Schließen versteckt das Fenster weiterhin (Close-to-Tray,
  unverändert), die App läuft im Hintergrund weiter; (2) das **Dock-Icon bleibt, solange die App
  läuft** (kein `Accessory`/Deaktivieren mehr), es verschwindet erst beim echten Beenden (normales
  macOS-Verhalten); (3) Rückweg über den Tray-Eintrag „Fenster anzeigen", einen **App-Neustart**
  (Finder/Spotlight/`open` aktivieren die laufende Instanz) oder einen Dock-Klick — immer derselbe
  Weg (Fenster zeigen + fokussieren) und **sofort**, ohne vorher woanders hinzufokussieren: Schließen
  gibt die Vordergrund-Position ab (`NSApplication::hide`, AppKit aktiviert die nächste App), weil
  ein Dock-Klick auf die bereits vorderste App nichts aktiviert; `show_main_window` holt die App mit
  `unhide` + `activate` zurück (siehe `src/app_activation.rs`); (4) **immer nur eine Instanz**: ein
  zweiter Start erzeugt **kein** zweites Fenster, sondern bittet die laufende Instanz, ihr Fenster zu
  zeigen, und beendet sich. Die **Einzelinstanz** ist Original-Parität (`../original/SPEC_OS_INTEGRATION.md`
  §3.3/§3.4: `LSMultipleInstancesProhibited` „single instance only"), das **bleibende Dock-Icon**
  dagegen weicht bewusst vom `LSUIElement`-Agent-Modell des Originals ab. Auf macOS lehnt
  Launch Services den zweiten Start schon über `LSMultipleInstancesProhibited` ab; für Startwege
  daran vorbei (zweiter Aufruf aus der Shell, Linux/Windows) wacht `src/single_instance.rs`
  (Unix-Socket `instance.sock` bzw. TCP-Loopback, gleicher Handshake).
  Die damalige `NSApplication::hide`-Begründung ist durch die Entscheidung vom 2026-10-25 überholt
  (Policy-only ohne `hide`; siehe „Decisions“).

- **Stiller Login-Start + Dock-Icon folgt dem Fenster (User-Entscheidung 2026-10-18) — gültig;
  löst die Entscheidung vom 2026-10-17 ab und wurde am 2026-10-25 für das Bundle-Verhalten
  präzisiert.** Neu: (1) ein Start **als Login-Item** (Autostart)
  läuft **ohne Fenster und ohne Dock-Icon**, nur mit Tray; ein **manueller** Start (Finder, Dock,
  Spotlight, `open -a`, Shell) zeigt das Fenster wie bisher; (2) das **Dock-Icon folgt dem
  Fenster**: versteckt ⇒ `Accessory` (kein Dock-Icon, kein App-Menü), sichtbar ⇒ `Regular`
  (Dock-Icon + Menü); (3) damit ist es **Original-Parität** statt Abweichung — das Original führt
  ein dynamisches Dock-Icon (`Regular` bei offenem, `Accessory` bei keinem Fenster,
  `../original/SPEC_OS_INTEGRATION.md` §3.4) und erkennt den Login-Start am Apple Event
  `kAEOpenApplication` mit `keyAEPropData == keyAELaunchedAsLogInItem` (`'lgit'`, §3.5); unsere
  Umsetzung macht beides genauso. Das Bundle setzt `LSUIElement = true` wie das Original; die
  Policy wird zur Laufzeit zwischen `Regular` und `Accessory` geschaltet. Die
  **Einzelinstanz** bleibt unverändert Original-Parität
  (`LSMultipleInstancesProhibited` + Socket-Wächter, s. o.); der Ein-Instanz-Schutz ist
  **unverändert**.

  **Technischer Grund für den Weg (nicht „verbessern"):** winit 0.30 registriert **selbst** einen
  `NSApplicationDelegate` und panict, wenn man ihn ersetzt („tried to get a delegate that was not
  the one Winit has registered"); die winit-Doku, die anderes behauptet, ist für 0.30 veraltet.
  Deshalb **kein** Delegate — und **kein** eigener Apple-Event-Handler: der Login-Start wird nur
  **gelesen** (`NSAppleEventManager::currentAppleEvent` im
  `NSApplicationDidFinishLaunchingNotification`-Callback in `src/app_activation.rs`); der
  App-Creator konsumiert den Cache. Ein registrierter Handler würde AppKits eigenen ersetzen. Das
  Fenster startet mit `.with_visible(false)` (`src/main.rs`) — das **hält es aber nicht
  versteckt**: eframe schaltet es nach dem ersten Frame selbst (bedingungslos) sichtbar, deshalb
  hält der stille Pfad den versteckten Zustand mit einem expliziten
  `ViewportCommand::Visible(false)` in `PrimePhotoApp::new` (live mit einer Scratch-App belegt).
  Die Startentscheidung ist die pure, unit-getestete Funktion
  `should_start_hidden(launched_as_log_in_item, has_tray)`; `has_tray` ist eine **Näherung** über
  die vorhandene Session (`has_pending_session()`), weil der Tray lazy im `update` gebaut wird —
  ohne möglichen Tray bleibt das Fenster sichtbar, damit die App erreichbar ist.

## Original reference

- `../original/SPEC_OS_INTEGRATION.md` — OS integration (tray, autostart, menu).
- `../dev/THIRD_PARTY.md` — Third-party icon attributions.
