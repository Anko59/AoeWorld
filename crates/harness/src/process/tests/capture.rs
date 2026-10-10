use super::*;

fn shell(root: &Path, script: &str, deadline: Duration, cancellation: &Cancellation) -> Captured {
    capture_in(root, "sh", &["-c", script], &[], deadline, cancellation)
}

#[test]
fn pre_cancelled_or_zero_budget_commands_never_spawn_even_fast_success() {
    let cancelled_root = tempfile::tempdir().unwrap();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let captured = shell(
        cancelled_root.path(),
        "printf executed > marker",
        Duration::from_secs(2),
        &cancelled,
    );
    assert!(matches!(captured.exit, CaptureExit::Cancelled));
    assert!(!cancelled_root.path().join("marker").exists());
    assert!(captured.stdout.is_empty() && captured.stderr.is_empty());
    let expired_root = tempfile::tempdir().unwrap();
    let captured = shell(
        expired_root.path(),
        "printf executed > marker",
        Duration::ZERO,
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Deadline));
    assert!(!expired_root.path().join("marker").exists());
    let positive_root = tempfile::tempdir().unwrap();
    let captured = shell(
        positive_root.path(),
        "printf executed > marker",
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Success));
    assert_eq!(
        fs::read(positive_root.path().join("marker")).unwrap(),
        b"executed"
    );
}

#[test]
fn successful_and_failed_captures_preserve_both_streams_and_codes() {
    let root = tempfile::tempdir().unwrap();
    let cancellation = Cancellation::default();
    let captured = shell(
        root.path(),
        "printf success; printf diagnostic >&2",
        Duration::from_secs(2),
        &cancellation,
    );
    assert!(matches!(captured.exit, CaptureExit::Success));
    assert_eq!(captured.stdout, b"success");
    assert_eq!(captured.stderr, b"diagnostic");
    assert!(!captured.truncated);
    assert!(captured.duration < Duration::from_secs(2));
    assert_eq!(
        fs::read_dir(root.path()).unwrap().count(),
        0,
        "capture never writes evidence into candidate root"
    );
    let captured = shell(
        root.path(),
        "printf output; printf failed >&2; exit 7",
        Duration::from_secs(2),
        &cancellation,
    );
    assert!(matches!(captured.exit, CaptureExit::Failed(Some(7))));
    assert_eq!(captured.stdout, b"output");
    assert_eq!(captured.stderr, b"failed");
    assert!(!captured.truncated);
}

#[test]
fn capture_explicit_environment_and_root_have_negative_controls() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("marker"), "context").unwrap();
    let parent = std::env::current_dir().unwrap();
    let captured = capture_in(
        root.path(),
        "sh",
        &[
            "-c",
            "test -f marker && printf '%s' \"$CAPTURE_TEST_VALUE\"",
        ],
        &[("CAPTURE_TEST_VALUE", "expected")],
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Success));
    assert_eq!(captured.stdout, b"expected");
    assert_eq!(std::env::current_dir().unwrap(), parent);
    let other = tempfile::tempdir().unwrap();
    let captured = shell(
        other.path(),
        "test -f marker",
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Failed(Some(1))));
    let captured = capture_in(
        root.path(),
        "/nonexistent/aoe-capture-executable",
        &[],
        &[],
        Duration::from_secs(1),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Start(_)));
    assert!(captured.stdout.is_empty() && captured.stderr.is_empty());
}

#[test]
fn capture_timeout_and_cancellation_keep_partial_output() {
    let root = tempfile::tempdir().unwrap();
    let captured = shell(
        root.path(),
        "printf before; exec /bin/sleep 5",
        Duration::from_secs(1),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Deadline));
    assert_eq!(captured.stdout, b"before");
    assert!(captured.duration < Duration::from_secs(2));
    let cancellation = Cancellation::default();
    let signal = cancellation.clone();
    let ready = root.path().join("partial-ready");
    let ready_for_worker = ready.clone();
    let worker = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !ready_for_worker.exists() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            ready_for_worker.exists(),
            "child did not emit partial output"
        );
        signal.cancel();
    });
    let script = format!(
        "printf partial; : > '{}'; exec /bin/sleep 5",
        ready.display()
    );
    let captured = shell(root.path(), &script, Duration::from_secs(3), &cancellation);
    worker.join().unwrap();
    assert!(matches!(captured.exit, CaptureExit::Cancelled));
    assert_eq!(captured.stdout, b"partial");
    assert!(captured.duration < Duration::from_secs(3));
}

#[test]
fn capture_retains_bounded_raw_tails_and_marks_truncation() {
    let root = tempfile::tempdir().unwrap();
    let captured = shell(
        root.path(),
        "i=0; while test $i -lt 18000; do printf 12345678; i=$((i+1)); done; printf final",
        Duration::from_secs(5),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Success));
    assert_eq!(captured.stdout.len(), LOG_LIMIT);
    assert!(captured.stdout.ends_with(b"final"));
    assert!(captured.truncated);
    let observation = safe_observation(&captured);
    assert!(observation.truncated);
    assert_eq!(observation.stdout.bytes, LOG_LIMIT);
    assert_eq!(
        observation.stdout.raw_blake3,
        blake3::hash(&captured.stdout).to_hex().to_string()
    );
    assert_eq!(observation.stderr.bytes, captured.stderr.len());
    assert_eq!(observation.duration_ms, captured.duration.as_millis());
    assert!(
        !captured.stdout.starts_with(b"[earlier output truncated]"),
        "receipt is raw bytes, not decorated legacy logs"
    );
}

#[cfg(unix)]
#[test]
fn escaped_pipe_holder_cannot_block_drain_or_leave_reader_threads() {
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::{Pid, getpgid},
    };
    let root = tempfile::tempdir().unwrap();
    // setsid escapes the supervised process group; its stdout stays inherited.
    // The parent prints the fixture PID ($!) before exiting: a child that
    // printed it itself could lose the race with the drain under load. A
    // background job is not a group leader, so setsid execs in place and the
    // PID is also the new session's group.
    let captured = shell(
        root.path(),
        "setsid sh -c 'sleep 5' & echo $!; sleep 0.05",
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    let elapsed = captured.duration;
    let observation = safe_observation(&captured);
    let stdout_len = captured.stdout.len();
    let stdout_hash = blake3::hash(&captured.stdout).to_hex().to_string();
    let pid: i32 = String::from_utf8(captured.stdout)
        .unwrap()
        .trim()
        .parse()
        .expect("owned fixture session PID");
    let pid = Pid::from_raw(pid);
    // Cleanup only our still-live fixture session, never the caller group.
    if getpgid(Some(pid)).ok() == Some(pid) {
        let _ = killpg(pid, Signal::SIGKILL);
    }
    assert_eq!(observation.truncated, captured.truncated);
    assert_eq!(observation.stdout.bytes, stdout_len);
    assert_eq!(observation.stdout.raw_blake3, stdout_hash);
    assert_eq!(observation.stderr.bytes, captured.stderr.len());
    assert_eq!(observation.duration_ms, elapsed.as_millis());
    assert!(observation.stdout.bytes < LOG_LIMIT && observation.stderr.bytes < LOG_LIMIT);
    assert!(matches!(captured.exit, CaptureExit::Success));
    assert!(
        captured.truncated,
        "unfinished inherited pipes must be reported"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "reader drain was not bounded: {elapsed:?}"
    );
}
