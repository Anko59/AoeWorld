use super::*;

#[test]
fn output_is_bounded_while_excess_bytes_are_drained() {
    let overflow = AtomicBool::new(false);
    assert_eq!(read_bounded(&b"abcdef"[..], 3, &overflow).unwrap(), b"abc");
    assert!(overflow.load(Ordering::Acquire));
    let overflow = AtomicBool::new(false);
    assert_eq!(read_bounded(&b"abc"[..], 3, &overflow).unwrap(), b"abc");
    assert!(!overflow.load(Ordering::Acquire));
}

/// No test waits this long: each run ends on the event it names, so the load of
/// the machine cannot turn it into a deadline error.
#[cfg(unix)]
const PATIENT: Duration = Duration::from_secs(3600);

/// A worker that leaves `sleep 3600` running, then says so by creating `started`
/// beside itself; the sleeper holds every pipe the worker was given.
#[cfg(unix)]
const LEAVES_A_SLEEPER: &str = "sleep 3600 &\ntouch \"$(dirname \"$0\")/started\"\nwait";

#[cfg(unix)]
fn fixture(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("worker");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (directory, path)
}

#[cfg(unix)]
#[test]
fn verbose_worker_fails_without_filling_a_pipe_forever() {
    let (_directory, path) = fixture("cat >/dev/null\nhead -c 1048576 /dev/zero\nsleep 30");
    let error =
        execute_with_deadline(&path, b"{}".to_vec(), &AtomicBool::new(false), PATIENT).unwrap_err();
    assert!(error.contains("output exceeds"), "{error}");
    let (_directory, path) = fixture("cat >/dev/null\nhead -c 65536 /dev/zero >&2\nsleep 30");
    let error =
        execute_with_deadline(&path, b"{}".to_vec(), &AtomicBool::new(false), PATIENT).unwrap_err();
    assert!(error.contains("output exceeds"), "{error}");
}

#[cfg(unix)]
#[test]
fn unread_input_can_be_cancelled_and_descendant_pipes_are_closed() {
    let (directory, path) = fixture(LEAVES_A_SLEEPER);
    let started = directory.path().join("started");
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    // Cancel once the sleeper runs, not after a guess of how long that takes.
    let cancellation = thread::spawn(move || {
        while !started.exists() {
            thread::yield_now();
        }
        signal.store(true, Ordering::Release);
    });
    // The sleeper holds the pipes for an hour: returning at all shows they were
    // closed (a call that waited for them would hang, not pass).
    let error =
        execute_with_deadline(&path, vec![0; MAX_REQUEST_BYTES], &cancelled, PATIENT).unwrap_err();
    cancellation.join().unwrap();
    assert!(error.contains("cancelled"), "{error}");
}

#[cfg(unix)]
#[test]
fn deadline_and_success_both_clean_up_inherited_output_pipes() {
    let (directory, path) = fixture(LEAVES_A_SLEEPER);
    let started = directory.path().join("started");
    // The deadline is already past; the observer holds the first look at it back
    // until the sleeper runs, so the deadline always meets a live sleeper.
    let error = execute_observed(
        &path,
        Vec::new(),
        &AtomicBool::new(false),
        Duration::ZERO,
        || {
            while !started.exists() {
                thread::yield_now();
            }
        },
    )
    .unwrap_err();
    assert!(error.contains("deadline"), "{error}");
    let (_directory, path) = fixture("cat >/dev/null\nsleep 3600 &\nprintf '{}'");
    assert_eq!(
        execute_with_deadline(&path, b"request".to_vec(), &AtomicBool::new(false), PATIENT)
            .unwrap(),
        b"{}"
    );
}

#[cfg(unix)]
#[test]
fn request_bounds_and_initial_cancellation_prevent_spawn() {
    let missing = Path::new("/no-such-map-worker");
    assert!(
        execute(
            missing,
            vec![0; MAX_REQUEST_BYTES + 1],
            &AtomicBool::new(false),
            || {}
        )
        .unwrap_err()
        .contains("request exceeds")
    );
    assert!(
        execute(missing, Vec::new(), &AtomicBool::new(true), || {})
            .unwrap_err()
            .contains("cancelled")
    );
}
