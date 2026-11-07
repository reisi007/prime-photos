pub(super) const RUNE_LOGIN_FIXTURE_FLAG: &str = "--rune-login-fixture";
pub(super) const RUNE_BACKUP_FIXTURE_FLAG: &str = "--rune-backup-fixture";
const RUNE_HEADLESS_FLAGS: [&str; 3] = ["--snap", "--tree", "--script"];

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn has_rune_headless_args(args: &[String]) -> bool {
    let args = rune::tooling::Args::parse_from(args.to_vec());
    args.headless() && (args.snap.is_some() || args.tree.is_some() || args.script.is_some())
}

fn rune_headless_flag(arg: &str) -> Option<&'static str> {
    RUNE_HEADLESS_FLAGS
        .iter()
        .copied()
        .find(|flag| arg.starts_with(flag))
}

pub(super) fn is_rune_fixture_or_tooling_arg(arg: &str) -> bool {
    arg.starts_with(RUNE_LOGIN_FIXTURE_FLAG)
        || arg.starts_with(RUNE_BACKUP_FIXTURE_FLAG)
        || rune_headless_flag(arg).is_some()
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
pub(super) fn is_malformed_rune_fixture_arg(arg: &str) -> bool {
    is_rune_fixture_or_tooling_arg(arg)
        && ((arg.starts_with(RUNE_LOGIN_FIXTURE_FLAG) && arg != RUNE_LOGIN_FIXTURE_FLAG)
            || (arg.starts_with(RUNE_BACKUP_FIXTURE_FLAG) && arg != RUNE_BACKUP_FIXTURE_FLAG)
            || rune_headless_flag(arg).is_some_and(|flag| arg != flag))
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn validate_rune_headless_options(args: &[String]) -> Result<bool, &'static str> {
    let mut found_option = false;
    let mut i = 0;

    while i < args.len() {
        if let Some(flag) = rune_headless_flag(&args[i]) {
            if args[i] != flag {
                return Err("malformed Rune tooling option");
            }

            found_option = true;
            let Some(value) = args.get(i + 1) else {
                return Err("Rune tooling option requires a file path");
            };
            if value.is_empty() || value.starts_with("--") {
                return Err("Rune tooling option requires a file path");
            }
            i += 1;
        }
        i += 1;
    }

    Ok(found_option)
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
pub(super) fn rune_settings_fixture_args(
    args: Vec<String>,
) -> Result<Option<Vec<String>>, &'static str> {
    if args.iter().any(|arg| arg == RUNE_BACKUP_FIXTURE_FLAG) {
        return Ok(None);
    }
    if args.iter().any(|arg| is_malformed_rune_fixture_arg(arg)) {
        return Err("malformed Rune fixture/tooling option");
    }

    if !validate_rune_headless_options(&args)? {
        return Ok(None);
    }
    if !has_rune_headless_args(&args) {
        return Err("malformed Rune headless action");
    }

    let filtered_args = filter_launcher_args(args);
    if has_rune_headless_args(&filtered_args) {
        Ok(Some(filtered_args))
    } else {
        Err("launcher argument filtering removed the Rune headless action")
    }
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
pub(super) fn rune_backup_fixture_args(
    args: Vec<String>,
) -> Result<Option<Vec<String>>, &'static str> {
    if args.iter().any(|arg| is_malformed_rune_fixture_arg(arg)) {
        return Err("malformed Rune fixture/tooling option");
    }

    let marker_count = args
        .iter()
        .filter(|arg| arg.as_str() == RUNE_BACKUP_FIXTURE_FLAG)
        .count();
    if marker_count == 0 {
        return Ok(None);
    }
    if marker_count != 1 || args.iter().any(|arg| arg == RUNE_LOGIN_FIXTURE_FLAG) {
        return Err("Rune fixture markers cannot be combined");
    }

    let stripped_args = strip_rune_backup_fixture_arg(args);
    if !validate_rune_headless_options(&stripped_args)? || !has_rune_headless_args(&stripped_args) {
        return Err("--rune-backup-fixture requires --snap FILE, --tree FILE, or --script FILE");
    }

    let filtered_args = filter_launcher_args(stripped_args);
    if has_rune_headless_args(&filtered_args) {
        Ok(Some(filtered_args))
    } else {
        Err("launcher argument filtering removed the Rune headless action")
    }
}

/// macOS launchers may inject process-serialization arguments.
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn filter_launcher_args(args: Vec<String>) -> Vec<String> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg.starts_with("-psn_") {
            continue;
        }
        if arg == "-NSDocumentRevisionsDebugMode" {
            let _ = args.next();
            continue;
        }
        filtered.push(arg);
    }
    filtered
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
pub(super) fn strip_rune_login_fixture_arg(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .filter(|arg| arg != RUNE_LOGIN_FIXTURE_FLAG)
        .collect()
}

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
fn strip_rune_backup_fixture_arg(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .filter(|arg| arg != RUNE_BACKUP_FIXTURE_FLAG)
        .collect()
}

#[cfg(all(test, debug_assertions, feature = "rune-ui-spike"))]
mod rune_login_routing_tests {
    use super::{RUNE_LOGIN_FIXTURE_FLAG, strip_rune_login_fixture_arg};

    #[test]
    fn fixture_routing_strips_only_its_own_marker() {
        let args = vec![
            RUNE_LOGIN_FIXTURE_FLAG.to_owned(),
            "--snap".to_owned(),
            "login.png".to_owned(),
            "--scene".to_owned(),
            "error".to_owned(),
        ];

        assert_eq!(
            strip_rune_login_fixture_arg(args),
            ["--snap", "login.png", "--scene", "error"]
        );
    }
}

#[cfg(all(test, debug_assertions, feature = "rune-ui-spike"))]
mod rune_backup_routing_tests {
    use super::{
        RUNE_BACKUP_FIXTURE_FLAG, RUNE_LOGIN_FIXTURE_FLAG, is_malformed_rune_fixture_arg,
        is_rune_fixture_or_tooling_arg, rune_backup_fixture_args, rune_settings_fixture_args,
        strip_rune_backup_fixture_arg,
    };

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn backup_fixture_routes_only_with_its_marker_and_headless_action() {
        for action in ["--snap", "--tree", "--script"] {
            let values = [RUNE_BACKUP_FIXTURE_FLAG, action, "fixture-output", "--scene", "empty"];
            assert_eq!(
                rune_backup_fixture_args(args(&values)),
                Ok(Some(args(&values[1..])))
            );
        }
        assert_eq!(
            rune_backup_fixture_args(args(&[RUNE_BACKUP_FIXTURE_FLAG, "--snap", "out.png"])),
            Ok(Some(args(&["--snap", "out.png"])))
        );
    }

    #[test]
    fn backup_marker_without_a_headless_action_is_rejected() {
        assert!(rune_backup_fixture_args(args(&[RUNE_BACKUP_FIXTURE_FLAG])).is_err());
        assert!(rune_backup_fixture_args(args(&[
            RUNE_BACKUP_FIXTURE_FLAG,
            "--scene",
            "empty",
        ]))
        .is_err());
    }

    #[test]
    fn malformed_and_ambiguous_fixture_arguments_fail_closed() {
        let malformed = args(&["--rune-backup-fixture=anything", "--snap", "out.png"]);
        assert!(is_malformed_rune_fixture_arg(&malformed[0]));
        assert!(rune_backup_fixture_args(malformed).is_err());
        assert!(rune_backup_fixture_args(args(&[
            RUNE_BACKUP_FIXTURE_FLAG,
            RUNE_LOGIN_FIXTURE_FLAG,
            "--snap",
            "out.png",
        ]))
        .is_err());
        assert!(rune_backup_fixture_args(args(&[
            RUNE_BACKUP_FIXTURE_FLAG,
            RUNE_BACKUP_FIXTURE_FLAG,
            "--snap",
            "out.png",
        ]))
        .is_err());
    }

    #[test]
    fn malformed_rune_headless_options_and_filtered_loss_fail_closed() {
        assert!(rune_backup_fixture_args(args(&[
            RUNE_BACKUP_FIXTURE_FLAG,
            "--snap=out.png",
        ]))
        .is_err());
        assert_eq!(
            rune_backup_fixture_args(args(&[
                RUNE_BACKUP_FIXTURE_FLAG,
                "-NSDocumentRevisionsDebugMode",
                "--snap",
                "out.png",
            ])),
            Err("launcher argument filtering removed the Rune headless action")
        );
    }

    #[test]
    fn unrelated_arguments_and_settings_routing_are_not_consumed_by_backup() {
        assert_eq!(rune_backup_fixture_args(args(&["--verbose"])), Ok(None));
        assert_eq!(
            rune_backup_fixture_args(args(&["--snap", "out.png"])),
            Ok(None)
        );
        assert_eq!(
            rune_settings_fixture_args(args(&[RUNE_BACKUP_FIXTURE_FLAG, "--snap", "out.png"])),
            Ok(None)
        );
        assert!(is_rune_fixture_or_tooling_arg(RUNE_BACKUP_FIXTURE_FLAG));
    }

    #[test]
    fn backup_marker_stripping_removes_only_its_exact_argument() {
        assert_eq!(
            strip_rune_backup_fixture_arg(args(&[
                RUNE_BACKUP_FIXTURE_FLAG,
                "--snap",
                "out.png",
                "--scene",
                "failure",
            ])),
            args(&["--snap", "out.png", "--scene", "failure"])
        );
    }
}

#[cfg(all(test, debug_assertions, feature = "rune-ui-spike"))]
mod rune_settings_routing_tests {
    use super::{has_rune_headless_args, is_malformed_rune_fixture_arg, rune_settings_fixture_args};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn settings_routes_for_each_parsed_headless_action() {
        for action in ["--snap", "--tree", "--script"] {
            assert!(
                has_rune_headless_args(&args(&[action, "fixture-output"])),
                "{action} should route to the Settings fixture"
            );
        }
    }

    #[test]
    fn settings_does_not_route_for_unparsed_or_non_headless_actions() {
        for values in [&[][..], &["--scene", "login"][..], &["--native"][..]] {
            assert!(
                !has_rune_headless_args(&args(values)),
                "{values:?} should not route to the Settings fixture"
            );
            assert_eq!(
                rune_settings_fixture_args(args(values)),
                Ok(None),
                "{values:?} should remain on the ordinary app path"
            );
        }

        assert_eq!(
            rune_settings_fixture_args(args(&["--native", "--script", "fixture-output"])),
            Err("malformed Rune headless action")
        );
        assert_eq!(
            rune_settings_fixture_args(args(&["--snap"])),
            Err("Rune tooling option requires a file path")
        );
    }

    #[test]
    fn filtered_launcher_args_must_retain_the_headless_action() {
        let injected_args = args(&["-NSDocumentRevisionsDebugMode", "--snap", "out.png"]);
        assert!(has_rune_headless_args(&injected_args));
        assert_eq!(
            rune_settings_fixture_args(injected_args),
            Err("launcher argument filtering removed the Rune headless action")
        );

        assert_eq!(
            rune_settings_fixture_args(args(&["--snap", "out.png"])),
            Ok(Some(args(&["--snap", "out.png"])))
        );
        assert_eq!(
            rune_settings_fixture_args(args(&["-psn_0_123", "--snap", "out.png"])),
            Ok(Some(args(&["--snap", "out.png"])))
        );
    }

    #[test]
    fn malformed_rune_fixture_options_are_rejected_instead_of_falling_through() {
        assert_eq!(
            rune_settings_fixture_args(args(&["--snap=fixture-output"])),
            Err("malformed Rune fixture/tooling option")
        );
        assert_eq!(
            rune_settings_fixture_args(args(&["--rune-login-fixture=login"])),
            Err("malformed Rune fixture/tooling option")
        );
        assert!(is_malformed_rune_fixture_arg("--rune-login-fixture=login"));
    }

    #[test]
    fn unrelated_arguments_do_not_route_to_the_settings_fixture() {
        assert_eq!(
            rune_settings_fixture_args(args(&["--foreign-launcher-arg", "value"])),
            Ok(None)
        );
    }
}
