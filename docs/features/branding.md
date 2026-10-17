# Branding and logo assets

Prime Photos is a third-party tool and never presents itself as an Amazon
app: no Amazon logos or wordmarks in our own chrome. Everything
functionally required stays — real hosts and URLs, header and cookie names,
and the "Mit Amazon anmelden" wording where a sign-in genuinely happens.

## How it works

- The project, git repo and app are named **"Prime Photos"**. Nothing in the
  project may give the impression of being an official Amazon app: no
  "Amazon Photos" as *our* app/product/bundle/wordmark identity, no Amazon
  logos or brand appearances in the chrome/UI of our own window.
- **Allowed:** all functionally necessary things — real hosts/URLs
  (`amazon.de`, `www.amazon.de`, `amazonaws.com`, `upload-photos…`), header
  and cookie names (`x-amzn-*`), and wording like "Bei Amazon anmelden" /
  "Mit Amazon anmelden" (where a sign-in actually happens).
- In docs describing the original app, the factual mention of the reference
  product remains allowed (that is recon of a real product); only **our**
  project/code/UI may never present itself as an official Amazon app.
- **Account block (2026-10-16):** the line "Amazon Photos" has been
  replaced by "Marktplatz / amazon.<domain>" since `0d66e05`. The **login
  subtitle** "Fotos & Videos zu Amazon Photos hochladen" **remains** — it
  names the target factually (like "Bei Amazon anmelden") and describes
  where our app uploads, not who it is.
- **App logo everywhere (F1, Welle 5.6, `50b834a`):** sidebar (24 px),
  About (64), login header (30) and the large login logo (84) show
  `assets/logo-small.svg` via the **shared** texture cache (no per-frame
  decoding); the code-drawn sphere `upload_logo` is **gone**.
- **App logo everywhere detailed (Welle 5.16).** The app UI (sidebar,
  About, login) shows `assets/logo.svg` (detailed logo with the photo
  cards) via `icons::app_logo`; the **small** `logo-small.svg` remains
  **only** the colored tray fallback for an unknown OS theme (user: „Damit
  nutzen wir das kleine Logo nur in der Taskbar"). Sidebar logo **30 px**
  (was 24), logo + title **vertically on a centre axis**.
  `assets/logo-small-black.svg` was **dead** prey and is deleted (with its
  `build.rs` entry).
- Marketplace flags and our logo (`upload_logo`) are **drawn by hand**;
  the status dot is a circle, not a glyph.

## Decisions

- **Name & Branding (Projektregel, 2026-10-13):** Projekt und git-Repo
  heißen **„Prime Photos"**. Nichts im Projekt darf den Eindruck erwecken,
  es sei eine offizielle App von Amazon: kein „Amazon Photos" als *unsere*
  App-/Produkt-/Bundle-/Wortmarken-Identität, keine Amazon-Logos oder
  Markenauftritte in der Chrome/UI des eigenen Fensters. **Zulässig bleibt
  alles funktional Notwendige:** echte Hosts/URLs (`amazon.de`,
  `www.amazon.de`, `amazonaws.com`, `upload-photos…`), Header-/Cookie-Namen
  (`x-amzn-*`), und Hinweise wie „Bei Amazon anmelden"/„Mit Amazon anmelden"
  (dort wird tatsächlich angemeldet). In Docs, die die Original-App
  beschreiben, bleibt die faktische Nennung des Referenz-Produkts erlaubt
  (das ist Recon eines echten Produkts); nur **unser** Projekt/Code/UI darf
  sich nirgends als offizielle Amazon-App darstellen.
  **Folge für den Account-Block (2026-10-16):** die Zeile „Amazon Photos"
  ist dort seit `0d66e05` durch „Marktplatz / amazon.<domain>" ersetzt. Der
  **Login-Untertitel** „Fotos & Videos zu Amazon Photos hochladen"
  **bleibt** — er nennt das Ziel faktisch (wie „Bei Amazon anmelden") et
  beschreibt, wohin unsere App hochlädt, nicht wer sie ist.

- **Name/Rename (user decision 2026-10-13):** project, git repo and app are
  **"Prime Photos"**. Cargo package/binary `prime-photos`; upload module
  `src/prime_photos.rs` with struct `PrimePhotos`; config dir
  `~/Library/Application Support/prime_photos/` (was `amzn_photos_uploader`);
  the project directory is renamed to `prime-photos`. The old config folder
  is migrated **once, locally, by hand** (no migration code in the app).

- **App-Logo überall (F1, Welle 5.6, `50b834a`).** Seitenleiste (24 px),
  About (64), Login-Kopf (30) und das große Login-Logo (84) zeigen
  `assets/logo-small.svg` über den **gemeinsamen** Textur-Cache (kein
  Pro-Frame-Dekodieren); die im Code gezeichnete Kugel `upload_logo` ist
  **weg**.

- **App-Logo überall detailliert (Welle 5.16).** Die App-UI (Seitenleiste,
  About, Login) zeigt `assets/logo.svg` (detailliertes Logo mit den
  Fotokarten) über `icons::app_logo`; das **kleine** `logo-small.svg` bleibt
  **nur** der farbige Tray-Fallback für ein unbekanntes OS-Theme (User:
  „Damit nutzen wir das kleine Logo nur in der Taskbar"). Seitenleisten-Logo
  **30 px** (vorher 24), Logo + Titel **vertikal auf einer Mittelachse**.
  `assets/logo-small-black.svg` war **tote** Beute und ist gelöscht (samt
  `build.rs`-Eintrag).

- **UI-Icons: Bibliothek statt selbst gezeichnet (Welle 3c, 2026-10-16).**
  Die Sidebar-Icons sind Material-Symbols-SVGs; die Zeilen-Icons bleiben als
  Linie. **Selbst gezeichnet bleiben:** die Marktplatz-Flaggen und unser Logo
  (`upload_logo`); der Status-Punkt ist ein Kreis, kein Glyph. Seit Welle 5.32
  (2026-10-17) sind auch die fünf Tray-Glyphen Material-Symbols-SVGs (siehe
  eigener Eintrag unten).
  **Verworfen:** `backup` (Wolke mit Hochpfeil) + `tune` (drei
  Schieberegler) — nicht die Motive des Originals; die
  **Outlined**-Variante für die Navigation — weicht sichtbar vom Original ab.

- **Tray-Glyphen aus Material Symbols (Welle 5.32, 2026-10-17).** Die fünf
  Tray-/Menüleisten-Glyphen kommen jetzt aus Material Symbols statt
  handgezeichnet: Idle `upload`, Aktiv `progress_activity`, Paused `pause`
  (gefüllte `fill1`-Variante — die Outlined-Variante rendert bei 32 px hohle
  Balken), Done `check`, Error `priority_high`. **Grund:** handgezeichnete
  Glyphen sind nach der Regel unzulässig („Icons werden nicht selbst
  gezeichnet"), und das alte Aktiv-Glyph war bei 32 px unleserlich — zwei
  kurze Striche verschmolzen mit dem Pfeilkopf, und ohne die Grundlinie des
  Idle-Pfeils war „aktiv" kaum von „idle" zu unterscheiden (User-Befund
  2026-10-17). **Selbst gezeichnet bleiben** damit nur noch die
  Marktplatz-Flaggen und unser Logo (`upload_logo`).

## Original reference

- `../dev/THIRD_PARTY.md` — Third-party icon attributions (Material Symbols).
