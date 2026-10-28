use std::path::Path;

use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PathStyle {
    Posix,
    Windows,
}

/// Convert an absolute local path into its normalized filesystem components.
/// The platform-independent `style` argument also lets tests exercise Windows
/// paths on POSIX hosts.
fn normalized_components(path: &str, style: PathStyle) -> Result<Vec<String>, String> {
    if path.is_empty() || path.contains('\0') {
        return Err("Der lokale Pfad ist leer oder ungültig.".to_string());
    }

    let components = match style {
        PathStyle::Posix => {
            if !path.starts_with('/') {
                return Err("Der lokale Pfad ist nicht absolut.".to_string());
            }
            normalize_tail(path, Vec::new(), 0)
        }
        PathStyle::Windows => normalize_windows_components(path)?,
    };
    validate_segments(&components)?;
    Ok(components)
}

fn normalize_windows_components(path: &str) -> Result<Vec<String>, String> {
    let path = path.replace('\\', "/");
    if path.starts_with("//./") || path == "//." {
        return Err("Windows-Gerätepfade werden nicht unterstützt.".to_string());
    }

    let path = if let Some(extended) = path.strip_prefix("//?/") {
        if extended
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("UNC/"))
        {
            return normalize_unc(&extended[4..]);
        }
        if let Some((volume, rest)) = extended.split_once('/')
            && volume.starts_with("Volume{")
            && volume.ends_with('}')
        {
            return Ok(normalize_tail(rest, vec![nfc(volume)], 1));
        }
        extended
    } else {
        path.as_str()
    };

    if path.starts_with("//") {
        if path.starts_with("///") {
            return Err("Der UNC-Pfad beginnt mit einem ungültigen Präfix.".to_string());
        }
        return normalize_unc(&path[2..]);
    }

    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
    {
        let drive = (bytes[0] as char).to_ascii_uppercase().to_string();
        return Ok(normalize_tail(&path[3..], vec![drive], 1));
    }

    Err("Der Windows-Pfad ist nicht absolut (Laufwerk oder UNC-Freigabe fehlt).".to_string())
}

fn normalize_unc(path: &str) -> Result<Vec<String>, String> {
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let server = parts
        .next()
        .ok_or_else(|| "Der UNC-Pfad enthält keinen Server.".to_string())?;
    let share = parts
        .next()
        .ok_or_else(|| "Der UNC-Pfad enthält keine Freigabe.".to_string())?;
    let prefix = vec!["UNC".to_string(), nfc(server), nfc(share)];
    Ok(normalize_tail(
        parts.collect::<Vec<_>>().join("/").as_str(),
        prefix,
        3,
    ))
}

fn normalize_tail(path: &str, mut components: Vec<String>, root_len: usize) -> Vec<String> {
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                // An absolute path cannot climb above its filesystem root.
                if components.len() > root_len {
                    components.pop();
                }
            }
            value => components.push(nfc(value)),
        }
    }
    components
}

fn nfc(value: &str) -> String {
    value.nfc().collect()
}

fn validate_segments(components: &[String]) -> Result<(), String> {
    if components.iter().any(|component| {
        component.is_empty()
            || component == "."
            || component == ".."
            || component.contains('/')
            || component.contains('\\')
            || component.chars().any(char::is_control)
    }) {
        return Err("Der lokale Pfad enthält ein ungültiges Zielordnersegment.".to_string());
    }
    Ok(())
}

fn native_style() -> PathStyle {
    if cfg!(windows) {
        PathStyle::Windows
    } else {
        PathStyle::Posix
    }
}

pub(crate) fn mirrored_path_components(
    path: &Path,
    machine_name: &str,
) -> Result<Vec<String>, String> {
    let path_text = path
        .to_str()
        .ok_or_else(|| "Der lokale Pfad enthält ungültige UTF-8-Zeichen.".to_string())?;
    mapped_components(path_text, machine_name, native_style())
}

fn mapped_components(
    path: &str,
    machine_name: &str,
    style: PathStyle,
) -> Result<Vec<String>, String> {
    let machine_name = nfc(machine_name.trim());
    if machine_name.is_empty()
        || machine_name == "."
        || machine_name == ".."
        || machine_name.contains('/')
        || machine_name.contains('\\')
        || machine_name.chars().any(char::is_control)
    {
        return Err("Der Maschinenname ist leer oder als Zielordner ungültig.".to_string());
    }

    let mut components = vec!["Backup".to_string(), machine_name];
    components.extend(normalized_components(path, style)?);
    validate_segments(&components)?;
    Ok(components)
}

/// Get a platform-appropriate machine name without falling back to a generic
/// shared destination. The result is used as a cloud-folder name.
pub(crate) fn machine_name() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    let candidate = command_name("scutil", &["--get", "ComputerName"])
        .or_else(|| command_name("hostname", &["-s"]))
        .or_else(|| std::env::var("HOSTNAME").ok());

    #[cfg(target_os = "windows")]
    let candidate = std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| command_name("hostname", &[]));

    #[cfg(target_os = "linux")]
    let candidate = std::fs::read_to_string("/etc/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| command_name("hostname", &["-s"]));

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let candidate = std::env::var("HOSTNAME").ok().or_else(|| command_name("hostname", &[]));

    let name = candidate
        .map(|name| nfc(name.trim()))
        .filter(|name| {
            !name.is_empty()
                && name != "."
                && name != ".."
                && !name.contains('/')
                && !name.contains('\\')
                && !name.chars().any(char::is_control)
        })
        .ok_or_else(|| "Der Maschinenname konnte nicht ermittelt werden.".to_string())?;
    Ok(name)
}

fn command_name(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::{PathStyle, mapped_components, normalized_components};

    #[test]
    fn posix_absolute_path_is_mapped_without_home_shortening() {
        let components = normalized_components("/Users/alex/Pictures/trip", PathStyle::Posix)
            .unwrap();
        assert_eq!(components, ["Users", "alex", "Pictures", "trip"]);
        assert_eq!(
            mapped_components(
                "/Users/alex/Pictures/trip",
                "Mac mini",
                PathStyle::Posix,
            )
            .unwrap(),
            ["Backup", "Mac mini", "Users", "alex", "Pictures", "trip"]
        );
    }

    #[test]
    fn posix_dot_components_are_normalized_and_root_is_valid() {
        assert_eq!(
            normalized_components("/Users/alex/./Pictures/../Photos", PathStyle::Posix)
                .unwrap(),
            ["Users", "alex", "Photos"]
        );
        assert!(normalized_components("/../../", PathStyle::Posix)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn windows_drive_paths_use_one_letter_segment() {
        assert_eq!(
            normalized_components("c:\\Users\\alex\\..\\Photos\\.", PathStyle::Windows)
                .unwrap(),
            ["C", "Users", "Photos"]
        );
        assert_eq!(
            normalized_components("D:\\", PathStyle::Windows).unwrap(),
            ["D"]
        );
    }

    #[test]
    fn windows_unc_paths_keep_server_and_share() {
        assert_eq!(
            normalized_components("\\\\nas\\photos\\2026\\..\\trip", PathStyle::Windows)
                .unwrap(),
            ["UNC", "nas", "photos", "trip"]
        );
        assert_eq!(
            normalized_components("\\\\?\\UNC\\nas\\photos\\trip", PathStyle::Windows)
                .unwrap(),
            ["UNC", "nas", "photos", "trip"]
        );
    }

    #[test]
    fn windows_unc_server_and_share_traversal_fails_closed() {
        for path in [
            "\\\\.\\photos",
            "\\\\..\\photos",
            "\\\\nas\\.",
            "\\\\nas\\..",
        ] {
            assert!(
                normalized_components(path, PathStyle::Windows).is_err(),
                "unsafe UNC path was accepted: {path}"
            );
        }
    }

    #[test]
    fn posix_backslash_and_control_characters_fail_closed() {
        assert!(normalized_components("/photos/bad\\name", PathStyle::Posix).is_err());
        assert!(normalized_components("/photos/bad\u{7f}name", PathStyle::Posix).is_err());
    }

    #[test]
    fn windows_volume_guid_paths_are_absolute() {
        assert_eq!(
            normalized_components(
                "\\\\?\\Volume{1234-ABCD}\\photos",
                PathStyle::Windows
            )
            .unwrap(),
            ["Volume{1234-ABCD}", "photos"]
        );
    }

    #[test]
    fn relative_and_incomplete_paths_fail_closed() {
        assert!(normalized_components("Users/alex", PathStyle::Posix).is_err());
        assert!(normalized_components("C:Users\\alex", PathStyle::Windows).is_err());
        assert!(normalized_components("\\\\nas", PathStyle::Windows).is_err());
        assert!(normalized_components("", PathStyle::Posix).is_err());
        assert!(normalized_components("//./C:/device", PathStyle::Windows).is_err());
    }

    #[test]
    fn machine_name_must_be_a_single_folder_component() {
        assert!(mapped_components("/Users/alex", "", PathStyle::Posix).is_err());
        assert!(mapped_components("/Users/alex", "bad/name", PathStyle::Posix).is_err());
    }
}
