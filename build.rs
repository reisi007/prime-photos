//! Build script for Prime Photos (asset wave, user decision 2026-10-14).
//!
//! Rasterises the SVG sources in `assets/` into PNGs inside `OUT_DIR`, where
//! `src/tray.rs` (the five tray glyphs plus the coloured logo fallback) and
//! `src/main.rs` (the window/dock icon) embed them with `include_bytes!`.
//!
//! Why a build script and why `resvg`:
//! * `sips`/`qlmanage` exist on macOS only, but the release matrix is
//!   Windows + Linux; `resvg` is pure Rust and cross-platform.
//! * The SVGs stay the single source of truth; the PNGs are derived and never
//!   checked in.
//! * Missing sources and render failures are hard build failures: the assets
//!   are compile-time mandatory, so there is deliberately **no** fallback and
//!   **no** `Option`/`exists()` guard.

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// File-name contract with the consuming code
// ---------------------------------------------------------------------------
//
// `src/tray.rs` embeds exactly the names built below and keys them by
// (state, colour, size) in its `TRAY_PNGS` table. If a name, colour or size
// changes here it must change there too — and vice versa. Keep this block and
// that table in sync.
//
//   Tray glyphs:  assets/<stem>.svg  ⇒  $OUT_DIR/<stem>-<colour>-<size>.png
//     stems:  "tray" (idle), "tray-active", "tray-paused", "tray-done",
//             "tray-error"
//     colours: "black" (#000000), "white" (#ffffff)
//     sizes:  32 and 64 px (square sources, 84×84 viewBox)
//
//   Logos (rastered as-is, no colour substitution):
//     assets/logo.svg             ⇒ $OUT_DIR/logo.png             (1024 px)
//     assets/logo-small.svg       ⇒ $OUT_DIR/logo-small.png       (256 px)
//     assets/logo-small-black.svg ⇒ $OUT_DIR/logo-small-black.png (256 px)

/// Tray glyph stems, in the order idle / active / paused / done / error.
const TRAY_STEMS: [&str; 5] = [
    "tray",
    "tray-active",
    "tray-paused",
    "tray-done",
    "tray-error",
];

/// Colour name (used in the file name) plus the hex value injected as a
/// `* { color: … }` override so the sources' `currentColor` resolves to it.
const TRAY_COLORS: [(&str, &str); 2] = [("black", "#000000"), ("white", "#ffffff")];

/// Pixel side of the generated tray glyphs.
const TRAY_SIZES: [u32; 2] = [32, 64];

/// Logo stem plus the pixel size to raster it at.
const LOGOS: [(&str, u32); 3] = [
    ("logo", 1024),
    ("logo-small", 256),
    ("logo-small-black", 256),
];

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set"));

    for stem in TRAY_STEMS {
        let svg = format!("{stem}.svg");
        for (color_name, color_hex) in TRAY_COLORS {
            for size in TRAY_SIZES {
                let png = format!("{stem}-{color_name}-{size}.png");
                render(&manifest_dir, &out_dir, &svg, Some(color_hex), &png, size);
            }
        }
    }

    for (stem, size) in LOGOS {
        let svg = format!("{stem}.svg");
        let png = format!("{stem}.png");
        render(&manifest_dir, &out_dir, &svg, None, &png, size);
    }
}

/// Raster `assets/<svg_name>` into `$OUT_DIR/<png_name>` at `size` px wide
/// (the height follows the source aspect ratio). Panics with the file name on
/// any missing source, parse or write error — a broken asset must fail the
/// build, never silently drop an icon.
fn render(
    manifest_dir: &Path,
    out_dir: &Path,
    svg_name: &str,
    color_hex: Option<&str>,
    png_name: &str,
    size: u32,
) {
    let svg_path = manifest_dir.join("assets").join(svg_name);
    // Emitted before reading so a *deleted* asset also re-runs the build.
    println!("cargo:rerun-if-changed=assets/{svg_name}");

    let source = std::fs::read_to_string(&svg_path).unwrap_or_else(|e| {
        panic!(
            "Prime Photos build.rs: SVG-Quelle fehlt oder ist nicht lesbar: {} ({e})",
            svg_path.display()
        )
    });
    // The tray glyphs paint with `stroke="currentColor"` (and one `fill=
    // "currentColor"`). `usvg` resolves `currentColor` against the *inherited*
    // `color` presentation attribute, so the requested colour goes in as a CSS
    // rule (`* { color: … }`) before parsing — the documented raster override
    // (AGENTS.md „Assets": `*{color:#fff}` / `#000`), no per-colour SVG files.
    let source = match color_hex {
        Some(hex) => inject_color_override(&source, hex, &svg_path),
        None => source,
    };

    let tree = resvg::usvg::Tree::from_str(&source, &resvg::usvg::Options::default())
        .unwrap_or_else(|e| {
            panic!(
                "Prime Photos build.rs: SVG konnte nicht geparst werden: {} ({e})",
                svg_path.display()
            )
        });
    let src = tree.size();
    if src.width() <= 0.0 || src.height() <= 0.0 {
        panic!(
            "Prime Photos build.rs: SVG hat keine positive Zeichenfläche: {}",
            svg_path.display()
        );
    }
    let scale = size as f32 / src.width();
    let height = (src.height() * scale).round().max(1.0) as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, height).unwrap_or_else(|| {
        panic!("Prime Photos build.rs: Pixmap {size}×{height} konnte nicht angelegt werden")
    });
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let out_path = out_dir.join(png_name);
    pixmap.save_png(&out_path).unwrap_or_else(|e| {
        panic!(
            "Prime Photos build.rs: PNG konnte nicht geschrieben werden: {} ({e})",
            out_path.display()
        )
    });
}

/// Insert a `* { color: <hex> }` rule right after the opening `<svg …>` tag.
///
/// The tray sources carry no colour of their own (`stroke`/`fill=
/// "currentColor"`); this rule makes `currentColor` resolve to `hex` through
/// the inheritable `color` attribute — the same path a browser would take.
/// Panics (with the file name) when there is no opening tag to inject into.
fn inject_color_override(svg: &str, hex: &str, path: &Path) -> String {
    let open_end = svg.find('>').map(|i| i + 1).unwrap_or_else(|| {
        panic!(
            "Prime Photos build.rs: kein <svg>-Tag für den Farb-Override gefunden: {}",
            path.display()
        )
    });
    let mut out = String::with_capacity(svg.len() + 32);
    out.push_str(&svg[..open_end]);
    out.push_str("<style>*{color:");
    out.push_str(hex);
    out.push_str("}</style>");
    out.push_str(&svg[open_end..]);
    out
}
