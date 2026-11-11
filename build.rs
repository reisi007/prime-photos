//! Build script for Prime Photos (asset wave, user decision 2026-10-14).
//!
//! Rasterises the SVG sources in `assets/` into PNGs inside `OUT_DIR`, where
//! `src/tray.rs` (the five tray glyphs plus the coloured logo fallback) and
//! `src/icons/rune_ui.rs` (the two sidebar navigation icons and the app logo)
//! embed them with `include_bytes!`; the two row-action glyphs are still
//! rastered but no module embeds them.
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
// `src/tray.rs` embeds exactly the names built below, keyed by state in its
// `BLACK_PNGS_64` / `BLACK_PNGS_32` / `WHITE_PNGS_32` tables. If a name, colour
// or size changes here it must change there too — and vice versa. Keep this
// block and those tables in sync.
//
//   Tray glyphs:  assets/<stem>.svg  ⇒  $OUT_DIR/<stem>-<colour>-<size>.png
//     stems:  "tray" (idle), "tray-active", "tray-paused", "tray-done",
//             "tray-error"
//     colours: "black" (#000000), "white" (#ffffff)
//     sizes:  32 and 64 px (square 24×24 Material Symbols sources, viewBox
//             "0 -960 960 960"; user decision 2026-10-17)
//
//   Logos (rastered as-is, no colour substitution):
//     assets/logo.svg             ⇒ $OUT_DIR/logo.png             (1024 px)
//     assets/logo-small.svg       ⇒ $OUT_DIR/logo-small.png       (256 px)
//
//   Tray glyphs and UI icons are Material Symbols (Apache-2.0 — see
//   docs/dev/THIRD_PARTY.md). The tray glyphs are the five states; the embedded
//   UI icons are the two sidebar navigation icons that `src/icons/rune_ui.rs`
//   tints to the theme colour at run time. The two row-action glyphs are still
//   rastered but currently have no consumer. All of these sources carry **no**
//   `fill`/`currentColor` (a bare `<path d="…"/>` paints black by default), so
//   they get a `path { fill: … }` override:
//     assets/icon-cloud.svg     ⇒ $OUT_DIR/icon-cloud.png     (64 px, white)
//     assets/icon-settings.svg  ⇒ $OUT_DIR/icon-settings.png  (64 px, white)
//     assets/icon-more-vert.svg ⇒ $OUT_DIR/icon-more-vert.png (64 px, white)
//     assets/icon-refresh.svg   ⇒ $OUT_DIR/icon-refresh.png   (64 px, white)
//
//   App icons (the macOS Dock/taskbar icon and the Linux `.deb` icons):
//     assets/logo.svg ⇒ <target>/pp-macos.iconset/<name>.png    (10 files, macOS)
//                      <target>/pp-appicon.iconset/<name>.png   (10 files, Linux)
//   Deliberately **not** inside `$OUT_DIR`: `packaging/make-app.sh` and
//   `make-deb.sh` name the directories, and the ten file names/pixel sizes are
//   fixed by `iconutil` — do not rename them. `<target>` is recovered from
//   `$OUT_DIR` (see `target_dir_from_out_dir`).
//   Only the fit differs: macOS gets Apple's grid (motif 824 of 1024 px,
//   centred, corners re-rounded to ~22.5 % — `Fit::Centred`), because the
//   edge-to-edge tile looks oversized in the Dock; the Linux `.deb` keeps the
//   full-bleed rasters (`Fit::Fill`) so its icon matches the window icon. The
//   SVG itself stays untouched: `logo.png`/`logo-small.png` (window, sidebar)
//   want the full-bleed tile. At 16/32/64 px the integer motif edge rounds to
//   13/26/52 px (81.25 %), the odd leftover pixel goes to one side.

/// Tray glyph stems, in the order idle / active / paused / done / error.
const TRAY_STEMS: [&str; 5] = [
    "tray",
    "tray-active",
    "tray-paused",
    "tray-done",
    "tray-error",
];

/// Colour name (used in the file name) plus the hex value injected as a
/// `path { fill: … }` override — the Material Symbols tray sources carry no
/// paint of their own (a bare `<path>` paints black by default).
const TRAY_COLORS: [(&str, &str); 2] = [("black", "#000000"), ("white", "#ffffff")];

/// Pixel side of the generated tray glyphs.
const TRAY_SIZES: [u32; 2] = [32, 64];

/// Logo stem plus the pixel size to raster it at.
const LOGOS: [(&str, u32); 2] = [
    ("logo", 1024),
    ("logo-small", 256),
];

/// UI icon stems (Material Symbols) plus the pixel size to raster them at: the
/// two sidebar navigation icons (`icon-cloud`, `icon-settings`) and the two
/// row-action icons (`icon-more-vert`, `icon-refresh`). Rastered white, one
/// file each — the UI tints them at run time.
const ICONS: [(&str, u32); 4] = [
    ("icon-cloud", 64),
    ("icon-settings", 64),
    ("icon-more-vert", 64),
    ("icon-refresh", 64),
];

/// Directory (under Cargo's `<target>`) with the **full-bleed** base rasters of
/// the ten icon sizes: `packaging/make-deb.sh` installs six of them as the
/// Linux hicolor icons. Historic name, named by the release workflow, so it
/// stays.
const BASE_ICONSET_DIR: &str = "pp-appicon.iconset";

/// Directory (under `<target>`) that `packaging/make-app.sh` feeds to
/// `iconutil`: the same ten rasters on Apple's icon grid (padded, re-rounded).
const MACOS_ICONSET_DIR: &str = "pp-macos.iconset";

/// Share of the raster edge the macOS app-icon motif occupies: Apple's grid
/// draws it on 824 of 1024 px (a ~10 % transparent margin on every side), so
/// third-party icons match the system ones in Dock, Launchpad and Finder. The
/// padding happens while rastering, never in the SVG — which is also the
/// window/sidebar logo.
const APPICON_MOTIF_FRACTION: f32 = 824.0 / 1024.0;

/// Corner radius of the macOS motif, as a share of its edge: Apple's grid
/// rounds the 824 px motif by ~185 px (22.5 %), `assets/logo.svg` only by
/// 19.8 %, so the raster masks the corners instead of changing the SVG.
const APPICON_CORNER_RADIUS_FRACTION: f32 = 0.225;

/// The ten iconset entries `iconutil` requires, as (file name, pixel size).
/// The names and sizes are mandated by `iconutil` and mirrored by the Tray/
/// Dock icon of the bundle; they are rastered from `assets/logo.svg`.
const APPICON_ICONSET: [(&str, u32); 10] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
];

/// How a source's paint is recoloured before rasterising.
enum Recolor {
    /// Leave the source untouched (the logos carry their own colours).
    AsIs,
    /// Inject `path { fill: <hex> }` — for sources with no paint of their own
    /// (a bare `<path>` defaults to black), e.g. the tray glyphs and the UI
    /// icons (Material Symbols).
    Fill(&'static str),
}

/// How the rendered motif sits inside its raster.
#[derive(Clone, Copy)]
enum Fit {
    /// Edge to edge (tray glyphs, UI icons, logos, Linux icons).
    Fill,
    /// Scaled to `fraction` of the raster width, centred (the raster is square
    /// for every caller that uses this), transparent surround, corners masked to
    /// `corner_radius` (share of the motif edge) — Apple's macOS app-icon grid.
    Centred {
        fraction: f32,
        corner_radius: f32,
    },
}

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set"));

    for stem in TRAY_STEMS {
        let svg = format!("{stem}.svg");
        for (color_name, color_hex) in TRAY_COLORS {
            for size in TRAY_SIZES {
                let png = format!("{stem}-{color_name}-{size}.png");
                render(&manifest_dir, &out_dir, &svg, Recolor::Fill(color_hex), &png, size, Fit::Fill);
            }
        }
    }

    for (stem, size) in LOGOS {
        let svg = format!("{stem}.svg");
        let png = format!("{stem}.png");
        render(&manifest_dir, &out_dir, &svg, Recolor::AsIs, &png, size, Fit::Fill);
    }

    for (stem, size) in ICONS {
        let svg = format!("{stem}.svg");
        let png = format!("{stem}.png");
        render(&manifest_dir, &out_dir, &svg, Recolor::Fill("#ffffff"), &png, size, Fit::Fill);
    }

    // The app icons: two directories next to (not inside) OUT_DIR, where the
    // packaging scripts pick them up. Same ten `iconutil` names and sizes, only
    // the fit differs (see the header contract).
    let macos_fit = Fit::Centred { fraction: APPICON_MOTIF_FRACTION, corner_radius: APPICON_CORNER_RADIUS_FRACTION };
    let target_dir = target_dir_from_out_dir(&out_dir);
    for (dir, fit) in [(BASE_ICONSET_DIR, Fit::Fill), (MACOS_ICONSET_DIR, macos_fit)] {
        let dir = reset_dir(&target_dir.join(dir));
        for (png, size) in APPICON_ICONSET {
            render(&manifest_dir, &dir, "logo.svg", Recolor::AsIs, png, size, fit);
        }
    }

    // Best-effort build-time commit id for the About window (`src/about.rs`),
    // which mirrors the original's `AssemblyVersion.CommitId` line. It is only
    // emitted when `git` is actually available and answers; a source tarball or
    // a missing `.git` must neither fail the build nor invent a value — the
    // About window then simply shows the crate version alone.
    //
    // Deliberately **no** `cargo:rerun-if-changed` here: `build.rs` already
    // emits one per SVG asset above, and emitting a directive switches Cargo
    // away from its default "rebuild on any package change" — adding another
    // one would not change that, but it would wrongly imply the commit id is
    // tracked. It is picked up whenever `build.rs` runs anyway.
    if let Some(sha) = git_short_sha(&manifest_dir) {
        println!("cargo:rustc-env=PRIME_PHOTOS_GIT_COMMIT={sha}");
    }
}

/// Best-effort `git rev-parse --short HEAD`, run in the crate root. `None` when
/// `git` is missing, the directory is not a work tree, or the command fails for
/// any other reason — the caller then emits no commit id at all.
fn git_short_sha(dir: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    let sha = sha.trim();
    if sha.is_empty() {
        None
    } else {
        Some(sha.to_string())
    }
}

/// Recover Cargo's `<target>` directory from `OUT_DIR`.
///
/// `OUT_DIR` has the shape `<target>/<profile>/build/<crate>-<hash>/out`. This
/// walks up to the ancestor named `build` and returns its grandparent. Panics
/// with the offending path when the shape is unexpected: the iconset location
/// is a contract with `packaging/make-app.sh`, so a silent fallback would drop
/// the files where `iconutil` never looks — fail fast instead.
fn target_dir_from_out_dir(out_dir: &Path) -> PathBuf {
    for ancestor in out_dir.ancestors() {
        if ancestor.file_name() == Some(std::ffi::OsStr::new("build")) {
            let profile = ancestor.parent().unwrap_or_else(|| {
                panic!(
                    "Prime Photos build.rs: '<target>/<profile>/build' erwartet, aber dem 'build'-Verzeichnis fehlt der Profil-Elternteil: {}",
                    out_dir.display()
                )
            });
            let target = profile.parent().unwrap_or_else(|| {
                panic!(
                    "Prime Photos build.rs: '<target>/<profile>/build' erwartet, aber dem Profil-Verzeichnis fehlt der target-Elternteil: {}",
                    out_dir.display()
                )
            });
            return target.to_path_buf();
        }
    }
    panic!(
        "Prime Photos build.rs: OUT_DIR ohne 'build'-Verzeichnis im Pfad (erwartet '<target>/<profile>/build/<crate>/out'): {}",
        out_dir.display()
    );
}

/// Empty `dir` and recreate it; panics on failure (a half-written iconset must
/// fail the build, the packaging scripts rely on it).
fn reset_dir(dir: &Path) -> PathBuf {
    if dir.is_dir() {
        std::fs::remove_dir_all(dir).unwrap_or_else(|e| {
            panic!(
                "Prime Photos build.rs: altes iconset-Verzeichnis konnte nicht entfernt werden: {} ({e})",
                dir.display()
            )
        });
    }
    std::fs::create_dir_all(dir).unwrap_or_else(|e| {
        panic!(
            "Prime Photos build.rs: iconset-Verzeichnis konnte nicht angelegt werden: {} ({e})",
            dir.display()
        )
    });
    dir.to_path_buf()
}

/// Raster `assets/<svg_name>` into `$OUT_DIR/<png_name>` at `size` px wide
/// (the height follows the source aspect ratio). `fit` decides whether the
/// motif fills the raster or is scaled down and centred on it (macOS app-icon
/// grid). Panics with the file name on any missing source, parse or write
/// error — a broken asset must fail the build, never silently drop an icon.
fn render(
    manifest_dir: &Path,
    out_dir: &Path,
    svg_name: &str,
    recolor: Recolor,
    png_name: &str,
    size: u32,
    fit: Fit,
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
    // Tray glyphs and UI icons (Material Symbols) carry **no** paint at all (a
    // bare `<path>` defaults to black), so a `path { fill: … }` rule is injected
    // before parsing — there are no per-colour SVG files. The logos keep their
    // own colours (`Recolor::AsIs`).
    let source = match recolor {
        Recolor::AsIs => source,
        Recolor::Fill(hex) => inject_fill_override(&source, hex, &svg_path),
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
    let height = (src.height() * (size as f32 / src.width())).round().max(1.0) as u32;

    // `Fill` is the historic behaviour: motif width = raster width, height from
    // the source aspect ratio. `Centred` shrinks the motif to whole pixels and
    // centres it; at 16/32 px the even split does not exist (16 − 13 = 3), so
    // one side gets the spare pixel instead of a half-covered edge.
    let (scale, offset_x, offset_y, corner_mask) = match fit {
        Fit::Fill => (size as f32 / src.width(), 0.0, 0.0, None),
        Fit::Centred {
            fraction,
            corner_radius,
        } => {
            let motif_w = (size as f32 * fraction).round().max(1.0);
            let motif_h = (height as f32 * (motif_w / size as f32)).round().max(1.0);
            let offset_x = ((size as f32 - motif_w) / 2.0).round();
            let offset_y = ((height as f32 - motif_h) / 2.0).round();
            (
                motif_w / src.width(),
                offset_x,
                offset_y,
                Some(CornerMask {
                    x: offset_x,
                    y: offset_y,
                    width: motif_w,
                    height: motif_h,
                    radius: motif_w * corner_radius,
                }),
            )
        }
    };

    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, height).unwrap_or_else(|| {
        panic!("Prime Photos build.rs: Pixmap {size}×{height} konnte nicht angelegt werden")
    });
    resvg::render(
        &tree,
        // x' = scale·x + offset_x, y' = scale·y + offset_y
        resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, offset_x, offset_y),
        &mut pixmap.as_mut(),
    );

    if let Some(mask) = corner_mask {
        round_corners(&mut pixmap, &mask);
    }

    let out_path = out_dir.join(png_name);
    pixmap.save_png(&out_path).unwrap_or_else(|e| {
        panic!(
            "Prime Photos build.rs: PNG konnte nicht geschrieben werden: {} ({e})",
            out_path.display()
        )
    });
}

/// Rounded rectangle (pixels) the macOS app-icon mask is cut from.
struct CornerMask {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
}

/// Re-round the motif's corners to `mask.radius`: `logo.svg` draws 19.8 %,
/// Apple's grid wants ~22.5 %, so the mask cuts deeper. Along the straight
/// sides it coincides with the motif edge, at the corners it cuts *inside* the
/// opaque tile — the mask edge is the only edge, no second outline appears.
fn round_corners(pixmap: &mut resvg::tiny_skia::Pixmap, mask: &CornerMask) {
    let mut alpha = resvg::tiny_skia::Pixmap::new(pixmap.width(), pixmap.height())
        .unwrap_or_else(|| panic!("Prime Photos build.rs: Masken-Pixmap fehlgeschlagen"));
    let mut paint = resvg::tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    let path = rounded_rect(mask);
    alpha.fill_path(
        &path,
        &paint,
        resvg::tiny_skia::FillRule::Winding,
        resvg::tiny_skia::Transform::identity(),
        None,
    );

    // `DestinationIn`: keep the destination where the mask is opaque, multiply
    // its alpha everywhere else.
    let mask_paint = resvg::tiny_skia::PixmapPaint {
        blend_mode: resvg::tiny_skia::BlendMode::DestinationIn,
        ..resvg::tiny_skia::PixmapPaint::default()
    };
    pixmap.draw_pixmap(0, 0, alpha.as_ref(), &mask_paint, resvg::tiny_skia::Transform::identity(), None);
}

/// Path of `mask`'s rounded rectangle: quarter circles as cubic Béziers
/// (circle constant 0.5523 · r).
fn rounded_rect(mask: &CornerMask) -> resvg::tiny_skia::Path {
    let r = mask.radius.min(mask.width / 2.0).min(mask.height / 2.0);
    let k = 0.552_284_8 * r;
    let (left, top) = (mask.x, mask.y);
    let (right, bottom) = (mask.x + mask.width, mask.y + mask.height);

    let mut builder = resvg::tiny_skia::PathBuilder::new();
    builder.move_to(left + r, top);
    builder.line_to(right - r, top);
    builder.cubic_to(right - r + k, top, right, top + r - k, right, top + r);
    builder.line_to(right, bottom - r);
    builder.cubic_to(right, bottom - r + k, right - r + k, bottom, right - r, bottom);
    builder.line_to(left + r, bottom);
    builder.cubic_to(left + r - k, bottom, left, bottom - r + k, left, bottom - r);
    builder.line_to(left, top + r);
    builder.cubic_to(left, top + r - k, left + r - k, top, left + r, top);
    builder.close();
    builder.finish().unwrap_or_else(|| {
        panic!("Prime Photos build.rs: Pfad der App-Icon-Maske konnte nicht gebaut werden")
    })
}

/// Insert a `path { fill: <hex> }` rule right after the opening `<svg …>` tag.
///
/// The tray glyphs and the UI icons (Material Symbols) have no
/// `fill`/`currentColor` of their own, so a `color` rule would not touch them:
/// they would stay black (the SVG default). A `fill` rule on the shape does
/// apply, which is what turns them the requested colour.
fn inject_fill_override(svg: &str, hex: &str, path: &Path) -> String {
    inject_style(svg, &format!("path{{fill:{hex}}}"), path)
}

/// Insert a `<style>…</style>` rule right after the opening `<svg …>` tag.
/// Panics (with the file name) when there is no opening tag to inject into.
fn inject_style(svg: &str, rule: &str, path: &Path) -> String {
    let open_end = svg.find('>').map(|i| i + 1).unwrap_or_else(|| {
        panic!(
            "Prime Photos build.rs: kein <svg>-Tag für den Farb-Override gefunden: {}",
            path.display()
        )
    });
    let mut out = String::with_capacity(svg.len() + rule.len() + 16);
    out.push_str(&svg[..open_end]);
    out.push_str("<style>");
    out.push_str(rule);
    out.push_str("</style>");
    out.push_str(&svg[open_end..]);
    out
}
