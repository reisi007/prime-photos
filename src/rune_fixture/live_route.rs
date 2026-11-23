//! Routing for `--rune-backup-live-fixture`, kept apart from the older fixture
//! parsers so the root `rune_fixture.rs` stays below the size ceiling.

use super::{
    RUNE_BACKUP_LIVE_FIXTURE_FLAG, RUNE_ROUTE_FLAGS, filter_launcher_args, has_rune_headless_args,
    is_malformed_rune_fixture_arg, validate_rune_headless_options,
};

pub(crate) fn rune_backup_live_fixture_args(
    args: Vec<String>,
) -> Result<Option<Vec<String>>, &'static str> {
    if args.iter().any(|arg| is_malformed_rune_fixture_arg(arg)) {
        return Err("malformed Rune fixture/tooling option");
    }

    let marker_count = args
        .iter()
        .filter(|arg| arg.as_str() == RUNE_BACKUP_LIVE_FIXTURE_FLAG)
        .count();
    if marker_count == 0 {
        return Ok(None);
    }
    if marker_count != 1
        || args.iter().any(|arg| {
            arg != RUNE_BACKUP_LIVE_FIXTURE_FLAG
                && RUNE_ROUTE_FLAGS.iter().any(|flag| arg.starts_with(flag))
        })
    {
        return Err("Rune fixture markers cannot be combined");
    }

    let stripped_args = strip_rune_backup_live_fixture_arg(args);
    if !validate_rune_headless_options(&stripped_args)? || !has_rune_headless_args(&stripped_args) {
        return Err("--rune-backup-live-fixture requires --snap FILE, --tree FILE, or --script FILE");
    }

    let filtered_args = filter_launcher_args(stripped_args);
    if has_rune_headless_args(&filtered_args) {
        Ok(Some(filtered_args))
    } else {
        Err("launcher argument filtering removed the Rune headless action")
    }
}

fn strip_rune_backup_live_fixture_arg(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .filter(|arg| arg != RUNE_BACKUP_LIVE_FIXTURE_FLAG)
        .collect()
}
