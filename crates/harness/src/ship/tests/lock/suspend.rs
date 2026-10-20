//! A ship lock held across a host suspend. The holder, the waiter and the gate's child are real
//! processes: the test binary runs itself in a role (as in `priority`). The suspend is
//! simulated by writing seconds into the file that `process::suspend::read` adds to the boot
//! clock in every process; the processes read it themselves, so nothing is injected into the
//! lock or the capture loop. The holder runs a gate through `run_gates_retrying`, the path
//! `make ship` takes.
use crate::{
    gates::registry::Gate,
    process::suspend::SIMULATED_GAP_FILE,
    ship::{
        lock::{self, Holder},
        run::{GateVerdict, Retry, run_gates_retrying},
    },
    slots::SlotEnv,
};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const ROLE: &str = "AOE_TEST_SUSPEND_ROLE";
const DIRECTORY: &str = "AOE_TEST_SUSPEND_DIR";
const PATIENCE: Duration = Duration::from_secs(60);
const NEVER: Duration = Duration::from_secs(3600);
/// Every run of the gate appends a line to `count`, then works for two seconds.
const GATE: &str = "echo run >> count; sleep 2";

fn holder(branch: &str) -> Holder {
    Holder {
        worktree: format!("/work/{branch}"),
        branch: branch.into(),
        command: "make ship".into(),
        started: 1_791_900_000,
        returning: false,
        pid: 0,
        pid_started: None,
        gap: None,
    }
}

fn lock_path(dir: &Path) -> PathBuf {
    dir.join("aoe-ship/ship.lock")
}

/// The role this process was started in, run by `role` below; a plain test run does nothing.
#[test]
fn role() {
    let (Ok(role), Ok(dir)) = (std::env::var(ROLE), std::env::var(DIRECTORY)) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let path = lock_path(&dir);
    let take = |name: &str, wait| {
        lock::acquire_at(
            &path,
            &holder(name),
            wait,
            Duration::from_millis(50),
            NEVER,
            &mut |line| println!("SAID {line}"),
        )
        .expect("the lock")
    };
    match role.as_str() {
        "holder" | "small-suspends" => {
            let _lock = take("holder", Duration::ZERO);
            println!("HELD");
            let gate: Gate = serde_json::from_value(serde_json::json!({
                "id": "work", "command": GATE, "requires": [], "select": "fixture",
                "evidence": "exit status", "suites": ["static"], "cadences": ["preflight"],
                "budget_s": 60, "static": false, "heavy": false, "capabilities": [], "blocks": []
            }))
            .unwrap();
            let mut first = gate.clone();
            first.id = "first".into();
            first.command = "echo run >> count; while [ ! -f next ]; do sleep 0.02; done".into();
            let gates = if role == "small-suspends" {
                vec![&first, &gate]
            } else {
                vec![&gate]
            };
            let results = run_gates_retrying(
                &dir,
                &gates,
                &SlotEnv::default(),
                crate::slots::hold,
                &Retry::never(),
            );
            assert!(results.iter().all(|r| r.verdict == GateVerdict::Pass));
            assert_eq!(
                results[0].verdict,
                GateVerdict::Pass,
                "{}",
                results[0].line()
            );
            println!("RESULT PASS");
        }
        "stuck" => {
            // Holds the lock and runs no supervised child, so nothing refreshes its record.
            let _lock = take("stuck", Duration::ZERO);
            println!("HELD");
            thread::sleep(Duration::from_secs(600));
        }
        _ => {
            let _lock = take("waiter", PATIENCE);
            let record = crate::slots::queue::read_json::<Holder>(&path).expect("record");
            println!("GAP {}", record.gap.expect("a gap"));
            println!("ACQUIRED");
        }
    }
}

struct Role {
    child: Child,
    lines: Receiver<String>,
    seen: Vec<String>,
}

fn start(role: &str, dir: &Path) -> Role {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    if role == "holder" || role == "small-suspends" {
        fs::create_dir_all(&bin).unwrap();
        // Recovery exercises the real Docker command path, against an empty fixture daemon.
        fs::write(bin.join("docker"), "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(bin.join("docker"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut child = Command::new(std::env::current_exe().expect("test binary"))
        .args(["ship::tests::lock::suspend::role", "--exact", "--nocapture"])
        .env(ROLE, role)
        .env(DIRECTORY, dir)
        .env(SIMULATED_GAP_FILE, dir.join("gap"))
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn a role");
    let stdout = child.stdout.take().unwrap();
    let (send, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    Role {
        child,
        lines,
        seen: Vec::new(),
    }
}

impl Role {
    fn expect(&mut self, text: &str) {
        let until = Instant::now() + PATIENCE;
        while Instant::now() < until {
            if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(100)) {
                self.seen.push(line.clone());
                if line.contains(text) {
                    return;
                }
            }
        }
        panic!("no line with {text:?}; saw {:?}", self.seen);
    }

    fn expect_seen(&self, text: &str) {
        assert!(
            self.seen.iter().any(|l| l.contains(text)),
            "no {text:?} in {:?}",
            self.seen
        );
    }

    fn finish(&mut self) -> ExitStatus {
        let status = self.child.wait().expect("wait");
        self.seen.extend(self.lines.try_iter());
        status
    }
}

impl Drop for Role {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn runs(dir: &Path) -> usize {
    fs::read_to_string(dir.join("count")).map_or(0, |text| text.lines().count())
}

fn wait_for(what: &str, ready: impl Fn() -> bool) {
    let until = Instant::now() + PATIENCE;
    while !ready() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn suspend(dir: &Path) {
    fs::write(dir.join("gap"), "300").unwrap();
}

#[test]
fn two_small_suspends_across_captures_do_not_end_a_recovering_holder() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("gap"), "0").unwrap();
    let mut holder = start("small-suspends", dir.path());
    holder.expect("HELD");
    let mut waiter = start("waiter", dir.path());
    waiter.expect("waiting for holder in /work/holder");
    wait_for("gate A", || runs(dir.path()) == 1);
    fs::write(dir.path().join("gap"), "40").unwrap();
    fs::write(dir.path().join("next"), "").unwrap();
    wait_for("gate B", || runs(dir.path()) == 2);
    fs::write(dir.path().join("gap"), "80").unwrap();
    assert!(
        holder.finish().success(),
        "healthy holder was killed: {:?}",
        holder.seen
    );
    holder.expect_seen("RESULT PASS");
    assert_eq!(
        runs(dir.path()),
        2,
        "neither 40s suspend interrupts its capture"
    );
    waiter.expect("ACQUIRED");
    assert!(waiter.finish().success());
    assert!(
        !waiter.seen.iter().any(|line| line.contains("taking over")),
        "{:?}",
        waiter.seen
    );
    writeln!(std::io::stdout(), "F1: two 40s sleeps across gates A/B; holder PASS, two gate runs, waiter acquired after release without takeover").unwrap();
}

#[test]
fn a_holder_that_recovers_from_a_suspend_is_not_taken_over_and_the_waiter_waits_for_it() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("gap"), "0").unwrap();
    let mut holder = start("holder", dir.path());
    holder.expect("HELD");
    let mut waiter = start("waiter", dir.path());
    waiter.expect("waiting for holder in /work/holder");
    wait_for("the gate's first run", || runs(dir.path()) == 1);

    suspend(dir.path());
    // The holder notices, records the new gap and runs the gate again, for two seconds; the
    // waiter polls every 50 ms throughout and must neither signal nor take over.
    assert!(
        holder.finish().success(),
        "the holder was ended: {:?}",
        holder.seen
    );
    holder.expect_seen("RESULT PASS");
    assert_eq!(runs(dir.path()), 2, "the gate ran again after the suspend");

    waiter.expect("ACQUIRED");
    assert!(waiter.finish().success());
    let taken = waiter.seen.iter().any(|line| line.contains("taking over"));
    assert!(
        !taken,
        "the waiter took over a live holder: {:?}",
        waiter.seen
    );
}

#[test]
fn a_holder_frozen_by_a_suspend_is_taken_over_and_named() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("gap"), "0").unwrap();
    let mut stuck = start("stuck", dir.path());
    stuck.expect("HELD");
    let mut waiter = start("waiter", dir.path());
    waiter.expect("waiting for stuck in /work/stuck");

    suspend(dir.path());
    waiter.expect("ACQUIRED");
    let line = waiter
        .seen
        .iter()
        .find(|line| line.contains("taking over"))
        .expect("the takeover is announced");
    assert!(line.contains("stuck in /work/stuck"), "{line}");
    assert!(line.contains("suspended"), "{line}");
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(stuck.finish().signal(), Some(9), "the holder was killed");
}

#[test]
fn the_gap_is_read_when_the_lock_is_taken_not_when_the_waiter_queued() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("gap"), "0").unwrap();
    let path = lock_path(dir.path());
    let first = lock::acquire_at(
        &path,
        &holder("first"),
        Duration::ZERO,
        Duration::from_millis(20),
        NEVER,
        &mut |_| {},
    )
    .expect("free lock");
    let mut second = start("waiter", dir.path());
    second.expect("waiting for first in /work/first");
    suspend(dir.path());
    drop(first);
    second.expect("ACQUIRED");
    let line = second.seen.iter().find(|l| l.starts_with("GAP ")).unwrap();
    let gap: u64 = line["GAP ".len()..].parse().unwrap();
    assert!(
        gap >= 300,
        "the record has the gap before the suspend: {gap}"
    );
}
