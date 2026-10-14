//! Which registry a gate reads and which queue it joins, from any directory: a subdirectory of the
//! checkout, a snapshot of it, or one outside every repository.
use super::{SERIAL, checkout, git, tickets};
use crate::{
    gates::registry::Registry,
    slots::guard::{checkout_of, dir, hold, hold_snapshot, is_heavy, run_in_slot, slot_dir_with},
};
use std::{ffi::OsStr, fs, path::Path};

#[test]
fn a_gate_run_from_a_subdirectory_reads_the_checkouts_registry_and_queue() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_temp, root) = checkout();
    let fuzz = root.join("fuzz");
    fs::create_dir_all(&fuzz).unwrap();
    assert_eq!(
        checkout_of(&fuzz).unwrap(),
        Some(root.canonicalize().unwrap())
    );
    let slot = hold(&fuzz, "slow").expect("the only slot is free");
    assert_eq!(tickets(&dir(&root).unwrap()), 1, "the root's queue");
    drop(slot);
    assert_eq!(tickets(&dir(&root).unwrap()), 0);
}

#[test]
fn a_directory_outside_every_checkout_takes_no_slot_because_no_gate_is_heavy_there() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let temp = tempfile::tempdir().expect("temp");
    assert_eq!(checkout_of(temp.path()).unwrap(), None);
    let guard = hold(temp.path(), "slow").expect("no registry, so nothing waits");
    assert!(guard.env().is_empty());
}

#[test]
fn a_checkout_whose_registry_cannot_be_read_fails_instead_of_running_unslotted() {
    let (_temp, root) = checkout();
    fs::write(root.join("gates/registry.json"), "not json").unwrap();
    let error = hold(&root, "slow")
        .expect_err("a registry that cannot be read is no answer")
        .to_string();
    assert!(error.contains("cannot read the registry"), "{error}");
}

#[test]
fn a_directory_without_git_fails_with_the_reason() {
    let temp = tempfile::tempdir().expect("temp");
    let error = slot_dir_with(None, temp.path())
        .expect_err("no git directory")
        .to_string();
    assert!(error.contains("heavy slot: no git directory"), "{error}");
}

#[test]
fn the_directory_a_holder_named_wins_and_an_empty_one_is_no_name() {
    let (_temp, root) = checkout();
    let named = Path::new("/var/lib/shared/heavy-slots");
    assert_eq!(
        slot_dir_with(Some(named.as_os_str()), &root).unwrap(),
        named
    );
    assert_eq!(
        slot_dir_with(Some(OsStr::new("")), &root).unwrap(),
        dir(&root).unwrap()
    );
}

#[test]
fn a_snapshot_queues_in_the_real_repositorys_directory_not_its_own() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_real_temp, real) = checkout();
    let snapshot_temp = tempfile::tempdir().expect("temp");
    let snapshot = snapshot_temp.path().join("aoe-snapshot");
    fs::create_dir_all(snapshot.join("gates")).unwrap();
    for file in ["gates/registry.json", "gates/heavy.json"] {
        fs::copy(real.join(file), snapshot.join(file)).unwrap();
    }
    git(
        snapshot_temp.path(),
        &["init", "-q", snapshot.to_str().unwrap()],
    );
    let real_dir = dir(&real).unwrap();
    let slot = hold_snapshot(&snapshot, &real, "slow").expect("the real queue has a slot");
    assert_eq!(tickets(&real_dir), 1, "the ticket is in the real queue");
    assert_eq!(
        tickets(&dir(&snapshot).unwrap()),
        0,
        "not in the snapshot's own Git directory, which is deleted with it"
    );
    drop(slot);
    assert_eq!(tickets(&real_dir), 0);
}

#[test]
fn run_in_slot_holds_the_slot_for_the_child_and_returns_its_status() {
    let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (_temp, root) = checkout();
    let shell = |script: &str| vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()];
    let code = run_in_slot(
        &root,
        "slow",
        &shell(r#"test -n "$AOE_HEAVY_SLOT" && test -n "$AOE_HEAVY_DIR" && exit 3; exit 9"#),
    )
    .expect("the slot is free");
    assert_eq!(code, 3, "the child sees the token and the queue");
    assert_eq!(tickets(&dir(&root).unwrap()), 0, "released when it exits");
    let code = run_in_slot(
        &root,
        "fmt-check",
        &shell(r#"test -z "${AOE_HEAVY_SLOT:-}" && exit 4; exit 5"#),
    )
    .expect("a light gate never waits");
    assert_eq!(code, 4, "a light gate passes no slot");
}

#[test]
fn a_gate_is_heavy_by_its_id_or_by_its_make_command() {
    let (_temp, root) = checkout();
    let mut registry = Registry::load(&root).unwrap();
    // `slow` is heavy in the fixture; a second gate runs `make coverage` under another id.
    let mut by_command = registry.gates[1].clone();
    by_command.id = "coverage-build".into();
    by_command.command = "make coverage".into();
    registry.gates.push(by_command);
    assert!(is_heavy(&registry, "slow"), "by id");
    assert!(is_heavy(&registry, "coverage"), "by its make command");
    assert!(!is_heavy(&registry, "fmt-check"));
    assert!(!is_heavy(&registry, "nope"));
}
