// ---------------------------------------------------------------------------
// Cookie parsing (manual fallback)
// ---------------------------------------------------------------------------

use crate::i18n::tr;

pub(crate) struct ParsedCookies {
    pub(crate) session_id: String,
    pub(crate) country: String,
    pub(crate) cookie_x_acb: String,
    pub(crate) cookie_at_acb: String,
    pub(crate) cookie_ubid_acb: String,
    pub(crate) cookie_x_amz_access_token: String,
}

pub(crate) fn parse_cookie_string(input: &str) -> ParsedCookies {
    let mut result = ParsedCookies {
        session_id: String::new(),
        country: String::new(),
        cookie_x_acb: String::new(),
        cookie_at_acb: String::new(),
        cookie_ubid_acb: String::new(),
        cookie_x_amz_access_token: String::new(),
    };
    let mut detected_country = String::new();
    for part in input.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((name, value)) = part.split_once('=') {
            let name = name.trim();
            let value = value.trim().trim_matches('"');
            match name {
                "session-id" => result.session_id = value.to_string(),
                n if n.starts_with("x-acb") && n.len() > 5 => {
                    if result.cookie_x_acb.is_empty() {
                        result.cookie_x_acb = value.to_string();
                    }
                    if detected_country.is_empty() {
                        detected_country = n["x-acb".len()..].to_lowercase();
                    }
                }
                n if n.starts_with("at-acb") => {
                    if result.cookie_at_acb.is_empty() {
                        result.cookie_at_acb = value.to_string();
                    }
                    if detected_country.is_empty() {
                        detected_country = n["at-acb".len()..].to_lowercase();
                    }
                }
                n if n.starts_with("ubid-acb") => {
                    if result.cookie_ubid_acb.is_empty() {
                        result.cookie_ubid_acb = value.to_string();
                    }
                }
                n if n.starts_with("sess-at-acb") => {
                    if result.cookie_x_amz_access_token.is_empty() {
                        result.cookie_x_amz_access_token = value.to_string();
                    }
                }
                _ => {}
            }
        }
    }
    result.country = detected_country;
    result
}

// ---------------------------------------------------------------------------
// Country / marketplace selection
// ---------------------------------------------------------------------------

/// Marketplaces as `(country code, amazon domain suffix, name key)`. The name
/// is a German source string resolved through [`tr`]; the domain stays literal.
pub(crate) const COUNTRIES: &[(&str, &str, &str)] = &[
    ("de", "de", "Deutschland"),
    ("at", "de", "Österreich"),
    ("ch", "de", "Schweiz"),
    ("us", "com", "USA"),
    ("uk", "co.uk", "Vereinigtes Königreich"),
    ("fr", "fr", "Frankreich"),
    ("it", "it", "Italien"),
    ("es", "es", "Spanien"),
    ("ca", "ca", "Kanada"),
    ("jp", "co.jp", "Japan"),
    ("in", "in", "Indien"),
    ("au", "com.au", "Australien"),
    ("br", "com.br", "Brasilien"),
    ("mx", "com.mx", "Mexiko"),
];

pub(crate) fn country_label(code: &str) -> String {
    COUNTRIES
        .iter()
        .find(|&&(c, _, _)| c == code)
        .map(|&(_, domain, name)| format!("{} · amazon.{}", tr(name), domain))
        .unwrap_or_else(|| code.to_string())
}

/// Best-effort country from the system locale (macOS `AppleLocale`, then `LANG`).
pub(crate) fn system_country() -> Option<String> {
    if let Ok(out) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(region) = s.split(['_', '-']).nth(1) {
                let code = region.to_lowercase();
                if COUNTRIES.iter().any(|&(c, _, _)| c == code) {
                    return Some(code);
                }
            }
        }
    }
    if let Ok(lang) = std::env::var("LANG") {
        if let Some(region) = lang.split(['_', '.', '-']).nth(1) {
            let code = region.to_lowercase();
            if COUNTRIES.iter().any(|&(c, _, _)| c == code) {
                return Some(code);
            }
        }
    }
    None
}
