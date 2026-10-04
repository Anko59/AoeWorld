use super::*;
mod capture;

#[test]
fn bounded_log_keeps_tail() {
    let mut log = BoundedLog::new();
    log.push(&vec![b'a'; LOG_LIMIT]);
    log.push(b"tail");
    let rendered = log.render();
    assert!(rendered.starts_with(b"[earlier output truncated]"));
    assert!(rendered.ends_with(b"tail"));
}

#[test]
fn real_exit_and_timeout_are_visible() {
    assert!(matches!(
        run("false", &[], Duration::from_secs(2)),
        Err(ProcessError::Exit { .. })
    ));
    assert!(matches!(
        run("sleep", &["2"], Duration::from_millis(10)),
        Err(ProcessError::Deadline { .. })
    ));
}

#[test]
fn supervised_child_receives_explicit_environment() {
    run_with_env(
        "sh",
        &["-c", "test \"$AOE_TEST_VALUE\" = expected"],
        &[("AOE_TEST_VALUE", "expected")],
        Duration::from_secs(2),
    )
    .expect("child environment");
}

#[test]
fn explicit_root_execution_uses_child_cwd_without_changing_parent() {
    let root = tempfile::tempdir().expect("child root");
    let parent = std::env::current_dir().expect("parent cwd");
    fs::write(root.path().join("marker"), "expected").expect("marker");
    run_in(
        root.path(),
        "sh",
        &[
            "-c",
            "test -f marker && test \"$AOE_TEST_VALUE\" = expected",
        ],
        &[("AOE_TEST_VALUE", "expected")],
        Duration::from_secs(2),
    )
    .expect("explicit cwd/environment");
    assert_eq!(std::env::current_dir().expect("unchanged cwd"), parent);
    let empty = tempfile::tempdir().expect("different root");
    assert!(matches!(
        run_in(
            empty.path(),
            "sh",
            &["-c", "test -f marker"],
            &[],
            Duration::from_secs(2)
        ),
        Err(ProcessError::Exit { .. })
    ));
}

#[test]
fn real_cancellation_stops_child_promptly() {
    let cancellation = Cancellation::default();
    let signal = cancellation.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        signal.cancel();
    });
    let start = Instant::now();
    assert!(matches!(
        run_cancellable("sleep", &["5"], Duration::from_secs(10), &cancellation),
        Err(ProcessError::Cancelled { .. })
    ));
    worker.join().expect("cancellation worker");
    assert!(start.elapsed() < Duration::from_secs(2));
}
