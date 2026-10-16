//! About window and the third-party license window (Welle 4b).
//!
//! Data comes from Welle 4a (already committed): the generated dependency list
//! (`assets/third-party-licenses.txt`, ~1.2 MB) and the curated icon attribution
//! (`docs/THIRD_PARTY.md`). Both are embedded as text — nothing is read at run
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

use crate::i18n::{tr, tr1};
use crate::icons;
use crate::theme;
use eframe::egui::{self, RichText};

/// Generated third-party license list (Welle 4a): every dependency from
/// `Cargo.lock` plus the full license texts. Embedded as text.
const THIRD_PARTY_LICENSES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/third-party-licenses.txt"
));

/// Curated icon/graphic attribution — the single source of truth is
/// `docs/THIRD_PARTY.md` (read-only here).
const THIRD_PARTY_ICONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/THIRD_PARTY.md"
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
fn license_lines() -> &'static [String] {
    LICENSE_LINES.get_or_init(|| wrap_lines(THIRD_PARTY_LICENSES, WRAP_COLS))
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
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LicenseTab {
    /// The generated component list + license texts.
    Components,
    /// The curated icon/graphic attribution.
    Icons,
}

/// The About window. `open` is driven by the app state; both the "Schließen"
/// button and the window's ✕ set it back to `false`. The
/// "Drittanbieterlizenzen" button raises `license_open`.
pub(crate) fn about_window(ctx: &egui::Context, open: &mut bool, license_open: &mut bool) {
    let mut close = false;
    let mut show_licenses = false;
    egui::Window::new(tr("Über Prime Photos"))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_size(egui::vec2(360.0, 260.0))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                icons::upload_logo(ui, 64.0);
                ui.add_space(8.0);
                ui.label(RichText::new(tr("Prime Photos")).size(20.0).strong());
                ui.add_space(4.0);
                // "Version: <crate> (<commit>)" — the commit id is appended only
                // when the build actually had one (see `build.rs`); nothing is
                // invented. Labels are selectable by default, so the value can
                // be copied.
                let version = match option_env!("PRIME_PHOTOS_GIT_COMMIT") {
                    Some(sha) => format!("{} ({sha})", env!("CARGO_PKG_VERSION")),
                    None => env!("CARGO_PKG_VERSION").to_string(),
                };
                ui.label(RichText::new(tr1("Version: {}", version)).color(theme::MUTED));
            });
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                if ui
                    .add(theme::secondary_button(tr("Drittanbieterlizenzen")))
                    .clicked()
                {
                    show_licenses = true;
                }
                ui.add_space(6.0);
                if ui.add(theme::primary_button(tr("Schließen"))).clicked() {
                    close = true;
                }
            });
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr1("© {} Prime Photos", current_year()))
                        .size(11.0)
                        .color(theme::MUTED),
                );
            });
        });
    if show_licenses {
        *license_open = true;
    }
    if close {
        *open = false;
    }
}

/// The license window: a resizable, scrollable viewer with two tabs. The first
/// tab is the generated list (virtualized, see the module docs); the second is
/// the small curated attribution rendered with a tiny Markdown subset.
pub(crate) fn license_window(ctx: &egui::Context, open: &mut bool, tab: &mut LicenseTab) {
    egui::Window::new(tr("Drittanbieterlizenzen"))
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_size(egui::vec2(760.0, 560.0))
        // Minimum width so the widest license line is never clipped. The text is
        // wrapped to `WRAP_COLS` (100) monospace characters, measured 700.25 px
        // wide (7.0025 px per character); the default floating scroll bar (10 px
        // plus a 4 px inner margin) floats over the right edge and would cover
        // the last characters. `min_width` sets the inner content width — the
        // scroll area fills it and a floating bar allocates no space — so
        // 700.25 + 14 ≈ 715 px, rounded up to 720 px for a small margin. There is
        // only vertical scrolling, so without this a user could shrink the window
        // until the line is cut off.
        .min_width(720.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    tab,
                    LicenseTab::Components,
                    tr("Komponenten und Lizenztexte"),
                );
                ui.selectable_value(tab, LicenseTab::Icons, tr("Icons und Grafiken"));
            });
            ui.separator();
            match tab {
                LicenseTab::Components => {
                    let lines = license_lines();
                    // The real row advance: `show_rows` adds the item spacing
                    // itself, so it needs the bare line height. `Extend` disables
                    // re-wrapping, so a pre-wrapped line stays exactly one row
                    // whatever the window width.
                    let row_h = monospace_row_height(ui);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show_rows(ui, row_h, lines.len(), |ui, range| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            for i in range {
                                ui.monospace(&lines[i]);
                            }
                        });
                }
                LicenseTab::Icons => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| render_markdown(ui, THIRD_PARTY_ICONS));
                }
            }
        });
}

/// Current calendar year, for the copyright line (runtime, not build time).
fn current_year() -> String {
    chrono::Local::now().format("%Y").to_string()
}

/// The **real** display height of one monospace line, measured from a laid-out
/// galley.
///
/// Deliberately not `ui.text_style_height(&TextStyle::Monospace)`: at the 12 px
/// monospace style the font's `row_height` is 13.96875 px, while an actually
/// laid out line occupies 14 px (measured headlessly) — a 0.03125 px difference
/// per row, ~777 px over the 24 874 wrapped lines.
///
/// That shortfall is **sub-pixel in its consequence**, not tens of unreachable
/// rows: egui positions every row by its real layout height, so only the virtual
/// list's skip offset uses the declared `row_h`. A `row_h` that is 0.03 px too
/// small therefore clips the last line by well under a pixel; the measured value
/// still avoids even that (headless-verified, `AGENTS.md` „Virtuelle Liste").
fn monospace_row_height(ui: &egui::Ui) -> f32 {
    let font_id = egui::TextStyle::Monospace.resolve(ui.style());
    ui.fonts(|f| {
        f.layout_no_wrap("X".to_owned(), font_id, egui::Color32::BLACK)
            .size()
            .y
    })
}

/// Render the curated attribution with a deliberately tiny Markdown subset:
/// `#`/`##`/`###` headings, `- ` bullets, blank lines and plain paragraphs.
/// Inline `**` markers and backticks are stripped. No Markdown crate.
fn render_markdown(ui: &mut egui::Ui, text: &str) {
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            ui.add_space(4.0);
        } else if let Some(h) = line.strip_prefix("### ") {
            ui.label(RichText::new(strip_inline(h)).size(14.0).strong());
        } else if let Some(h) = line.strip_prefix("## ") {
            ui.add_space(6.0);
            ui.label(RichText::new(strip_inline(h)).size(16.0).strong());
        } else if let Some(h) = line.strip_prefix("# ") {
            ui.add_space(6.0);
            ui.label(RichText::new(strip_inline(h)).size(18.0).strong());
        } else if let Some(b) = line.strip_prefix("- ") {
            ui.horizontal_wrapped(|ui| {
                ui.label("•");
                ui.label(strip_inline(b));
            });
        } else {
            ui.label(strip_inline(line));
        }
    }
}

/// Drop the inline Markdown markers we do not render (`**bold**`, `` `code` ``).
fn strip_inline(s: &str) -> String {
    s.replace("**", "").replace('`', "")
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
    fn strip_inline_removes_bold_and_code_markers() {
        assert_eq!(strip_inline("**Name:** `x`"), "Name: x");
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
