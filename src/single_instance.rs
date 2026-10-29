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
//! sets an `AtomicBool` and wakes the render loop, exactly like `src/tray.rs`
//! and `src/app_menu.rs`. `PrimePhotoApp::update` drains the request through
//! [`take_show_request`] and shows the (possibly hidden) window.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Unix ownership locks outlive the detached accept thread. If that thread
/// exits or panics while the app remains alive, another process must still not
/// take over the socket path.
#[cfg(unix)]
static UNIX_INSTANCE_LOCKS: OnceLock<std::sync::Mutex<Vec<std::fs::File>>> = OnceLock::new();

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

/// Serialises the two tests: `SHOW_REQUESTED` is a process-global flag, so two
/// tests draining it in parallel could steal each other's request. Poisoning is
/// ignored — a failed test must not fail the other one.
#[cfg(test)]
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Make a fresh directory for each Unix test. The test's listener and process-
/// lifetime sidecar lock intentionally remain alive until the test process
/// exits, so neither path may be reused or unlinked during the test run.
#[cfg(all(test, unix))]
fn unique_unix_test_socket() -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_PATH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    loop {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros()
            % 1_000_000;
        let sequence = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("pp-{}-{timestamp}-{sequence}", std::process::id()));
        match std::fs::create_dir(&dir) {
            Ok(()) => return dir.join("sock"),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("Temp-Verzeichnis anlegen: {error}"),
        }
    }
}

#[cfg(all(test, unix))]
#[test]
fn second_instance_defers_and_the_first_shows_its_window() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let path = unique_unix_test_socket();

    assert!(acquire_at(&path), "der erste Start muss primär werden");
    assert!(!acquire_at(&path), "der zweite Start muss sich fügen");
    assert!(
        take_show_request(),
        "die laufende Instanz muss den Show-Wunsch sehen"
    );

    // Keep the socket and locked sidecar in this unique directory. The accept
    // thread and ownership lock live until this test process exits.
}

#[cfg(all(test, unix))]
#[test]
fn concurrent_stale_socket_takeover_has_exactly_one_owner() {
    use std::os::unix::net::UnixListener;
    use std::sync::Barrier;

    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let path = unique_unix_test_socket();

    // Leave behind the filesystem entry from a listener that crashed.
    drop(UnixListener::bind(&path).expect("stale socket anlegen"));
    assert!(
        path.exists(),
        "der verwaiste Socket-Pfad muss bestehen bleiben"
    );

    let barrier = std::sync::Arc::new(Barrier::new(3));
    let contenders = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                acquire_at(&path)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();

    let owners = contenders
        .into_iter()
        .map(|contender| contender.join().expect("contender panicked"))
        .filter(|owns_guard| *owns_guard)
        .count();
    assert_eq!(owners, 1, "genau ein Contender darf Guard-Eigentümer sein");

    // Even when the loser could not handshake during recovery and controlled-
    // rejected, it must not have removed/rebound the eventual owner's socket.
    assert!(
        ask_existing_instance(&path),
        "der Gewinner-Socket muss nach dem Rennen erreichbar bleiben"
    );
    assert!(
        take_show_request(),
        "der Gewinner muss mindestens eine Show-Anfrage erhalten"
    );

    // Keep the socket and locked sidecar in this unique directory. The accept
    // thread and ownership lock live until this test process exits.
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
