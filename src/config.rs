use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

use crate::folders::ConflictResolution;

fn default_true() -> bool {
    true
}

fn default_background_sync() -> bool {
    true
}

/// Serde default of `Config::conflict_resolution_default`.
///
/// The **application** default — the preselection of a new backup target and
/// the policy of the one-time (drag & drop) queue — is `Replace`
/// (user decision 2026-10-15).
///
/// Deliberately **not** `ConflictResolution::default()`: that enum default
/// stays `SaveAsNew` because it is the **file** fallback for a `folders.toml`
/// entry without the key. The two roles are not unified on purpose — a new
/// target overwrites a taken name, an existing target keeps whatever it stored.
fn default_replace() -> ConflictResolution {
    ConflictResolution::Replace
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Config {
    pub country: String,
    pub zone: String,
    pub user_agent: String,
    pub session_id: String,
    pub cookie_x_acb: String,
    pub cookie_at_acb: String,
    pub cookie_ubid_acb: String,
    pub cookie_x_amz_access_token: String,
    /// Desktop notifications when an upload run finishes. Defaults to `true`
    /// for existing `config.toml` files that predate this field.
    #[serde(default = "default_true")]
    pub notifications: bool,
    /// UI language override (`"de"`/`"en"`); `None` = follow the system locale.
    /// `#[serde(default)]` so an existing `config.toml` without this key still
    /// loads (and a `None` is omitted again on save).
    #[serde(default)]
    pub language: Option<String>,
    /// Periodic background sync (PORT-5): while the app runs, the enabled
    /// backup folders are re-scanned and newly found files are uploaded.
    /// `#[serde(default = "default_background_sync")]` (true) so an existing
    /// `config.toml` without this key still loads — with the feature active.
    #[serde(default = "default_background_sync")]
    pub background_sync: bool,
    /// Global pause state (user decision 2026-10-14): while paused, no upload
    /// run starts — neither the background sync nor a queued one. The toggle
    /// also works in the idle state, and the value is persisted, so the pause
    /// survives a restart (like the original). `#[serde(default)]` (false) so an
    /// existing `config.toml` without this key still loads — unpaused.
    #[serde(default)]
    pub paused: bool,
    /// Persisted **intent** for the auto-launch (login item), user decision
    /// 2026-10-14: `None` = never decided / platform unsupported, `Some(true)`
    /// = it should be on, `Some(false)` = the user deliberately turned it off.
    /// The OS state stays the operational truth; this field only records what
    /// the app should enforce on the next start. `#[serde(default)]` (`None`)
    /// so an existing `config.toml` without this key still loads.
    #[serde(default)]
    pub autostart_enabled: Option<bool>,
    /// Global dedupe default (user decision 2026-10-15): the preselection when
    /// a new backup target is created and the policy of the one-time
    /// (drag & drop) queue. An existing target keeps its stored `dedup` value.
    /// `#[serde(default = "default_true")]` (true) so an existing `config.toml`
    /// without this key still loads — with dedupe on.
    #[serde(default = "default_true")]
    pub dedup_default: bool,
    /// Application default for the name-conflict policy: preselected when a new
    /// backup target is created and applied by the one-time (drag & drop) queue.
    /// `#[serde(default = "default_replace")]` (`Replace`, user decision
    /// 2026-10-15) so an existing `config.toml` without this key still loads.
    ///
    /// Two defaults, deliberately **not** unified: the **file** fallback for a
    /// `folders.toml` entry without the key is `ConflictResolution`'s own
    /// `#[default]` (`SaveAsNew`), so an existing target is never silently
    /// switched to overwriting.
    #[serde(default = "default_replace")]
    pub conflict_resolution_default: ConflictResolution,
}

impl Config {
    pub fn get_config_path() -> io::Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No config directory found"))?;
        Ok(config_dir.join("prime_photos").join("config.toml"))
    }

    /// Load the configuration from the file system.
    pub fn load() -> io::Result<Self> {
        let path = Self::get_config_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: Config = match toml::from_str(&content) {
                Ok(c) => c,
                Err(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "Couldn't deserialize the configuration",
                    ));
                }
            };
            Ok(config)
        } else {
            // If no config file exists, return a default configuration
            Ok(Self::default())
        }
    }

    /// Save the configuration to the file system.
    #[allow(dead_code)]
    pub fn save(&self) -> Result<(), io::Error> {
        let path = Self::get_config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?; // Ensure the directory exists
        }
        let content = match toml::to_string_pretty(self) {
            Ok(c) => c,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Couldn't serialize the configuration",
                ));
            }
        };
        fs::write(path, content)?;
        Ok(())
    }

    /// Default configuration values.
    pub fn default() -> Self {
        Self {
            country: "us".to_string(),
            zone: "eu".to_string(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.2 Safari/605.1.15".to_string(),
            session_id: "".to_string(),
            cookie_x_acb: "".to_string(),
            cookie_at_acb: "".to_string(),
            cookie_ubid_acb: "".to_string(),
            cookie_x_amz_access_token: "".to_string(),
            notifications: true,
            language: None,
            background_sync: true,
            paused: false,
            autostart_enabled: None,
            dedup_default: true,
            // Application default: a new target overwrites a taken name. The
            // enum's own default (`SaveAsNew`, the `folders.toml` fallback for
            // an entry without the key) is deliberately different — see
            // `default_replace` and the field comment.
            conflict_resolution_default: ConflictResolution::Replace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal TOML that satisfies every non-defaulted field of `Config`.
    /// Deliberately has **no** `autostart_enabled`, **no** `dedup_default` and
    /// **no** `conflict_resolution_default` key.
    const LEGACY_TOML: &str = r#"
country = "de"
zone = "eu"
user_agent = "ua"
session_id = ""
cookie_x_acb = ""
cookie_at_acb = ""
cookie_ubid_acb = ""
cookie_x_amz_access_token = ""
"#;

    #[test]
    fn config_without_autostart_key_loads_as_none() {
        let config: Config = toml::from_str(LEGACY_TOML).unwrap();
        assert_eq!(config.autostart_enabled, None);
    }

    #[test]
    fn config_autostart_intent_roundtrips() {
        for intent in [Some(true), Some(false)] {
            let mut config = Config::default();
            config.autostart_enabled = intent;
            let serialized = toml::to_string_pretty(&config).unwrap();
            let back: Config = toml::from_str(&serialized).unwrap();
            assert_eq!(back.autostart_enabled, intent);
        }
    }

    #[test]
    fn config_without_conflict_default_key_loads_as_replace() {
        // `#[serde(default = "default_replace")]` reads the **application**
        // default (`Replace`, user decision 2026-10-15), **not** the enum's own
        // `#[default]` (`SaveAsNew`, which stays the file fallback for a
        // `folders.toml` entry without the key) — an existing `config.toml`
        // predating the field must land on the application default.
        let config: Config = toml::from_str(LEGACY_TOML).unwrap();
        assert_eq!(
            config.conflict_resolution_default,
            ConflictResolution::Replace
        );
    }

    #[test]
    fn config_defaults_to_replace_and_dedupe_on() {
        let config = Config::default();
        assert_eq!(
            config.conflict_resolution_default,
            ConflictResolution::Replace
        );
        assert!(config.dedup_default);
    }

    #[test]
    fn config_without_both_default_keys_loads_as_replace_and_dedup_on() {
        // One legacy `config.toml` that predates both fields: the serde
        // defaults apply, so nothing about an existing install changes.
        let config: Config = toml::from_str(LEGACY_TOML).unwrap();
        assert_eq!(
            config.conflict_resolution_default,
            ConflictResolution::Replace
        );
        assert!(config.dedup_default);
    }

    #[test]
    fn config_dedup_default_roundtrips() {
        let mut config = Config::default();
        config.dedup_default = false;
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized.contains("dedup_default = false"));
        let back: Config = toml::from_str(&serialized).unwrap();
        assert!(!back.dedup_default);
    }

    #[test]
    fn config_conflict_default_roundtrips_with_a_non_default_value() {
        let mut config = Config::default();
        config.conflict_resolution_default = ConflictResolution::Replace;
        let serialized = toml::to_string_pretty(&config).unwrap();
        // `ConflictResolution` is `rename_all = "lowercase"` (same rule as in
        // `folders.toml`), so the wire value is the lowercase variant name.
        assert!(serialized.contains(r#"conflict_resolution_default = "replace""#));
        let back: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(
            back.conflict_resolution_default,
            ConflictResolution::Replace
        );
    }
}
