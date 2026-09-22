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
    let error = execute_with_deadline(
        &path,
        b"{}".to_vec(),
        &AtomicBool::new(false),
        Duration::from_secs(5),
    )
    .unwrap_err();
    assert!(error.contains("output exceeds"), "{error}");
    let (_directory, path) = fixture("cat >/dev/null\nhead -c 65536 /dev/zero >&2\nsleep 30");
    let error = execute_with_deadline(
        &path,
        b"{}".to_vec(),
        &AtomicBool::new(false),
        Duration::from_secs(5),
    )
    .unwrap_err();
    assert!(error.contains("output exceeds"), "{error}");
}

#[cfg(unix)]
#[test]
fn unread_input_can_be_cancelled_and_descendant_pipes_are_closed() {
    let (_directory, path) = fixture("sleep 30 &\nwait");
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let cancellation = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        signal.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let error = execute_with_deadline(
        &path,
        vec![0; MAX_REQUEST_BYTES],
        &cancelled,
        Duration::from_secs(5),
    )
    .unwrap_err();
    cancellation.join().unwrap();
    assert!(error.contains("cancelled"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
#[test]
fn deadline_and_success_both_clean_up_inherited_output_pipes() {
    let (_directory, path) = fixture("sleep 30 &\nwait");
    let started = Instant::now();
    let error = execute_with_deadline(
        &path,
        Vec::new(),
        &AtomicBool::new(false),
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(error.contains("deadline"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    let (_directory, path) = fixture("cat >/dev/null\nsleep 30 &\nprintf '{}'");
    let started = Instant::now();
    assert_eq!(
        execute_with_deadline(
            &path,
            b"request".to_vec(),
            &AtomicBool::new(false),
            Duration::from_secs(5)
        )
        .unwrap(),
        b"{}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
#[test]
fn request_bounds_and_initial_cancellation_prevent_spawn() {
    let missing = Path::new("/no-such-map-worker");
    assert!(
        execute(
            missing,
            vec![0; MAX_REQUEST_BYTES + 1],
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("request exceeds")
    );
    assert!(
        execute(missing, Vec::new(), &AtomicBool::new(true))
            .unwrap_err()
            .contains("cancelled")
    );
}
