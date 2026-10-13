use std::{
    path::PathBuf,
    process::Child,
    time::Duration,
};

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Base port for the Chrome DevTools Protocol listener.
const BASE_PORT: u16 = 9223;
/// How many ports we try before giving up.
const PORT_RANGE: u16 = 10;
/// Overall login timeout (5 minutes) for the automatic browser login.
const LOGIN_TIMEOUT_SECS: u64 = 300;
/// How long we wait for Chrome to expose its DevTools endpoint.
const BROWSER_START_TIMEOUT_SECS: u64 = 45;

/// Result of the browser login flow.
pub struct LoginOutcome {
    pub cookies: Vec<(String, String)>,
    pub browser_name: String,
}

pub struct BrowserLogin {
    pub child: Option<Child>,
    pub message: String,
    pub running: bool,
    pub outcome: Option<LoginOutcome>,
    pub error: Option<String>,
}

impl BrowserLogin {
    pub fn new() -> Self {
        Self {
            child: None,
            message: String::new(),
            running: false,
            outcome: None,
            error: None,
        }
    }

    /// Kill the helper browser window if we still own it.
    pub fn stop_browser(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
    }
}

/// Browsers we can drive via CDP, in order of preference.
fn browser_candidates() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();

    let mac_candidates = [
        ("Google Chrome", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        ("Chromium", "/Applications/Chromium.app/Contents/MacOS/Chromium"),
        ("Microsoft Edge", "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
        ("Brave Browser", "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"),
        ("Vivaldi", "/Applications/Vivaldi.app/Contents/MacOS/Vivaldi"),
        ("Arc", "/Applications/Arc.app/Contents/MacOS/Arc"),
    ];
    for (name, path) in mac_candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            out.push((name.to_string(), p));
        }
    }

    for (name, cmd) in [
        ("Google Chrome", "google-chrome"),
        ("Google Chrome", "google-chrome-stable"),
        ("Chromium", "chromium"),
        ("Chromium", "chromium-browser"),
        ("Microsoft Edge", "microsoft-edge"),
    ] {
        if let Ok(path) = which(cmd) {
            out.push((name.to_string(), path));
        }
    }

    out
}

fn which(cmd: &str) -> std::io::Result<PathBuf> {
    // Minimal `which` so we avoid an extra dependency.
    let paths = std::env::var("PATH").unwrap_or_default();
    for dir in paths.split(':') {
        let candidate = PathBuf::from(dir).join(cmd);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "not found",
    ))
}

/// Amazon Photos marketplace for a country code.
fn amazon_domain(country: &str) -> String {
    match country {
        "uk" | "gb" => "co.uk".to_string(),
        "jp" => "co.jp".to_string(),
        "mx" => "com.mx".to_string(),
        "br" => "com.br".to_string(),
        "au" => "com.au".to_string(),
        "cn" => "cn".to_string(),
        "in" => "in".to_string(),
        other => other.to_string(),
    }
}

fn login_url(country: &str) -> String {
    format!("https://www.amazon.{}/photos", amazon_domain(country))
}

fn profile_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("amzn_photos_uploader")
        .join("browser-profile")
}

/// Find a free CDP port.
async fn free_port() -> u16 {
    for port in BASE_PORT..=BASE_PORT + PORT_RANGE {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        if std::net::TcpListener::bind(addr).is_ok() {
            return port;
        }
    }
    BASE_PORT
}

/// Start the helper browser and return the spawned child plus the login URL.
fn spawn_browser(browser: &PathBuf, port: u16, country: &str) -> std::io::Result<Child> {
    let profile = profile_dir();
    let _ = std::fs::create_dir_all(&profile);

    let url = login_url(country);

    let child = std::process::Command::new(browser)
        .arg(format!("--remote-debugging-port={}", port))
        // Required by Chrome >= 111 for CDP WebSocket connections.
        .arg("--remote-allow-origins=*")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-session-crashed-bubble")
        .arg("--new-window")
        .arg(url)
        .spawn()?;

    Ok(child)
}

async fn list_targets(port: u16) -> Option<Value> {
    let url = format!("http://127.0.0.1:{}/json/list", port);
    let resp = reqwest::get(&url).await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().await.ok()
}

fn pick_page_ws(targets: &Value) -> Option<String> {
    let arr = targets.as_array()?;
    for t in arr {
        if t.get("type").and_then(|v| v.as_str()) == Some("page") {
            if let Some(ws) = t.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
                return Some(ws.to_string());
            }
        }
    }
    // Fallback: first page target of any kind.
    arr.iter()
        .find(|t| {
            t.get("type").and_then(|v| v.as_str()) == Some("page")
        })
        .and_then(|t| t.get("webSocketDebuggerUrl").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

fn cookies_from_result(value: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(cookies) = value
        .get("result")
        .and_then(|r| r.get("cookies"))
        .and_then(|c| c.as_array())
    {
        for c in cookies {
            if let (Some(name), Some(val)) = (
                c.get("name").and_then(|v| v.as_str()),
                c.get("value").and_then(|v| v.as_str()),
            ) {
                out.push((name.to_string(), val.to_string()));
            }
        }
    }
    out
}

/// Ask the browser for its cookies over the DevTools WebSocket.
async fn fetch_cookies_via_cdp(ws_url: &str) -> Option<Vec<(String, String)>> {
    let (mut ws, _) = connect_async(ws_url).await.ok()?;

    let enable = json!({"id": 1, "method": "Network.enable"});
    ws.send(Message::Text(enable.to_string())).await.ok()?;

    let get_cookies = json!({"id": 2, "method": "Network.getCookies"});
    ws.send(Message::Text(get_cookies.to_string())).await.ok()?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while let Ok(Some(msg)) = tokio::time::timeout_at(deadline, ws.next()).await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                if value.get("id").and_then(|v| v.as_u64()) == Some(2) {
                    let cookies = cookies_from_result(&value);
                    if !cookies.is_empty() {
                        return Some(cookies);
                    }
                    // Newer browsers sometimes only implement Storage.getCookies.
                    let storage_enable = json!({"id": 3, "method": "Storage.enable"});
                    let _ = ws.send(Message::Text(storage_enable.to_string())).await;
                    let storage_get =
                        json!({"id": 4, "method": "Storage.getCookies"});
                    let _ = ws.send(Message::Text(storage_get.to_string())).await;
                } else if value.get("id").and_then(|v| v.as_u64()) == Some(4) {
                    let cookies = cookies_from_result(&value);
                    if !cookies.is_empty() {
                        return Some(cookies);
                    }
                }
            }
        }
    }
    None
}

/// True when we have the minimum cookie set to talk to the Photos API.
fn has_login_cookies(cookies: &[(String, String)]) -> bool {
    let has_session = cookies.iter().any(|(n, _)| n == "session-id");
    let has_acb = cookies
        .iter()
        .any(|(n, _)| n.starts_with("x-acb") || n.starts_with("at-acb") || n.starts_with("ubid-acb"));
    has_session && has_acb
}

/// Run the whole automatic browser login flow.
pub async fn run_browser_login(country: String, login: std::sync::Arc<std::sync::Mutex<BrowserLogin>>) {
    let browsers = browser_candidates();
    if browsers.is_empty() {
        let mut l = login.lock().unwrap();
        l.running = false;
        l.error = Some(
            "Kein steuerbarer Browser gefunden (Chrome, Chromium, Edge, Brave oder Vivaldi). Bitte Paste-Login verwenden.".to_string(),
        );
        return;
    }

    let (browser_name, browser_path) = browsers[0].clone();
    let port = free_port().await;

    {
        let mut l = login.lock().unwrap();
        l.message = format!("Öffne {} für Login…", browser_name);
    }

    let child = match spawn_browser(&browser_path, port, &country) {
        Ok(c) => c,
        Err(e) => {
            let mut l = login.lock().unwrap();
            l.running = false;
            l.error = Some(format!("Browser konnte nicht gestartet werden: {}", e));
            return;
        }
    };

    {
        let mut l = login.lock().unwrap();
        l.child = Some(child);
    }

    // Wait for the DevTools endpoint.
    let start = tokio::time::Instant::now();
    let ws_url = loop {
        if start.elapsed() > Duration::from_secs(BROWSER_START_TIMEOUT_SECS) {
            let mut l = login.lock().unwrap();
            l.running = false;
            l.error = Some("DevTools-Schnittstelle des Browsers wurde nicht rechtzeitig gefunden.".to_string());
            l.stop_browser();
            return;
        }
        if let Some(targets) = list_targets(port).await {
            if let Some(ws) = pick_page_ws(&targets) {
                break ws;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };

    {
        let mut l = login.lock().unwrap();
        l.message = format!("Bitte jetzt bei Amazon Photos anmelden ({}).", browser_name);
    }

    // Poll for cookies until the login completed or we time out.
    let start = tokio::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(LOGIN_TIMEOUT_SECS) {
            let mut l = login.lock().unwrap();
            l.running = false;
            l.error = Some("Zeitüberschreitung beim Login.".to_string());
            l.stop_browser();
            return;
        }

        if let Some(cookies) = fetch_cookies_via_cdp(&ws_url).await {
            if has_login_cookies(&cookies) {
                let mut l = login.lock().unwrap();
                l.running = false;
                l.outcome = Some(LoginOutcome {
                    cookies,
                    browser_name: browser_name.clone(),
                });
                l.stop_browser();
                return;
            }
        }

        {
            let mut l = login.lock().unwrap();
            if !l.running {
                // Cancelled from the UI.
                l.stop_browser();
                return;
            }
            l.message = format!("Warte auf Anmeldung bei Amazon Photos ({}).", browser_name);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
