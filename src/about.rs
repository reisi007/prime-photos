//! About window and the third-party license window (Welle 4b).
//!
//! Data comes from Welle 4a (already committed): the generated dependency list
//! (`assets/third-party-licenses.txt`, ~1.2 MB) and the curated icon attribution
//! (`docs/dev/THIRD_PARTY.md`). Both are embedded as text — nothing is read at run
//! time.
//!
//! ## Why the license list is virtualized
//!
//! Handing the whole 1.2 MB string to a single `ui.monospace(...)` would build
//! one giant galley (~1.2 M glyphs) **per frame** — unusable in memory and frame
//! time. Instead the text is split and hard-wrapped **once** into lines that are
//! each exactly one display row, and only the visible range is laid out via
//! `ScrollArea::show_rows`.

use std::sync::OnceLock;

#[cfg(feature = "legacy-eframe")]
pub(crate) mod eframe_ui;

/// Generated third-party license list (Welle 4a): every dependency from
/// `Cargo.lock` plus the full license texts. Embedded as text.
const THIRD_PARTY_LICENSES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/third-party-licenses.txt"
));

/// Curated icon/graphic attribution — the single source of truth is
/// `docs/dev/THIRD_PARTY.md` (read-only here).
const THIRD_PARTY_ICONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/dev/THIRD_PARTY.md"
));

/// Hard wrap width for the license text, in characters. The generated list is
/// mostly monospace-friendly license bodies; wrapping up front gives every line
/// exactly one display row, which is what lets `show_rows` work. The value fits
/// the window's minimum inner width (`760 px`, `src/main.rs`) at the monospace
/// text style, so no line is clipped at the smallest supported size.
const WRAP_COLS: usize = 100;

/// The wrapped license lines, computed **once** (not per frame).
static LICENSE_LINES: OnceLock<Vec<String>> = OnceLock::new();

/// The license lines, splitting and wrapping the embedded text on first use.
pub(crate) fn license_lines() -> &'static [String] {
    LICENSE_LINES.get_or_init(|| wrap_lines(THIRD_PARTY_LICENSES, WRAP_COLS))
}

/// Curated icon and graphic attributions, embedded from the maintained notice file.
#[cfg(feature = "rune-backup")]
pub(crate) fn icon_attributions() -> &'static str {
    THIRD_PARTY_ICONS
}

/// Split `text` into lines and hard-wrap every line longer than `cols`
/// characters into chunks of at most `cols` characters. Shorter lines are
/// unchanged, so every resulting line is a single display row. `chars()` keeps
/// multi-byte code points intact (a break may land mid-word — fine for license
/// text).
fn wrap_lines(text: &str, cols: usize) -> Vec<String> {
    debug_assert!(cols > 0);
    let mut out = Vec::new();
    for line in text.lines() {
        if line.chars().count() <= cols {
            out.push(line.to_string());
            continue;
        }
        let mut current = String::new();
        let mut n = 0;
        for ch in line.chars() {
            if n == cols {
                out.push(std::mem::take(&mut current));
                n = 0;
            }
            current.push(ch);
            n += 1;
        }
        out.push(current);
    }
    out
}

/// Which of the two license sources the window shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LicenseTab {
    /// The generated component list + license texts.
    #[default]
    Components,
    /// The curated icon/graphic attribution.
    Icons,
}

/// Current calendar year, for the copyright line (runtime, not build time).
pub(crate) fn current_year() -> String {
    chrono::Local::now().format("%Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_lines_leaves_short_lines_untouched() {
        assert_eq!(wrap_lines("a\nbb\nccc", 10), vec!["a", "bb", "ccc"]);
    }

    #[test]
    fn wrap_lines_breaks_only_longer_lines() {
        // 5 chars into chunks of 2 ⇒ 2, 2, 1.
        assert_eq!(wrap_lines("abcde", 2), vec!["ab", "cd", "e"]);
        // Exactly `cols` fits in one piece.
        assert_eq!(wrap_lines("abcd", 4), vec!["abcd"]);
    }

    #[test]
    fn wrap_lines_keeps_multibyte_code_points_intact() {
        // Each of these is one char; a break must never split a code point.
        assert_eq!(wrap_lines("äöüß", 2), vec!["äö", "üß"]);
    }

    #[test]
    fn wrap_lines_handles_empty_text() {
        assert!(wrap_lines("", 10).is_empty());
    }

    #[test]
    fn embedded_license_lines_are_all_within_the_wrap_width() {
        // The pre-wrap invariant that makes the virtual list correct: every
        // line is at most WRAP_COLS characters.
        for (i, line) in license_lines().iter().enumerate() {
            assert!(
                line.chars().count() <= WRAP_COLS,
                "Zeile {i} ist {} Zeichen lang",
                line.chars().count()
            );
        }
    }
}
