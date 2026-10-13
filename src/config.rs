use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

fn default_true() -> bool {
    true
}

fn default_background_sync() -> bool {
    true
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
        }
    }
}
