//! Priority in the ship queue: a ship back from review (`returning`) is admitted before every
//! plain waiter, in arrival order among the returning ships. A returning ship killed while
//! queued (same rank, earlier) does not block the next one. The holders are real threads and child processes on a temp lock.
use crate::ship::lock::{self, Holder};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(20);
const NEVER: Duration = Duration::from_secs(3600);
const PATIENCE: Duration = Duration::from_secs(10);
const WAITER_VARIABLE: &str = "AOE_TEST_PRIORITY_WAITER";

fn holder(branch: &str, returning: bool) -> Holder {
    Holder {
        worktree: "/work/priority".into(),
        branch: branch.into(),
        command: "make ship".into(),
        started: 1_791_900_000,
        returning,
    }
}

fn lock_file() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temp");
    let path = temp.path().join("aoe-ship/ship.lock");
    (temp, path)
}

/// Wait until exactly `count` live ships are queued (the holder included).
fn queued(path: &Path, count: usize) {
    let begun = Instant::now();
    while lock::queue(path).expect("queue").len() != count {
        assert!(begun.elapsed() < PATIENCE, "{count} ships never queued");
        thread::sleep(POLL);
    }
}

/// Queue `holder` in its own thread. Once admitted, the thread reports its branch and keeps
/// the lock until the returned sender sends (or is dropped).
fn queue_ship(path: &Path, holder: Holder, admitted: &Sender<String>) -> Sender<()> {
    let (release, hold) = mpsc::channel::<()>();
    let (path, admitted) = (path.to_owned(), admitted.clone());
    thread::spawn(move || {
        let branch = holder.branch.clone();
        let lock = lock::acquire_at(
            &path,
            &holder,
            Duration::from_secs(60),
            POLL,
            NEVER,
            &mut |_| {},
        )
        .expect("admitted");
        admitted.send(branch).expect("report");
        let _ = hold.recv();
        drop(lock);
    });
    release
}

fn next_admitted(admitted: &Receiver<String>) -> String {
    admitted
        .recv_timeout(PATIENCE)
        .expect("a ship was admitted")
}

/// Re-run this test binary as a child that queues as a plain waiter and waits forever.
#[test]
fn priority_waiter_child() {
    let Ok(path) = std::env::var(WAITER_VARIABLE) else {
        return;
    };
    let _lock = lock::acquire_at(
        Path::new(&path),
        &holder("doomed", true),
        NEVER,
        POLL,
        NEVER,
        &mut |_| {},
    )
    .expect("child lock");
    thread::sleep(Duration::from_secs(600));
}

#[test]
fn a_returning_ship_is_admitted_before_plain_waiters_queued_earlier() {
    let (_temp, path) = lock_file();
    let first = lock::acquire_at(
        &path,
        &holder("first", false),
        Duration::ZERO,
        POLL,
        NEVER,
        &mut |_| {},
    )
    .expect("free lock");
    let (admitted_tx, admitted) = mpsc::channel();
    let a = queue_ship(&path, holder("a", false), &admitted_tx);
    queued(&path, 2);
    let b = queue_ship(&path, holder("b", false), &admitted_tx);
    queued(&path, 3);
    let r = queue_ship(&path, holder("r", true), &admitted_tx);
    queued(&path, 4);
    drop(first);
    assert_eq!(next_admitted(&admitted), "r");
    r.send(()).expect("release r");
    assert_eq!(next_admitted(&admitted), "a");
    a.send(()).expect("release a");
    assert_eq!(next_admitted(&admitted), "b");
    b.send(()).expect("release b");
}

#[test]
fn returning_ships_keep_their_order_and_go_before_an_earlier_plain_waiter() {
    let (_temp, path) = lock_file();
    let first = lock::acquire_at(
        &path,
        &holder("first", false),
        Duration::ZERO,
        POLL,
        NEVER,
        &mut |_| {},
    )
    .expect("free lock");
    let (admitted_tx, admitted) = mpsc::channel();
    let p = queue_ship(&path, holder("p", false), &admitted_tx);
    queued(&path, 2);
    let r1 = queue_ship(&path, holder("r1", true), &admitted_tx);
    queued(&path, 3);
    let r2 = queue_ship(&path, holder("r2", true), &admitted_tx);
    queued(&path, 4);
    drop(first);
    assert_eq!(next_admitted(&admitted), "r1");
    r1.send(()).expect("release r1");
    assert_eq!(next_admitted(&admitted), "r2");
    r2.send(()).expect("release r2");
    assert_eq!(next_admitted(&admitted), "p");
    p.send(()).expect("release p");
}

#[test]
fn a_returning_ship_killed_while_queued_does_not_block_the_next_returning_ship() {
    let (_temp, path) = lock_file();
    let first = lock::acquire_at(
        &path,
        &holder("first", false),
        Duration::ZERO,
        POLL,
        NEVER,
        &mut |_| {},
    )
    .expect("free lock");
    let mut doomed = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "ship::tests::lock::priority::priority_waiter_child",
            "--nocapture",
        ])
        .env(WAITER_VARIABLE, &path)
        .spawn()
        .expect("spawn the waiter");
    queued(&path, 2);
    doomed.kill().expect("kill the waiter");
    doomed.wait().expect("reap the waiter");
    queued(&path, 1);
    let (admitted_tx, admitted) = mpsc::channel();
    let r = queue_ship(&path, holder("r", true), &admitted_tx);
    queued(&path, 2);
    drop(first);
    assert_eq!(next_admitted(&admitted), "r");
    r.send(()).expect("release r");
}

#[test]
fn a_returning_ship_says_so_while_it_waits() {
    let (_temp, path) = lock_file();
    let _first = lock::acquire_at(
        &path,
        &holder("first", false),
        Duration::ZERO,
        POLL,
        NEVER,
        &mut |_| {},
    )
    .expect("free lock");
    let (said, lines) = mpsc::channel();
    let waiting = path.clone();
    thread::spawn(move || {
        let _ = lock::acquire_at(
            &waiting,
            &holder("r", true),
            Duration::from_secs(60),
            POLL,
            NEVER,
            &mut |line| {
                let _ = said.send(line.to_owned());
            },
        );
    });
    let position = loop {
        let line = lines.recv_timeout(PATIENCE).expect("a position line");
        if line.starts_with("ship: position") {
            break line;
        }
    };
    assert_eq!(
        position,
        "ship: position 1 in the queue (returning from review)"
    );
}

#[test]
fn the_queue_names_a_returning_ship() {
    assert_eq!(
        holder("r", true).describe_place(),
        "r in /work/priority (returning from review)"
    );
    assert_eq!(holder("p", false).describe_place(), "p in /work/priority");
}

#[test]
fn the_queue_listing_drops_the_holder_by_identity_and_lists_in_admission_order() {
    let mut returning_holder = holder("r1", true);
    returning_holder.started += 30;
    let mut later_plain = holder("p2", false);
    later_plain.started += 10;
    let mut waiting_returning = holder("r2", true);
    waiting_returning.started += 40;
    // Live tickets oldest first: p1, p2, then r1 (the holder, back from review), then r2.
    let waiting = vec![
        holder("p1", false),
        later_plain,
        returning_holder.clone(),
        waiting_returning,
    ];
    let lines = lock::queue_lines(Some(&returning_holder), waiting);
    let order: Vec<&str> = lines[1..]
        .iter()
        .map(|line| {
            line.split(" on ")
                .nth(1)
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(order, ["r2", "p1", "p2"], "{lines:?}");
    assert!(
        lines.iter().all(|line| !line.contains("r1 in")),
        "{lines:?}"
    );
}
