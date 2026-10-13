// ---------------------------------------------------------------------------
// Cookie parsing (manual fallback)
// ---------------------------------------------------------------------------

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

pub(crate) const COUNTRIES: &[(&str, &str)] = &[
    ("de", "Deutschland · amazon.de"),
    ("at", "Österreich · amazon.de"),
    ("ch", "Schweiz · amazon.de"),
    ("us", "USA · amazon.com"),
    ("uk", "Vereinigtes Königreich · amazon.co.uk"),
    ("fr", "Frankreich · amazon.fr"),
    ("it", "Italien · amazon.it"),
    ("es", "Spanien · amazon.es"),
    ("ca", "Kanada · amazon.ca"),
    ("jp", "Japan · amazon.co.jp"),
    ("in", "Indien · amazon.in"),
    ("au", "Australien · amazon.com.au"),
    ("br", "Brasilien · amazon.com.br"),
    ("mx", "Mexiko · amazon.com.mx"),
];

pub(crate) fn country_label(code: &str) -> String {
    COUNTRIES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, l)| l.to_string())
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
                if COUNTRIES.iter().any(|(c, _)| *c == code) {
                    return Some(code);
                }
            }
        }
    }
    if let Ok(lang) = std::env::var("LANG") {
        if let Some(region) = lang.split(['_', '.', '-']).nth(1) {
            let code = region.to_lowercase();
            if COUNTRIES.iter().any(|(c, _)| *c == code) {
                return Some(code);
            }
        }
    }
    None
}
