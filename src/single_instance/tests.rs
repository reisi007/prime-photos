use super::*;

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

#[cfg(test)]
#[test]
fn show_request_sets_flag_and_wakes_attached_ui() {
    use std::sync::atomic::AtomicUsize;

    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let wakeups = Arc::new(AtomicUsize::new(0));
    let wakeup: Wakeup = {
        let wakeups = wakeups.clone();
        Arc::new(move || {
            wakeups.fetch_add(1, Ordering::Relaxed);
        })
    };

    request_show(Some(&wakeup));

    assert!(take_show_request(), "der Show-Wunsch muss gesetzt sein");
    assert_eq!(wakeups.load(Ordering::Relaxed), 1, "der UI-Waker muss laufen");
}

#[cfg(test)]
#[test]
fn show_request_sets_flag_without_attached_ui() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    request_show(None);

    assert!(
        take_show_request(),
        "der Show-Wunsch muss auch ohne UI-Waker gesetzt sein"
    );
}
