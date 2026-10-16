//! Small, shared text helpers.

use unicode_normalization::UnicodeNormalization;

/// Normalise `s` to Unicode **NFC**.
///
/// macOS hands out file names in **NFD** (`Ö` is the two codepoints `O` +
/// U+0308), Windows in **NFC** (`Ö` is the single codepoint U+00D6), so the
/// same name is a different byte sequence on the two platforms. Amazon stores
/// and compares names exactly/ordinally (the original's
/// `GetNodeByNameAndParentIfExists` is a plain `name = ?`), so without
/// normalisation "Replace" finds the node written from Windows when run on
/// Windows, but not when run on macOS — and creates a second copy. NFC is the
/// common form (Windows and the web UI are NFC). The embedded egui font also
/// lacks a glyph for the combining diaeresis, so an NFD name renders as
/// `O□FB`.
///
/// Applied to the **name sent to the server** (`urls::upload_url`) and to
/// **displayed text** only. The local path and the MD5/dedup index keys stay
/// untouched: they are path bytes on disk and must not shift. NFC of an
/// already-NFC (or pure ASCII) string is a no-op.
pub(crate) fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Human-readable byte size, matching the original's `HumanizedSize`
/// (docs/original/SPEC_UPLOAD_PROGRESS.md §7.6): **binary** (÷1024), units
/// `B/KB/MB/GB/TB` (short, untranslated), whole bytes without a decimal and
/// every larger value with **one decimal, truncated** (abgerundet — not
/// rounded), so `1535` is `1.4 KB`, never `1.5`.
pub(crate) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // Pick the largest unit the value fills, then keep the first decimal by
    // working in tenths of that unit and truncating (no rounding).
    let mut unit = 0usize;
    let mut divisor = 1u64;
    while unit + 1 < UNITS.len() && bytes / divisor >= 1024 {
        divisor *= 1024;
        unit += 1;
    }
    let tenths = (u128::from(bytes) * 10) / u128::from(divisor);
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_is_binary_and_truncates() {
        // Whole bytes stay an integer.
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
        // Binary steps at 1024, one decimal.
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_size(1024u64.pow(4)), "1.0 TB");
        // 1535 / 1024 = 1.499… → truncated 1.4 (rounded would be 1.5).
        assert_eq!(format_size(1535), "1.4 KB");
        assert_eq!(format_size(1024 + 512), "1.5 KB");
    }

    #[test]
    fn format_size_steps_at_exactly_1024_and_truncates() {
        // The unit steps at **exactly** 1024: one byte below stays in the
        // smaller unit and truncates (a rounded value would read 1024.0).
        assert_eq!(format_size(1024 * 1024 - 1), "1023.9 KB");
        assert_eq!(format_size(1024 * 1024 * 1024 - 1), "1023.9 MB");
        assert_eq!(format_size(1024u64.pow(4) - 1), "1023.9 GB");
    }

    #[test]
    fn nfc_folds_nfd_to_nfc() {
        // "O" + combining diaeresis (NFD) becomes the single "Ö" (U+00D6).
        assert_eq!(nfc("O\u{308}"), "\u{D6}");
        // A full NFD file name folds the same way.
        assert_eq!(nfc("O\u{308}FB-Reisinger_01.dng"), "\u{D6}FB-Reisinger_01.dng");
    }

    #[test]
    fn nfc_keeps_already_nfc_unchanged() {
        assert_eq!(nfc("\u{D6}FB-Reisinger_01.dng"), "\u{D6}FB-Reisinger_01.dng");
        assert_eq!(nfc("\u{D6}sterreich"), "\u{D6}sterreich");
    }

    #[test]
    fn nfc_keeps_ascii_unchanged() {
        assert_eq!(nfc("photo_01.jpg"), "photo_01.jpg");
        assert_eq!(nfc(""), "");
    }
}
