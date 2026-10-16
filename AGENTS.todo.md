# TODO / decisions ledger

Regel (User-Entscheidung 2026-10-15, steht in `AGENTS.md` → „Arbeitsablauf"): dieses Protokoll
hält **nur offene Arbeit**. Umgesetztes **und** unabhängig Verifiziertes wird **gelöscht**, nicht
abgehakt — die Historie steht in den Commits. Wissen, das **nur** hier stand, ist vor dem Löschen
in `docs/` ausgelagert (Stand 2026-10-15: `docs/PROTOCOL.md` § 5, `docs/SPEC_SCOPE_INVENTORY.md`,
`docs/DEV_NOTES.md`, `docs/features/LIVE_ACCEPTANCE.md`, `docs/RELEASE.md` § 10). „Verworfen" wird
**nie** gelöscht.

## Aktueller Auftrag (2026-10-15, Restbestand)

Erledigt und gelöscht: Commits der verifizierten Runde (`9cad019`), Tray-Icon erst nach Login
(`746392d`, Verifier APPROVE), Fortschritts-Leiste volle Breite (`0363bf1`, Verifier APPROVE).

1. [ ] **`docs/`-Gerüst:** bestehende Spec-Dateien in Unterverzeichnisse sortieren
   (`docs/original/`, `docs/protocol/`, `docs/dev/` …), ein `docs/README.md` als Index — **alle**
   Referenzen in `AGENTS.md`, `AGENTS.todo.md`, `docs/` und Code mitziehen. Anlass: User-Wunsch
   „gerne einen docs Ordner mit einer größeren Struktur". **Bekannter Rest:** `docs/features/FEATURES.md:8`
   nennt `docs/original/` und `docs/protocol/`, die es **noch nicht gibt** — beim Umzug mitziehen.
2. [ ] **Release-Build + Install (DMG).** **Voraussetzung: die 14 Befunde aus dem manuellen Test
   (Abschnitt „Manueller Test 2026-10-16" oben) sind abgearbeitet** — der Test selbst ist gelaufen
   (App aus `02e0fca`, signiert, installiert, vom User am echten Fenster geprüft, Start
   warnungsfrei). Am 2026-10-16 einmal gelaufen: `cargo build --release` +
   `packaging/make-app.sh` (signiert mit **„Prime Photos Local Signer"**),
   `hdiutil create -volname "Prime Photos" -srcfolder <staging> -ov -format UDZO`, Installation nach
   `/Applications` + `xattr -dr com.apple.quarantine`. **Dieser Stand ist inzwischen veraltet** —
   Welle 3a–3c haben die UI geändert (Warteschlange, Log-Panel, Nav-Icons). **Offen: nach Welle 4/5
   neu bauen**, DMG nach `target/dist/` legen und installieren. **Voraussetzung:** der
   Keychain-Dialog „… möchte den Schlüssel »refresh-token« verwenden" muss einmalig mit **„Immer
   erlauben"** bestätigt sein (stabiles Zertifikat, daher nur einmal).

## Manueller Test 2026-10-16 — Rückmeldungen des Users am echten Fenster

App aus `02e0fca` installiert und gestartet. Der Start war sauber: **keine** Warnung/kein Fehler im
Log (Default-Filter `warn`, `src/logging.rs:74`) und der Dedup-Index steht unverworfen mit **591
Zeilen** (`origin`+`node_id` vorhanden) — **Echtbeleg** für die Selbstheilung aus 5.4: der Fehlalarm
(„gesunder Index wird verworfen") ist auf echten Daten nicht eingetreten.

1. [ ] **Logo (User-Entscheidung: App-Logo überall).** Die Seitenleiste zeigt eine **im Code
   gezeichnete** teal Kugel (`src/icons.rs::upload_logo`), nicht unser App-Logo. Unser Logo liegt
   brach: `assets/logo.svg` → Dock-Icon (`logo.png`, 1024 px) und die **ungenutzte** kleine Variante
   `assets/logo-small.svg` (256 px; `logo-small-black` ebenfalls ungenutzt — `build.rs` rastert
   beide, kein `include_bytes!`). Umsetzung: Sidebar (24 px), About (64), Login-Kopf (30) und das
   große Login-Logo (84) zeigen das App-Logo; die gezeichnete Kugel entfällt samt Code.
2. [ ] **Inhaltsfilter (User-Entscheidung: zwei Checkboxen wie das Original).** Unser Dialog zeigt
   **drei deaktivierte Buttons** („Alles" vorausgewählt); die Recon belegt: das Original zeigt
   **zwei Checkboxen** mit den Labels **„Fotos sichern"** / **„Videos sichern"**, und „Alles" ist
   dort **nicht** wählbar (`docs/SPEC_CONTENT_FILTER_DESTINATION.md` §2.1). Umsetzung: zwei
   Checkboxen mit Original-Labels, „Fotos sichern" an und wählbar, „Videos sichern" sichtbar aber
   deaktiviert mit ehrlichem Hinweis; Auswahl **pro Ziel** persistiert (Parität zu
   `schedules.content_filter`, serde-Default = Fotos).
3. [ ] **Kein Fortschritt, wenn nichts hochzuladen ist.** User-Erwartung: dort steht dann
   **„100% hochgeladen"**. Heute rendert `status_text` für `Idle` mit `total == 0` einen **leeren**
   Text. **Falle:** ein nie gescannter Ordner hat ebenfalls `total == 0` — „100%" darf dort nicht
   stehen, sonst behauptet ein frisch angelegter Ordner Fortschritt.
4. [ ] **Upload-Ort wie das Original — Spec läuft** (User-Auftrag 2026-10-16: „Upload Ort soll
   gleich sein wie bei der Originalen App, bitte einem subagenten ein clean room spec schreiben
   lassen"). Deliverable `docs/SPEC_UPLOAD_DESTINATION.md`. Grundlage:
   `docs/SPEC_CONTENT_FILTER_DESTINATION.md` §3 (F3) — dort steht bereits „**F3 komplett offen** …
   größte Architektur-Lücke: wir haben weder `BackupPathMapper` noch `destination_node_id` noch den
   Cloud-Picker". **Erst nach der Spec** entscheidet der User über die Umsetzung.
5. [ ] **Einstellungen (User-Entscheidung: Sektion weg + sofort speichern).** Die Sektion
   „Erweitert (Konto)" fällt **samt Inhalt** weg (Konfigurationspfad, Marktplatz, Zone, User-Agent);
   der **Speichern-Knopf** in beiden Reitern entfällt, jede Änderung wird **sofort** gespeichert.
   **Bewusste Folge:** den Marktplatz kann nur noch beim Anmelden wählen, wer ihn später ändern
   will, muss sich ab- und wieder anmelden.
6. [ ] **Zusatzbefund (User-Entscheidung: mitbeheben).** „Gleichzeitige Uploads" und die Upload-Rate
   liegen **nur im Speicher** (`src/app.rs:225-228`), `Config` kennt sie **gar nicht** — der
   Speichern-Knopf hat sie **nie** gespeichert; nach einem Neustart standen sie auf dem Default. Sie
   kommen als Felder in `Config` (serde-Default, damit eine alte `config.toml` weiter lädt).
7. [ ] **Nicht angeschlossene Quelle (externe Festplatte) — Analyse läuft** (User-Auftrag
   2026-10-16: „was wenn ich eine externe Festplatte synce und die nicht immer angeschlossen ist?
   wie verhalten wir uns da, wie verhält sich die original App? cleanroom spec schreiben und TODOs
   ableiten"). Deliverable `docs/SPEC_MISSING_SOURCE.md` + **abgeleitete TODOs** (die wandern nach
   der Spec hierher). Berührt den Watcher (`Re-arm bei Fehler`), den Kaltstart-Wasserzeichen-Pass,
   den manuellen Rescan, den manuellen Rescan und die Zeilen-Anzeige — **eine besondere Falle:** die
   Wassermarke darf **nicht** vorrücken, wenn der Ordner weg war, sonst gelten die Dateien nach dem
   Anschließen als „unverändert".
   **Zielverhalten (User-Entscheidung 2026-10-16): „forced pause".** Ein nicht erreichbarer
   Quellordner soll das Ziel **anhalten** statt in einen Fehler zu laufen: Zustand wie eine Pause
   (`FolderStatus::Paused` existiert bereits, „Angehalten ({pct}%)", AMBER), **keine** Wassermarke,
   **kein** Datenverlust, und beim Wiederanschließen läuft es weiter. Die Spec soll dieses Ziel
   gegen das Verhalten des Originals stellen und die TODOs dorthin ableiten. **Achtung:** das ist
   mehr als Kosmetik — „Pause pro Ordner" ist heute **Post-MVP** (Original hat ein Zeilen-Symbol);
   der Unterschied ist, dass diese Pause **automatisch** aus dem fehlenden Ordner folgt.
8. [ ] **Warteschlange zu hoch (User-Entscheidung: feste Regel „Ordner zuerst, Warteschlange
   max. 50 %").** Ursache im Code belegt: `table_h = free − QUEUE_BELOW_H(88) −
   FOLDER_LIST_MIN_H(200)` (`src/backup.rs:1751`) — die Warteschlange bekommt **jeden Überschuss**,
   während die Ordnerliste bei `FOLDER_LIST_MAX_H = 320` gedeckelt ist (`:94`). Auf einem hohen
   Fenster (Screenshot ~1050 px) wächst dadurch nur die Warteschlange (~610 px), die Ordnerliste
   bleibt auf ihrer Untergrenze. **Neue Regel:** die Ordnerliste bekommt ihren Platz **zuerst** (bis
   320 px, danach scrollt sie), die Warteschlange den Rest, aber **höchstens 50 % der freien Höhe**
   (`QUEUE_MIN_H = 120` bleibt Untergrenze); beim Fenster-Minimum 760×520 darf nichts abgeschnitten
   werden. **Verworfen:** ziehbare Trennlinie (mehr Mechanik, Position überlebt den Neustart
   ohnehin nicht); eigener Reiter für die Warteschlange (widerspricht der früheren Entscheidung
   „Warteschlange und Fortschrittsleiste gehören zusammen", Welle 3a/Bug 4).
9. [ ] **Ziel der Datensicherung aus dem Dialog entfernen (User-Auftrag 2026-10-16).** Wörtlich:
   „Im Dialog ‚Ziel der Datensicherung' → genau gleich wie in den Settings. Das soll weg und genauso
   wie global automatisch berechnet werden." Also: das Feld „Ziel für Datensicherung:" samt
   deaktiviertem „Ändern …"-Knopf (`src/backup_config.rs:71-75`) **fällt weg**, das Ziel wird
   **automatisch** bestimmt — dieselbe Behandlung wie die eben entfernte Einstellungs-Sektion
   (Befund 5). **Die Umsetzung hängt an der Spec aus Punkt 4** (`docs/SPEC_UPLOAD_DESTINATION.md`):
   dort steht, wie das Original das Ziel ableitet (`Backup/<Macname>/<Pfad>`, Segmente werden bei
   Bedarf angelegt) und dass der User es „normalerweise nie ändern" muss.
   **User-Entscheidung 2026-10-16: „Feld jetzt weg, Spiegelung nach der Spec."** Das Feld samt
   Knopf fällt **sofort** aus dem Dialog (die App entscheidet wie heute automatisch, heute
   `Pictures/Web` — `src/prime_photos.rs:820-843` sucht die beiden Ordner und lädt dort hinein).
   Die **Spiegelung wie das Original** (`Backup/<Macname>/<Quellpfad>`) wird als eigener Auftrag
   **nach** der Spec gebaut; offene Frage dort: **kann unser Cookie-Host Ordner überhaupt anlegen**
   (heute wird nur gesucht)? Bestehende Uploads in `Pictures/Web` bleiben, wo sie sind.
   **Verworfen:** Feld dauerhaft behalten und bei `Pictures/Web` bleiben (widerspricht dem
   Paritäts-Auftrag „Upload Ort soll gleich sein wie bei der Originalen App").
10. [ ] **Clean-Room-Spec „Wie erkennt das Original, dass eine Datei dieselbe ist?"** (User-Auftrag
   2026-10-16). Deliverable `docs/SPEC_FILE_IDENTITY.md`. Recon existiert bereits
   (`docs/SPEC_DEDUP.md`, `docs/SPEC_DUPLICATES.md`, `docs/SPEC_FILENAME_ENCODING.md`,
   `docs/SPEC_CONTENT_FILTER_DESTINATION.md` §1.3) — der Auftrag ist **Nachprüfen und Verdichten**,
   Widersprüche zwischen den Docs aufdecken, **nicht** abschreiben.
11. [ ] **Höhe: responsiv statt absolut, Ordner wichtiger als die Warteschlange** (User-Auftrag
   2026-10-16: „bitte keine absoluten höhen nehmen, responsive und nicht absolute höhe, Ordner sind
   wichtiger als Warteschlange"). Die Aufteilung wird **relativ** (Anteile der freien Höhe) statt
   über die absoluten Konstanten `QUEUE_BELOW_H = 88`, `FOLDER_LIST_MIN_H = 200`,
   `FOLDER_LIST_MAX_H = 320`, `QUEUE_MIN_H = 120` (`src/backup.rs:77-97`); die Ordnerliste bekommt
   den **größeren** Anteil. Zeilen-/Kopfhöhen bleiben absolut (`show_rows` braucht sie) — gemeint
   sind die Bereichshöhen. Dazu zwei Wünsche:
   - [ ] **Prozent pro Datei** in den „In Bearbeitung"-Zeilen — **User-Entscheidung: vor dem DMG
     bauen.** Heute unmöglich: wir senden den Body am Stück (`.body(contents_vec)`,
     `src/prime_photos.rs:997`/`:1228`) ⇒ kein Byte-Zähler. Die Recon hat es vorweggenommen
     (`docs/SPEC_UPLOAD_PROGRESS.md` §4.2: „ob reqwest dort einen Byte-/Chunk-Progress liefert, ist
     **nicht belegt**"). Umsetzung: **streamender** Body (`Body::wrap_stream` über einen
     Chunk-Reader mit Zähler, `Content-Length` explizit setzen), Zähler je Datei in `UploadShared`,
     Zeile zeigt „45 %". **Berührt den heikelsten Pfad** ⇒ eigener Auftrag + **Live-Test am Konto
     durch den Hauptagent** (Testdatei hochladen, Knoten danach löschen).
   - [ ] **Ordnerpfad im Namen der Warteschlangen-Zeile** (User-Wunsch: „Beim Namen eventuell auch
     Ordner Pfad anzeigen?"). Heute steht dort nur der Dateiname, der volle Pfad nur im Tooltip.
12. [ ] **Mausrad scrollt beide Container gleichzeitig** (User-Befund 2026-10-16: „Scrollen mit
   Mausrad scrollt gleichzeitig die Ordner ansicht und die Warteschlange obwohl das nicht der
   gleiche Scroll COntainer ist"). Zwei `ScrollArea::vertical()` **ohne eigene ID**
   (`src/backup.rs:1754` Warteschlange, `:1919` Ordnerliste). **Ursache an der egui-Quelle belegt
   (2026-10-16):** `scroll_area.rs:534` setzt ohne eigenes `id_salt` die **Konstante**
   `Id::new("scroll_area")` und bildet daraus `ui.make_persistent_id(id_salt)` — zwei ScrollAreas im
   **selben Eltern-`Ui`** erhalten also **dieselbe ID** und teilen sich den Scroll-Zustand (egui
   verlangt für den zweiten ausdrücklich ein `id_salt`). Das erklärt „beide scrollen gleichzeitig"
   exakt: ein Rad-Ereignis bewegt **einen** Zustand, den beide zeichnen. **Fix:** jeder der beiden
   Bereiche bekommt ein **eigenes** `id_salt`; **vorher messen** (beide IDs bzw. Zustände
   gegenüberstellen), damit der Fix belegt ist und nicht geraten.
13. [ ] **Ordner in der Warteschlange sichtbar machen** (User-Klarstellung 2026-10-16 zu Befund 11:
   „Ich sehe in der Warteschlange keine Spalte für die Gruppe, aber ja der Ordner Name oder wenn der
   Ordner Name nicht eindeutig ist Ordner + Pfad"). Also **nicht** nur den Tooltip: die
   Warteschlangen-Zeilen zeigen den **Ordner je Zeile** (kompakte Spalte/Beigabe), und wenn **zwei
   Sicherungsordner denselben Namen** tragen, wird **Name + Pfad** angezeigt (klein/gekürzt), damit
   die Zeilen unterscheidbar bleiben. Der Gruppenkopf bekommt dieselbe Regel (heute nur der Name).
   Damit ist Befund 11s Wunsch „Ordnerpfad im Zeilennamen" präzisiert und **ersetzt** ihn.
14. [ ] **Ordner-Karte: Variant A (User-Entscheidung 2026-10-16).** Anlass: die Karte zeigt
   **zweimal** dieselbe Prozentzahl (grau unter dem Namen, farbig rechts) — weil beide Stellen
   `status_text` benutzen. Das Original hat dort **drei Bausteine**
   (`docs/SPEC_UPLOAD_PROGRESS.md` §2.2): **Status** („Datensicherung abgeschlossen" /
   „Upload-Vorgang läuft" / „ANGEHALTEN" / „Warten" / „Konfiguration wird durchgeführt"),
   **Fortschrittszeile** („Vollständig" / „{0}% hochgeladen" / „Angehalten bei {0} %") und
   **„Zuletzt aktualisiert: {0}"** (Original **relativ**, „Vor 1 Min.").
   **Variant A:** Zeile 1 = Name links, **Statuswort** rechts (farbig, je `BackupStatusAlertLevel`),
   daneben die drei Zeilen-Symbole; Zeile 2 klein = **Pfad · Fortschritt · Zuletzt aktualisiert**.
   Beispiel: „Sport … Datensicherung abgeschlossen" / „/Users/…/Pictures/Sport · Vollständig ·
   Zuletzt aktualisiert: 15.10.2026 21:56".
   **User-Vorgabe: „Zuletzt aktualisiert" bleibt.** Damit steht jeder Baustein **einmal**; der Pfad
   ist immer sichtbar.
   **Verworfen:** Variant B (kompakt, eine Zeile, Pfad nur bei Mehrdeutigkeit); Variant C (nur die
   Dopplung entfernen, Pfad wandert in die graue Zeile).
   **Offene Wortlaut-Frage:** bei 100 % nennt das Original **„Vollständig"**, wir bisher
   „100% hochgeladen" — die Spec ist die Vorgabe, also „Vollständig" (User kann das beim Test
   umdrehen).

## Prio 1 — Bugs aus der Live-Nutzung 2026-10-15 (User-Meldungen)

Erledigt und gelöscht (jeweils implementiert, unabhängig verifiziert = Verifier APPROVE, committet):
Bug 1 „Where is `use_default?" (`2899bc5`; Wissen in `docs/DEV_NOTES.md` § 1), Bug 7 „Klick auf den
leeren Bereich = Nichts" (nichts zu bauen — Belege in `docs/DEV_NOTES.md` § 6), Bug 9 „Warnungen als
Fehler" (`c019528`; Regel in `AGENTS.md`), Bug 8 „Sichere Ersetz-Regel" (`2e2f183`; Regel in
`AGENTS.md` → „Sichere Ersetz-Regel", Belege in `docs/SPEC_DUPLICATES.md` § 5 und
`docs/SPEC_CONFLICT_OPTIONS.md`), Bug 3 „Logo neben „Prime Photos" in der Seitenleiste" (`ecb9bb8`;
nur `src/app.rs`, Verifier APPROVE) und Bug 6 „Aufnahmedatum/Online-Sortierung" (`fcbc14d`; Regel in
`AGENTS.md` → „Aufnahmedatum", Live-Belege in `docs/SPEC_CAPTURE_DATE.md` § 12, PUT-Abnahme in
`docs/features/LIVE_ACCEPTANCE.md`), Bug 2 „Dock-Icon fehlt" (`02f5aa3`; iconset im Build →
`iconutil` in `packaging/make-app.sh`, Entscheidung in `AGENTS.md` → „Assets") sowie Bug 4
„Warteschlange und Fortschrittsleiste gehören zusammen" und Bug 5 „Protokoll-Bereich unten
abgeschnitten" (`4c0f337` = Welle 3a, `a55f8e3`/`a22c713` = 3b, `b9141dd` = Panel-Reihenfolge;
Wissen in `AGENTS.md` → „Panel-Reihenfolge", „Virtuelle Liste", „Live-Status der Ordner-Zeile"
und `docs/SPEC_UPLOAD_PROGRESS.md` § 7.8).

**Prio 1 ist damit vollständig abgearbeitet — hier ist nichts offen.**

## Offene Punkte aus der Konflikt-/Duplikat-Kette (Welle 2)

Die Umsetzung selbst ist abgeschlossen und gelöscht (Commits `42149bb`, `4ec98e7`, `f3c773b`,
`db68cff`, `a71191b`, `261501e`, `36aee39`, `40cf024`, `506df58`, `b0fa4c4`, `2803aa9` — alle
Verifier APPROVE). Geblieben ist **offene** Arbeit:

- [ ] **Live-Abnahme der Rückfall-Regel (2026-10-16).** Ebenfalls nur durch Tests gepinnt, nicht
  durch die Leitung: ein `Replace`-Ziel, dessen Pfad der Index **nicht** an die vom 409 benannte
  Node bindet, setzt den Lauf als **`RENAME`** fort (lokaler Zweig `resolution = SaveAsNew;
  continue;` in `src/upload.rs`). Belegbar an einer **fremden** Datei gleichen Namens — sie muss
  danach **unverändert** im Konto stehen (kein Datenverlust). Sinnvoll in **einem** Lauf mit der
  PUT-Abnahme oben (derselbe Namenskonflikt, einmal **mit** Bindung ⇒ PUT, einmal **ohne** ⇒ RENAME).
- [ ] **Zwei Befunde aus der Hänger-Diagnose (angelegt, nicht beauftragt).** (a) Ein Ziel, dessen
  Dateien **alle** fehlschlagen, behält `status = "uploading"` dauerhaft:
  `finalize_background_store` (`src/backup.rs`) schreibt Done/Error nur für Indizes aus
  `folder_done` — bei 0 Erfolgen fehlt der Index. (b) `FolderStore::load()` (`src/folders.rs`)
  macht bei **jedem** TOML-Parse-Fehler `unwrap_or_default()`: **ein** fehlerhafter Eintrag in
  `folders.toml` leert **still** den ganzen Store — danach läuft **kein** Hintergrund-Lauf mehr,
  ohne eine einzige Meldung.
- [ ] **Kein Test deckt die drei Vorbelegungsstellen** (`add_backup_folder`, transiente Einmal-Zeile,
  `UploadPlan::from_files`): der Verifier hat jede einzeln auf `true` zurückgedreht — alle 95 Tests
  blieben grün. Ursache: `PrimePhotoApp` braucht egui-Kontext bzw. Datei-Dialog. Ein **reiner
  Helfer** („Config ⇒ (dedup, conflict) für die Einmal-Queue") würde die Entscheidung testbar machen.
- [ ] **Ein Label-Test prüft deutschen Wortlaut** (`remember_label_names_the_folder`): unter
  `Lang::En` wäre er rot. Unkritisch, solange die Tests mit Deutsch laufen — beim nächsten Anfassen
  sprachneutral formulieren.
- [ ] **Das Häkchen-Gating ist eine Stufe großzügiger als das Schreiben:**
  `remember_checkbox_label` gated nur auf „Store kennt den Index", `remember_conflict_in_folder`
  zusätzlich auf `file_path.starts_with(&folder.path)`. Im Sonderfall eines verschobenen Index
  *mit* fremdem Pfad benennt das Häkchen einen Ordner, in den nichts geschrieben würde.
- [ ] **i18n-Hygiene:** toter Key „Als neue Dateien speichern" (`src/i18n.rs`) außerhalb der Tabelle
  unbenutzt; dazu **Doppel-Keys** in der Tabelle — `"Ersetzen"` (zweimal) und `"Ignorieren"`
  (zweimal), je eine Kopie ist toter Text (gleicher Wert, daher harmlos). Es gibt keine
  Eindeutigkeitsprüfung — Kandidat für eine Prüfung im Test.
- [ ] **`docs/SPEC_DUPLICATES.md` zwei stale Empfehlungszellen + eine neue Zeile** (Verifier-Funde,
  echt): (1)+(2) die **Empfehlungszellen** neben „Global dedup default setting" bzw. `:269` sagen
  noch „Optional. Only worth it if a preferences screen lands" — auf den Nachbarzeilen-Stil („Done")
  ziehen. (3) **neu (2026-10-16, vom Umsetzer gemeldet):** die Zeile `:275` „**Global conflict
  setting** | *Does not exist* | **We also have none**" ist auf unserer Seite überholt — seit
  `Config.conflict_resolution_default` gibt es die globale Konflikt-Standardwahl (Einstellungen →
  Upload); die Original-Spalte bleibt „gibt es dort nicht". **Erledigt und damit aus dieser Liste
  raus** (Runde 2026-10-16, Commit `83376ba`): die Zeilen `:268` und `:273`/`:278` (App-Default
  `Replace`, voller POST-Query-Satz) sowie der §5-Text.

## Offene Kandidaten derselben Absturzklasse (2026-10-14)

Regel aus dem Absturz am Laufende (2026-10-14, behoben; Details in `docs/DEV_NOTES.md` § 1):
**keine synchrone AppKit-/RunLoop-API direkt aus `update`.**
**Erledigt (2026-10-16, `e36488d`):** der letzte Kandidat `rfd::FileDialog::pick_folder()` ist auf
`AsyncFileDialog` + `self.rt` umgestellt — der synchrone Pfad blockierte das UI und gehörte in genau
diese Klasse. Offen ist hier **nichts**; die Regel bleibt als Prüfmaßstab für neue AppKit-Aufrufe.

## Offene Punkte (Bugs & Wünsche aus 2026-10-13)

Oberziel des Users: **nur neue Dateien hochladen, keine Bandbreite verschwenden.**

- [ ] **Icons nicht sauber ausgerichtet.** Nav-/Sidebar-Icons an die Doku-Werte ausrichten
  (Header 64 pt bzw. 104 pt mit Banner, 4 pt Akzentleiste, Nav-Spalte — `docs/UI_FLOWS.md` §3.1);
  braucht die Maßwerte/Design-Abmachung, kein Rate-Wert. **Stand 2026-10-16: verschoben in den
  manuellen Test** (Schritt 2 der Auftragsliste) — der Recon liefert für die Nav-Zeile keine
  Icon-Maße, und nach Welle 3c sitzen die Icons in einem festen 18×18-Rect. Ein blindes „Ausrichten"
  würde ändern, was der User noch gar nicht beurteilt hat.
- [ ] **Deutsche Doku-Kommentare in `src/notify.rs`** (Altbestand in HEAD, Verifier-Befund
  2026-10-16, kosmetisch): dort stehen deutsche `///`-Doku-Texte, während der Projektstil in `src/`
  englische Kommentare vorsieht — die inline-`//`-Kommentare sind außerhalb von `i18n.rs`/`tr(...)`
  inzwischen **frei** von Deutsch (Runde `83376ba`). Kein eigener Auftrag; beim nächsten Anfassen
  von `notify.rs` mitziehen.

**Erledigt und damit raus (2026-10-16):** die **Logout-Residuen** (laufender Upload: Abbruch-Flag
`2a3b636`; `config_error` verständlich und auf dem Login-Screen: dito; die Login-WebView-Residue war
hinfällig, der Login-Screen ersetzt die Haupt-UI) und die **Nav-Accessibility** (`widget_info`,
`e36488d`).

## Weitere offene Punkte (dokumentiert, nicht eingeplant)

Ausdrücklich **ohne** Starttermin:
- **Kleiner Rest aus Welle 5.2 (nicht blockierend, Verifier-Befund 2026-10-16):** die
  `cancelled`-Zuweisung im Ablauf-Zweig (`src/login.rs:491`) liegt **innerhalb** des
  `session`-Guards — der Guard im `if`-Kopf wird über den ganzen Block gehalten, Ordnung also
  `session → upload`. Der Verifier hat das als **deadlock-frei** belegt (es gibt keine
  Gegenrichtung `upload → session`); beim nächsten Anfassen von `poll_session` nach außen ziehen,
  damit die Reihenfolge nicht von einer Abwesenheit abhängt.
- Queue-Paginierung (heute bricht das Panel bei 300 Zeilen ab, Original lädt seitenweise 1000).
- Fehlerfenster mit `readdtoqueue`/`retryall`; Semantik laut `SPEC_STATUS_DETAIL.md` §3.
- Pause **pro Ordner** (Zeilen-Symbol) — im Original vorhanden, bei uns Post-MVP.
- Sortierung/Filter der Tabelle; Spalte „Größe" + relative Zeit (`datetimeago_*`).
- Tray-Status-Popup mit Statistiken (`statuspopup_stats_*`).
- Kaltstart-Verhalten nach Login; Pro-Ziel-Limit; netzwerkgesteuerte Drosselung; Logout während
  laufendem Upload; Nav-Accessibility.
- **Toter Dependency-Eintrag `kamadak-exif`** (Befund des Verifiers der contentDate-Runde,
  2026-10-16): seit `fcbc14d` liest kein Code mehr EXIF, der Eintrag in `Cargo.toml` steht aber
  weiter und wird mitgebaut. Entfernen wäre ein eigener kleiner Auftrag (`Cargo.toml` +
  `Cargo.lock`, `cargo build`/`cargo test` als Gate) — bewusst **nicht** in der contentDate-Runde
  mitgemacht, damit dort ein Auftrag = ein Commit bleibt.
- **Nicht** eingeplant (bewusst Out of Scope): die vollständige In/Out-Liste mit Beleg-Keys steht in
  `docs/SPEC_SCOPE_INVENTORY.md`; Sync, Downloads/Restore, Datei-Management, Apple-Photos-Import,
  Videos, Autoplay, Upsell-/Quota-Banner, Auto-Update, Proxy, ausgeblendete Dateien,
  iOS-Dedupe-Prompt, Crash-Relaunch bleiben draußen.

## Referenz: was **nicht** neu erfunden werden darf

Alle Rechte-/Verhaltensfragen sind entweder aus den Clean-Room-Specs belegt oder als UNKNOWN
markiert (`docs/SPEC_*.md`, `docs/PROTOCOL.md`, `docs/openapi.yaml`). Wo eine Spec einen
Original-Wert nennt, gilt er als Vorgabe, nicht als Ratewert. Unser **eigenes** Verhalten —
Fallstricke, Ops-Tricks, Gates — steht in `docs/DEV_NOTES.md`; Live-Nachweise in
`docs/features/LIVE_ACCEPTANCE.md`.

## Geplante Wellen — **NUR PLAN**, keine Welle startet ohne ausdrücklichen Auftrag

Geschnitten nach **gemeinsamem Umfeld/Dateien**, damit jede Welle in einem Durchgang ohne
Merge-Konflikt läuft; die Reihenfolge ist eine Empfehlung.

**Wellen 1–5 sind abgeschlossen** (jeweils umgesetzt, unabhängig verifiziert und committet; die
Historie steht in den Commits, die Entscheidungen in `AGENTS.md`). Offen ist nur **Welle 6** unten
— und die nicht-blockierenden Reste in den Listen weiter oben.

### Welle 6 — Post-MVP / bewusst später (dokumentiert, **nicht** eingeplant)
Pro-Ziel-Limit, netzwerkgesteuerte Drosselung, Cooldown nach Event-Läufen, Watcher-Sicherheitsnetz,
Queue-Paginierung, Fehlerfenster (`readdtoqueue`/`retryall`), Pause **pro Ordner**, Sortierung/Filter
+ Größen-Spalte, Tray-Status-Popup, „Details anzeigen"-Panel, Videos, echte Speichernutzung (Quota),
`.app`-Bundle + Icon, „Startseite".

## Zurückgestellt — braucht eine bewusste User-Entscheidung (nicht autonom lösbar)

Nichts davon ist umgeszt; alle Punkte brauchen eine Vorgabe.

1. **Hintergrund-Sync-Rest:** (a) ob nach einem Event-Lauf ein Cooldown gelten soll (mehrere Drops
   kurz hintereinander) und (b) ob ein Lauf nur bei bestehendem WLAN/nicht-kostenpflichtigem Netz
   starten soll — beides Plattform-APIs, noch nicht festgelegt.
2. **Pro-Ziel-Limit / netzwerkgesteuerte Drosselung** (bereits als Post-MVP markiert).
3. **macOS-Signatur:** die selbstsignierte Identität „Prime Photos Local Signer" ist importiert und
   getrustt (damit der Keychain-Dialog nur **einmal** erscheint). Offen bleibt, ob **Benachrichtigungen**
   im signierten Bundle erscheinen und ob `--options runtime` für ad-hoc nötig ist.
4. **Logout während laufendem Upload:** `logout()` löscht die Auth-Cookies, stoppt aber den
   laufenden Lauf nicht. Soll der Upload stoppen/pausieren oder weiterlaufen und mit 401 scheitern?
   (Eine saubere Antwort bräuchte einen Abbruch-Kanal in der Upload-Pipeline.)
5. **Farbe/Sprache des Multi-Fenster-Verhaltens** („Einmalige Sicherung"-Zeilendarstellung).

## Offen (Backlog)

- [ ] **Optional/Branding:** Account-Block zeigt unter „Angemeldet als:" noch die Zeile „Amazon
  Photos" (Label des angemeldeten Amazon-Kontos, kein Eigen-Branding). Falls keine Amazon-Wortmarke
  im eigenen Fenster gewünscht ist: durch die Domain (`amazon.<cc>`) ersetzen.
- [ ] „Details anzeigen"-Panel (verschachtelt/flach wie das Original, `docs/UI_FLOWS.md` §4.7).
- [ ] Menüleisten-Status-Item (Status-Popup: Status, Benachrichtigungen, Speicher, §9).
- [ ] Videos hochladen (bisher nur Fotos). Offene Fragen dazu: `docs/SPEC_SCOPE_INVENTORY.md`.
- [ ] „Startseite"/Übersichtsseite? (Strings vorhanden, im Build ungenutzt.)
- [ ] `.app`-Icon für das Bundle (siehe Prio-1-Bug 2) — gehört zu `packaging/make-app.sh`.
- [ ] Rest aus `Amazon Photos.dll`: Quota-Form, exakter Extension-Filter, Watcher-Intervall,
  Konfliktbehandlung, Video-Support.
- [ ] Echte „Speichernutzung"-Anzeige (nur Prime; bis dahin bewusst nichts anzeigen).
- [ ] **`pct` in der Ordner-Zeile kappen** (`src/backup.rs` rechnet `done/total` ungekappt): ein
  inkonsistenter Alt-Zustand (`total_files` < `uploaded_files` — genau so wurden einmal „320 %"
  angezeigt) würde wieder über 100 % anzeigen.
- [ ] **Win/Linux: In-App-„Beenden".** Ohne sichtbares Tray-Icon gibt es dort keinen Weg zurück
  (macOS ist safe, ⌘Q) — Residuum aus PORT-3.
- [ ] **Gezeichnete UI-Glyphen ersetzen:** `nav_icon`, Flagge und Logo auf die Material-Symbols-
  Pipeline umstellen (Icons werden nicht handgezeichnet, siehe `AGENTS.md`); nicht beauftragt.

## Verworfen

Kein Eintrag hier darf nachträglich „doch noch" umgesetzt werden — verworfen heißt verworfen.

- **Separates Login-Fenster** (`ppu-login` Helper-Prozess mit eigener `tao`-Eventloop): wirkte
  ungewohnt und passte nicht zum Original, das im Hauptfenster anmeldet. Ersetzt durch die
  eingebettete Child-WebView im Hauptfenster.
- **Synchrone Cookie-Abfrage in `update`** (wry `WebView::cookies`): Panic durch winit-Reentrancy
  (dreht die NSRunLoop). Ersetzt durch `dispatch_async` auf der Main-Queue; später **komplett
  entfernt**, weil sie aus einem Main-Queue-Block immer in den Timeout lief (Geräteweg ist der
  einzige Anmeldeweg).
- **mitmproxy live capture:** kept only as a debugging tool; app uses the cookie upload path.
- **Device registration / OAuth from scratch: replaced by browser-cookie auth** — **Aufgehoben am
  2026-10-15** (User-Entscheidung „so gut es geht wie das Original"): genau dieser Weg ist jetzt
  das **Ziel**. Verworfen bleibt daran nur der **CDP-Browser-Login** (externer Browser) und die
  **Paste-Variante** als Hauptweg.
- **Standalone paste-only login:** superseded by CDP browser login (paste stays as fallback).
- **Avatar-Thumbnail per Raten/Probieren (2026-10-14):** `/drive/v1/nodes/{id}/content` (mit und
  ohne `tempLink=true`), `/drive/v1/nodes/{id}/thumbnail` (mit `width`/`height`/`transform`),
  `/drive/v1/nodes/{id}/transforms/IMAGE_THUMBNAIL` (mit und ohne `?tempLink=true`) sowie
  `/drive/v1/nodes/{id}/content?transform=IMAGE_THUMBNAIL&tempLink=true` — **alle HTTP 404**. Der
  Parameter `?transform=IMAGE_THUMBNAIL` wird **ignoriert** (der `tempLink` liefert weiterhin die
  Rohdatei). Das `transforms: ["IMAGE_THUMBNAIL"]` eines Suchtreffers ist eine **Deklaration**, keine
  URL. Stattdessen belegbar über das Assembly (`GetSmartCroppedThumbnail`/`GetThumbnailUrl`).
- **Avatar lokal dekodieren/schneiden (2026-10-14):** der Avatar-Quellknoten kann ein RAW sein
  (bei diesem Konto ein Canon CR3, 13,5 MB) — die `image`-Crate kann kein CR3. Der 60×60-Crop muss
  daher **serverseitig** passieren (wie im Original), nicht bei uns.
- **`deepseek-v4.1-flash` als Umsetzer-/Verifier-Modell (2026-10-14 gesetzt, 2026-10-15
  zurückgenommen):** lief genau einen Tag, ohne dass ein Fehler oder Gate daran schuld war — der
  User hat schlicht wieder zurückgestellt. Ein erneuter Wechsel auf ein Gratis-/Billig-Modell für
  Subagenten braucht eine **neue** User-Entscheidung, nicht den alten Eintrag. Gegenwärtig gilt
  `opencode/mimo-v2.6-flash-free` für beide Rollen.
- **`SaveAsNew` als Application Default** (galt 2026-10-14 bis 2026-10-15). Wer ihn zurückdreht,
  ändert eine getroffene Entscheidung. Der Enum-`#[default]` bleibt trotzdem `SaveAsNew`
  (Datei-Fallback für `folders.toml`-Einträge ohne Schlüssel) — **zwei verschiedene Defaults, nicht
  verschmelzen**. *Aufgelöst 2026-10-16:* die sichere Ersetz-Regel ist entschieden (Bug 8) — der
  Application Default **bleibt `Replace`** (bestätigt, „alles auf replace"), die Sicherheit kommt aus
  der **Bindung** (`node_id` an den Pfad), nicht aus einem anderen Default; der Enum-`#[default]`
  bleibt `SaveAsNew`. **Verworfen:** `Replace` nur wegen Bug 8 zurückdrehen.
- **Den Application Default beim Drag&Drop still zu überschreiben** (statt nur run-weit zu merken).
- **„Nicht noch einmal fragen" pro Datei als neues persistiertes Feld.** Geltende Regel: die Antwort
  wird in das **bestehende** `BackupFolder.conflict_resolution` des Ordners der gefragten Datei
  geschrieben, plus Application Default für neue Ziele und Drag&Drop — **kein** Schemawechsel,
  **kein** Per-Datei-Feld.
- **Die sieben Warnungen pauschal mit `#[allow(...)]` stumm stellen** — das beseitigt das Rauschen,
  versteckt aber genau die toten Funktionen, um die es geht. Ebenfalls verworfen: nur die
  future-incompatible Warnung zu fixen und die Baseline bei sechs zu lassen (der User wollte 0 als
  Vergleichsbasis).
- **Der reine Web-/Cookie-Weg als Ziel** — und mit der Live-Abnahme auch als **Rückfall** (ganz
  entfernt). Der **CDP-Browser-Login** (externer Browser) bleibt ebenso verworfen wie die
  **Paste-Variante** als Hauptweg. Ebenfalls verworfen: die Original-Login-**UI** (zwei Spalten,
  Willkommens-Panel, Fenster 880 × 530) zu übernehmen — unsere UI bleibt.
- **Den Cookie-Rückfall asynchron über `objc2` neu zu bauen** (mehr Aufwand, kein Bedarf).
- **Den Fehlschlag „es bleibt `com.apple.Finder` stehen" als Ursache des `use_default`-Panels**
  (2026-10-15): er trifft nicht zu — der Dialog entsteht **vor** dem Fallback.
- **Die Vermutung, der User habe das LaunchServices-Panel für den Finder gehalten** (2026-10-15):
  ausdrücklich vom User korrigiert — es geht **kein** Finder auf. Keine Klick-Logik daraus ableiten.
