# UI-Review-Checkliste (Issue #30)

Prüfliste und Befehle für die visuelle Abnahme von Fenster-Layout, Abständen und
Textabbrüchen. Die Zustände kommen aus der Demo-Datenquelle
([`UI_FIXTURES.md`](UI_FIXTURES.md)); es wird **nichts** am echten Konto, an
`config.toml`, Keychain, SQLite oder im Netzwerk angefasst.

## Bauen

```bash
cargo build --locked
```

Das Release-Bundle kann separat für eine **Retina-Sichtprüfung der normalen
UI** gebaut werden ([`DEV_NOTES.md`](DEV_NOTES.md) §7). Es enthält keinen
Demo-Harness; Fixture-Screenshots müssen aus der Debug-Binary kommen.

```bash
packaging/make-app.sh
```

## Screenshot-Matrix

Alle Bilder entstehen über den Harness (`--screenshot`), nicht von Hand. Die
PNG-Datei enthält **physische Pixel**: auf einem 2×-Display ist ein
1400×900-Fenster 2800×1800 Pixel groß, ein 760×520-Fenster 1520×1040.

```bash
OUT="$PWD/target/prime-ui-review"
mkdir -p "$OUT"
BIN=target/debug/prime-photos

# Zehn Zustände bei 1400×900
for s in mixed uploading paused all-done failures empty many-folders \
         settings settings-bandwidth signed-out; do
  "$BIN" --demo="$s" --size 1400x900 --screenshot="$OUT/${s}-1400x900.png"
done

# Minimum-Fenster und eine mittlere Größe
"$BIN" --demo=mixed    --size 760x520  --screenshot="$OUT/mixed-760x520.png"
"$BIN" --demo=mixed    --size 1000x700 --screenshot="$OUT/mixed-1000x700.png"
"$BIN" --demo=failures --size 760x520  --screenshot="$OUT/failures-760x520.png"
"$BIN" --demo=failures --size 760x700  --screenshot="$OUT/failures-760x700.png"
"$BIN" --demo=signed-out --size 760x520 --screenshot="$OUT/signed-out-760x520.png"

# Theme-Abnahme: beide Seiten in beiden Themes, English and deterministic
for theme in light dark; do
  "$BIN" --demo=settings --demo-language=en --demo-theme="$theme" \
      --size 1400x900 --screenshot="$OUT/settings-${theme}-1400x900.png"
  "$BIN" --demo=mixed --demo-language=en --demo-theme="$theme" \
      --size 1400x900 --screenshot="$OUT/mixed-${theme}-1400x900.png"
done
```

| Zustand | Größe |
|---|---|
| `mixed`, `uploading`, `paused`, `all-done`, `failures`, `empty`, `many-folders`, `settings`, `settings-bandwidth`, `signed-out` | 1400×900 |
| `mixed` | 760×520 und 1000×700 |
| `failures` | 760×520 und 760×700 |
| `signed-out` | 760×520 |
| `settings` and `mixed` | 1400×900 in both `light` and `dark` (`--demo-theme`) |

## Prüfung pro Zustand

Die ersten sieben Zustände sind die Backup-Seite, dazu `settings`/
`settings-bandwidth` (Einstellungen) und `signed-out` (Anmeldung).

- [ ] **Seitenrand 16 px** links und rechts: kein Inhalt berührt den
  Fensterrand. Die Inhaltskante liegt bei 1400×900 auf x = 236 (links) bzw.
  x = 1384 (rechts) in Punkten.
- [ ] **Sidebar und Inhalt**: Die Sidebar beginnt links bei 16 px; ihr Logo und
  die erste Inhaltszeile der Seite liegen auf derselben y-Höhe (12 px
  Innenabstand oben).
- [ ] **Tabelle**: Kopfzeile und Werte stehen exakt übereinander — Ordner, Name
  und Status linksbündig, Größe rechtsbündig. Die Trennlinie unter dem Kopf ist
  1 px hoch und endet auf derselben Kante wie die Tabelle.
- [ ] **Statusspalte**: „In Bearbeitung · 100 %“ (bzw. die längste Übersetzung)
  ist vollständig sichtbar und läuft nie in die nächste Spalte.
- [ ] **Gemeinsame rechte Kante**: Tabelle, Fortschrittsbalken und Ordnerkarten
  enden auf derselben x-Position; die 18 px Scrollbar-Reserve steht konstant zur
  Verfügung (auch ohne Hover).
- [ ] **Ordnerkarte**: rechter Rahmen sichtbar; Status und Aktionssymbole enden
  16 px vor dem Kartenrand; kein Text läuft unter die Symbole.
- [ ] **Umbrechende Karte (760)**: in `failures-760x700` ist die zweite Karte
  samt unterem Rahmen und letzter Meta-Zeile vollständig sichtbar (die Karte
  zeichnet in natürlicher Höhe im scrollenden Split-Viewport; es gibt keine
  Höhenreservierung aus Zeilenumbrüchen mehr). In `failures-760x520` darf
  sie nur als Teil des **scrollenden** Listen-Viewports angeschnitten sein.
- [ ] **Gesamtbalken** liegt vollständig innerhalb des Seitenrands.
- [ ] **Kontoblock**: Marktplatz, Kontoname und „Abmelden“ sind vollständig
  sichtbar — auch im Zustand `failures` mit zweizeiliger Fehlermeldung und bei
  760×520. Unter dem Knopf bleibt der 12-px-Rand frei.
- [ ] **Anmeldebildschirm**: Logo, Titel und Marktplatz-Auswahl sind zentriert;
  nichts berührt den Fensterrand bei 760×520.
- [ ] **Keine unerwarteten Textabbrüche**: Lange Pfade/Dateinamen werden
  gekürzt und bleiben im Tooltip erreichbar; halb sichtbare Zeilen/Karten sind
  nur dort erlaubt, wo die Liste tatsächlich scrollt.
- [ ] **Theme-Abstände**: `item_spacing` ist 8/8, die Scrollbar-Reserve 18 px,
  die Seite hat den Hintergrund `#F7F8F8`, Karten bleiben weiß.
- [ ] **Einstellungen**: Tab-Zeile und Trennlinien beginnen am Seitenrand
  (x = 236) und enden an der Seitenkante (x = 1384); kein Redesign.
- [ ] **Beide Themes**: Settings, Backup, Dialoge und Statusanzeigen haben
  passende dunkle/helle Flächen; keine alten hellen Karten oder dunklen Texte
  auf dunklen Oberflächen.
- [ ] **Kontrast**: normaler Text einschließlich gedämpfter und semantischer
  Statusfarben erreicht mindestens **4.5:1**; bedeutungsvolle Control-Ränder,
  Slider-Schienen und Fokusindikatoren mindestens **3:1**. Die Palette-Tests
  berechnen die WCAG-Kontrastwerte für Hell und Dunkel; die Screenshots prüfen
  zusätzlich sichtbare Hover-/Fokuszustände. Die geprüften Palettenfarben sind
  opak; bei einem künftigen Alpha-Farbwert muss zuerst die tatsächliche
  Compositing-Farbe über der jeweiligen Oberfläche berechnet werden.
- [ ] **Slider**: die 1–8 Upload-Parallelität lässt sich per Maus ziehen,
  aktive und inaktive Schiene sind klar unterscheidbar, und der exakte
  ganzzahlige Wert bleibt sichtbar.

## Automatisierte Randprüfung

Ein kurzes Skript (Python + Pillow) beantwortet die harten Fragen aus der
Matrix:

```bash
python3 - <<'PY'
from PIL import Image
from pathlib import Path
OUT = Path("target/prime-ui-review")
BG = (0xF7, 0xF8, 0xF8)          # theme::PAGE_BG
for png in sorted(OUT.glob("*.png")):
    im = Image.open(png).convert("RGB")
    w, h = im.size
    if "1400x900" in png.name:
        scale = w / 1400
        # Einstellungen haben keine Scrollbar; andere Seiten reservieren 18 px.
        reserve = 16 if png.name.startswith("settings-") else 16 + 18
        limit = round(w - reserve * scale)
        right = max(x for y in range(h - 80) for x in range(w - 1, 440, -1)
                    if im.getpixel((x, y)) != BG)
        assert right <= limit, (png.name, right, limit)
    print(f"{png.name}: {w}x{h} ok")
PY
```

Die PNG-Breite bestimmt die Skala (`Breite / 1400`): Bei 1× liegen die Grenzen
für Queue-Seiten bei **1366 px** und für Einstellungen bei **1384 px**; bei 2×
sind es **2732** bzw. **2768 physische Pixel**. Queue-Seiten reservieren 16
logische Pixel Seitenrand plus 18 für die Scrollbar; Einstellungen haben keine
Scrollbar und enden daher an der Seitenkante. Der Trennstrich der Log-Leiste ist
Panel-Chrome und wird nicht mitgemessen.
