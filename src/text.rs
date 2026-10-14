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

#[cfg(test)]
mod tests {
    use super::*;

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
