mod prime_photos;
mod about;
mod app_activation;
mod app_menu;
mod auth;
mod config;
mod countries;
mod dedup;
mod destination;
mod folders;
mod i18n;
mod icons;
mod login;
mod logging;
mod notify;
mod platform;
mod text;
mod tray;
mod upload;
mod urls;
mod backup;
mod single_instance;
mod rune_settings;
mod rune_theme;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_login;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_backup;
mod rune_backup_live;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_production_login;
#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
mod rune_wry_smoke;
mod rune_fixture;
mod wry_host;

use rune_fixture::RUNE_BACKUP_FLAG;
#[cfg(not(all(debug_assertions, feature = "rune-ui-spike")))]
use rune_fixture::is_rune_route_or_tooling_arg;

#[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
use rune_fixture::{
    RUNE_BACKUP_FIXTURE_FLAG, RUNE_BACKUP_SMOKE_FLAG, RUNE_LOGIN_FIXTURE_FLAG,
    RUNE_PRODUCTION_LOGIN_FLAG, RUNE_WRY_SMOKE_FLAG, is_malformed_rune_fixture_arg,
    rune_backup_fixture_args, rune_interactive_route_args, rune_login_fixture_args,
    rune_settings_fixture_args,
};
use rune_fixture::rune_production_backup_route_args;

fn main() {
    logging::init();

    // Rune tooling parses `args_os` + a UTF-8 filter instead of
    // `std::env::args()`: the latter panics on a non-UTF-8 argument, and a
    // launcher may hand us one. Such an argument cannot be one of our flags, so
    // dropping it is the same as "not ours".
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .filter_map(|arg| arg.into_string().ok())
        .collect();

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    if let Some(arg) = args.iter().find(|arg| is_malformed_rune_fixture_arg(arg)) {
        eprintln!(
            "Malformed Rune fixture/tooling argument: {arg}. Use --snap FILE, --tree FILE, or --script FILE."
        );
        std::process::exit(2);
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    if args.iter().any(|arg| arg == RUNE_LOGIN_FIXTURE_FLAG)
        && args.iter().any(|arg| arg == RUNE_BACKUP_FIXTURE_FLAG)
    {
        eprintln!("Rune fixture markers cannot be combined.");
        std::process::exit(2);
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    {
        let login_args = match rune_interactive_route_args(&args, RUNE_PRODUCTION_LOGIN_FLAG) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("Prime Photos Rune login route: {message}");
                std::process::exit(2);
            }
        };
        let smoke_args = match rune_interactive_route_args(&args, RUNE_WRY_SMOKE_FLAG) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("Prime Photos Wry smoke route: {message}");
                std::process::exit(2);
            }
        };
        let backup_args = match rune_interactive_route_args(&args, RUNE_BACKUP_FLAG) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("Prime Photos Rune backup route: {message}");
                std::process::exit(2);
            }
        };
        let backup_smoke_args = match rune_interactive_route_args(&args, RUNE_BACKUP_SMOKE_FLAG) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("Prime Photos Rune backup smoke route: {message}");
                std::process::exit(2);
            }
        };
        let selected_routes = [
            login_args.is_some(),
            smoke_args.is_some(),
            backup_args.is_some(),
            backup_smoke_args.is_some(),
        ]
        .into_iter()
        .filter(|selected| *selected)
        .count();
        if selected_routes > 1 {
            eprintln!("Rune interactive routes cannot be combined.");
            std::process::exit(2);
        }
        match (login_args, smoke_args, backup_args, backup_smoke_args) {
            (Some(route_args), None, None, None) => {
                if let Err(message) = rune_production_login::run(route_args) {
                    eprintln!("Prime Photos Rune login route: {message}");
                    std::process::exit(1);
                }
                return;
            }
            (None, Some(route_args), None, None) => {
                if let Err(message) = rune_wry_smoke::run(route_args) {
                    eprintln!("Prime Photos Wry smoke route: {message}");
                    std::process::exit(1);
                }
                return;
            }
            (None, None, Some(route_args), None) => {
                if let Err(message) = rune_backup_live::run_production(route_args) {
                    eprintln!("Prime Photos Rune backup route: {message}");
                    std::process::exit(1);
                }
                return;
            }
            (None, None, None, Some(route_args)) => {
                if let Err(message) = rune_backup_live::run_smoke(route_args) {
                    eprintln!("Prime Photos Rune backup smoke route: {message}");
                    std::process::exit(1);
                }
                return;
            }
            (None, None, None, None) => {}
            _ => unreachable!("interactive route count was validated"),
        }
    }

    // Fixtures, smoke routes, login routes and headless tooling are debug-only.
    // A valid `--rune-backup` production marker is still accepted below; every
    // other Rune route/tooling argument is a start-argument error (stderr,
    // exit 2) in a build without `rune-ui-spike`.
    #[cfg(not(all(debug_assertions, feature = "rune-ui-spike")))]
    if let Some(arg) = args.iter().find(|arg| is_rune_route_or_tooling_arg(arg)) {
        let is_production_marker = arg.as_str() == RUNE_BACKUP_FLAG
            || arg
                .strip_prefix(RUNE_BACKUP_FLAG)
                .is_some_and(|suffix| suffix.starts_with('='));
        if !is_production_marker {
            eprintln!(
                "Rune fixtures, smoke routes, login routes, and tooling require a debug build with `--features rune-ui-spike`: {arg}"
            );
            std::process::exit(2);
        }
    }

    // An explicit `--rune-backup` marker selects the production backup host and
    // is stripped here; bare launch falls through to the same route below.
    match rune_production_backup_route_args(&args) {
        Ok(Some(route_args)) => {
            if let Err(message) = rune_backup_live::run_production(route_args) {
                eprintln!("Prime Photos Rune backup route: {message}");
                std::process::exit(1);
            }
            return;
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("Prime Photos Rune backup route: {message}");
            std::process::exit(2);
        }
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    match rune_backup_fixture_args(args.clone()) {
        Ok(Some(rune_args)) => {
            if let Err(message) = rune_backup::run(rune_args) {
                eprintln!("Prime Photos Rune backup fixture: {message}");
                std::process::exit(1);
            }
            return;
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("Prime Photos Rune backup fixture: {message}");
            std::process::exit(2);
        }
    }

    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    match rune_login_fixture_args(args.clone()) {
        Ok(Some(rune_args)) => {
            if let Err(message) = rune_login::run(rune_args) {
                eprintln!("Prime Photos Rune login fixture: {message}");
                std::process::exit(1);
            }
            return;
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("Prime Photos Rune login fixture: {message}");
            std::process::exit(2);
        }
    }

    // This remaining Rune Settings fixture is headless-only; interactive login
    // and Wry verification routes are dispatched above.
    #[cfg(all(debug_assertions, feature = "rune-ui-spike"))]
    match rune_settings_fixture_args(args.clone()) {
        Ok(Some(rune_args)) => {
            if let Err(message) = rune_settings::run(rune_args) {
                eprintln!("Prime Photos Rune spike: {message}");
                std::process::exit(1);
            }
            return;
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("Prime Photos Rune spike: {message}");
            std::process::exit(2);
        }
    }

    // The normal launch is the Rune production backup route. An explicit
    // `--rune-backup` marker was already stripped by the route parser above; the
    // remaining arguments are the launcher/foreign arguments Rune ignores.
    if let Err(message) = rune_backup_live::run_production(args.clone()) {
        eprintln!("Prime Photos: {message}");
        std::process::exit(1);
    }
}
