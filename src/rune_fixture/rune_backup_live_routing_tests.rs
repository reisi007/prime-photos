use super::{
    RUNE_BACKUP_FLAG, RUNE_BACKUP_FIXTURE_FLAG, RUNE_BACKUP_LIVE_FIXTURE_FLAG,
    RUNE_LOGIN_FIXTURE_FLAG, is_rune_route_or_tooling_arg, rune_backup_live_fixture_args,
    rune_settings_fixture_args,
};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn live_fixture_routes_only_with_its_marker_and_headless_action() {
    for action in ["--snap", "--tree", "--script"] {
        let values = [
            RUNE_BACKUP_LIVE_FIXTURE_FLAG,
            action,
            "fixture-output",
            "--scene",
            "mixed",
            "--theme",
            "dark",
        ];
        assert_eq!(
            rune_backup_live_fixture_args(args(&values)),
            Ok(Some(args(&values[1..])))
        );
    }
    assert!(is_rune_route_or_tooling_arg(RUNE_BACKUP_LIVE_FIXTURE_FLAG));
}

#[test]
fn live_fixture_requires_a_headless_action_and_rejects_other_routes() {
    assert!(rune_backup_live_fixture_args(args(&[RUNE_BACKUP_LIVE_FIXTURE_FLAG])).is_err());
    assert!(rune_backup_live_fixture_args(args(&[
        RUNE_BACKUP_LIVE_FIXTURE_FLAG,
        "--scene",
        "mixed"
    ]))
    .is_err());
    assert!(rune_backup_live_fixture_args(args(&["--snap", "out.png"])).is_ok_and(|v| v.is_none()));
    for other in [
        RUNE_BACKUP_FLAG,
        RUNE_BACKUP_FIXTURE_FLAG,
        RUNE_LOGIN_FIXTURE_FLAG,
    ] {
        assert!(rune_backup_live_fixture_args(args(&[
            RUNE_BACKUP_LIVE_FIXTURE_FLAG,
            other,
            "--snap",
            "out.png",
        ]))
        .is_err());
    }
    assert!(rune_backup_live_fixture_args(args(&[
        RUNE_BACKUP_LIVE_FIXTURE_FLAG,
        "--snap=out.png",
    ]))
    .is_err());
    assert_eq!(
        rune_settings_fixture_args(args(&[RUNE_BACKUP_LIVE_FIXTURE_FLAG, "--snap", "out.png"])),
        Ok(None)
    );
}
