#!/usr/bin/env bash
#
# gen-third-party-licenses.sh — erzeugt assets/third-party-licenses.txt
#
# Zweck: sammelt Name/Version/Lizenz/Repository ALLER Abhängigkeiten aus
#        Cargo.lock (via `cargo metadata --format-version 1 --locked`) und dazu
#        die vollständigen Lizenztexte aus den lokalen Crate-Kopien. Die Ausgabe
#        ist deterministisch (kein Zeitstempel) und läuft offline (kein Netz).
# Abhängigkeiten: cargo, python3.
# Aufruf: packaging/gen-third-party-licenses.sh   (aus jedem Arbeitsverzeichnis)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT="$REPO_ROOT/assets/third-party-licenses.txt"

python3 - "$REPO_ROOT" "$OUT" <<'PY'
import hashlib
import json
import os
import subprocess
import sys

repo_root = sys.argv[1]
out_path = sys.argv[2]

PROJECT = "prime-photos"
PREFIXES = ("LICENSE", "COPYING", "NOTICE", "UNLICENSE")

metadata = json.loads(
    subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--manifest-path",
            os.path.join(repo_root, "Cargo.toml"),
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
)

packages = [p for p in metadata["packages"] if p["name"] != PROJECT]
packages.sort(key=lambda p: (p["name"], p["version"]))

texts = {}      # sha256 -> {"text", "files", "components", "exprs"}
no_text = []    # (name, version, expr) — Lizenzfeld vorhanden, aber kein Text
skipped = []    # (name, version, filename) — nicht als UTF-8 lesbar
components = []  # (name, version, expr, repo)


def find_license_files(manifest_path):
    """Lizenz-/Copyright-Dateien direkt im Crate-Verzeichnis (nicht rekursiv)."""
    directory = os.path.dirname(os.path.abspath(manifest_path))
    try:
        entries = sorted(os.listdir(directory))
    except OSError:
        return []
    found = []
    for entry in entries:
        if entry.upper().startswith(PREFIXES):
            path = os.path.join(directory, entry)
            if os.path.isfile(path):
                found.append((entry, path))
    return found


for pkg in packages:
    name = pkg["name"]
    version = pkg["version"]
    license_expr = pkg.get("license")
    license_file = pkg.get("license_file")
    repository = pkg.get("repository")

    expr = license_expr
    if not expr and license_file:
        expr = os.path.basename(license_file)
    display_expr = expr if expr else "(keine Angabe)"

    files = find_license_files(pkg["manifest_path"])
    if not license_expr and not license_file and not files:
        sys.stderr.write(
            "FEHLER: Komponente %s %s hat weder Lizenzfeld noch Lizenzdatei "
            "(%s)\n"
            % (name, version, os.path.dirname(pkg["manifest_path"]))
        )
        sys.exit(1)

    components.append((name, version, display_expr, repository))

    collected = 0
    for filename, file_path in files:
        try:
            with open(file_path, "rb") as fh:
                data = fh.read()
            text = data.decode("utf-8")
        except (OSError, UnicodeDecodeError):
            skipped.append((name, version, filename))
            continue
        key = hashlib.sha256(data).hexdigest()
        group = texts.setdefault(
            key,
            {"text": text, "files": set(), "components": set(), "exprs": set()},
        )
        group["files"].add(filename)
        group["components"].add((name, version))
        if expr:
            group["exprs"].add(expr)
        collected += 1

    if collected == 0:
        no_text.append((name, version, display_expr))


def join_limited(items, limit=6):
    if len(items) <= limit:
        return ", ".join(items)
    return ", ".join(items[:limit]) + " und %d weitere" % (len(items) - limit)


def text_label(group):
    # Ein einziger Lizenzausdruck benennt den Text; bringen mehrere
    # Komponenten mit unterschiedlichen Ausdrücken denselben Text mit, ist der
    # Dateiname die eindeutige Bezeichnung.
    exprs = sorted(group["exprs"])
    if len(exprs) == 1:
        return exprs[0]
    return ", ".join(sorted(group["files"]))


# --- Ausgabe aufbauen ------------------------------------------------------

lines = []
lines.append("Prime Photos — Drittanbieter-Lizenzen")
lines.append("=====================================")
lines.append("")
lines.append(
    "Diese Datei nennt und belegt die Lizenzen aller Abhängigkeiten von Prime"
)
lines.append(
    "Photos. Sie ist die Datenquelle für das About-/Lizenz-Fenster der App."
)
lines.append("")
lines.append(
    "Quelle:   cargo metadata --format-version 1 --locked  (gegen Cargo.lock)"
)
lines.append(
    "Umfang:   %d Komponenten (alle Abhängigkeiten aus Cargo.lock, transitiv)"
    % len(components)
)
lines.append("Erzeugen: packaging/gen-third-party-licenses.sh")
lines.append("")
lines.append(
    "Bewusst übervollständig: die Liste enthält auch Crates, die nur unter"
)
lines.append(
    "Windows oder Linux kompiliert werden und nicht im macOS-Binary landen."
)
lines.append(
    "Das erfüllt die Attributionspflicht, ohne Zielplattform zu unterscheiden."
)
lines.append("")
lines.append(
    "Eigene Lizenz des Projekts: MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE)."
)
lines.append("")
lines.append("")
lines.append("Komponenten nach Lizenz")
lines.append("=======================")
lines.append("")

by_expr = {}
for name, version, expr, repository in components:
    by_expr.setdefault(expr, []).append((name, version, repository))

for expr in sorted(by_expr):
    entries = by_expr[expr]
    lines.append("## %s  (%d)" % (expr, len(entries)))
    for name, version, repository in entries:
        entry = "  %s %s" % (name, version)
        if repository:
            entry += "  —  %s" % repository
        lines.append(entry)
    lines.append("")

lines.append("")
lines.append("Lizenztexte")
lines.append("===========")
lines.append("")

ordered = sorted(texts.items(), key=lambda kv: (text_label(kv[1]), kv[0]))
for key, group in ordered:
    comps = sorted("%s %s" % (n, v) for n, v in group["components"])
    lines.append("=== %s ===" % text_label(group))
    lines.append("Dateien: %s" % ", ".join(sorted(group["files"])))
    lines.append(
        "Komponenten (%d): %s" % (len(comps), join_limited(comps))
    )
    lines.append("")
    lines.append(group["text"])
    if not group["text"].endswith("\n"):
        lines.append("")
    lines.append("")

if no_text:
    lines.append("")
    lines.append("Ohne Text in der lokalen Kopie")
    lines.append("==============================")
    lines.append("")
    lines.append(
        "Diese Komponenten tragen ein Lizenzfeld, aber in ihrer lokalen"
    )
    lines.append(
        "Crate-Kopie wurde keine Lizenz-/Copyright-Datei gefunden. Es wird"
    )
    lines.append("nichts erfunden:")
    lines.append("")
    for name, version, expr in sorted(no_text):
        lines.append("  %s %s  —  kein Text in der lokalen Kopie gefunden (%s)" % (name, version, expr))
    lines.append("")

if skipped:
    lines.append("")
    lines.append("Nicht gelesen (ungültiges UTF-8)")
    lines.append("================================")
    lines.append("")
    lines.append(
        "Folgende Lizenzdateien waren nicht als UTF-8 lesbar und wurden"
    )
    lines.append("übersprungen:")
    lines.append("")
    for name, version, filename in sorted(skipped):
        lines.append("  %s %s  —  %s" % (name, version, filename))
    lines.append("")

output = "\n".join(lines).rstrip("\n") + "\n"
with open(out_path, "w", encoding="utf-8") as fh:
    fh.write(output)

sys.stderr.write(
    "geschrieben: %s (%d Komponenten, %d verschiedene Lizenztexte)\n"
    % (out_path, len(components), len(texts))
)
PY
