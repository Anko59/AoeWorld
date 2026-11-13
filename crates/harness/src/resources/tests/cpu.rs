//! The build pool admits within its machine, worktree and coordinator limits.
use super::super::{
    cpu::{self, Holder, Plan},
    lock,
};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

fn holder(worktree: &str, coordinator: &str) -> Holder {
    Holder {
        worktree: worktree.into(),
        coordinator: coordinator.into(),
        description: "`cargo test`".into(),
    }
}

/// Stands in for the launcher's `--build-locked` mode: record, then exec.
fn launcher(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("launcher.sh");
    std::fs::write(
        &path,
        "#!/bin/sh\nshift\npath=$1; proof=$2; holder=$3; shift 4\nprintf '%s\\n%s' \"$proof\" \"$holder\" > \"$path\"\nexec \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn free_slots_are_offered_in_order() {
    let held = [Some(holder("/a", "")), None, None];
    assert_eq!(
        cpu::plan(&held, &[], &holder("/b", "")),
        Plan::Try(vec![1, 2])
    );
}

#[test]
fn one_worktree_cannot_take_the_whole_pool_and_its_holders_are_named() {
    let held = [Some(holder("/a", "")), Some(holder("/a", "")), None, None];
    let Plan::Refuse(reason) = cpu::plan(&held, &[], &holder("/a", "")) else {
        panic!("a third container from one worktree was admitted");
    };
    assert!(reason.contains("limit 2 per worktree"), "{reason}");
    assert!(
        reason.contains("build slot 0: `cargo test` in /a"),
        "{reason}"
    );
    assert!(
        reason.contains("build slot 1: `cargo test` in /a"),
        "{reason}"
    );
}

#[test]
fn one_coordinator_and_its_agents_share_a_limit() {
    let held = [
        Some(holder("/a", "landscape")),
        Some(holder("/b", "landscape")),
        Some(holder("/c", "landscape")),
        None,
        None,
    ];
    let Plan::Refuse(reason) = cpu::plan(&held, &[], &holder("/d", "landscape")) else {
        panic!("a fourth container for one coordinator was admitted");
    };
    assert!(
        reason.contains("build slot 2: `cargo test` in /c for landscape"),
        "{reason}"
    );
    assert_eq!(
        cpu::plan(&held, &[], &holder("/d", "harness")),
        Plan::Try(vec![3, 4])
    );
    // Without a coordinator only the worktree and machine limits apply.
    assert_eq!(
        cpu::plan(&held, &[], &holder("/d", "")),
        Plan::Try(vec![3, 4])
    );
}

#[test]
fn running_long_jobs_count_against_the_pool() {
    let held = [Some(holder("/a", "")), None, None];
    let long = [
        "`make coverage` in /b".to_owned(),
        "`make save-replay` in /c".to_owned(),
    ];
    let Plan::Refuse(reason) = cpu::plan(&held, &long, &holder("/d", "")) else {
        panic!("a build was admitted past running long jobs");
    };
    assert!(reason.contains("3 of 3 build slots"), "{reason}");
    assert!(
        reason.contains("long job: `make coverage` in /b"),
        "{reason}"
    );
    assert_eq!(
        cpu::plan(&held, &long[..1], &holder("/d", "")),
        Plan::Try(vec![1, 2])
    );
}

#[test]
fn a_started_command_holds_its_slot_until_it_exits() {
    let dir = tempfile::tempdir().unwrap();
    let me = holder("/work/tree", "landscape");
    let (mut child, slot) = cpu::start(
        dir.path(),
        &me,
        (2, 1),
        Duration::ZERO,
        &launcher(dir.path()),
        "sleep",
        &["1".into()],
    )
    .unwrap();
    assert_eq!(slot, 0);
    assert_eq!(cpu::holders(dir.path(), 2).unwrap(), vec![Some(me), None]);
    assert!(child.wait().unwrap().success());
    assert_eq!(cpu::holders(dir.path(), 2).unwrap(), vec![None, None]);
}

#[test]
fn a_held_pool_refuses_after_the_wait_with_exit_75() {
    let dir = tempfile::tempdir().unwrap();
    let start = |worktree: &str| {
        let fake = launcher(dir.path());
        cpu::start(
            dir.path(),
            &holder(worktree, ""),
            (1, 1),
            Duration::ZERO,
            &fake,
            "sleep",
            &["2".into()],
        )
    };
    let (mut first, _) = start("/a").unwrap();
    let error = start("/b").expect_err("second job admitted into a one-slot pool");
    assert_eq!(lock::exit_code(error.as_ref()), 75);
    assert!(error.to_string().contains("make load-status"), "{error}");
    assert!(
        error
            .to_string()
            .contains("build slot 0: `cargo test` in /a"),
        "{error}"
    );
    first.wait().unwrap();
}

#[test]
fn a_held_long_job_slot_takes_build_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let _long = lock::LongLock::acquire(
        &lock::slot_path(dir.path(), 0),
        "`make coverage`",
        "proof",
        None,
    )
    .unwrap();
    let fake = launcher(dir.path());
    let error = cpu::start(
        dir.path(),
        &holder("/a", ""),
        (1, 1),
        Duration::ZERO,
        &fake,
        "true",
        &[],
    )
    .expect_err("a build was admitted past a long job in a one-slot pool");
    assert!(
        error.to_string().contains("long job: `make coverage`"),
        "{error}"
    );
}

#[test]
fn docker_containers_receive_the_slot_token_after_run() {
    let args = ["run".to_owned(), "--rm".to_owned(), "image".to_owned()];
    assert_eq!(
        cpu::with_token(&args, "/x/cpu-0.lock:abc"),
        [
            "run",
            "-e",
            "AOE_CPU_TOKEN=/x/cpu-0.lock:abc",
            "--rm",
            "image"
        ]
    );
}

#[test]
fn slot_count_is_bounded() {
    assert_eq!(cpu::slot_count(None).unwrap(), cpu::DEFAULT_SLOTS);
    assert_eq!(cpu::slot_count(Some("6")).unwrap(), 6);
    assert!(cpu::slot_count(Some("0")).is_err());
    assert!(cpu::slot_count(Some("17")).is_err());
}

#[test]
fn ledger_lines_are_json() {
    assert_eq!(cpu::quote("a\"b\\c\n\t"), "\"a\\\"b\\\\c\\n\\t\"");
    let dir = tempfile::tempdir().unwrap();
    cpu::ledger(
        dir.path(),
        "build",
        "admitted",
        &holder("/a\"", "x"),
        &[("exit", "0".into())],
    );
    let line = std::fs::read_to_string(dir.path().join("ledger.jsonl")).unwrap();
    let value: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(value["worktree"], "/a\"");
    assert_eq!(value["kind"], "build");
    assert_eq!(value["exit"], "0");
}
