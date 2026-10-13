//! Standalone login helper: shows Amazon's real sign-in page in a native
//! WKWebView, waits until the session cookies exist, writes them to a file and
//! exits. Runs in its own process so it does not fight egui for the main loop.
//!
//! Usage: ppu-login <cookie-output-file> <login-url>

use std::io::Write;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

fn has_login(cookies: &[wry::cookie::Cookie<'static>]) -> bool {
    // Require the auth cookies, not just an anonymous session.
    let has_session = cookies.iter().any(|c| c.name() == "session-id");
    let has_at = cookies.iter().any(|c| c.name().starts_with("at-acb"));
    let has_x = cookies.iter().any(|c| c.name().starts_with("x-acb"));
    has_session && has_at && has_x
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out_path = args
        .next()
        .unwrap_or_else(|| "/tmp/ppu-cookies.txt".to_string());
    let url = args
        .next()
        .unwrap_or_else(|| "https://www.amazon.com/photos".to_string());

    let event_loop = EventLoopBuilder::new().build();
    let window = WindowBuilder::new()
        .with_title("Bei Amazon Photos anmelden")
        .with_inner_size(LogicalSize::new(960.0, 780.0))
        .build(&event_loop)
        .expect("Fenster konnte nicht erstellt werden");

    let webview = WebViewBuilder::new()
        .with_url(url)
        .build(&window)
        .expect("WebView konnte nicht erstellt werden");

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::MainEventsCleared => {
                if let Ok(cookies) = webview.cookies() {
                    if has_login(&cookies) {
                        let mut s = String::new();
                        for c in &cookies {
                            s.push_str(c.name());
                            s.push('=');
                            s.push_str(c.value());
                            s.push_str("; ");
                        }
                        if let Ok(mut f) = std::fs::File::create(&out_path) {
                            let _ = f.write_all(s.as_bytes());
                        }
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            _ => {}
        }
    });
}
