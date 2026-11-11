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
//! shows its window. A Unix sidecar file lock serializes stale-socket recovery
//! and remains held for the process lifetime, independently of the listener
//! thread. A foreign program on the Windows port is logged and tolerated — the
//! app then runs **without** the guard rather than refusing to start.
//!
//! The accept loop runs on its own thread and never touches the UI: it only
//! sets an `AtomicBool` and invokes a UI-neutral wake callback. The active UI
//! host drains the request through [`take_show_request`] and shows its window.

use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
mod tests;

type Wakeup = Arc<dyn Fn() + Send + Sync>;

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
/// window; consumed once per Rune poll through [`take_show_request`].
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The active UI host's wake callback. The accept loop only flags + wakes,
/// never touches the UI directly. Missing callback = the flag is still set.
static UI_WAKEUP: OnceLock<Wakeup> = OnceLock::new();

/// Unix ownership locks outlive the detached accept thread. If that thread
/// exits or panics while the app remains alive, another process must still not
/// take over the socket path.
#[cfg(unix)]
static UNIX_INSTANCE_LOCKS: OnceLock<std::sync::Mutex<Vec<std::fs::File>>> = OnceLock::new();

/// Remember a UI-neutral render-loop waker for the active host.
pub(crate) fn attach_wakeup(wakeup: Arc<dyn Fn() + Send + Sync>) {
    let _ = UI_WAKEUP.set(wakeup);
    // A launch request can arrive between acquiring the guard and attaching
    // the UI. Wake the newly attached host so it can drain that request.
    if SHOW_REQUESTED.load(Ordering::Relaxed)
        && let Some(wakeup) = UI_WAKEUP.get()
    {
        wakeup();
    }
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
/// `false` means another instance answered or this process could not safely
/// acquire the guard; the caller must exit rather than start unguarded. `true`
/// means this process owns the slot.
#[cfg(unix)]
pub fn acquire() -> bool {
    let Some(dir) = dirs::config_dir() else {
        log::warn!("Prime Photos: kein Konfigurationsverzeichnis für den Einzelinstanz-Schutz");
        return false;
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
        log::warn!("Prime Photos: Instanz-Verzeichnis nicht anlegbar: {e}");
        return false;
    }

    // Acquire this before even attempting stale-socket cleanup. A contender
    // that cannot take the lock may activate the current owner, but must never
    // remove or rebind its socket.
    let Some(instance_lock) = acquire_unix_lock(path) else {
        return false;
    };

    let listener = match UnixListener::bind(path) {
        Ok(listener) => listener,
        Err(bind_error) => {
            // The path may belong to a running instance or be a socket file a
            // crash left behind. Only remove it after the exclusive sidecar
            // lock is ours and the existing instance did not answer.
            if ask_existing_instance(path) {
                return false;
            }
            log::debug!("Prime Photos: verwaister Instanz-Socket ({bind_error}), wird übernommen");
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    log::warn!("Prime Photos: verwaister Instanz-Socket nicht entfernbar: {error}");
                    return false;
                }
            }
            match UnixListener::bind(path) {
                Ok(listener) => listener,
                Err(error) => {
                    log::warn!("Prime Photos: Einzelinstanz-Schutz nicht verfügbar: {error}");
                    return false;
                }
            }
        }
    };

    if let Err(error) = spawn_accept_loop(listener) {
        // The socket was bound but no thread can serve it. Fail closed and
        // remove the path if possible; the lock is released as this returns.
        let _ = std::fs::remove_file(path);
        log::warn!("Prime Photos: Instanz-Thread nicht startbar: {error}");
        return false;
    }

    // Keep ownership in process-lifetime storage, not in the detached thread:
    // even if accepting later fails (or the thread panics), contenders remain
    // unable to remove or rebind this socket while this process is alive.
    retain_unix_lock(instance_lock);

    true
}

/// Open and exclusively lock the sidecar file (`instance.sock.lock`).
#[cfg(unix)]
fn acquire_unix_lock(path: &std::path::Path) -> Option<std::fs::File> {
    use std::fs::OpenOptions;

    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    let lock_file = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(std::path::PathBuf::from(lock_path))
    {
        Ok(file) => file,
        Err(error) => {
            if !ask_existing_instance(path) {
                log::warn!("Prime Photos: Einzelinstanz-Sperrdatei nicht verfügbar: {error}");
            }
            return None;
        }
    };

    match lock_file.try_lock() {
        Ok(()) => Some(lock_file),
        Err(error) => {
            if !ask_existing_instance(path) {
                log::warn!(
                    "Prime Photos: Einzelinstanz-Schutz bereits belegt oder nicht verfügbar: {error}"
                );
            }
            None
        }
    }
}

/// Retain the acquired sidecar lock until process exit. Recover the mutex's
/// inner value if another caller panicked while holding it; poisoning must not
/// release the single-instance guard.
#[cfg(unix)]
fn retain_unix_lock(lock_file: std::fs::File) {
    let locks = UNIX_INSTANCE_LOCKS.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(lock_file);
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
    request_show(UI_WAKEUP.get());
    let _ = stream.write_all(SHOW_ANSWER.as_bytes());
    let _ = stream.flush();
}

/// Record a show request and wake the UI if its host has registered a callback.
/// Kept separate from the process guard so both outcomes can be tested without
/// binding the user's configured socket or port.
fn request_show(wakeup: Option<&Wakeup>) {
    SHOW_REQUESTED.store(true, Ordering::Relaxed);
    if let Some(wakeup) = wakeup {
        wakeup();
    }
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

/// Accept connections until a non-interrupted listener error occurs.
#[cfg(unix)]
fn spawn_accept_loop(listener: std::os::unix::net::UnixListener) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("prime-photos-instance".into())
        .spawn(move || {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
                        serve(&mut stream);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        log::warn!("Prime Photos: Fehler beim Instanz-Listener: {error}");
                        return;
                    }
                };
            }
        })
        .map(|_| ())
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
