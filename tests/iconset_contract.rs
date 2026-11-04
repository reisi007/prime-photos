use std::fs;
use std::path::{Path, PathBuf};

const ICONSET_NAMES: [&str; 2] = ["pp-appicon.iconset", "pp-macos.iconset"];
const ICON_SIZES: [u32; 5] = [16, 32, 128, 256, 512];

fn target_dir() -> PathBuf {
    let executable = std::env::current_exe().expect("test executable path");
    let deps_dir = executable
        .ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "deps"))
        .expect("Cargo test executable must be under target/<profile>/deps");
    deps_dir
        .parent()
        .and_then(Path::parent)
        .expect("Cargo deps directory must be under target/<profile>/deps")
        .to_path_buf()
}

fn expected_entries() -> Vec<(String, u32)> {
    ICON_SIZES
        .into_iter()
        .flat_map(|size| {
            [
                (format!("icon_{size}x{size}.png"), size),
                (format!("icon_{size}x{size}@2x.png"), size * 2),
            ]
        })
        .collect()
}

fn assert_iconset_contract(iconset: &Path) {
    assert!(
        iconset.is_dir(),
        "missing generated iconset: {}",
        iconset.display()
    );

    let mut actual_names = fs::read_dir(iconset)
        .unwrap_or_else(|error| panic!("read {}: {error}", iconset.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("read entry in {}: {error}", iconset.display()))
                .file_name()
                .into_string()
                .unwrap_or_else(|name| {
                    panic!("non-UTF-8 filename in {}: {name:?}", iconset.display())
                })
        })
        .collect::<Vec<_>>();
    actual_names.sort_unstable();

    let mut expected = expected_entries();
    expected.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let expected_names = expected
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        actual_names,
        expected_names,
        "unexpected files in {}",
        iconset.display()
    );

    for (name, edge) in expected {
        let path = iconset.join(&name);
        let reader = image::ImageReader::open(&path)
            .unwrap_or_else(|error| panic!("open {}: {error}", path.display()))
            .with_guessed_format()
            .unwrap_or_else(|error| panic!("identify {}: {error}", path.display()));
        assert_eq!(
            reader.format(),
            Some(image::ImageFormat::Png),
            "{} must be a PNG",
            path.display()
        );
        let image = reader
            .decode()
            .unwrap_or_else(|error| panic!("decode {}: {error}", path.display()));
        assert_eq!(
            (image.width(), image.height()),
            (edge, edge),
            "wrong dimensions for {}",
            path.display()
        );
    }
}

#[derive(Debug)]
struct AlphaBounds {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

fn alpha_bounds(path: &Path) -> AlphaBounds {
    let image = image::open(path)
        .unwrap_or_else(|error| panic!("decode {}: {error}", path.display()))
        .to_rgba8();
    let mut bounds = AlphaBounds {
        min_x: image.width(),
        min_y: image.height(),
        max_x: 0,
        max_y: 0,
    };
    let mut found_opaque_pixel = false;

    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] != 0 {
            found_opaque_pixel = true;
            bounds.min_x = bounds.min_x.min(x);
            bounds.min_y = bounds.min_y.min(y);
            bounds.max_x = bounds.max_x.max(x);
            bounds.max_y = bounds.max_y.max(y);
        }
    }

    assert!(
        found_opaque_pixel,
        "{} contains no visible pixels",
        path.display()
    );
    bounds
}

#[test]
fn both_generated_iconsets_match_the_iconutil_file_contract() {
    let target = target_dir();
    for name in ICONSET_NAMES {
        assert_iconset_contract(&target.join(name));
    }
}

#[test]
fn macos_iconset_is_padded_and_rounded_while_debian_iconset_is_full_bleed() {
    let target = target_dir();
    let full_bleed_path = target
        .join("pp-appicon.iconset")
        .join("icon_512x512@2x.png");
    let padded_path = target.join("pp-macos.iconset").join("icon_512x512@2x.png");
    let full_bleed = alpha_bounds(&full_bleed_path);
    let padded = alpha_bounds(&padded_path);

    // Fit::Fill reaches the canvas edges. Allow a few pixels for rasterizer
    // coverage differences while still catching accidental centring/padding.
    assert!(
        full_bleed.min_x <= 4,
        "unexpected left inset: {full_bleed:?}"
    );
    assert!(
        full_bleed.min_y <= 4,
        "unexpected top inset: {full_bleed:?}"
    );
    assert!(
        full_bleed.max_x >= 1019,
        "unexpected right inset: {full_bleed:?}"
    );
    assert!(
        full_bleed.max_y >= 1019,
        "unexpected bottom inset: {full_bleed:?}"
    );

    // The 824/1024 motif has a nominal 100 px margin at 1024 px. Broad bounds
    // tolerate antialiasing while distinguishing it from the full-bleed set.
    assert!(
        (80..=120).contains(&padded.min_x),
        "unexpected left margin: {padded:?}"
    );
    assert!(
        (80..=120).contains(&padded.min_y),
        "unexpected top margin: {padded:?}"
    );
    assert!(
        (80..=120).contains(&(1023 - padded.max_x)),
        "unexpected right margin: {padded:?}"
    );
    assert!(
        (80..=120).contains(&(1023 - padded.max_y)),
        "unexpected bottom margin: {padded:?}"
    );

    // At this near-corner point the source is painted, but the larger macOS
    // corner radius cuts it off. The probes sit several pixels from each edge.
    let full_bleed_image = image::open(&full_bleed_path)
        .unwrap_or_else(|error| panic!("decode {}: {error}", full_bleed_path.display()))
        .to_rgba8();
    let padded_image = image::open(&padded_path)
        .unwrap_or_else(|error| panic!("decode {}: {error}", padded_path.display()))
        .to_rgba8();
    assert!(full_bleed_image.get_pixel(114, 25)[3] > 200);
    assert_eq!(padded_image.get_pixel(192, 120)[3], 0);
}
