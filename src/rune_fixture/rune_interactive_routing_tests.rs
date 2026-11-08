use super::{
    RUNE_BACKUP_FLAG, RUNE_BACKUP_FIXTURE_FLAG, RUNE_BACKUP_SMOKE_FLAG,
    RUNE_LOGIN_FIXTURE_FLAG, RUNE_PRODUCTION_LOGIN_FLAG, RUNE_ROUTE_FLAGS, RUNE_WRY_SMOKE_FLAG,
    is_malformed_rune_fixture_arg, rune_interactive_route_args,
};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn interactive_routes_strip_only_their_explicit_marker() {
    for route_flag in [
        RUNE_PRODUCTION_LOGIN_FLAG,
        RUNE_WRY_SMOKE_FLAG,
        RUNE_BACKUP_FLAG,
        RUNE_BACKUP_SMOKE_FLAG,
    ] {
        assert_eq!(
            rune_interactive_route_args(&args(&[route_flag, "--theme", "light"]), route_flag),
            Ok(Some(args(&["--theme", "light"]))),
            "route should preserve unrelated args: {route_flag}"
        );
        assert_eq!(
            rune_interactive_route_args(&args(&[route_flag]), route_flag),
            Ok(Some(Vec::new())),
            "route should accept its marker alone: {route_flag}"
        );
    }
    assert_eq!(
        rune_interactive_route_args(&args(&["--theme", "light"]), RUNE_BACKUP_FLAG),
        Ok(None)
    );
}

#[test]
fn interactive_routes_reject_duplicate_markers_and_every_other_route() {
    for route_flag in [
        RUNE_PRODUCTION_LOGIN_FLAG,
        RUNE_WRY_SMOKE_FLAG,
        RUNE_BACKUP_FLAG,
        RUNE_BACKUP_SMOKE_FLAG,
    ] {
        assert!(
            rune_interactive_route_args(&args(&[route_flag, route_flag]), route_flag).is_err(),
            "duplicate marker should fail: {route_flag}"
        );
        for conflicting_flag in RUNE_ROUTE_FLAGS {
            if conflicting_flag == route_flag {
                continue;
            }
            assert!(
                rune_interactive_route_args(&args(&[route_flag, conflicting_flag]), route_flag)
                    .is_err(),
                "{route_flag} must reject {conflicting_flag}"
            );
        }
    }
}

#[test]
fn interactive_routes_reject_headless_and_malformed_tooling_args() {
    for route_flag in [
        RUNE_PRODUCTION_LOGIN_FLAG,
        RUNE_WRY_SMOKE_FLAG,
        RUNE_BACKUP_FLAG,
    ] {
        for action in ["--snap", "--tree", "--script"] {
            assert!(
                rune_interactive_route_args(&args(&[route_flag, action, "out.png"]), route_flag)
                    .is_err(),
                "{route_flag} must reject {action}"
            );
        }
        for tooling in ["--snap=out.png", "--tree-output", "--script=script.json", "--headless"] {
            assert!(
                rune_interactive_route_args(&args(&[route_flag, tooling]), route_flag).is_err(),
                "{route_flag} must reject {tooling}"
            );
        }
    }

    for action in ["--snap", "--tree", "--script"] {
        assert_eq!(
            rune_interactive_route_args(
                &args(&[RUNE_BACKUP_SMOKE_FLAG, action, "synthetic-output", "--scene", "empty"]),
                RUNE_BACKUP_SMOKE_FLAG,
            ),
            Ok(Some(args(&[action, "synthetic-output", "--scene", "empty"]))),
        );
    }
    for malformed in [
        args(&[RUNE_BACKUP_SMOKE_FLAG, "--snap=out.png"]),
        args(&[RUNE_BACKUP_SMOKE_FLAG, "--snap"]),
        args(&[RUNE_BACKUP_SMOKE_FLAG, "--snap", "out.png", "--tree", "tree.json"]),
        args(&[RUNE_BACKUP_SMOKE_FLAG, "--headless"]),
    ] {
        assert!(rune_interactive_route_args(&malformed, RUNE_BACKUP_SMOKE_FLAG).is_err());
    }
}

#[test]
fn valid_route_flags_are_not_malformed_and_route_prefixes_are_rejected() {
    for flag in [
        RUNE_BACKUP_FIXTURE_FLAG,
        RUNE_LOGIN_FIXTURE_FLAG,
        RUNE_PRODUCTION_LOGIN_FLAG,
        RUNE_WRY_SMOKE_FLAG,
        RUNE_BACKUP_FLAG,
        RUNE_BACKUP_SMOKE_FLAG,
    ] {
        assert!(!is_malformed_rune_fixture_arg(flag), "{flag} should be valid");
    }
    assert!(!is_malformed_rune_fixture_arg(RUNE_ROUTE_FLAGS[0]));

    for arg in [
        "--rune-login-typo",
        "--rune-login-fixture-typo",
        "--rune-backup-fixture-typo",
        "--rune-wry-smoke-typo",
        "--rune-backup=unexpected",
        "--rune-backup-smoke-extra",
        "--rune-login=unexpected",
    ] {
        assert!(is_malformed_rune_fixture_arg(arg), "{arg} should be rejected");
    }
}
