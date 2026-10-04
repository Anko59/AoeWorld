use super::*;
use std::cell::Cell;

struct FakeClock(Cell<Duration>);
impl Clock for FakeClock {
    fn elapsed(&self) -> Duration {
        self.0.get()
    }
    fn sleep(&self, duration: Duration) {
        self.0.set(self.0.get() + duration);
    }
}
struct FakeProcess {
    polls: usize,
    exit_after: usize,
    terminated: bool,
}
impl ProcessHandle for FakeProcess {
    fn poll(&mut self) -> io::Result<Option<ExitState>> {
        self.polls += 1;
        Ok((self.polls >= self.exit_after).then_some(ExitState {
            success: true,
            code: Some(0),
        }))
    }
    fn terminate(&mut self) -> io::Result<()> {
        self.terminated = true;
        Ok(())
    }
}

#[test]
fn fake_deadline_and_cancellation_terminate() {
    let clock = FakeClock(Cell::new(Duration::ZERO));
    let mut process = FakeProcess {
        polls: 0,
        exit_after: usize::MAX,
        terminated: false,
    };
    assert!(matches!(
        wait_loop(
            &clock,
            &mut process,
            Duration::from_millis(50),
            &Cancellation::default()
        )
        .expect("wait"),
        Outcome::Deadline
    ));
    assert!(process.terminated);
    let cancel = Cancellation::default();
    cancel.cancel();
    let mut process = FakeProcess {
        polls: 0,
        exit_after: usize::MAX,
        terminated: false,
    };
    assert!(matches!(
        wait_loop(&clock, &mut process, Duration::from_secs(1), &cancel).expect("wait"),
        Outcome::Cancelled
    ));
    assert!(process.terminated);
}

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
