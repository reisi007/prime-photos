//! The screenshot runtime: request one frame, write it as PNG, close the app.

use super::Scenario;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Frames rendered before the screenshot is requested: the first frames still
/// upload textures (icons, fonts) and would capture an incomplete picture.
const SETTLE_FRAMES: u32 = 3;

/// Frames we wait for the window system's answer before giving up loudly. A
/// backend without screenshot support never answers; hanging would be worse
/// than a clear failure. This is the secondary guard — the wall-clock deadline
/// below also covers a stalled event loop that renders no frames at all.
const SCREENSHOT_WAIT_FRAMES: u32 = 3000;

/// Hard wall-clock deadline for the whole capture. A screenshot normally lands
/// within a second; 15 s leaves room for a slow first frame (font/texture
/// upload) while still failing a CI step instead of hanging it.
const SCREENSHOT_DEADLINE: Duration = Duration::from_secs(15);

/// Set once the PNG is on disk. The watchdog reads it; it is never reset (one
/// screenshot run per process).
static SCREENSHOT_WRITTEN: AtomicBool = AtomicBool::new(false);

/// Per-run demo state, owned by [`crate::app::PrimePhotoApp`].
///
/// Interactive demo runs only carry the fixture flags; screenshot runs also
/// drive the request/save/close sequence from [`Self::handle_ui`].
pub(crate) struct DemoRuntime {
    scenario: Scenario,
    screenshot: Option<PathBuf>,
    frames: u32,
    requested: bool,
    /// The PNG was handed to the writer: no further request, no frame timeout.
    /// A second `update` after `ViewportCommand::Close` must not turn a
    /// successful capture into a failure.
    written: bool,
    /// The faked `path.is_dir()` per `store.folders[i]` (see
    /// [`crate::demo::fixture`]).
    pub(crate) source_exists: Vec<bool>,
}

impl DemoRuntime {
    pub(crate) fn new(
        scenario: Scenario,
        screenshot: Option<PathBuf>,
        source_exists: Vec<bool>,
    ) -> Self {
        // A stalled event loop renders no frames, so the frame limit above can
        // never trigger. The watchdog only reads the latch and exits the
        // process; it never touches the UI.
        if let Some(path) = &screenshot {
            spawn_watchdog(path.clone());
        }
        Self {
            scenario,
            screenshot,
            frames: 0,
            requested: false,
            written: false,
            source_exists,
        }
    }

    /// Screenshot step, called early in every `update`.
    ///
    /// Interactive demo: a cheap no-op. Screenshot run: ask for one frame after
    /// the UI has settled, write the answer as PNG and close the window so the
    /// process exits by itself.
    pub(crate) fn handle_ui(&mut self, ctx: &egui::Context) {
        // Already written and the close is on its way: nothing left to do.
        if self.written {
            return;
        }
        let Some(path) = self.screenshot.clone() else {
            return;
        };

        // The answer can arrive in the frame the request was answered.
        if let Some(image) = take_screenshot(ctx) {
            self.written = true;
            self.write_and_close(ctx, &path, &image);
            return;
        }

        self.frames += 1;
        if self.requested {
            if self.frames > SCREENSHOT_WAIT_FRAMES {
                let msg = format!(
                    "Prime Photos: Das Fenstersystem hat innerhalb von \
                     {SCREENSHOT_WAIT_FRAMES} Frames keinen Screenshot geliefert \
                     (Ziel: {}).",
                    path.display()
                );
                eprintln!("{msg}");
                log::error!("{msg}");
                std::process::exit(1);
            }
            // Keep the event loop alive until the answer arrives.
            ctx.request_repaint();
            return;
        }
        if self.frames < SETTLE_FRAMES {
            ctx.request_repaint();
            return;
        }
        log::debug!(
            "Demo-Szenario {}: Screenshot wird angefordert",
            self.scenario.id()
        );
        self.frames = 0;
        self.requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        ctx.request_repaint();
    }

    fn write_and_close(&self, ctx: &egui::Context, path: &Path, image: &egui::ColorImage) {
        match save_png(path, image) {
            Ok(()) => {
                // Latch **before** the close: the watchdog must never mistake a
                // finished run for a stuck one while the event loop shuts down.
                SCREENSHOT_WRITTEN.store(true, Ordering::SeqCst);
                log::info!("Screenshot gespeichert: {}", path.display());
                println!("Screenshot gespeichert: {}", path.display());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => {
                // A failed capture must never look like a success: report the
                // path and the cause, then exit nonzero.
                let msg = format!(
                    "Prime Photos: Screenshot konnte nicht nach {} geschrieben werden: {e}",
                    path.display()
                );
                eprintln!("{msg}");
                log::error!("{msg}");
                std::process::exit(1);
            }
        }
    }
}

/// Force-exit if no PNG was written within [`SCREENSHOT_DEADLINE`].
///
/// A sleeping thread is the only mechanism that still fires when the render
/// loop makes no progress (a stuck compositor, a backend that never answers).
/// It exits the process instead of leaving a CI step hanging.
fn spawn_watchdog(path: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("prime-photos-screenshot-watchdog".into())
        .spawn(move || {
            std::thread::sleep(SCREENSHOT_DEADLINE);
            if !SCREENSHOT_WRITTEN.load(Ordering::SeqCst) {
                let msg = format!(
                    "Prime Photos: Screenshot-Timeout nach {} s, es wurde keine Datei \
                     geschrieben (Ziel: {}).",
                    SCREENSHOT_DEADLINE.as_secs(),
                    path.display()
                );
                eprintln!("{msg}");
                log::error!("{msg}");
                std::process::exit(1);
            }
        });
    if let Err(e) = spawned {
        // The frame limit in `handle_ui` still applies; only the stalled-loop
        // case would go unnoticed.
        log::warn!("Screenshot-Watchdog konnte nicht gestartet werden: {e}");
    }
}

/// The screenshot event of this frame, if one arrived.
fn take_screenshot(ctx: &egui::Context) -> Option<egui::ColorImage> {
    ctx.input(|input| {
        input.events.iter().rev().find_map(|event| match event {
            egui::Event::Screenshot { image, .. } => Some((**image).clone()),
            _ => None,
        })
    })
}

/// Write the captured frame as PNG. The format is pinned instead of derived
/// from the extension, so `--screenshot foo.bin` still writes a valid PNG (or
/// fails for a real reason, not for a missing extension).
fn save_png(path: &Path, image: &egui::ColorImage) -> Result<(), String> {
    let [width, height] = image.size;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in &image.pixels {
        rgba.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }
    image::save_buffer_with_format(
        path,
        &rgba,
        width as u32,
        height as u32,
        image::ExtendedColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())
}
