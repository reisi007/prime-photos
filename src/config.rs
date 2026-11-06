use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

use crate::folders::ConflictResolution;

/// The persisted appearance preference. `System` follows the native theme
/// reported by eframe/winit; explicit modes override it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    #[cfg(debug_assertions)]
    pub(crate) fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Appearance::System),
            "light" => Some(Appearance::Light),
            "dark" => Some(Appearance::Dark),
            _ => None,
        }
    }
}

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

/// Serde default of `Config::concurrency` (user decision 2026-10-16: the
/// worker count is persisted so it survives a restart). Matches the app's
/// previous hard-coded initial value.
fn default_concurrency() -> usize {
    3
}

/// Serde default of `Config::limit_kbps` (user decision 2026-10-16: persisted
/// like `concurrency`). Matches the app's previous hard-coded initial value.
fn default_limit_kbps() -> u32 {
    100
}

/// Serde default of `Config::backup_queue_ratio`: the queue takes 35 % of the
/// splittable height while both backup sections are open (Issue #33).
fn default_backup_queue_ratio() -> f32 {
    crate::backup::BACKUP_QUEUE_RATIO_DEFAULT
}

/// Serialize the split ratio as a short decimal. `toml` widens the `f32` to
/// `f64` (`0.35` becomes `0.3499999940395355`); four decimals keep it readable.
fn serialize_backup_queue_ratio<S>(ratio: &f32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let ratio = crate::backup::sanitize_backup_queue_ratio(*ratio);
    serializer.serialize_f64((f64::from(ratio) * 10_000.0).round() / 10_000.0)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Config {
    pub country: String,
    pub zone: String,
    pub user_agent: String,
    /// Only runtime state, never persisted (session material lives in the
    /// keychain / memory).
    #[serde(skip)]
    pub session_id: String,
    /// Only runtime state, never persisted (session material lives in the
    /// keychain / memory).
    #[serde(skip)]
    pub cookie_x_acb: String,
    /// Only runtime state, never persisted (session material lives in the
    /// keychain / memory).
    #[serde(skip)]
    pub cookie_at_acb: String,
    /// Only runtime state, never persisted (session material lives in the
    /// keychain / memory).
    #[serde(skip)]
    pub cookie_ubid_acb: String,
    /// Only runtime state, never persisted (session material lives in the
    /// keychain / memory).
    #[serde(skip)]
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
    /// Appearance override (`system`, `light` or `dark`). Old config files
    /// without this key follow the system appearance.
    #[serde(default)]
    pub appearance: Appearance,
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
    /// Concurrent upload workers (user decision 2026-10-16: persisted so the
    /// setting survives a restart — before, it lived only in memory and reset
    /// to the default on every start). The UI allows 1..=8; a value read from
    /// disk is clamped at the read site ([`Self::concurrency`]) so a
    /// hand-written `config.toml` cannot start an arbitrary number of workers.
    /// `#[serde(default = "default_concurrency")]` (3) so an existing
    /// `config.toml` without this key still loads.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// Per-file upload rate limit switch (user decision 2026-10-16: persisted
    /// like `concurrency`). `#[serde(default)]` (false) so an existing
    /// `config.toml` without this key still loads — unlimited.
    #[serde(default)]
    pub limit_enabled: bool,
    /// Per-file upload rate limit in KB/s (user decision 2026-10-16: persisted
    /// like `concurrency`). `#[serde(default = "default_limit_kbps")]` (100) so
    /// an existing `config.toml` without this key still loads.
    #[serde(default = "default_limit_kbps")]
    pub limit_kbps: u32,
    /// Device serial of the device login flow (user decision 2026-10-15): 32
    /// hex characters, generated **once** by [`crate::auth::new_device_serial`]
    /// and then stable. This is the one identity value that is ours — every
    /// other device-flow value is the original's. `#[serde(default)]` (empty)
    /// so an existing `config.toml` without this key still loads; the empty
    /// value means "not generated yet" and is filled in when the flow is wired.
    #[serde(default)]
    pub device_serial: String,
    /// Share of the splittable height taken by the queue while both backup
    /// sections are open (Issue #33, user wish 2026-11-03). Persisted so the
    /// splitter position survives a restart; `#[serde(default = ...)]` keeps
    /// existing `config.toml` files on the 35 % default. Read through
    /// [`Self::backup_queue_ratio`], which clamps hand-written values.
    #[serde(
        default = "default_backup_queue_ratio",
        serialize_with = "serialize_backup_queue_ratio"
    )]
    pub backup_queue_ratio: f32,
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

    /// The upload worker count actually used, clamped to the UI range 1..=8.
    /// A hand-written `config.toml` carrying an out-of-range value (e.g. 99)
    /// must not start that many workers; the clamp happens here, at the read
    /// site, so every upload path is covered.
    pub fn concurrency(&self) -> usize {
        self.concurrency.clamp(1, 8)
    }

    /// The queue's share of the splittable height, clamped to 0.20..=0.80; a
    /// non-finite hand-written value falls back to the default (Issue #33).
    pub fn backup_queue_ratio(&self) -> f32 {
        crate::backup::sanitize_backup_queue_ratio(self.backup_queue_ratio)
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
            appearance: Appearance::System,
            background_sync: true,
            paused: false,
            autostart_enabled: None,
            dedup_default: true,
            // Application default: a new target overwrites a taken name. The
            // enum's own default (`SaveAsNew`, the `folders.toml` fallback for
            // an entry without the key) is deliberately different — see
            // `default_replace` and the field comment.
            conflict_resolution_default: ConflictResolution::Replace,
            concurrency: 3,
            limit_enabled: false,
            limit_kbps: 100,
            // Empty = "not generated yet"; the device login flow fills it once.
            device_serial: String::new(),
            backup_queue_ratio: crate::backup::BACKUP_QUEUE_RATIO_DEFAULT,
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
