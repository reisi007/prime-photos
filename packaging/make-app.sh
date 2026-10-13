#!/usr/bin/env bash
#
# make-app.sh — baut das macOS-.app-Bundle für "Prime Photos" (PORT-6).
#
# Ablauf:
#   1. cargo build --release            (mit SKIP_BUILD=1 überspringbar)
#   2. target/dist/Prime Photos.app/ assemblieren (Contents/MacOS + Contents/Resources)
#   3. target/release/prime-photos -> Contents/MacOS/prime-photos
#   4. Contents/Info.plist aus packaging/Info.plist schreiben (__VERSION__/__BUILD__ ersetzt)
#   5. codesign --force --options runtime --sign "$CODESIGN_IDENTITY" (Standard: "-" = ad-hoc)
#      und anschließendes codesign --verify --verbose=2
#   6. Pfad + Startkommando ausgeben
#
# Umgebungsvariablen:
#   SKIP_BUILD=1                 überspringt `cargo build --release`
#   VERSION=<x.y.z>              CFBundleShortVersionString  (Standard: 0.1.0)
#   BUILD=<n>                    CFBundleVersion             (Standard: 1)
#   CODESIGN_IDENTITY="<Name>"   Signatur-Identität; "-" = ad-hoc (Standard)
#   DIST_DIR=<dir>               Ausgabeordner des Bundles (Standard: target/dist)
#
# Das Ausgabeverzeichnis liegt per Standard unter dem ignorierten `target/`, damit kein
# unversionierter Build-Müll im Repo entsteht.
# Idempotent: ein erneuter Lauf entfernt das Bundle und baut es sauber neu.
# Hinweis: bindet keine neuen Abhängigkeiten und braucht kein Netz. Nur macOS.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

APP_NAME="Prime Photos"
BIN_NAME="prime-photos"

VERSION="${VERSION:-0.1.0}"
BUILD="${BUILD:-1}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"

BIN_SRC="target/release/${BIN_NAME}"
DIST_DIR="${DIST_DIR:-target/dist}"
APP_DIR="${DIST_DIR}/${APP_NAME}.app"
CONTENTS="${APP_DIR}/Contents"

# --- 1) Build -----------------------------------------------------------------
if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
	echo "==> SKIP_BUILD=1: überspringe 'cargo build --release'"
else
	echo "==> cargo build --release"
	cargo build --release
fi

if [[ ! -f "${BIN_SRC}" ]]; then
	echo "FEHLER: ${BIN_SRC} nicht gefunden. Ohne SKIP_BUILD=1 baut das Skript es; sonst zuerst 'cargo build --release' ausführen." >&2
	exit 1
fi

# --- 2) Bundle sauber assemblieren (idempotent) -------------------------------
echo "==> Bundle assemblieren: ${APP_DIR}"
rm -rf -- "${APP_DIR}"
mkdir -p -- "${CONTENTS}/MacOS" "${CONTENTS}/Resources"

# --- 3) Binary kopieren -------------------------------------------------------
cp -- "${BIN_SRC}" "${CONTENTS}/MacOS/${BIN_NAME}"
chmod +x "${CONTENTS}/MacOS/${BIN_NAME}"

# --- 4) Info.plist aus der Vorlage schreiben ----------------------------------
echo "==> Info.plist schreiben (Version ${VERSION}, Build ${BUILD})"
sed -e "s/__VERSION__/${VERSION}/g" -e "s/__BUILD__/${BUILD}/g" \
	"${SCRIPT_DIR}/Info.plist" > "${CONTENTS}/Info.plist"
plutil -lint "${CONTENTS}/Info.plist" >/dev/null
echo "    Info.plist gültig"

# --- 5) Codesignieren + verifizieren ------------------------------------------
echo "==> codesign (Identität: ${CODESIGN_IDENTITY})"
codesign --force --options runtime --sign "${CODESIGN_IDENTITY}" "${APP_DIR}"
codesign --verify --verbose=2 "${APP_DIR}"

# --- 6) Ergebnis melden -------------------------------------------------------
echo
echo "Fertig: ${REPO_ROOT}/${APP_DIR}"
echo "Starten:"
echo "  open \"${APP_DIR}\""
