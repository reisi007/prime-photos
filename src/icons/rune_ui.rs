//! Rune UI icons.
//!
//! Build-time rasters from `build.rs`: the two sidebar navigation glyphs are
//! white 64-px PNGs, the logo is the coloured 1024-px app icon. Rune draws
//! pixels rather than textures, so this module decodes each PNG once into
//! `rune::Pixels`, keeps the result in a [`OnceLock`], and hands out cheap
//! clones (`Pixels` is an `Arc`). Tinting a navigation icon copies the decoded
//! white raster once per target colour and only rewrites the RGB channels; the
//! alpha coverage that draws the glyph stays untouched.
//!
//! Coupling contract with `build.rs` (the other half of the pair):
//!   assets/icon-cloud.svg    ⇒ $OUT_DIR/icon-cloud.png    (64 px, white)
//!   assets/icon-settings.svg ⇒ $OUT_DIR/icon-settings.png (64 px, white)
//!   assets/logo.svg          ⇒ $OUT_DIR/logo.png          (1024 px, colour)

use rune::{Color, Pixels};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The filled "cloud" sidebar-navigation icon ("Datensicherung").
const NAV_CLOUD_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-cloud.png"));
/// The filled "settings" sidebar-navigation icon ("Einstellungen").
const NAV_SETTINGS_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-settings.png"));
/// The detailed app logo (`assets/logo.svg`, 1024 px, coloured).
const APP_LOGO_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo.png"));

/// Logical display size of a sidebar navigation icon, in points. Matches the
/// sidebar's 18-px icon column; the 64-px source stays crisp up to ~3.5×
/// DPI.
pub(crate) const NAV_ICON_PX: f32 = 18.0;

/// Logical display size of the brand logo in points, matching the sidebar's
/// 30-pt logo.
pub(crate) const BRAND_LOGO_PX: f32 = 30.0;

/// Which sidebar navigation glyph to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NavIcon {
    Backup,
    Settings,
}

/// A decoded source raster, ready to be tinted or shown as-is.
struct Raster {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn decode(png: &[u8]) -> Raster {
    let image = image::load_from_memory(png)
        .expect("Prime Photos: eingebettetes Rune-Icon-PNG konnte nicht dekodiert werden")
        .to_rgba8();
    Raster {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

fn base_raster(icon: NavIcon) -> &'static Raster {
    static CLOUD: OnceLock<Raster> = OnceLock::new();
    static SETTINGS: OnceLock<Raster> = OnceLock::new();
    match icon {
        NavIcon::Backup => CLOUD.get_or_init(|| decode(NAV_CLOUD_PNG)),
        NavIcon::Settings => SETTINGS.get_or_init(|| decode(NAV_SETTINGS_PNG)),
    }
}

/// A navigation icon tinted to `color`. Decoding happens once per source; the
/// tinted pixels are cached per (icon, colour) so a frame only clones an `Arc`.
pub(crate) fn nav_icon(icon: NavIcon, color: Color) -> Pixels {
    static TINTED: OnceLock<Mutex<HashMap<(NavIcon, [u8; 3]), Pixels>>> = OnceLock::new();
    let key = (icon, srgb_rgb(color));
    let mut tinted = TINTED
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    tinted
        .entry(key)
        .or_insert_with(|| {
            let raster = base_raster(icon);
            let pixels = Pixels::new();
            pixels.set(raster.width, raster.height, tint(&raster.rgba, key.1));
            pixels
        })
        .clone()
}

/// The coloured app logo. Unlike the navigation icons it is never tinted.
pub(crate) fn brand_logo() -> Pixels {
    static LOGO: OnceLock<Pixels> = OnceLock::new();
    LOGO.get_or_init(|| {
        let raster = decode(APP_LOGO_PNG);
        let pixels = Pixels::new();
        pixels.set(raster.width, raster.height, raster.rgba);
        pixels
    })
    .clone()
}

/// Replace the RGB channels of a straight-alpha white raster with `rgb`,
/// keeping every alpha value (the glyph's coverage).
fn tint(rgba: &[u8], [r, g, b]: [u8; 3]) -> Vec<u8> {
    let mut tinted = rgba.to_vec();
    for pixel in tinted.chunks_exact_mut(4) {
        pixel[0] = r;
        pixel[1] = g;
        pixel[2] = b;
    }
    tinted
}

/// The FFI colour as the straight sRGB bytes `Pixels::set` expects.
///
/// Rune stores colours in linear light; `Color::hex` prints them in sRGB,
/// which is exactly the encoding the build-time rasters and the pixel buffer
/// use.
fn srgb_rgb(color: Color) -> [u8; 3] {
    let hex = color.hex();
    let byte = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).expect("Color::hex prints two hex digits per channel")
    };
    [byte(1..3), byte(3..5), byte(5..7)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_replaces_rgb_and_keeps_the_alpha_coverage() {
        let white = [
            255, 255, 255, 255, // opaque glyph
            255, 255, 255, 128, // anti-aliased edge
            255, 255, 255, 0,   // transparent surround
        ];
        let tinted = tint(&white, [0x00, 0x67, 0x75]);
        assert_eq!(&tinted[..4], &[0x00, 0x67, 0x75, 255]);
        assert_eq!(&tinted[4..8], &[0x00, 0x67, 0x75, 128]);
        assert_eq!(&tinted[8..12], &[0x00, 0x67, 0x75, 0]);
    }

    #[test]
    fn ffi_colors_become_the_srgb_bytes_the_raster_pipeline_used() {
        for (hex, expected) in [
            ("#006775", [0x00, 0x67, 0x75]),
            ("#72d8e8", [0x72, 0xd8, 0xe8]),
            ("#4a5558", [0x4a, 0x55, 0x58]),
            ("#bac4c6", [0xba, 0xc4, 0xc6]),
        ] {
            assert_eq!(
                srgb_rgb(rune::color::hex(hex).expect("test hex is valid")),
                expected,
                "{hex} keeps its exact sRGB channels"
            );
        }
    }

    /// The embedded rasters come from the build pipeline; decoding them here
    /// proves the contract (`build.rs` `ICONS`/`LOGOS`) holds for the Rune path
    /// without reading any asset at test time.
    #[test]
    fn embedded_rasters_decode_to_their_build_pipeline_size() {
        for (name, png, side) in [
            ("icon-cloud", NAV_CLOUD_PNG, 64),
            ("icon-settings", NAV_SETTINGS_PNG, 64),
            ("logo", APP_LOGO_PNG, 1024),
        ] {
            let raster = decode(png);
            assert_eq!(
                (raster.width, raster.height),
                (side, side),
                "{name} is a square {side}-px raster"
            );
            assert_eq!(
                raster.rgba.len(),
                (raster.width * raster.height * 4) as usize,
                "{name} is straight RGBA"
            );
        }
    }
}
