//! Developer-only demo data source and deterministic screenshot harness
//! (issue #29).
//!
//! A real UI review needs a signed-in Amazon session and real photo folders —
//! neither is available on a build machine, and neither may be touched by an
//! automated check. This module therefore supplies **fixtures**: a fixed
//! `Config`, a `FolderStore`, an `UploadShared` queue and a few UI flags, all
//! shaped exactly like the data the real app holds. The app renders them
//! through its **existing** paths (`PrimePhotoApp::update`, `backup_ui`, …) —
//! there is deliberately no second rendering path, so what a screenshot shows
//! is what a real frame shows.
//!
//! Everything here is read-only by construction (see `docs/dev/UI_FIXTURES.md`):
//! no `config.toml`, no keychain, no SQLite, no network, no notifications and no
//! single-instance handshake. The app's own guards for those side effects sit in
//! `app.rs` / `backup.rs` / `login.rs` and are all keyed on
//! [`crate::app::PrimePhotoApp::is_demo`].
//!
//! The only IO this module performs is the screenshot file: `--screenshot
//! <path>` lets the window system capture one frame, writes it as a PNG and
//! closes the app. The capture goes through `egui`'s normal screenshot event,
//! i.e. through the same renderer that paints the screen. (The app's normal log
//! file is still opened by `logging::init`, exactly as in the non-demo start.)

mod fixture;
mod live_scenarios;
mod runtime;
mod scenario;
mod settled_scenarios;

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod tests;

pub(crate) use fixture::fixture;
pub(crate) use runtime::DemoRuntime;
pub(crate) use scenario::Scenario;

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

/// What the process was asked to do. `Off` is the normal app: nothing about it
/// changes, not even the start path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Off,
    /// `--demo-list`: print the scenarios and exit, without starting the app.
    List,
    /// Interactive demo window: fixtures, no session, no side effects.
    Demo(Scenario),
    /// Fixtures, one captured frame, then exit. The path must be absolute so
    /// the process can write it regardless of its working directory.
    Screenshot {
        scenario: Scenario,
        path: PathBuf,
    },
}

impl Mode {
    /// Whether this run uses the demo fixtures at all. `List` does not: it only
    /// prints the scenario table and exits.
    pub(crate) fn is_demo(&self) -> bool {
        matches!(self, Mode::Demo(_) | Mode::Screenshot { .. })
    }

    /// Split the mode into what the app constructor needs: the scenario and an
    /// optional screenshot target. `None` means "do not start the app".
    pub(crate) fn into_demo(self) -> Option<(Scenario, Option<PathBuf>)> {
        match self {
            Mode::Off | Mode::List => None,
            Mode::Demo(scenario) => Some((scenario, None)),
            Mode::Screenshot { scenario, path } => Some((scenario, Some(path))),
        }
    }
}

/// The usage block printed with every argument error.
pub(crate) fn usage() -> &'static str {
    "Verwendung:
  prime-photos                          Normale Anwendung starten
  prime-photos --demo                   Demo mit dem Szenario \"mixed\" starten
  prime-photos --demo=<id>              Demo mit einem bestimmten Szenario starten
  prime-photos --demo-list              Alle Szenarien mit ID und Beschreibung ausgeben
  prime-photos --screenshot <Pfad>      Demo-Frame als PNG nach <Pfad> schreiben, dann beenden
  prime-photos --screenshot=<Pfad> --demo=<id>
                                        Screenshot eines bestimmten Szenarios

Der Screenshot-Pfad muss absolut sein. Die Demo liest und schreibt weder
config.toml noch Keychain, SQLite oder Netzwerk.

Ohne Demo-Argument wird die normale Anwendung gestartet; Argumente, die nicht
zu Prime Photos gehören (z. B. von macOS injizierte), werden dabei ignoriert."
}

/// Turn the process arguments (without `argv[0]`) into a [`Mode`].
///
/// Pure: no filesystem, no environment, no egui — so every accepted and
/// rejected form is unit-testable.
pub(crate) fn parse_args(args: &[String]) -> Result<Mode, String> {
    let mut scenario: Option<Scenario> = None;
    let mut screenshot: Option<PathBuf> = None;
    let mut demo_requested = false;
    let mut list_requested = false;
    // Tokens that are not ours at all. Launchers inject such arguments (macOS
    // sends `-psn_0_…` on a fresh bundle start, Xcode-style
    // `-NSDocumentRevisionsDebugMode`); they must never abort a normal start,
    // so they are only an error once a demo run was requested.
    let mut foreign: Vec<&str> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if let Some(value) = arg.strip_prefix("--demo=") {
            scenario = Some(parse_scenario(value)?);
            demo_requested = true;
        } else if arg == "--demo" {
            demo_requested = true;
            // The scenario is optional: `--demo` alone means "mixed", while
            // `--demo uploading` (and `--demo --screenshot=…`) must not swallow
            // the next argument.
            if let Some(next) = args.get(i + 1).filter(|next| !next.starts_with('-')) {
                scenario = Some(parse_scenario(next)?);
                i += 1;
            }
        } else if arg == "--demo-list" {
            list_requested = true;
        } else if let Some(value) = arg.strip_prefix("--screenshot=") {
            screenshot = Some(absolute_screenshot_path(value)?);
        } else if arg == "--screenshot" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| arg_error("--screenshot erwartet einen Pfad".to_string()))?;
            screenshot = Some(absolute_screenshot_path(value)?);
            i += 1;
        } else if arg.starts_with("--demo") || arg.starts_with("--screenshot") {
            // Our namespace, but not a form we know: always an error, even for
            // a normal start — a typo must not silently start the real app.
            return Err(arg_error(format!("Unbekanntes Argument: {arg}")));
        } else {
            foreign.push(arg);
        }
        i += 1;
    }

    if list_requested {
        if demo_requested || screenshot.is_some() {
            return Err(arg_error(
                "--demo-list kann nicht mit --demo/--screenshot kombiniert werden".to_string(),
            ));
        }
        if let Some(arg) = foreign.first() {
            return Err(arg_error(format!(
                "--demo-list erwartet keine weiteren Argumente: {arg}"
            )));
        }
        return Ok(Mode::List);
    }

    if demo_requested || screenshot.is_some() {
        // A demo run is an explicit developer action: a stray argument is more
        // likely a typo than launcher noise, so it is reported instead of
        // silently ignored.
        if let Some(arg) = foreign.first() {
            return Err(arg_error(format!("Unbekanntes Argument: {arg}")));
        }
    }

    match screenshot {
        Some(path) => Ok(Mode::Screenshot {
            // A screenshot always implies the demo fixtures; `--demo=<id>`
            // selects which one, otherwise the everyday picture.
            scenario: scenario.unwrap_or(Scenario::Mixed),
            path,
        }),
        None if demo_requested => Ok(Mode::Demo(scenario.unwrap_or(Scenario::Mixed))),
        // `Mode::Off` with foreign arguments: the normal app ignores them.
        None => Ok(Mode::Off),
    }
}

fn parse_scenario(value: &str) -> Result<Scenario, String> {
    Scenario::parse(value).ok_or_else(|| {
        let ids: Vec<&str> = Scenario::ALL.iter().map(|s| s.id()).collect();
        arg_error(format!(
            "Unbekanntes Szenario: {value} (bekannt: {})",
            ids.join(", ")
        ))
    })
}

/// The screenshot is written by this process, so a relative path would resolve
/// against whatever working directory it happens to have (Finder, `open`, a
/// test runner). Reject it instead of guessing.
fn absolute_screenshot_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(arg_error(format!(
            "Der Screenshot-Pfad muss absolut sein: {value}"
        )))
    }
}

fn arg_error(message: String) -> String {
    format!("{message}\n\n{}", usage())
}
