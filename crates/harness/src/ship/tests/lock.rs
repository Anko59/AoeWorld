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
        returning: false,
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
    // Windows wide enough for a loaded machine: the ticket setup alone can
    // take a few hundred milliseconds there, before the first line is printed.
    let result = lock::acquire_at(
        &path,
        &holder("second"),
        Duration::from_secs(2),
        POLL,
        Duration::from_millis(300),
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
    let queued = lock::queue(&second).unwrap();
    assert_eq!(queued.len(), 1, "the queue is shared too");
    assert_eq!(queued[0].command, "make ship");
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
    let mut stack = vec![src.clone()];
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
                users.push(
                    path.strip_prefix(&src)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    users.sort();
    assert_eq!(
        users,
        ["ship/check/mod.rs", "ship/duel/mod.rs"],
        "only ship and duel acquire the ship lock; make red takes only a heavy slot"
    );
}

fn tickets(path: &Path) -> Vec<String> {
    let dir = PathBuf::from(format!("{}.queue", path.display()));
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Queue `branch` behind the current holder; returns once its first waiting line is out.
fn enqueue(
    path: &Path,
    branch: &'static str,
    order: &std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
) -> thread::JoinHandle<()> {
    let (path, order) = (path.to_owned(), order.clone());
    let (said, lines) = mpsc::channel();
    let waiter = thread::spawn(move || {
        let _held = lock::acquire_at(
            &path,
            &holder(branch),
            Duration::from_secs(60),
            POLL,
            NEVER,
            &mut |line| {
                let _ = said.send(line.to_owned());
            },
        )
        .expect("served in the end");
        order.lock().unwrap().push(branch);
        thread::sleep(Duration::from_millis(30));
    });
    let line = lines.recv_timeout(Duration::from_secs(10)).expect("queued");
    assert!(line.starts_with("ship: waiting for first"), "{line}");
    waiter
}

#[test]
fn waiters_are_served_in_arrival_order() {
    let (_temp, path) = lock_file();
    let first = take(&path, "first", Duration::ZERO).expect("free lock");
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let waiters = [
        enqueue(&path, "second", &order),
        enqueue(&path, "third", &order),
        enqueue(&path, "fourth", &order),
    ];
    thread::sleep(Duration::from_millis(200));
    drop(first);
    for waiter in waiters {
        waiter.join().unwrap();
    }
    assert_eq!(*order.lock().unwrap(), ["second", "third", "fourth"]);
    assert!(tickets(&path).is_empty(), "{:?}", tickets(&path));
}

#[test]
fn progress_lines_give_the_position_in_the_queue() {
    let (_temp, path) = lock_file();
    let first = take(&path, "first", Duration::ZERO).expect("free lock");
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let ahead = enqueue(&path, "second", &order);
    let mut lines = Vec::new();
    let error = lock::acquire_at(
        &path,
        &holder("third"),
        Duration::from_secs(2),
        POLL,
        NEVER,
        &mut |line| lines.push(line.to_owned()),
    )
    .expect_err("two ahead")
    .to_string();
    assert!(
        lines.contains(&"ship: position 3 in the queue".to_owned()),
        "{lines:?}"
    );
    assert!(error.contains("position 3 in the queue"), "{error}");
    drop(first);
    ahead.join().unwrap();
    assert!(tickets(&path).is_empty(), "a failed wait leaves no ticket");
}

/// Re-run this test binary as a child that queues behind the holder and sleeps.
#[test]
fn lock_waiter_child() {
    let Ok(path) = std::env::var("AOE_TEST_LOCK_WAITER") else {
        return;
    };
    let _ = lock::acquire_at(
        Path::new(&path),
        &holder("child"),
        NEVER,
        POLL,
        NEVER,
        &mut |line| {
            println!("{line}");
        },
    );
}

#[test]
fn a_killed_waiters_ticket_does_not_block_the_others() {
    let (_temp, path) = lock_file();
    let first = take(&path, "first", Duration::ZERO).expect("free lock");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ship::tests::lock::lock_waiter_child",
            "--nocapture",
        ])
        .env("AOE_TEST_LOCK_WAITER", &path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("child");
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    while !line.starts_with("ship: position") {
        line.clear();
        assert!(out.read_line(&mut line).unwrap() > 0, "child never queued");
    }
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let behind = enqueue(&path, "third", &order);
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(
        tickets(&path).len(),
        3,
        "the dead waiter's ticket stays on disk"
    );
    drop(first);
    behind.join().unwrap();
    assert_eq!(*order.lock().unwrap(), ["third"]);
    assert!(tickets(&path).is_empty(), "{:?}", tickets(&path));
}

#[test]
fn a_zero_wait_with_a_queue_ahead_names_the_running_ship_and_leaves_nothing() {
    let (_temp, path) = lock_file();
    let first = take(&path, "first", Duration::ZERO).expect("free lock");
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let ahead = enqueue(&path, "second", &order);
    let error = take(&path, "third", Duration::ZERO)
        .expect_err("a queue ahead")
        .to_string();
    assert!(error.contains("first in /work/first"), "{error}");
    assert!(error.contains("SHIP_LOCK_WAIT_MINUTES"), "{error}");
    let waiting = lock::queue(&path).unwrap();
    assert_eq!(
        waiting
            .iter()
            .map(|h| h.branch.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"],
        "the queue lists those ahead in order"
    );
    drop(first);
    ahead.join().unwrap();
    assert!(tickets(&path).is_empty());
}

mod priority;
