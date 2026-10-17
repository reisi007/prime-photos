# TODO / decisions ledger

**Nur offene Arbeit** — kurze Checkliste, **keine Doku**. Regel: erledigt **und** verifiziert = löschen
(globale `AGENTS.md` → „Arbeitsregeln"; Historie = Git). Doku → `docs/` (Feature-Dateien:
`docs/features/`).

**Entscheidungen stehen hier nicht mehr** (User-Entscheidung 2026-10-17, auch kein „Verworfen"): sie
liegen bei der jeweiligen Sache — in `docs/features/<gebiet>.md` (Abschnitt „Decisions" bzw.
eingerücktes `**Verworfen:**`) und für Prozess-/Werkzeug-Entscheidungen in `docs/dev/DEV_NOTES.md`.
Dieser Ledger verweist nur noch darauf. Einträge werden als Checkliste geführt: offen = `- [ ]`,
erledigt **und** verifiziert = **löschen** (Historie = Git).

## Reihenfolge (Stand 2026-10-17)

Priorisiert nach **Nutzen und Spec-Reife**, nicht nach Alter. Details stehen in den Abschnitten darunter —
hier nur die Reihenfolge und die Spec-Lage (Recon-Abgleich am 2026-10-17).

**1. Kleine Hygiene, kein Recon nötig, sofort machbar:** `cargo fmt` einmalig (schaltet ein Gate frei)
→ Actions Node 20 → 24.

**2. Spec vollständig vorhanden — umsetzbar ohne neue Recon**, in dieser Reihenfolge:
1. Upload-Ziel-Spiegelung (`SPEC_UPLOAD_DESTINATION.md`; Live-Beleg fürs Ordner-Anlegen liegt vor)
2. Größe-Spalte + relatives „Letzte Aktualisierung" (`SPEC_UPLOAD_PROGRESS.md` §7.2,
   `SPEC_STATUS_DETAIL.md` §3.4; im Scope-Inventar als **In** geführt)
3. Queue-Paginierung: 300er-Kappung raus, `ScrollArea::show_rows` (`SPEC_STATUS_DETAIL.md` §3,
   `SPEC_UPLOAD_PROGRESS.md` §7)
4. „Details anzeigen"-Panel (`UI_FLOWS.md` §4.7)
5. Fehlerfenster `readdtoqueue`/`retryall` (`SPEC_STATUS_DETAIL.md` §3; Inventar #21 „teilweise In")
6. Sortierung/Filter der Tabelle (Inventar #16, `SPEC_STATUS_DETAIL.md` §3.3)
7. Pause **pro Ordner** (`SPEC_BACKGROUND_SYNC.md` §7: „Datensicherung anhalten" je Schedule)
8. Tray-Status-Popup (Inventar #15, `SPEC_UPLOAD_PROGRESS.md`/`SPEC_NOTIFICATIONS.md`)

**3. Braucht erst eine Entscheidung oder neue Recon** (nicht einfach „nächste Welle"): Netz-/Kosten-Gate
(im Original **nicht** belegt — „netzwerkgesteuerte Drosselung" wäre eine Neuerung), „Pro-Ziel-Limit"
(Begriff im Original unbelegt, dort nur globale Concurrency), Videos (Konzept belegt,
Details **UNKNOWN**), Nav-Icon-Maße (Recon liefert keine Pixelwerte), echte Speichernutzung (Inventar #7
bereits **Out**), macOS-Signatur/Benachrichtigungen, Multi-Fenster-Zeile, Win/Linux-„Beenden".

**4. Hygiene-Reste aus Verifikationen** — bei der nächsten Berührung der jeweiligen Datei mitziehen
(`src/login.rs:491`, `src/i18n.rs:149`, Naht-Test Autostart, `aggregate_progress`-Designfrage,
DMG-Skript, Token-Rotation) plus die zwei Live-Abnahmen (Rückfall-Regel, PUT-Bindung).

## Erster Punkt beim nächsten Mal

- [ ] **Einmal `cargo fmt` über das ganze Repo** (mechanischer Commit; nachgemessen 2026-10-17:
  `cargo fmt --check` liefert **102 Abweichungen in 19 Dateien** — `app.rs auth.rs backup_config.rs
  backup.rs build.rs countries.rs dedup.rs i18n.rs icons.rs logging.rs login.rs main.rs platform.rs
  prime_photos.rs settings.rs text.rs tray.rs upload.rs urls.rs` — auch in unangetasteten). Danach
  gilt `cargo fmt --check` als Gate (User-Entscheidung 2026-10-16; Warum es das Gate bisher nicht gab:
  `docs/dev/DEV_NOTES.md` §8).

## Offen nach dem Release v0.1.0 (2026-10-17)

Das erste Release ist ausgelöst und belegt (`docs/dev/RELEASE.md` §9): Tag `v0.1.0` auf `1de3e2b`,
`macOS-DMG` manuell angehängt. Offen daraus:

- [ ] **Actions von Node 20 auf Node 24 anheben** (User-Entscheidung 2026-10-17: **eigener** Durchgang,
  nicht im Release erledigt). Beide Job-Arten melden `Node.js 20 is deprecated` für
  `actions/checkout@v4`, `actions/upload-artifact@v4`, `actions/download-artifact@v4` — sie werden auf
  Node.js 24 gezwungen; dazu der Hinweis, dass `ubuntu-latest` ab **2026-10-19** auf Ubuntu 26
  migriert. Zu prüfen: Zielversionen in `.github/workflows/ci.yml` **und** `release.yml` (beide Dateien
  haben eigene Action-Referenzen) und die Verträglichkeit mit `--locked`; Beleg braucht einen echten
  Lauf (CI auf `main`; das nächste Release erst beim nächsten Tag). **Dabei mitziehen:** die Aufzählung
  in `docs/dev/RELEASE.md` §9 nennt `softprops/action-gh-release@v2` nicht, das ebenfalls „Node.js 20 is
  deprecated" meldet (Verifier-`nit` aus Welle 5.25).
- **Nicht** offen, nur zur Einordnung: Windows-Authenticode, macOS-Notarisierung und Prüfsummendateien
  bleiben laut `docs/dev/RELEASE.md` §9 unverifiziert bzw. nicht implementiert — dafür gibt es keinen
  Auftrag.

## Offen aus der Live-Abnahme 2026-10-17 (Bundle aus 5.21–5.24)

Zwei der drei Checks sind **bestanden und belegt** — Systemsprache (Finder-Start deutsch, ohne
`LANG`/`LC_ALL`) und Autostart (Eintrag **sofort** sichtbar, **keine** Freigabe; damit ist auch das
Risiko „selbstsignierte Signatur vs. `SMAppService`" widerlegt). Belege: `docs/features/LIVE_ACCEPTANCE.md`,
`docs/features/languages.md`, `docs/features/autostart.md`. Der dritte hat einen **Bug** aufgedeckt
(behoben in Welle 5.33):

- [ ] **Live-Re-Check offen: Rückkehr eines fehlenden Laufwerks.** Der Live-Bug vom 2026-10-17 (Label
  „Ordner nicht gefunden" korrekt, aber **kein** Wiederanlauf; uneinheitlicher Zeilenzustand) ist in
  Welle 5.33 umgesetzt und verifiziert (`83c4f07`): die Flanke `NotFound` → Pfad vorhanden stößt einen
  **Vollscan des Bestands** an, flankengetriggert (einmal pro Rückkehr) und **ohne** Umweg um die globale
  Pause. Offen bleibt allein die **Live-Bestätigung** an einem echten Laufwerk (samt der bis zu 30 s
  Latenz nach dem Aufheben der Pause): `docs/features/backup-targets.md` („Still open") und
  `docs/features/LIVE_ACCEPTANCE.md`.
- **Nicht getrackt, weil dokumentiert:** ob `LSMinimumSystemVersion 11.0` (Info.plist) vs. Autostart ab
  macOS 13 je relevant wird — bekannte Degradation in den „unsupported"-Pfad
  (`docs/features/autostart.md`; „not live-checked" in `docs/features/LIVE_ACCEPTANCE.md`).

## Offen aus dem geschlossenen Test 2026-10-16

1. [ ] **Upload-Ziel-Spiegelung wie das Original.** Zielfeld ist aus dem Dialog entfernt (Welle 5.7),
   die App wählt das Ziel automatisch (`Pictures/Web`); Auftrag nach
   `docs/original/SPEC_UPLOAD_DESTINATION.md`: Spiegelung `Backup/<Macname>/<Quellpfad>`, Segmente bei
   Bedarf anlegen. Bestehende Uploads in `Pictures/Web` bleiben. Umsetzung ist **frei** — Live-Beleg
   und Entscheidungslage: `docs/features/destination.md`.
   **Beim Bauen beachten (User-Frage 2026-10-17):** der Content-Skip ist **kein** Move im Konto — eine
   **verschobene/umbenannte** Datei (gleicher MD5, anderer Quellpfad) wird lokal übersprungen und bekommt
   **keinen** Knoten am neuen gespiegelten Pfad; ihr alter Knoten bleibt unter dem **alten** Pfad liegen.
   Doku: `docs/features/duplicates-conflicts.md` (Cross-path-Entscheidung) und `destination.md`.

## Offene Punkte aus der Konflikt-/Duplikat-Kette (Welle 2)

- [ ] **Live-Abnahme der Rückfall-Regel (2026-10-16).** Nur durch Tests gepinnt, nicht durch die
  Leitung: ein `Replace`-Ziel, dessen Pfad der Index **nicht** an die vom 409 benannte Node bindet,
  läuft als **`RENAME`** weiter. Zu belegen an einer **fremden** Datei gleichen Namens (sie muss danach
  **unverändert** im Konto stehen — kein Datenverlust), sinnvoll in **einem** Lauf mit der
  PUT-Abnahme: derselbe Namenskonflikt, einmal **mit** Bindung ⇒ PUT, einmal **ohne** ⇒ RENAME.
  Regel/Details: `docs/features/duplicates-conflicts.md`.

## Offene Punkte (Bugs & Wünsche aus 2026-10-13)

- [ ] **Icons nicht sauber ausgerichtet.** Nav-/Sidebar-Icons an die Doku-Werte ausrichten
  (`docs/original/UI_FLOWS.md` §3.1). **Kein Ratewert** — der Recon liefert für die Nav-Zeile keine
  Icon-Maße; braucht die Design-Abmachung des Users. Stand/Feststellung:
  `docs/features/window-layout.md`.

## Weitere offene Punkte (dokumentiert, nicht eingeplant)

Ausdrücklich **ohne** Starttermin:
- **Kleiner Rest aus Welle 5.2 (nicht blockierend, Verifier-Befund 2026-10-16):** die
  `cancelled`-Zuweisung im Ablauf-Zweig (`src/login.rs:491`) liegt **innerhalb** des
  `session`-Guards — der Guard im `if`-Kopf wird über den ganzen Block gehalten, Ordnung also
  `session → upload`. Der Verifier hat das als **deadlock-frei** belegt (es gibt keine
  Gegenrichtung `upload → session`); beim nächsten Anfassen von `poll_session` nach außen ziehen,
  damit die Reihenfolge nicht von einer Abwesenheit abhängt.
- **Footer-Summe und fehlende Ordner (Verifier-Befund 2026-10-17, `low`):** `aggregate_progress`
  (`src/backup.rs`) summiert `done/total` **statusunabhängig** — ein `NotFound`-Ordner trägt also
  weiter zur Zeile „Gesamt: X von Y" bei, obwohl seine eigene Fortschrittszeile leer ist. Vor 5.22
  war das für den damaligen `Paused`-Zustand identisch (kein Regress); es ist eine **Design-Frage**
  (soll ein fehlendes Ziel in der Gesamt-Summe mitzählen?), keine Fehlfunktion.
- **Naht-Test ohne Aussage (Verifier-Befund 2026-10-17, `nit`):** `autostart_intent_to_record()`
  (`src/app.rs`) ist die Identität, ihr Test damit tautologisch — die **Verdrahtung** in
  `set_autostart` sichert er nicht ab (nur mittelbar über `dead_code`, falls jemand die Funktion
  entfernt). Ein echter Guard bräuchte eine injizierbare Registrierung.
- **Der DMG-Schritt ist nicht skriptet:** `packaging/make-app.sh` baut nur die `.app`; das DMG
  entstand 2026-10-17 von Hand und ist damit nicht reproduzierbar. Gehört als eigener, dokumentierter
  Schritt (Volname, Dist-Pfad, Reihenfolge `.app` → DMG) nach `docs/dev/BUILD.md`/`RELEASE.md`;
  Anhängen ans Release ist inzwischen in `docs/dev/RELEASE.md` §4 beschrieben. **Belegt 2026-10-17:**
  `hdiutil` warnt selbst zugunsten von `diskutil image create from/blank --volumeName --format …`
  (das heutige Image war mit `hdiutil verify` gültig).
- **Token-Rotation offen (Vorfall 2026-10-16):** ein `security add-generic-password`-Aufruf hatte den
  Refresh-Token kurz in der Prozessliste (`ps`) stehen; eine **neue Anmeldung** rotiert ihn. Nur
  offen, falls seither nicht neu angemeldet wurde.
- **Code-Kommentar hinter der Doku (Verifier-Merker 2026-10-17):** der Kommentar an
  `src/i18n.rs:149` nennt weiterhin „`C`/`POSIX`/`unset` → Englisch" — auf macOS ist der
  unset-Fall seit 5.21 die `NSLocale`-Abfrage. Die Doku wurde korrigiert, der Code-Kommentar
  nicht; bei der nächsten Berührung von `src/i18n.rs` mitziehen.
- Queue-Paginierung, Fehlerfenster (`readdtoqueue`/`retryall`), Pause **pro Ordner**, Sortierung/Filter
  + Größen-Spalte, Tray-Status-Popup, „Details anzeigen"-Panel, Videos: siehe „Reihenfolge" oben bzw.
  die jeweilige `docs/features/`-Datei.
- Kaltstart-Verhalten nach Login; Pro-Ziel-Limit; netzwerkgesteuerte Drosselung; Logout während
  laufendem Upload; Nav-Accessibility — Lage und Belege: `docs/features/scope.md` und
  `docs/features/change-detection.md`/`sign-out.md`.
- **Kleinigkeiten aus den Verifikationen der Wellen 5.31–5.33 (bewusst zurückgestellt, nicht blockierend):**
  der Worker nimmt seinen Queue-Pop unter der `shared`-Sperre (`src/upload.rs`) — Worker serialisieren
  also am selben Mutex (Laufzeit, keine Korrektheit); der `ScansInFlight`-Guard in `inject_into_live_run`
  entsteht **nach** dem `active`-Check (`src/backup.rs`), das Fenster führt nur zu Ablehnung + Parken
  (kein Verlust); ein bereits laufender, erneut angestoßener Ordner wird pro Durchlauf erneut gewalkt und
  dann abgelehnt (Dedupe-Guard in `inject_into_live_run`) — Parken statt Doppel-Upload. Das
  Test-Scratch-Verzeichnis `prime-photos-test-store-<pid>` bleibt in `$TMPDIR` liegen (`src/folders.rs`),
  konsistent zu den vorhandenen `prime-photos-folders-*`. Aus **5.33** (Verifier-Beobachtungen, `low`):
  das „stay parked" in `docs/features/backup-targets.md` gilt nur **ohne** aktiven Lauf (läuft einer, wird
  der Ordner eingehängt und die Pause hält stattdessen die Worker an — die Zusage „kein Upload während der
  Pause" bleibt in beiden Zweigen wahr); derselbe Absatz nennt die Hintergrund-Sync-Abhängigkeit des
  Wiederanlaufs nicht explizit (sie steht in `change-detection.md`); ein **dauerhaft** fehlschlagendes
  `store.save()` würde den Ordner pro 30-s-Pass erneut einparken (degenerativer I/O-Fall, kein
  Doppel-Upload).
- **Nicht** eingeplant (bewusst Out of Scope): die vollständige In/Out-Liste mit Beleg-Keys steht in
  `docs/original/SPEC_SCOPE_INVENTORY.md` und in `docs/features/scope.md`.

## Geplante Wellen — **NUR PLAN**, keine Welle startet ohne ausdrücklichen Auftrag

Geschnitten nach **gemeinsamem Umfeld/Dateien**. **Wellen 1–5 sind abgeschlossen** (Historie in den
Commits, Entscheidungen in `docs/features/`); offen ist nur **Welle 6** — plus die Reste in den
Listen weiter oben.

### Welle 6 — Post-MVP / bewusst später (dokumentiert, **nicht** eingeplant)

Reihenfolge und Spec-Lage: Abschnitt „Reihenfolge" oben. Die Liste hier ist die **Bündelung**, keine
zweite Quelle — Details und §-Verweise stehen in den Abschnitten darüber.

Pro-Ziel-Limit, netzwerkgesteuerte Drosselung, Cooldown nach Event-Läufen, Watcher-Sicherheitsnetz,
Queue-Paginierung, Fehlerfenster (`readdtoqueue`/`retryall`), Pause **pro Ordner**, Sortierung/Filter
+ Größen-Spalte, Tray-Status-Popup, „Details anzeigen"-Panel, Videos, echte Speichernutzung (Quota).

## Zurückgestellt — braucht eine bewusste User-Entscheidung (nicht autonom lösbar)

Nichts davon ist umgesetzt; alle Punkte brauchen eine Vorgabe.

1. **Hintergrund-Sync-Rest:** (a) ob nach einem Event-Lauf ein Cooldown gelten soll (mehrere Drops
   kurz hintereinander) und (b) ob ein Lauf nur bei bestehendem WLAN/nicht-kostenpflichtigem Netz
   starten soll — beides Plattform-APIs, noch nicht festgelegt (das Original kennt **nur** die
   Bandbreiten-Drosselung, siehe `docs/features/scope.md`).
2. **Pro-Ziel-Limit / netzwerkgesteuerte Drosselung** (bereits als Post-MVP markiert).
3. **macOS-Signatur:** die selbstsignierte Identität „Prime Photos Local Signer" ist importiert und
   getrustt (damit der Keychain-Dialog nur **einmal** erscheint). Offen bleibt, ob **Benachrichtigungen**
   im signierten Bundle erscheinen und ob `--options runtime` für ad-hoc nötig ist (Signaturlage:
   `docs/features/platform.md`).
4. **Farbe/Sprache des Multi-Fenster-Verhaltens** („Einmalige Sicherung"-Zeilendarstellung;
   Original-Referenz: `docs/original/UI_FLOWS.md`, `SPEC_ONE_TIME_UPLOAD.md`).

## Offen (Backlog)

- [ ] „Details anzeigen"-Panel (verschachtelt/flach wie das Original, `docs/original/UI_FLOWS.md` §4.7).
- [ ] Menüleisten-Status-Item / Tray-Status-Popup (Status, Benachrichtigungen, Speicher; Strings
  `statuspopup_stats_*` und `contextmenu_*`, `UI_FLOWS.md` §9, `SPEC_UPLOAD_PROGRESS.md` §1,
  `SPEC_NOTIFICATIONS.md`; Inventar #15).
- [ ] Videos hochladen (bisher nur Fotos). Offene Fragen dazu: `docs/original/SPEC_SCOPE_INVENTORY.md`.
- [ ] Rest aus `Amazon Photos.dll`: Quota-Form, exakter Extension-Filter, Watcher-Intervall,
  Konfliktbehandlung, Video-Support.
- [ ] Echte „Speichernutzung"-Anzeige (nur Prime; bis dahin bewusst nichts anzeigen).
- [ ] **Win/Linux: In-App-„Beenden".** Ohne sichtbares Tray-Icon gibt es dort keinen Weg zurück
  (macOS ist safe, ⌘Q) — Residuum aus PORT-3; Feststellung: `docs/features/platform.md`.
- [ ] **Gezeichnete UI-Glyphen ersetzen:** `nav_icon`, Flagge und Logo auf die Material-Symbols-
  Pipeline umstellen (Icons werden nicht handgezeichnet, siehe `docs/features/branding.md`); nicht beauftragt.

## Verworfen → steht in der Doku

Die verworfenen Wege und ihre Begründungen stehen seit 2026-10-17 **nicht mehr hier**, sondern bei der
jeweiligen Sache: in `docs/features/<gebiet>.md` (Abschnitt „Decisions" bzw. eingerücktes
`**Verworfen:**`) und für Prozess-/Werkzeug-Entscheidungen in `docs/dev/DEV_NOTES.md` (§8 Warnungs-
Politik, §11 Modellwahl). **Kein Eintrag dort darf nachträglich „doch noch" umgesetzt werden —
verworfen heißt verworfen.** Wer hier etwas wieder aufnimmt, muss zuerst die Begründung in der
jeweiligen Doku widerlegen.
