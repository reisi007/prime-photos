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
fn album_name_defaults_for_legacy_configs_and_roundtrips() {
    let legacy: Config = toml::from_str(LEGACY_TOML).unwrap();
    assert!(legacy.album_name.is_empty());

    let mut config = Config::default();
    config.album_name = "Camera uploads".to_owned();
    let serialized = toml::to_string_pretty(&config).unwrap();
    let back: Config = toml::from_str(&serialized).unwrap();
    assert_eq!(back.album_name, "Camera uploads");
}

#[test]
fn appearance_defaults_to_system_for_new_and_legacy_configs() {
    assert_eq!(Config::default().appearance, Appearance::System);
    let config: Config = toml::from_str(LEGACY_TOML).unwrap();
    assert_eq!(config.appearance, Appearance::System);
}

#[test]
fn appearance_serializes_stable_lowercase_values_and_roundtrips() {
    for (appearance, value) in [
        (Appearance::System, "system"),
        (Appearance::Light, "light"),
        (Appearance::Dark, "dark"),
    ] {
        let mut config = Config::default();
        config.appearance = appearance;
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized.contains(&format!("appearance = \"{value}\"")));
        let back: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(back.appearance, appearance);
    }
}

#[test]
fn config_without_device_serial_key_loads_as_empty() {
    // `#[serde(default)]` (empty string) so an existing `config.toml`
    // predating the device login flow still loads; empty = "not generated
    // yet" (the flow fills it on first use).
    let config: Config = toml::from_str(LEGACY_TOML).unwrap();
    assert!(config.device_serial.is_empty());
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

#[test]
fn config_without_worker_keys_loads_with_previous_defaults() {
    // A `config.toml` predating the persisted worker settings (user
    // decision 2026-10-16) loads with the values the app used to
    // hard-code: 3 workers, no rate limit, 100 KB/s.
    let config: Config = toml::from_str(LEGACY_TOML).unwrap();
    assert_eq!(config.concurrency(), 3);
    assert!(!config.limit_enabled);
    assert_eq!(config.limit_kbps, 100);
}

#[test]
fn config_concurrency_is_clamped_to_the_ui_range() {
    // A hand-written `config.toml` with an out-of-range value must not
    // start that many workers: the read site clamps to 1..=8.
    let toml = r#"
country = "de"
zone = "eu"
user_agent = "ua"
concurrency = 99
limit_enabled = true
limit_kbps = 500
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.concurrency(), 8);
    assert!(config.limit_enabled);
    assert_eq!(config.limit_kbps, 500);
}

#[test]
fn config_worker_settings_roundtrip() {
    let mut config = Config::default();
    config.concurrency = 5;
    config.limit_enabled = true;
    config.limit_kbps = 250;
    let serialized = toml::to_string_pretty(&config).unwrap();
    let back: Config = toml::from_str(&serialized).unwrap();
    assert_eq!(back.concurrency(), 5);
    assert!(back.limit_enabled);
    assert_eq!(back.limit_kbps, 250);
}

#[test]
fn config_without_backup_queue_ratio_loads_the_default() {
    let config: Config = toml::from_str(LEGACY_TOML).unwrap();
    assert_eq!(
        config.backup_queue_ratio(),
        crate::backup::BACKUP_QUEUE_RATIO_DEFAULT
    );
    assert_eq!(config.backup_queue_ratio, 0.35);
}

#[test]
fn backup_queue_ratio_clamps_out_of_range_and_non_finite_values() {
    for (value, expected) in [
        ("9.0", crate::backup::BACKUP_QUEUE_RATIO_MAX),
        ("-3.0", crate::backup::BACKUP_QUEUE_RATIO_MIN),
        ("nan", crate::backup::BACKUP_QUEUE_RATIO_DEFAULT),
        ("inf", crate::backup::BACKUP_QUEUE_RATIO_DEFAULT),
        ("-inf", crate::backup::BACKUP_QUEUE_RATIO_DEFAULT),
    ] {
        let config: Config =
            toml::from_str(&format!("{LEGACY_TOML}\nbackup_queue_ratio = {value}\n")).unwrap();
        assert_eq!(config.backup_queue_ratio(), expected, "value {value}");
    }
}

#[test]
fn backup_queue_ratio_roundtrips_as_a_numeric_toml_key() {
    let mut config = Config::default();
    config.backup_queue_ratio = 0.6;

    let serialized = toml::to_string_pretty(&config).unwrap();
    assert!(serialized
        .lines()
        .any(|line| line == "backup_queue_ratio = 0.6"));
    let back: Config = toml::from_str(&serialized).unwrap();
    assert_eq!(back.backup_queue_ratio(), 0.6);
}

#[test]
fn session_fields_are_never_serialized() {
    // The session material is memory-only (keychain / memory, user decision
    // 2026-10-15): a `Config` with cookies set must not write them to
    // `config.toml`.
    let mut config = Config::default();
    config.session_id = "SID".to_string();
    config.cookie_x_acb = "ACB".to_string();
    config.cookie_at_acb = "AT".to_string();
    config.cookie_ubid_acb = "UBID".to_string();
    config.cookie_x_amz_access_token = "TOKEN".to_string();
    let serialized = toml::to_string_pretty(&config).unwrap();
    // `#[serde(skip)]` drops both key and value, so the stable proof is the
    // absence of the **field names**. Asserting on short values ("AT", "SID")
    // was fragile — a future default could contain them by accident.
    assert!(!serialized.contains("session_id"));
    assert!(!serialized.contains("cookie_x_acb"));
    assert!(!serialized.contains("cookie_at_acb"));
    assert!(!serialized.contains("cookie_ubid_acb"));
    assert!(!serialized.contains("cookie_x_amz_access_token"));
    // The load round-trip must succeed and leave the fields empty.
    let back: Config = toml::from_str(&serialized).unwrap();
    assert!(back.session_id.is_empty());
    assert!(back.cookie_x_acb.is_empty());
    assert!(back.cookie_at_acb.is_empty());
    assert!(back.cookie_ubid_acb.is_empty());
    assert!(back.cookie_x_amz_access_token.is_empty());
}

#[test]
fn toml_with_legacy_session_keys_still_loads() {
    // A `config.toml` written by an older version still contains the
    // session fields; they are ignored on load (the fields stay empty).
    let toml = r#"
country = "de"
zone = "eu"
user_agent = "ua"
session-id = "OLD"
session_id = "OLD"
cookie_x_acb = "OLD"
cookie_at_acb = "OLD"
cookie_ubid_acb = "OLD"
cookie_x_amz_access_token = "OLD"
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.country, "de");
    assert!(config.session_id.is_empty());
    assert!(config.cookie_x_acb.is_empty());
    assert!(config.cookie_at_acb.is_empty());
    assert!(config.cookie_ubid_acb.is_empty());
    assert!(config.cookie_x_amz_access_token.is_empty());
}
