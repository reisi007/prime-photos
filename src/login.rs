//! Login via an embedded webview. Spawns the sibling `ppu-login` helper (an
//! own process with its own event loop, so it does not fight egui), waits for
//! it to write the captured cookies, and hands them back.

use std::path::PathBuf;
use std::process::{Child, Command};

/// Amazon marketplace for a country code.
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
    format!("https://www.amazon.{}/photos", amazon_domain(country))
}

pub struct WebviewLogin {
    pub running: bool,
    pub message: String,
    /// Raw cookie string ("name=value; …") written by the helper.
    pub outcome: Option<String>,
    pub error: Option<String>,
    child: Option<Child>,
    out_path: PathBuf,
}

impl WebviewLogin {
    pub fn new() -> Self {
        let out_path = std::env::temp_dir().join("prime-photo-uploader-cookies.txt");
        Self {
            running: false,
            message: String::new(),
            outcome: None,
            error: None,
            child: None,
            out_path,
        }
    }

    /// The `ppu-login` binary that sits next to our own executable.
    fn helper_bin() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let candidate = exe.parent()?.join("ppu-login");
        if candidate.exists() {
            Some(candidate)
        } else {
            None
        }
    }

    pub fn start(&mut self, url: String) {
        if self.running {
            return;
        }
        let Some(bin) = Self::helper_bin() else {
            self.error = Some(
                "Login-Komponente (ppu-login) nicht gefunden – bitte die App neu bauen."
                    .to_string(),
            );
            return;
        };
        let _ = std::fs::remove_file(&self.out_path);
        match Command::new(bin).arg(&self.out_path).arg(url).spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.running = true;
                self.message = "Bitte im Fenster bei Amazon Photos anmelden.".to_string();
                self.outcome = None;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(format!(
                    "Login-Fenster konnte nicht gestartet werden: {}",
                    e
                ));
            }
        }
    }

    fn kill_child(&mut self) {
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.child = None;
    }

    pub fn cancel(&mut self) {
        self.running = false;
        self.kill_child();
    }

    /// Called every UI frame: picks up the cookies or the helper's exit.
    pub fn poll(&mut self) {
        if !self.running {
            return;
        }
        if let Ok(s) = std::fs::read_to_string(&self.out_path) {
            if !s.is_empty() {
                self.outcome = Some(s);
                self.running = false;
                self.kill_child();
                return;
            }
        }
        if let Some(child) = self.child.as_mut() {
            if let Ok(Some(_)) = child.try_wait() {
                // Helper exited without writing cookies (window closed).
                self.running = false;
                self.error = Some("Anmeldung abgebrochen.".to_string());
                self.child = None;
            }
        }
    }
}
