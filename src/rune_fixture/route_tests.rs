use super::{
    RUNE_BACKUP_FIXTURE_FLAG, RUNE_LOGIN_FIXTURE_FLAG, RUNE_PRODUCTION_LOGIN_FLAG,
    RUNE_WRY_SMOKE_FLAG, RUNE_BACKUP_FLAG, RUNE_BACKUP_SMOKE_FLAG,
    is_rune_route_or_tooling_arg,
};
#[cfg(feature = "rune-backup")]
use super::rune_production_backup_route_args;

#[test]
fn every_explicit_rune_route_is_recognized_for_unavailable_route_rejection() {
    for flag in [
        RUNE_BACKUP_FIXTURE_FLAG,
        RUNE_LOGIN_FIXTURE_FLAG,
        RUNE_PRODUCTION_LOGIN_FLAG,
        RUNE_WRY_SMOKE_FLAG,
        RUNE_BACKUP_FLAG,
        RUNE_BACKUP_SMOKE_FLAG,
    ] {
        assert!(is_rune_route_or_tooling_arg(flag));
    }
}

#[cfg(feature = "rune-backup")]
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[cfg(feature = "rune-backup")]
#[test]
fn production_backup_route_requires_its_explicit_marker_and_strips_only_it() {
    assert_eq!(rune_production_backup_route_args(&args(&["--verbose"])), Ok(None));
    assert_eq!(
        rune_production_backup_route_args(&args(&["--rune-backup-smoke"])),
        Ok(None)
    );
    assert_eq!(
        rune_production_backup_route_args(&args(&[RUNE_BACKUP_FLAG, "--verbose", "value"])),
        Ok(Some(args(&["--verbose", "value"])))
    );
}

#[cfg(feature = "rune-backup")]
#[test]
fn production_backup_route_rejects_malformed_or_ambiguous_arguments() {
    assert_eq!(
        rune_production_backup_route_args(&args(&["--rune-backup=anything"])),
        Err("malformed --rune-backup route flag")
    );
    assert!(rune_production_backup_route_args(&args(&[
        RUNE_BACKUP_FLAG,
        RUNE_BACKUP_FLAG,
    ]))
    .is_err());
    for values in [
        &[RUNE_BACKUP_FLAG, "--rune-backup-smoke"][..],
        &[RUNE_BACKUP_FLAG, "--rune-login-fixture"][..],
        &[RUNE_BACKUP_FLAG, "--snap", "capture.png"][..],
        &[RUNE_BACKUP_FLAG, "--headless"][..],
    ] {
        assert!(
            rune_production_backup_route_args(&args(values)).is_err(),
            "must reject {values:?}"
        );
    }
}
