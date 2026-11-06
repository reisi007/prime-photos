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

use crate::config::Appearance;

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
    Demo {
        scenario: Scenario,
        size: DemoSize,
        language: Option<crate::i18n::Lang>,
        theme: Option<Appearance>,
    },
    /// Fixtures, one captured frame, then exit. The path must be absolute so
    /// the process can write it regardless of its working directory.
    Screenshot {
        scenario: Scenario,
        path: PathBuf,
        size: DemoSize,
        language: Option<crate::i18n::Lang>,
        theme: Option<Appearance>,
    },
}

/// Window size of a demo run in points (Issue #30).
///
/// The normal app keeps its screen-derived start size; a demo run is meant to
/// be looked at and captured, so its size is explicit and reproducible — also
/// at the minimum window size, where clipping shows up first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DemoSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl DemoSize {
    /// Default of a screenshot run: the full reference picture.
    pub(crate) const SCREENSHOT: Self = Self {
        width: 1400,
        height: 900,
    };
    /// Default of the interactive demo window.
    pub(crate) const INTERACTIVE: Self = Self {
        width: 1200,
        height: 800,
    };
    /// The app's minimum window (`main.rs` sets the same viewport minimum).
    pub(crate) const MIN_WIDTH: u32 = 760;
    pub(crate) const MIN_HEIGHT: u32 = 520;
    /// Upper bound for a demo window. Anything larger is almost certainly a
    /// typo and would only be clamped by the window manager, so it is rejected
    /// instead of handed to the OS.
    pub(crate) const MAX_WIDTH: u32 = 8192;
    pub(crate) const MAX_HEIGHT: u32 = 8192;

    /// `WxH` in points, e.g. `760x520`. Any other shape and anything outside
    /// the app's supported range is a usage error, never a silently clamped
    /// window.
    fn parse(value: &str) -> Result<Self, String> {
        let invalid = || {
            arg_error(format!(
                "Ungültige Fenstergröße: {value} (erwartet z. B. 760x520)"
            ))
        };
        let (width, height) = value
            .split_once(|c: char| c == 'x' || c == 'X')
            .ok_or_else(invalid)?;
        let width: u32 = width.parse().map_err(|_| invalid())?;
        let height: u32 = height.parse().map_err(|_| invalid())?;
        if width < Self::MIN_WIDTH || height < Self::MIN_HEIGHT {
            return Err(arg_error(format!(
                "Fenstergröße zu klein: {value} (Minimum {}x{})",
                Self::MIN_WIDTH,
                Self::MIN_HEIGHT
            )));
        }
        if width > Self::MAX_WIDTH || height > Self::MAX_HEIGHT {
            return Err(arg_error(format!(
                "Fenstergröße zu groß: {value} (Maximum {}x{})",
                Self::MAX_WIDTH,
                Self::MAX_HEIGHT
            )));
        }
        Ok(Self { width, height })
    }

    /// The size as egui points (the viewport API takes floats).
    pub(crate) fn points(self) -> [f32; 2] {
        [self.width as f32, self.height as f32]
    }
}

impl Mode {
    /// Whether this run uses the demo fixtures at all. `List` does not: it only
    /// prints the scenario table and exits.
    pub(crate) fn is_demo(&self) -> bool {
        matches!(self, Mode::Demo { .. } | Mode::Screenshot { .. })
    }

    /// Split the mode into what the app constructor needs: the scenario and an
    /// optional screenshot target. `None` means "do not start the app".
    pub(crate) fn into_demo(
        self,
    ) -> Option<(
        Scenario,
        Option<PathBuf>,
        Option<crate::i18n::Lang>,
        Option<Appearance>,
    )> {
        match self {
            Mode::Off | Mode::List => None,
            Mode::Demo {
                scenario,
                language,
                theme,
                ..
            } => Some((scenario, None, language, theme)),
            Mode::Screenshot {
                scenario,
                path,
                language,
                theme,
                ..
            } => Some((scenario, Some(path), language, theme)),
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
  prime-photos --demo=mixed --size 760x520
                                        Demo in einer bestimmten Fenstergröße (760x520…8192x8192)
  prime-photos --demo --demo-language en
                                        Demo-Sprache wählen (de oder en; Standard: de)
  prime-photos --demo-theme=dark       Darstellung wählen (system, light oder dark)

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
    let mut size: Option<DemoSize> = None;
    let mut demo_language: Option<crate::i18n::Lang> = None;
    let mut demo_language_requested = false;
    let mut demo_theme: Option<Appearance> = None;
    let mut demo_theme_requested = false;
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
        } else if let Some(value) = arg.strip_prefix("--demo-language=") {
            demo_language = Some(parse_demo_language(value)?);
            demo_language_requested = true;
        } else if arg == "--demo-language" {
            let value = args
                .get(i + 1)
                .filter(|next| !next.starts_with('-'))
                .ok_or_else(|| arg_error("--demo-language erwartet de oder en".to_string()))?;
            demo_language = Some(parse_demo_language(value)?);
            demo_language_requested = true;
            i += 1;
        } else if let Some(value) = arg.strip_prefix("--demo-theme=") {
            demo_theme = Some(parse_demo_theme(value)?);
            demo_theme_requested = true;
        } else if arg == "--demo-theme" {
            let value = args
                .get(i + 1)
                .filter(|next| !next.starts_with('-'))
                .ok_or_else(|| {
                    arg_error("--demo-theme erwartet system, light oder dark".to_string())
                })?;
            demo_theme = Some(parse_demo_theme(value)?);
            demo_theme_requested = true;
            i += 1;
        } else if let Some(value) = arg.strip_prefix("--screenshot=") {
            screenshot = Some(absolute_screenshot_path(value)?);
        } else if arg == "--screenshot" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| arg_error("--screenshot erwartet einen Pfad".to_string()))?;
            screenshot = Some(absolute_screenshot_path(value)?);
            i += 1;
        } else if let Some(value) = arg.strip_prefix("--size=") {
            size = Some(DemoSize::parse(value)?);
        } else if arg == "--size" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| arg_error("--size erwartet WxH".to_string()))?;
            size = Some(DemoSize::parse(value)?);
            i += 1;
        } else if arg.starts_with("--demo")
            || arg.starts_with("--screenshot")
            || arg.starts_with("--size")
        {
            // Our namespace, but not a form we know: always an error, even for
            // a normal start — a typo must not silently start the real app.
            return Err(arg_error(format!("Unbekanntes Argument: {arg}")));
        } else {
            foreign.push(arg);
        }
        i += 1;
    }

    if list_requested {
        if demo_requested
            || screenshot.is_some()
            || size.is_some()
            || demo_language_requested
            || demo_theme_requested
        {
            return Err(arg_error(
                "--demo-list kann nicht mit --demo/--demo-language/--demo-theme/--screenshot/--size kombiniert werden"
                    .to_string(),
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
    } else if size.is_some() {
        // `--size` only makes sense for a demo/screenshot run; the normal app
        // keeps its screen-derived start size.
        return Err(arg_error(
            "--size ist nur mit --demo oder --screenshot gültig".to_string(),
        ));
    } else if demo_language_requested {
        return Err(arg_error(
            "--demo-language ist nur mit --demo oder --screenshot gültig".to_string(),
        ));
    } else if demo_theme_requested {
        return Err(arg_error(
            "--demo-theme ist nur mit --demo oder --screenshot gültig".to_string(),
        ));
    }

    match screenshot {
        Some(path) => Ok(Mode::Screenshot {
            // A screenshot always implies the demo fixtures; `--demo=<id>`
            // selects which one, otherwise the everyday picture.
            scenario: scenario.unwrap_or(Scenario::Mixed),
            path,
            size: size.unwrap_or(DemoSize::SCREENSHOT),
            language: demo_language,
            theme: demo_theme,
        }),
        None if demo_requested => Ok(Mode::Demo {
            scenario: scenario.unwrap_or(Scenario::Mixed),
            size: size.unwrap_or(DemoSize::INTERACTIVE),
            language: demo_language,
            theme: demo_theme,
        }),
        // `Mode::Off` with foreign arguments: the normal app ignores them.
        None => Ok(Mode::Off),
    }
}

fn parse_demo_language(value: &str) -> Result<crate::i18n::Lang, String> {
    crate::i18n::Lang::from_code(value).ok_or_else(|| {
        arg_error(format!(
            "Unbekannte Demo-Sprache: {value} (erwartet de oder en)"
        ))
    })
}

fn parse_demo_theme(value: &str) -> Result<Appearance, String> {
    Appearance::from_code(value).ok_or_else(|| {
        arg_error(format!(
            "Unbekanntes Demo-Theme: {value} (erwartet system, light oder dark)"
        ))
    })
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
