//! Embedded login. Amazon's real sign-in page is shown in a WKWebView that is a
//! **child view of the main application window** — no extra window, no external
//! browser, no Chrome DevTools Protocol.
//!
//! Cookies are read asynchronously. On macOS the probe is scheduled on the main
//! dispatch queue, because wry's `WebView::cookies` is synchronous and spins the
//! macOS run loop, which would re-enter winit's event handler (and panic) if
//! called directly from egui's `update`. That run-loop re-entrancy is a
//! macOS-specific problem: off macOS the probe runs directly.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{Rect, WebView, WebViewBuilder};

use crate::app::{PrimePhotoApp, HEADER_H};
use crate::countries::{country_label, parse_cookie_string, COUNTRIES};
use crate::i18n::{tr, tr1};
use crate::icons::upload_logo;
use crate::login;
use crate::theme;
use eframe::egui::{self, RichText};

thread_local! {
    /// The login webview lives on the main thread only (a WebKit requirement).
    static WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

/// How often the cookie store is checked while the sign-in page is open.
const PROBE_INTERVAL: Duration = Duration::from_millis(500);

/// Amazon marketplace domain for a country code.
pub fn amazon_domain(country: &str) -> String {
    match country {
        "us" => "com".to_string(),
        "uk" | "gb" => "co.uk".to_string(),
        "at" | "ch" => "de".to_string(),
        "jp" => "co.jp".to_string(),
        "mx" => "com.mx".to_string(),
        "br" => "com.br".to_string(),
        "au" => "com.au".to_string(),
        "cn" => "cn".to_string(),
        "in" => "in".to_string(),
        other => other.to_string(),
    }
}

pub fn login_url(country: &str) -> String {
    let domain = amazon_domain(country);
    let cc = match country {
        "at" | "ch" => "de",
        other => other,
    };
    // Amazon's Photos sign-in (two-column layout). The marketplace is encoded
    // in the domain + assoc handle (`amzn_photos_web_<cc>`).
    let return_to = format!(
        "https%3A%2F%2Fwww.amazon.{d}%2Fphotos%3Fsf%3D1%26ref_%3DAPH_D_PA1_{cc}",
        d = domain,
        cc = cc.to_uppercase()
    );
    format!(
        "https://www.amazon.{domain}/ap/signin?openid.pape.max_auth_age=3600\
&openid.return_to={return_to}\
&openid.identity=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0%2Fidentifier_select\
&openid.assoc_handle=amzn_photos_web_{cc}\
&openid.mode=checkid_setup\
&openid.claimed_id=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0%2Fidentifier_select\
&openid.ns=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0"
    )
}

/// A rectangle in egui/logical points with a **top-left** origin, as wry wants.
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect {
        position: LogicalPosition::new(x as f64, y as f64).into(),
        size: LogicalSize::new(w.max(1.0) as f64, h.max(1.0) as f64).into(),
    }
}

/// True once the **authenticated** Amazon cookies exist (not just an anonymous
/// session): `session-id` plus the `at-acb*` / `x-acb*` auth cookies.
fn has_login(cookies: &[wry::cookie::Cookie<'static>]) -> bool {
    let has_session = cookies.iter().any(|c| c.name() == "session-id");
    let has_at = cookies.iter().any(|c| c.name().starts_with("at-acb"));
    let has_x = cookies.iter().any(|c| c.name().starts_with("x-acb"));
    has_session && has_at && has_x
}

fn cookie_string(cookies: &[wry::cookie::Cookie<'static>]) -> String {
    let mut s = String::new();
    for c in cookies {
        s.push_str(c.name());
        s.push('=');
        s.push_str(c.value());
        s.push_str("; ");
    }
    s
}

/// State shared with the asynchronous cookie probe.
#[derive(Default)]
struct Probe {
    in_flight: bool,
    last: Option<Instant>,
    outcome: Option<String>,
    error: Option<String>,
}

/// Owns the login webview for the duration of the sign-in.
pub struct WebviewLogin {
    pub running: bool,
    /// Raw cookie string ("name=value; …") once login completed.
    pub outcome: Option<String>,
    pub error: Option<String>,
    probe: Arc<Mutex<Probe>>,
}

impl Default for WebviewLogin {
    fn default() -> Self {
        Self::new()
    }
}

impl WebviewLogin {
    pub fn new() -> Self {
        Self {
            running: false,
            outcome: None,
            error: None,
            probe: Arc::new(Mutex::new(Probe::default())),
        }
    }

    /// Create the webview as a child of `window`, filling `bounds`.
    pub fn start<W: HasWindowHandle>(&mut self, window: &W, url: &str, bounds: Rect) {
        if self.running {
            return;
        }
        match WebViewBuilder::new()
            .with_url(url)
            .with_bounds(bounds)
            .build_as_child(window)
        {
            Ok(wv) => {
                WEBVIEW.with(|slot| *slot.borrow_mut() = Some(wv));
                *self.probe.lock().unwrap() = Probe::default();
                self.running = true;
                self.outcome = None;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(tr1("Anmeldeseite konnte nicht geöffnet werden: {}", e));
            }
        }
    }

    /// Keep the webview aligned with the area the UI reserved for it.
    pub fn set_bounds(&self, bounds: Rect) {
        WEBVIEW.with(|slot| {
            if let Some(wv) = slot.borrow().as_ref() {
                let _ = wv.set_bounds(bounds);
            }
        });
    }

    pub fn cancel(&mut self) {
        WEBVIEW.with(|slot| {
            slot.borrow_mut().take();
        });
        self.running = false;
    }

    /// Called every UI frame: schedule a cookie probe and pick up its result.
    pub fn poll(&mut self) {
        if !self.running {
            return;
        }

        let mut outcome = None;
        let mut error = None;
        let mut schedule = false;
        {
            let mut p = self.probe.lock().unwrap();
            outcome = p.outcome.take();
            if outcome.is_none() {
                error = p.error.take();
                let due = p
                    .last
                    .map(|t| t.elapsed() >= PROBE_INTERVAL)
                    .unwrap_or(true);
                if !p.in_flight && due {
                    p.in_flight = true;
                    p.last = Some(Instant::now());
                    schedule = true;
                }
            }
        }

        if let Some(cs) = outcome {
            self.outcome = Some(cs);
            self.running = false;
            WEBVIEW.with(|slot| {
                slot.borrow_mut().take();
            });
            return;
        }
        if let Some(e) = error {
            self.error = Some(e);
        }
        if schedule {
            schedule_probe(Arc::clone(&self.probe));
        }
    }
}

/// Read the webview's cookie store and record the outcome in `probe`. Shared by
/// both targets; `in_flight` is cleared when the probe finishes.
fn run_probe(probe: &Arc<Mutex<Probe>>) {
    WEBVIEW.with(|slot| {
        if let Some(wv) = slot.borrow().as_ref() {
            match wv.cookies() {
                Ok(cookies) => {
                    if has_login(&cookies) {
                        probe.lock().unwrap().outcome = Some(cookie_string(&cookies));
                    }
                }
                Err(e) => {
                    let mut p = probe.lock().unwrap();
                    if p.error.is_none() {
                        p.error = Some(tr1("Cookies konnten nicht gelesen werden: {}", e));
                    }
                }
            }
        }
    });
    probe.lock().unwrap().in_flight = false;
}

/// macOS: schedule the probe on the main dispatch queue, i.e. outside winit's
/// event handler (avoids the run-loop re-entrancy described above).
#[cfg(target_os = "macos")]
fn schedule_probe(probe: Arc<Mutex<Probe>>) {
    DispatchQueue::main().exec_async(move || run_probe(&probe));
}

/// Non-macOS: there is no run-loop re-entrancy problem, so run the probe
/// directly/synchronously on the calling (UI) thread.
#[cfg(not(target_os = "macos"))]
fn schedule_probe(probe: Arc<Mutex<Probe>>) {
    run_probe(&probe);
}

// -- login --------------------------------------------------------------

impl PrimePhotoApp {
    pub(crate) fn cancel_login(&mut self) {
        self.login.cancel();
    }

    /// Sign out: drop the stored cookies and return to the login screen.
    pub(crate) fn logout(&mut self) {
        self.config_form = None;
        self.config.session_id.clear();
        self.config.cookie_x_acb.clear();
        self.config.cookie_at_acb.clear();
        self.config.cookie_ubid_acb.clear();
        self.config.cookie_x_amz_access_token.clear();
        match self.config.save() {
            Ok(_) => self.log(tr("Abgemeldet.")),
            Err(e) => self.config_error = Some(e.to_string()),
        }
    }

    /// Create the login webview. Needs the frame for the window handle.
    pub(crate) fn begin_login(&mut self, frame: &eframe::Frame, ctx: &egui::Context) {
        if self.login.running {
            return;
        }
        let url = login::login_url(&self.config.country);
        let screen = ctx.screen_rect();
        // Start just below an estimated header; the exact bounds are corrected
        // on the next frame once the header has been measured.
        let bounds = login::rect(0.0, HEADER_H, screen.width(), screen.height() - HEADER_H);
        self.login.start(frame, &url, bounds);
    }

    pub(crate) fn poll_login(&mut self) {
        self.login.poll();
        let outcome = self.login.outcome.take();
        if let Some(cookie_string) = outcome {
            self.apply_cookie_string(&cookie_string);
            match self.config.save() {
                Ok(_) => self.log(tr("Login erfolgreich – Cookies übernommen.")),
                Err(e) => self.config_error = Some(e.to_string()),
            }
        }
    }

    pub(crate) fn apply_cookie_string(&mut self, cookie_string: &str) {
        let parsed = parse_cookie_string(cookie_string);
        self.config.session_id = parsed.session_id;
        if !parsed.country.is_empty() {
            self.config.country = parsed.country;
        }
        self.config.cookie_x_acb = parsed.cookie_x_acb;
        self.config.cookie_at_acb = parsed.cookie_at_acb;
        self.config.cookie_ubid_acb = parsed.cookie_ubid_acb;
        self.config.cookie_x_amz_access_token = parsed.cookie_x_amz_access_token;
    }

    /// Login phase (same window as the app, like the original).
    ///
    /// While Amazon's page is open we show a slim branded header and let the
    /// webview fill the rest of the window; before that we show our own
    /// sign-in screen with the marketplace picker.
    pub(crate) fn login_ui(&mut self, ctx: &egui::Context) {
        if self.login.running {
            let mut cancel = false;
            let header = egui::TopBottomPanel::top("login_header")
                .exact_height(HEADER_H)
                .frame(
                    egui::Frame::new()
                        .fill(theme::SIDEBAR_BG)
                        .inner_margin(egui::Margin::symmetric(16, 8)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        upload_logo(ui, 30.0);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(tr("Prime Photos")).strong().size(15.0));
                            ui.label(theme::muted(tr("Bei Amazon anmelden")));
                        });
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.add(theme::secondary_button(tr("Abbrechen"))).clicked() {
                                    cancel = true;
                                }
                                if let Some(e) = &self.login.error {
                                    ui.colored_label(theme::RED, e);
                                } else {
                                    ui.spinner();
                                }
                            },
                        );
                    });
                });
            let top_h = header.response.rect.height();
            let screen = ctx.screen_rect();
            self.login.set_bounds(login::rect(
                0.0,
                top_h,
                screen.width(),
                screen.height() - top_h,
            ));
            if cancel {
                self.cancel_login();
            }
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| self.login_screen_ui(ui));
    }

    /// Our own sign-in screen, shown before the Amazon page is opened.
    pub(crate) fn login_screen_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(56.0);
            upload_logo(ui, 84.0);
            ui.add_space(12.0);
            ui.label(RichText::new(tr("Prime Photos")).size(28.0).strong());
            ui.add_space(4.0);
            ui.label(theme::muted(tr("Fotos & Videos zu Amazon Photos hochladen")));
            ui.add_space(26.0);

            // Marketplace selection before signing in.
            ui.horizontal(|ui| {
                ui.label(tr("Land / Amazon-Marktplatz:"));
                let current = self.config.country.clone();
                egui::ComboBox::from_id_salt("login-country")
                    .selected_text(country_label(&current))
                    .show_ui(ui, |ui| {
                        for &(code, _, _) in COUNTRIES {
                            ui.selectable_value(
                                &mut self.config.country,
                                code.to_string(),
                                country_label(code),
                            );
                        }
                    });
            });
            ui.add_space(14.0);

            if ui.add(theme::primary_button(tr("Mit Amazon anmelden"))).clicked() {
                self.pending_login = true;
            }
            if let Some(e) = &self.login.error {
                ui.add_space(6.0);
                ui.colored_label(theme::RED, e);
            }

            ui.add_space(16.0);
            if ui.link(tr("Amazon Photos im Browser öffnen")).clicked() {
                let url = login::login_url(&self.config.country);
                let _ = std::process::Command::new("open").arg(url).spawn();
            }
        });
    }
}
