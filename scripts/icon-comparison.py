#!/usr/bin/env python3
"""Build the visual app-icon comparison for #34 as a PNG.

Usage:
    cargo build --release              # creates target/pp-macos.iconset/ and
                                       # target/pp-appicon.iconset/
    python3 scripts/icon-comparison.py [--output <png>]

The script writes `target/icon-comparison.png` (gitignored, never checked in):
three 512 px tiles side by side on a light background —

    1. `target/pp-appicon.iconset/icon_512x512@2x.png` — the full-bleed raster
       the Linux `.deb` ships (motif 100 % of the edge),
    2. `target/pp-macos.iconset/icon_512x512@2x.png` — the macOS bundle raster
       (motif 824/1024, corners ~22.5 %) that `make-app.sh` feeds to `iconutil`,
    3. a system icon of this Mac as the reference (Preview; extracted with
       `iconutil -c iconset` from its `.icns` into a temporary directory).

The footer labels carry the motif share measured from the alpha bounding box,
so the numbers in the picture are the numbers of the icons shown.

Requirements: Python 3 and Pillow (`pip install Pillow`); only the optional
reference tile additionally needs macOS with `/usr/bin/iconutil`. Without it
the picture is built from the two local rasters and the reference is skipped.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

REPO_ROOT = Path(__file__).resolve().parent.parent
TILE = 512
GAP = 24
CAPTION = 28
BACKGROUND = (236, 236, 236, 255)

# System apps whose `.icns` carries a 256 px entry; the first one that exists
# and converts wins. Purely a reference for Apple's own app-icon grid.
SYSTEM_ICONS = [
    "/System/Applications/Preview.app",
    "/System/Applications/Utilities/Calculator.app",
]


def motif_share(path: Path) -> float:
    """Share of the raster edge the visible motif occupies (alpha > 8/255)."""
    image = Image.open(path).convert("RGBA")
    box = image.getchannel("A").point(lambda a: 255 if a > 8 else 0).getbbox()
    if box is None:
        return 0.0
    return (box[2] - box[0]) / image.width


def tile(path: Path) -> Image.Image:
    return Image.open(path).convert("RGBA").resize((TILE, TILE), Image.LANCZOS)


def system_tile() -> Image.Image | None:
    """Reference icon from a system app, via `iconutil` (macOS only)."""
    iconutil = shutil.which("iconutil")
    if iconutil is None:
        print("iconutil not found — skipping the system reference tile")
        return None
    workdir = Path(tempfile.mkdtemp(prefix="icon-comparison-"))
    try:
        for app in SYSTEM_ICONS:
            icns = Path(app) / "Contents" / "Resources" / "AppIcon.icns"
            if not icns.is_file():
                continue
            out = workdir / "ref.iconset"
            result = subprocess.run(
                [iconutil, "-c", "iconset", str(icns), "-o", str(out)],
                capture_output=True,
                text=True,
            )
            if result.returncode != 0:
                continue
            png = out / "icon_128x128@2x.png"
            if png.is_file():
                return tile(png)
        print("no system icon could be extracted — skipping the reference tile")
        return None
    finally:
        shutil.rmtree(workdir, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--output",
        type=Path,
        default=REPO_ROOT / "target" / "icon-comparison.png",
        help="output PNG (default: target/icon-comparison.png)",
    )
    args = parser.parse_args()

    base = REPO_ROOT / "target" / "pp-appicon.iconset" / "icon_512x512@2x.png"
    macos = REPO_ROOT / "target" / "pp-macos.iconset" / "icon_512x512@2x.png"
    for path in (base, macos):
        if not path.is_file():
            print(f"missing {path} — run 'cargo build --release' first", file=sys.stderr)
            return 1

    entries = [
        (tile(base), f"vollflaechig (.deb) {motif_share(base) * 100:.1f} %"),
        (tile(macos), f"macOS-Raster (#34) {motif_share(macos) * 100:.1f} %"),
    ]
    reference = system_tile()
    if reference is not None:
        entries.append((reference, "System-Icon (Referenz)"))

    width = len(entries) * TILE + (len(entries) + 1) * GAP
    height = TILE + 2 * GAP + CAPTION
    image = Image.new("RGBA", (width, height), BACKGROUND)
    for index, (icon, label) in enumerate(entries):
        image.alpha_composite(icon, (GAP + index * (TILE + GAP), GAP))
        ImageDraw.Draw(image).text(
            (GAP + index * (TILE + GAP) + 4, TILE + GAP + 8),
            label,
            fill=(40, 40, 40, 255),
        )

    args.output.parent.mkdir(parents=True, exist_ok=True)
    image.convert("RGB").save(args.output)
    print(f"wrote {args.output} ({width}x{height})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
