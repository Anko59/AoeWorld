//! The build pool admits within its machine, worktree and coordinator limits.
use super::super::cpu::{self, Holder, Plan};
use std::time::Duration;

fn holder(worktree: &str, coordinator: &str) -> Holder {
    Holder {
        worktree: worktree.into(),
        coordinator: coordinator.into(),
        description: "`cargo test`".into(),
    }
}

#[test]
fn free_slots_are_offered_in_order() {
    let held = [Some(holder("/a", "")), None, None];
    assert_eq!(cpu::plan(&held, &holder("/b", "")), Plan::Try(vec![1, 2]));
}

#[test]
fn one_worktree_cannot_take_the_whole_pool() {
    let held = [Some(holder("/a", "")), Some(holder("/a", "")), None, None];
    let Plan::Refuse(reason) = cpu::plan(&held, &holder("/a", "")) else {
        panic!("a third container from one worktree was admitted");
    };
    assert!(reason.contains("limit 2 per worktree"), "{reason}");
}

#[test]
fn one_coordinator_and_its_agents_share_a_limit() {
    let held = [
        Some(holder("/a", "landscape")),
        Some(holder("/b", "landscape")),
        Some(holder("/c", "landscape")),
        None,
    ];
    assert!(matches!(
        cpu::plan(&held, &holder("/d", "landscape")),
        Plan::Refuse(_)
    ));
    assert_eq!(
        cpu::plan(&held, &holder("/d", "harness")),
        Plan::Try(vec![3])
    );
    // Without a coordinator only the worktree and machine limits apply.
    assert_eq!(cpu::plan(&held, &holder("/d", "")), Plan::Try(vec![3]));
}

#[test]
fn a_full_pool_refuses_and_names_every_holder() {
    let held = [Some(holder("/a", "x")), Some(holder("/b", ""))];
    let Plan::Refuse(reason) = cpu::plan(&held, &holder("/c", "")) else {
        panic!("a full pool admitted a job");
    };
    assert!(
        reason.contains("build slot 0: `cargo test` in /a for x"),
        "{reason}"
    );
    assert!(
        reason.contains("build slot 1: `cargo test` in /b"),
        "{reason}"
    );
}

#[test]
fn holders_round_trip_through_the_lock_record() {
    let original = holder("/work/tree", "landscape");
    let dir = tempfile::tempdir().unwrap();
    let slot = cpu::admit_with(dir.path(), &original, 2, Duration::ZERO).unwrap();
    assert_eq!(
        cpu::holders(dir.path(), 2).unwrap(),
        vec![Some(original), None]
    );
    assert!(
        slot.token
            .starts_with(&cpu::slot_path(dir.path(), 0).display().to_string())
    );
    drop(slot);
    assert_eq!(cpu::holders(dir.path(), 2).unwrap(), vec![None, None]);
}

#[test]
fn a_held_pool_refuses_after_the_wait_with_exit_75() {
    let dir = tempfile::tempdir().unwrap();
    let _first = cpu::admit_with(dir.path(), &holder("/a", ""), 1, Duration::ZERO).unwrap();
    let error = cpu::admit_with(dir.path(), &holder("/b", ""), 1, Duration::ZERO)
        .err()
        .expect("second job admitted into a one-slot pool");
    assert_eq!(super::super::lock::exit_code(error.as_ref()), 75);
    assert!(error.to_string().contains("make load-status"), "{error}");
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
        "admitted",
        &holder("/a\"", "x"),
        &[("exit", "0".into())],
    );
    let line = std::fs::read_to_string(dir.path().join("ledger.jsonl")).unwrap();
    let value: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(value["worktree"], "/a\"");
    assert_eq!(value["event"], "admitted");
    assert_eq!(value["exit"], "0");
}
