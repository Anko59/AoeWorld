use super::run;
use crate::ship::lock::{self, Holder};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(20);
const NEVER: Duration = Duration::from_secs(3600);

fn holder(branch: &str) -> Holder {
    Holder {
        worktree: format!("/work/{branch}"),
        branch: branch.into(),
        command: "make ship".into(),
        started: 1_791_900_000,
    }
}

fn take(path: &Path, branch: &str, wait: Duration) -> crate::ship::Result<lock::ShipLock> {
    lock::acquire_at(path, &holder(branch), wait, POLL, NEVER, &mut |_| {})
}

fn lock_file() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("aoe-ship/ship.lock");
    (temp, path)
}

#[test]
fn a_second_acquirer_waits_and_proceeds_when_the_first_releases() {
    let (_temp, path) = lock_file();
    let first = take(&path, "first", Duration::ZERO).expect("free lock");
    let (said, lines) = mpsc::channel();
    let waiting = {
        let path = path.clone();
        thread::spawn(move || {
            lock::acquire_at(
                &path,
                &holder("second"),
                Duration::from_secs(30),
                POLL,
                NEVER,
                &mut |line| said.send(line.to_owned()).unwrap(),
            )
            .map_err(|e| e.to_string())
            .map(|_| ())
        })
    };
    let announced = lines.recv_timeout(Duration::from_secs(10)).expect("a line");
    assert_eq!(
        announced,
        "ship: waiting for first in /work/first (started 2026-10-13 14:00:00Z)"
    );
    assert!(
        !waiting.is_finished(),
        "it must wait while the lock is held"
    );
    drop(first);
    waiting.join().unwrap().expect("proceeds once released");
}

#[test]
fn without_a_wait_it_fails_at_once_and_names_the_holder() {
    let (_temp, path) = lock_file();
    let _first = take(&path, "first", Duration::ZERO).expect("free lock");
    let begun = Instant::now();
    let error = take(&path, "second", Duration::ZERO)
        .expect_err("held")
        .to_string();
    assert!(begun.elapsed() < Duration::from_secs(2), "{error}");
    assert!(error.contains("first in /work/first"), "{error}");
    assert!(error.contains("SHIP_LOCK_WAIT_MINUTES"), "{error}");
}

#[test]
fn the_wait_is_bounded_and_repeats_a_line_while_waiting() {
    let (_temp, path) = lock_file();
    let _first = take(&path, "first", Duration::ZERO).expect("free lock");
    let mut lines = Vec::new();
    let begun = Instant::now();
    let result = lock::acquire_at(
        &path,
        &holder("second"),
        Duration::from_millis(400),
        POLL,
        Duration::from_millis(100),
        &mut |line| lines.push(line.to_owned()),
    );
    assert!(result.is_err());
    assert!(begun.elapsed() < Duration::from_secs(10));
    assert!(lines[0].starts_with("ship: waiting for first in /work/first"));
    assert!(lines.iter().any(|l| l.starts_with("ship: still waiting")));
}

/// Re-run this test binary as a child that takes the lock and sleeps.
#[test]
fn lock_holder_child() {
    let Ok(path) = std::env::var("AOE_TEST_LOCK_HOLDER") else {
        return;
    };
    let _held = take(Path::new(&path), "child", Duration::ZERO).expect("child lock");
    println!("HELD");
    thread::sleep(Duration::from_secs(600));
}

#[test]
fn a_killed_holder_leaves_the_lock_free() {
    let (_temp, path) = lock_file();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ship::tests::lock::lock_holder_child",
            "--nocapture",
        ])
        .env("AOE_TEST_LOCK_HOLDER", &path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("child");
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    while line.trim() != "HELD" {
        line.clear();
        assert!(out.read_line(&mut line).unwrap() > 0, "child never locked");
    }
    let held = lock::status(&path).unwrap().expect("held by the child");
    assert_eq!(held.branch, "child");
    take(&path, "other", Duration::ZERO).expect_err("child holds it");
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(
        lock::status(&path).unwrap().is_none(),
        "kill frees the lock"
    );
    take(&path, "other", Duration::ZERO).expect("free after the holder was killed");
}

#[test]
fn two_worktrees_of_one_repository_contend_on_one_lock() {
    let temp = tempfile::tempdir().expect("temp");
    let main = temp.path().join("main");
    let other = temp.path().join("other");
    run(
        temp.path(),
        &["init", "-q", "-b", "dev", main.to_str().unwrap()],
    );
    run(&main, &["commit", "-q", "--allow-empty", "-m", "start"]);
    run(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            other.to_str().unwrap(),
        ],
    );
    let (first, second) = (lock::path(&main).unwrap(), lock::path(&other).unwrap());
    assert_eq!(first, second);
    assert!(first.ends_with("aoe-ship/ship.lock"));
    assert!(first.starts_with(main.canonicalize().unwrap().join(".git")));
    let _held = lock::acquire(&main, "make ship").expect("free");
    let error = std::env::var("SHIP_LOCK_WAIT_MINUTES").map_or(true, |v| v.is_empty());
    if error {
        // The default wait is two hours; contend through the same path without it.
        take(&second, "feature", Duration::ZERO).expect_err("the main checkout holds it");
    }
    let held = lock::status(&second)
        .unwrap()
        .expect("visible from the worktree");
    assert_eq!(held.command, "make ship");
    assert_eq!(held.branch, "dev");
}

#[test]
fn read_only_status_neither_blocks_nor_takes_the_lock() {
    let (_temp, path) = lock_file();
    assert!(lock::status(&path).unwrap().is_none(), "no file is free");
    let held = take(&path, "first", Duration::ZERO).unwrap();
    let begun = Instant::now();
    let seen = lock::status(&path).unwrap().expect("held");
    assert_eq!(seen.branch, "first");
    assert!(begun.elapsed() < Duration::from_secs(2));
    drop(held);
    assert!(
        lock::status(&path).unwrap().is_none(),
        "stale text is not a holder"
    );
    take(&path, "next", Duration::ZERO).expect("status kept nothing");
}

#[test]
fn only_ship_and_duel_ask_for_the_lock() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut users = Vec::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !path.components().any(|c| c.as_os_str() == "tests")
                && !path.ends_with("lock.rs")
                && !path.ends_with("tests.rs")
                && std::fs::read_to_string(&path)
                    .unwrap()
                    .contains("lock::acquire(")
            {
                users.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    users.sort();
    assert_eq!(
        users,
        ["mod.rs", "mod.rs"],
        "ship/mod.rs and duel/mod.rs only"
    );
}
