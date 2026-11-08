use eframe::egui::{self, RichText};

use crate::i18n::{tr, tr1};
use crate::icons::eframe_ui;
use crate::theme::eframe_ui as theme;

use super::{current_year, license_lines, LicenseTab, THIRD_PARTY_ICONS};

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
                ui.add(eframe_ui::app_logo(ui, 64.0));
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
                ui.label(theme::muted(tr1("Version: {}", version)));
            });
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                if ui
                    .add(theme::secondary_button(
                        tr("Drittanbieterlizenzen"),
                        ui.visuals().dark_mode,
                    ))
                    .clicked()
                {
                    show_licenses = true;
                }
                ui.add_space(6.0);
                if ui
                    .add(theme::primary_button(
                        tr("Schließen"),
                        ui.visuals().dark_mode,
                    ))
                    .clicked()
                {
                    close = true;
                }
            });
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr1("© {} Prime Photos", current_year()))
                        .size(11.0)
                        .weak(),
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
/// tab is the generated list (virtualized, see the parent module docs); the second is
/// the small curated attribution rendered with a tiny Markdown subset.
pub(crate) fn license_window(ctx: &egui::Context, open: &mut bool, tab: &mut LicenseTab) {
    egui::Window::new(tr("Drittanbieterlizenzen"))
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_size(egui::vec2(760.0, 560.0))
        // Minimum width so the widest license line is never clipped. The text is
        // wrapped to `WRAP_COLS` (100) monospace characters, measured 700.25 px
        // wide (7.0025 px per character). `min_width` sets the inner content
        // width. The floating scroll bar now reserves its full strip
        // (`floating_allocated_width = bar_width + 8` in `theme.rs`, i.e. 18 px),
        // so the scroll area no longer lets content run under the bar and the
        // former slack for the 10 px handle plus 4 px inner margin is no longer
        // needed: 720 px is a deliberately chosen round minimum, not a computed
        // fit — with 18 px reserved it still leaves 720 − 18 − 700.25 ≈ 1.75 px
        // of slack for the widest line. There is only vertical scrolling, so
        // without this a user could shrink the window until the line is cut off.
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
    fn strip_inline_removes_bold_and_code_markers() {
        assert_eq!(strip_inline("**Name:** `x`"), "Name: x");
    }
}
