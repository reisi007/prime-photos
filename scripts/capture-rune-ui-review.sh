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
# The Settings fixture follows the system locale; fix it so every capture is German.
export LC_ALL=de_DE.UTF-8

capture_png() {
  local filename="$1"
  shift
  "$BIN" "$@" --scale 1 --snap "$OUT/$filename.png"
}

capture_tree() {
  local filename="$1"
  shift
  "$BIN" "$@" --scale 1 --tree "$OUT/$filename.json"
}

backup_scenes=(mixed uploading paused all-done failures empty many-folders about licenses)
for theme in light dark; do
  for scene in "${backup_scenes[@]}"; do
    capture_png "backup-$scene-$theme-1400x900" \
      --rune-backup-smoke --scene "$scene" --theme "$theme" --size 1400x900
  done
  for scene in mixed failures many-folders; do
    for size in 760x560 1000x700; do
      capture_png "backup-$scene-$theme-$size" \
        --rune-backup-smoke --scene "$scene" --theme "$theme" --size "$size"
    done
  done
done

# Keep an inspectable headless element tree for every named backup smoke scene.
for scene in "${backup_scenes[@]}"; do
  capture_tree "backup-$scene-light-1400x900" \
    --rune-backup-smoke --scene "$scene" --theme light --size 1400x900
done

for theme in light dark; do
  for size in 760x560 1000x700; do
    for scene in signed-out error; do
      capture_png "login-$scene-$theme-$size" \
        --rune-login-fixture --scene "$scene" --theme "$theme" --size "$size"
    done
    for tab in general bandwidth; do
      settings_args=(--theme "$theme" --size "$size")
      if [[ "$tab" == bandwidth ]]; then
        settings_args+=(--scene bandwidth)
      fi
      capture_png "settings-$tab-$theme-$size" "${settings_args[@]}"
    done
  done
done

printf 'Rune UI review captures and trees written to %s\n' "$OUT"
