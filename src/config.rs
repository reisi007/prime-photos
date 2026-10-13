use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

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
}

impl Config {
    pub fn get_config_path() -> io::Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No config directory found"))?;
        Ok(config_dir.join("amzn_photos_uploader").join("config.toml"))
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
        }
    }
}
