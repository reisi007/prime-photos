#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${RUNE_UI_BIN:-$ROOT/target/debug/prime-photos}"
OUT="${RUNE_UI_REVIEW_OUT:-$ROOT/target/rune-ui-review}"
if [[ "$BIN" != /* ]]; then
  BIN="$ROOT/$BIN"
fi
if [[ "$OUT" != /* ]]; then
  OUT="$ROOT/$OUT"
fi
if [[ ! -x "$BIN" ]]; then
  printf 'Rune review binary not executable: %s\nBuild with: cargo build --locked --features rune-ui-spike\n' "$BIN" >&2
  exit 1
fi

mkdir -p "$OUT"
# Generated artefacts only: drop every capture from an earlier matrix, including
# retired smoke/login/settings/backup-fixture PNGs and JSON trees.
find "$OUT" -maxdepth 1 -type f \( -name '*.png' -o -name '*.json' \) -delete

# The fixture fixes German itself via i18n::set_lang(Lang::De); the locale keeps
# number/date formatting in the captured trees deterministic as well.
export LC_ALL=de_DE.UTF-8

capture_png() {
  local filename="$1"
  shift
  "$BIN" --rune-backup-live-fixture "$@" --scale 1 --snap "$OUT/$filename.png"
}

capture_tree() {
  local filename="$1"
  shift
  "$BIN" --rune-backup-live-fixture "$@" --scale 1 --tree "$OUT/$filename.json"
}

# Real production live_view, rendered from a synthetic LiveState. No login,
# settings or old backup fixture and no retired backup-smoke route.
scenes=(mixed empty settings about licenses)
extra_sizes=(1000x700 760x560)

capture_all() {
  local theme scene size filename
  for theme in light dark; do
    for scene in "${scenes[@]}"; do
      filename="live-$scene-$theme-1400x900"
      capture_png "$filename" --scene "$scene" --theme "$theme" --size 1400x900
      capture_tree "$filename" --scene "$scene" --theme "$theme" --size 1400x900
    done
    for scene in mixed empty; do
      for size in "${extra_sizes[@]}"; do
        filename="live-$scene-$theme-$size"
        capture_png "$filename" --scene "$scene" --theme "$theme" --size "$size"
        capture_tree "$filename" --scene "$scene" --theme "$theme" --size "$size"
      done
    done
  done
}

capture_all
printf 'Live-view UI review captures and trees written to %s\n' "$OUT"
