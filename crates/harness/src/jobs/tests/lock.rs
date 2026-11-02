use super::super::lock::{self, LongLock};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct TestDir(std::path::PathBuf);
impl TestDir {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tempdir() -> TestDir {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("noqueue-lock-test-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    TestDir(path)
}

pub(crate) static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn holder(command: &str) -> String {
    format!("`{command}` on feature in /work/feature (started 2026-10-13 14:00:00Z)")
}

#[test]
fn long_contention_refuses_immediately_with_the_complete_holder() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempdir();
    let path = temp.path().join("long.lock");
    let first =
        LongLock::acquire(&path, &holder("make coverage"), "test-owner-proof", None).unwrap();
    let begun = Instant::now();
    let error =
        LongLock::acquire(&path, &holder("make perf-full"), "test-owner-proof", None).unwrap_err();
    assert!(begun.elapsed() < Duration::from_secs(1));
    assert_eq!(lock::exit_code(error.as_ref()), 75);
    let message = error.to_string();
    for expected in [
        "busy:",
        "make coverage",
        "feature",
        "/work/feature",
        "2026-10-13 14:00:00Z",
        "retry later",
    ] {
        assert!(message.contains(expected), "{message}");
    }
    drop(first);
    LongLock::acquire(&path, &holder("next"), "test-owner-proof", None).unwrap();
}

#[test]
fn nested_jobs_accept_only_a_live_owners_proof() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempdir();
    let path = temp.path().join("long.lock");
    let first = LongLock::acquire(&path, &holder("outer"), "test-owner-proof", None).unwrap();
    let token = first.token().to_owned();
    assert!(lock::verify(&path, &token).unwrap());
    assert!(!lock::verify(&path, "forged").unwrap());
    let nested =
        LongLock::acquire(&path, &holder("nested"), "new-owner-proof", Some(&token)).unwrap();
    assert!(!nested.owned());
    assert!(
        LongLock::acquire(&path, &holder("forged"), "new-owner-proof", Some("forged")).is_err()
    );
    drop(nested);
    assert!(LongLock::acquire(&path, &holder("other"), "test-owner-proof", None).is_err());
    drop(first);
    assert!(!lock::verify(&path, &token).unwrap());
    let new = LongLock::acquire(&path, &holder("new"), "new-owner-proof", Some(&token)).unwrap();
    assert!(new.owned(), "a stale token cannot bypass acquisition");
}

#[test]
fn long_lock_child() {
    let Ok(path) = std::env::var("AOE_TEST_LONG_LOCK") else {
        return;
    };
    let _held =
        LongLock::acquire(Path::new(&path), &holder("child"), "test-owner-proof", None).unwrap();
    let descendant = std::env::var_os("AOE_TEST_LONG_DESCENDANT").map(|_| {
        let mut command = Command::new("sleep");
        command.arg("30").stdout(Stdio::null());
        _held.inherit_to(&mut command).unwrap();
        command.spawn().unwrap()
    });
    println!("HELD {}", descendant.as_ref().map_or(0, |child| child.id()));
    thread::sleep(Duration::from_secs(60));
}

fn child(path: &Path, descendant: bool) -> (std::process::Child, u32) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    let name = format!(
        "{}::long_lock_child",
        module_path!().split_once("::").unwrap().1
    );
    command
        .args(["--exact", &name, "--nocapture"])
        .env("AOE_TEST_LONG_LOCK", path)
        .stdout(Stdio::piped());
    if descendant {
        command.env("AOE_TEST_LONG_DESCENDANT", "1");
    }
    let mut child = command.spawn().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let pid = loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 {
            let status = child.wait().unwrap();
            panic!("child failed before locking: {status}");
        }
        if let Some(pid) = line.trim().strip_prefix("HELD ") {
            break pid.parse().unwrap();
        }
    };
    (child, pid)
}

#[test]
fn kernel_releases_a_killed_holder_without_a_stale_ticket() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempdir();
    let path = temp.path().join("long.lock");
    let (mut child, _) = child(&path, false);
    assert_eq!(lock::status(&path).unwrap().unwrap(), holder("child"));
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(lock::status(&path).unwrap().is_none());
    LongLock::acquire(&path, &holder("next"), "test-owner-proof", None).unwrap();
}

#[test]
fn an_exec_child_retains_the_kernel_lock_after_its_parent_is_killed() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempdir();
    let path = temp.path().join("long.lock");
    let (mut parent, descendant) = child(&path, true);
    parent.kill().unwrap();
    parent.wait().unwrap();
    let busy = LongLock::acquire(&path, &holder("other"), "test-owner-proof", None);
    let _ = Command::new("kill")
        .args(["-KILL", &descendant.to_string()])
        .status();
    assert!(
        busy.is_err(),
        "an inherited descriptor must keep the lock alive"
    );
    let begun = Instant::now();
    while lock::status(&path).unwrap().is_some() {
        assert!(begun.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(10));
    }
    LongLock::acquire(&path, &holder("next"), "test-owner-proof", None).unwrap();
}

#[test]
fn intended_exec_children_retain_the_lock_after_normal_guard_drop() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempdir();
    let path = temp.path().join("long.lock");
    let held = LongLock::acquire(&path, &holder("parent"), "proof", None).unwrap();
    let mut command = Command::new("sleep");
    command.arg("30");
    held.inherit_to(&mut command).unwrap();
    let mut child = command.spawn().unwrap();
    drop(command);
    drop(held);
    let blocked = LongLock::acquire(&path, &holder("other"), "other", None);
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(
        blocked.is_err(),
        "the intended child still owns the descriptor"
    );
    LongLock::acquire(&path, &holder("next"), "next", None).unwrap();
}

#[test]
fn unrelated_exec_children_do_not_retain_another_scopes_lock() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempdir();
    let path = temp.path().join("long.lock");
    let held = LongLock::acquire(&path, &holder("parent"), "proof", None).unwrap();
    let mut unrelated = Command::new("sleep").arg("30").spawn().unwrap();
    drop(held);
    let next = LongLock::acquire(&path, &holder("next"), "next", None);
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
    assert!(
        next.is_ok(),
        "an unrelated exec must not acquire an inheritable descriptor"
    );
}
