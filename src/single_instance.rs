//! Single-instance guard: only one Prime Photos process at a time.
//!
//! Starting the app a second time (Finder, Spotlight, `open`, a Dock click)
//! must not produce a second window: the new process hands a "show your
//! window" request to the running instance and exits. macOS covers the
//! Launch-Services path itself with `LSMultipleInstancesProhibited` (see
//! `packaging/Info.plist`); this module covers every start that bypasses it (a
//! second `./prime-photos` from a shell, and Linux/Windows in general).
//!
//! Transport: a `UnixListener` on
//! `dirs::config_dir()/prime_photos/instance.sock` on unix, a loopback
//! `TcpListener` on a fixed port on Windows (std has no named pipes). The
//! handshake is a single line in each direction: the newcomer sends
//! `prime-photos:show`, the running instance answers `prime-photos:ok` and
//! shows its window. A socket file a crash left behind is detected (the
//! connect fails) and taken over; a foreign program on the Windows port is
//! logged and tolerated — the app then runs **without** the guard rather than
//! refusing to start.
//!
//! The accept loop runs on its own thread and never touches the UI: it only
//! sets an `AtomicBool` and wakes the render loop, exactly like `src/tray.rs`
//! and `src/app_menu.rs`. `PrimePhotoApp::update` drains the request through
//! [`take_show_request`] and shows the (possibly hidden) window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The line a second process sends to ask the running instance to show itself.
const SHOW_REQUEST: &str = "prime-photos:show\n";

/// The running instance's answer; anything else means "not us".
const SHOW_ANSWER: &str = "prime-photos:ok\n";

/// How long a starting process waits for that answer before it treats the
/// endpoint as stale (unix) or foreign (Windows). Keeps a dead or silent
/// endpoint from blocking the start.
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

/// Fixed loopback port of the Windows transport. Deliberately below the
/// ephemeral port range (49152+), so an outgoing connection cannot squat it.
#[cfg(windows)]
const WINDOWS_INSTANCE_PORT: u16 = 47733;

/// Set by the accept loop when another process asked this instance to show its
/// window; consumed once per frame by `PrimePhotoApp::update` through
/// [`take_show_request`].
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The egui context to wake when a second process asks for the window. Stored
/// by [`attach_ctx`]; the accept loop only flags + repaints, never touches the
/// UI directly. Missing context = the flag is still set, only the immediate
/// repaint is skipped.
static UI_CTX: OnceLock<eframe::egui::Context> = OnceLock::new();

/// Remember the render-loop context. `PrimePhotoApp::new` calls this next to
/// `app_menu::install`; the `egui::Context` only exists once `eframe` has
/// started, which is why the accept loop cannot just capture it.
pub fn attach_ctx(ctx: &eframe::egui::Context) {
    let _ = UI_CTX.set(ctx.clone());
}

/// Whether another process asked this instance to show its window since the
/// last call. Drains the flag, so the caller shows the window exactly once per
/// request.
pub fn take_show_request() -> bool {
    SHOW_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Take the single-instance slot, or ask the running instance to show its
/// window.
///
/// `false` means "another instance answered — it has been asked to show its
/// window, this process must exit". `true` means "this process owns the slot"
/// (also when the guard is unavailable for an environmental reason: the app
/// must always start).
#[cfg(unix)]
pub fn acquire() -> bool {
    let Some(dir) = dirs::config_dir() else {
        log::warn!("Prime Photos: kein Konfigurationsverzeichnis für den Einzelinstanz-Schutz");
        return true;
    };
    acquire_at(&dir.join("prime_photos").join("instance.sock"))
}

/// Unix variant of [`acquire`] with an injectable socket path, so a test can
/// use a temp directory instead of the real config directory.
#[cfg(unix)]
pub fn acquire_at(path: &std::path::Path) -> bool {
    use std::os::unix::net::UnixListener;

    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        // Not fatal: the bind below reports the real problem.
        log::warn!("Prime Photos: Instanz-Verzeichnis nicht anlegbar: {e}");
    }

    match UnixListener::bind(path) {
        Ok(listener) => {
            spawn_accept_loop(listener);
            true
        }
        Err(e) => {
            // The path is taken — either by a running instance or by a socket
            // file a crash left behind. Only a running instance answers the
            // handshake; then we hand the request over and step aside.
            if ask_existing_instance(path) {
                return false;
            }
            log::debug!("Prime Photos: verwaister Instanz-Socket ({e}), wird übernommen");
            let _ = std::fs::remove_file(path);
            match UnixListener::bind(path) {
                Ok(listener) => {
                    spawn_accept_loop(listener);
                    true
                }
                Err(e) => {
                    // No guard, but the app must start (never abort on this).
                    log::warn!("Prime Photos: Einzelinstanz-Schutz nicht verfügbar: {e}");
                    true
                }
            }
        }
    }
}

/// Windows: same handshake over a fixed loopback port (std has no named
/// pipes).
#[cfg(windows)]
pub fn acquire() -> bool {
    acquire_tcp_at(WINDOWS_INSTANCE_PORT)
}

/// No guard on other targets; the app must still start.
#[cfg(not(any(unix, windows)))]
pub fn acquire() -> bool {
    true
}

/// Handle one accepted connection: verify the request line, flag + wake the UI
/// and answer. Shared by both transports; never blocks on a silent peer
/// (the caller sets a read timeout).
fn serve<S: std::io::Read + std::io::Write>(stream: &mut S) {
    use std::io::{BufRead, BufReader};

    let mut line = String::new();
    let read_ok = {
        let mut reader = BufReader::new(&mut *stream);
        reader.read_line(&mut line).is_ok()
    };
    if !read_ok || line.trim_end() != SHOW_REQUEST.trim_end() {
        log::debug!("Prime Photos: unbekannte Anfrage am Instanz-Endpunkt");
        return;
    }

    // The requester only needs to know that we got it; the UI is woken, never
    // touched from here.
    SHOW_REQUESTED.store(true, Ordering::Relaxed);
    if let Some(ctx) = UI_CTX.get() {
        ctx.request_repaint();
    }
    let _ = stream.write_all(SHOW_ANSWER.as_bytes());
    let _ = stream.flush();
}

/// Ask the instance behind `path` to show its window. `true` only when a Prime
/// Photos instance answered the handshake.
#[cfg(unix)]
fn ask_existing_instance(path: &std::path::Path) -> bool {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let Ok(mut stream) = UnixStream::connect(path) else {
        // Nothing listens behind the path: a crash left the socket file.
        return false;
    };
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    if stream.write_all(SHOW_REQUEST.as_bytes()).is_err() || stream.flush().is_err() {
        return false;
    }
    let mut answer = String::new();
    if BufReader::new(&mut stream).read_line(&mut answer).is_err() {
        return false;
    }
    answer.trim_end() == SHOW_ANSWER.trim_end()
}

/// Accept connections for the rest of the process lifetime.
#[cfg(unix)]
fn spawn_accept_loop(listener: std::os::unix::net::UnixListener) {
    let spawned = std::thread::Builder::new()
        .name("prime-photos-instance".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    log::warn!("Prime Photos: Instanz-Listener beendet sich nach einem Fehler");
                    return;
                };
                let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
                serve(&mut stream);
            }
        });
    if let Err(e) = spawned {
        log::warn!("Prime Photos: Instanz-Thread nicht startbar: {e}");
    }
}

/// Windows (and test): the same as [`acquire_at`], over TCP. `port` is
/// injectable so the test does not have to touch the production port.
#[cfg(any(windows, test))]
fn acquire_tcp_at(port: u16) -> bool {
    use std::net::{Ipv4Addr, TcpListener};

    match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => {
            spawn_tcp_accept_loop(listener);
            true
        }
        Err(e) => {
            if ask_existing_tcp_instance(port) {
                return false;
            }
            // Either a foreign program holds the port or the bind failed for
            // another reason: run **without** the guard (and say so) instead of
            // refusing to start.
            log::warn!("Prime Photos: Einzelinstanz-Schutz nicht verfügbar (Port {port}): {e}");
            true
        }
    }
}

/// Ask the instance behind `port` to show its window. `true` only when a Prime
/// Photos instance answered the handshake.
#[cfg(any(windows, test))]
fn ask_existing_tcp_instance(port: u16) -> bool {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{Ipv4Addr, TcpStream};

    let Ok(mut stream) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    if stream.write_all(SHOW_REQUEST.as_bytes()).is_err() || stream.flush().is_err() {
        return false;
    }
    let mut answer = String::new();
    if BufReader::new(&mut stream).read_line(&mut answer).is_err() {
        return false;
    }
    answer.trim_end() == SHOW_ANSWER.trim_end()
}

/// Accept connections for the rest of the process lifetime.
#[cfg(any(windows, test))]
fn spawn_tcp_accept_loop(listener: std::net::TcpListener) {
    let spawned = std::thread::Builder::new()
        .name("prime-photos-instance".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    log::warn!("Prime Photos: Instanz-Listener beendet sich nach einem Fehler");
                    return;
                };
                let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
                serve(&mut stream);
            }
        });
    if let Err(e) = spawned {
        log::warn!("Prime Photos: Instanz-Thread nicht startbar: {e}");
    }
}

/// Serialises the two tests: `SHOW_REQUESTED` is a process-global flag, so two
/// tests draining it in parallel could steal each other's request. Poisoning is
/// ignored — a failed test must not fail the other one.
#[cfg(test)]
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(all(test, unix))]
#[test]
fn second_instance_defers_and_the_first_shows_its_window() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let dir = std::env::temp_dir().join(format!("prime-photos-instance-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("Temp-Verzeichnis");
    let path = dir.join("instance.sock");

    assert!(acquire_at(&path), "der erste Start muss primär werden");
    assert!(!acquire_at(&path), "der zweite Start muss sich fügen");
    assert!(
        take_show_request(),
        "die laufende Instanz muss den Show-Wunsch sehen"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The Windows transport is exercised on every platform (it is plain `std`),
/// so its handshake is covered by `cargo test` even without a Windows host.
#[cfg(test)]
#[test]
fn tcp_second_instance_defers_and_the_first_shows_its_window() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Below the ephemeral range and derived from the pid, so the test does not
    // collide with the production port or with a random outgoing connection.
    let port = 44733 + (std::process::id() % 1000) as u16;

    assert!(acquire_tcp_at(port), "der erste Start muss primär werden");
    assert!(!acquire_tcp_at(port), "der zweite Start muss sich fügen");
    assert!(
        take_show_request(),
        "die laufende Instanz muss den Show-Wunsch sehen"
    );
}
