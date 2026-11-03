use super::run;
use crate::ship::lock::{self, Holder};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn holder(branch: &str) -> Holder {
    Holder {
        worktree: format!("/work/{branch}"),
        branch: branch.into(),
        command: "make ship".into(),
        started: 1_791_900_000,
    }
}
fn take(path: &Path, branch: &str) -> crate::ship::Result<lock::ShipLock> {
    lock::acquire_at(path, &holder(branch))
}
fn lock_file() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ship.lock");
    (temp, path)
}

#[test]
fn busy_ship_refuses_in_under_one_second_with_the_complete_holder() {
    let (_temp, path) = lock_file();
    let first = take(&path, "first").unwrap();
    let begun = Instant::now();
    let error = take(&path, "second").unwrap_err();
    assert!(begun.elapsed() < Duration::from_secs(1));
    assert_eq!(crate::jobs::lock::exit_code(error.as_ref()), 75);
    let text = error.to_string();
    for expected in [
        "busy:",
        "make ship",
        "first",
        "/work/first",
        "2026-10-13 14:00:00Z",
        "retry later",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    drop(first);
    take(&path, "next").unwrap();
}

#[test]
fn status_is_read_only_and_ignores_a_dead_holders_record() {
    let (_temp, path) = lock_file();
    assert!(lock::status(&path).unwrap().is_none());
    let held = take(&path, "first").unwrap();
    assert_eq!(lock::status(&path).unwrap().unwrap().branch, "first");
    drop(held);
    assert!(lock::status(&path).unwrap().is_none());
    take(&path, "second").unwrap();
}

#[test]
fn ship_holder_child() {
    let Ok(path) = std::env::var("AOE_TEST_SHIP_HOLDER") else {
        return;
    };
    let held = take(Path::new(&path), "child").unwrap();
    let descendant = if std::env::var_os("AOE_TEST_SHIP_DESCENDANT").is_some() {
        let mut command = Command::new("sleep");
        command.arg("30").stdout(Stdio::null());
        held.inherit_to(&mut command).unwrap();
        Some(command.spawn().unwrap())
    } else {
        None
    };
    println!("HELD {}", descendant.as_ref().map_or(0, |child| child.id()));
    thread::sleep(Duration::from_secs(60));
}

fn child(path: &Path, descendant: bool) -> (std::process::Child, u32) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "ship::tests::lock::ship_holder_child",
            "--nocapture",
        ])
        .env("AOE_TEST_SHIP_HOLDER", path)
        .stdout(Stdio::piped());
    if descendant {
        command.env("AOE_TEST_SHIP_DESCENDANT", "1");
    }
    let mut child = command.spawn().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let pid = loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 {
            let status = child.wait().unwrap();
            panic!("child never locked: {status}");
        }
        if let Some(pid) = line.trim().strip_prefix("HELD ") {
            break pid.parse().unwrap();
        }
    };
    (child, pid)
}

#[test]
fn kernel_releases_a_killed_ship_without_waiter_tickets() {
    let (_temp, path) = lock_file();
    let (mut child, _) = child(&path, false);
    take(&path, "other").unwrap_err();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(lock::status(&path).unwrap().is_none());
    take(&path, "next").unwrap();
}

#[test]
fn an_active_mutator_child_keeps_serialization_after_controller_death() {
    let (_temp, path) = lock_file();
    let (mut parent, descendant) = child(&path, true);
    parent.kill().unwrap();
    parent.wait().unwrap();
    let blocked = take(&path, "other");
    let _ = Command::new("kill")
        .args(["-KILL", &descendant.to_string()])
        .status();
    assert!(
        blocked.is_err(),
        "the mutation child must retain the descriptor"
    );
    let begun = Instant::now();
    while lock::status(&path).unwrap().is_some() {
        assert!(begun.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(10));
    }
    take(&path, "next").unwrap();
}

#[test]
fn worktrees_share_one_ship_lock() {
    let temp = tempfile::tempdir().unwrap();
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
    assert_eq!(lock::path(&main).unwrap(), lock::path(&other).unwrap());
    let _held = lock::acquire(&main, "make ship").unwrap();
    let error = lock::acquire(&other, "make ship").unwrap_err().to_string();
    assert!(error.contains("busy:"));
    assert!(error.contains(main.to_str().unwrap()));
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
        "only ship and duel acquire the ship lock"
    );
}
