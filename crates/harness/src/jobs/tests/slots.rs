//! N concurrent long-job slots: first free wins, all-held refuses with every holder.
use super::super::lock::{self, LongLock, slot_count, slot_path};
use super::lock::SERIAL;

fn take(dir: &std::path::Path, name: &str, slots: usize) -> crate::jobs::Result<(LongLock, usize)> {
    LongLock::acquire_any(
        dir,
        &format!("`make {name}` on {name} in /w/{name}"),
        name,
        None,
        slots,
    )
}

#[test]
fn three_jobs_are_admitted_and_the_fourth_lists_all_three_holders() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let held: Vec<_> = ["a", "b", "c"]
        .map(|name| take(dir.path(), name, 3).unwrap())
        .into();
    assert_eq!(
        held.iter().map(|(_, slot)| *slot).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    let error = take(dir.path(), "d", 3).unwrap_err();
    assert_eq!(lock::exit_code(error.as_ref()), 75);
    let message = error.to_string();
    for expected in [
        "slot 0",
        "slot 1",
        "slot 2",
        "make a",
        "make b",
        "make c",
        "retry later",
    ] {
        assert!(message.contains(expected), "{message}");
    }
    drop(held);
    assert_eq!(take(dir.path(), "e", 3).unwrap().1, 0);
}

#[test]
fn a_nested_child_inherits_its_own_slot_and_is_not_refused() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let _first = take(dir.path(), "a", 3).unwrap();
    let _second = take(dir.path(), "b", 3).unwrap();
    let (third, slot) = take(dir.path(), "c", 3).unwrap();
    assert_eq!(slot, 2);
    assert!(take(dir.path(), "d", 3).is_err());
    let token = third.token().to_owned();
    let (nested, nested_slot) =
        LongLock::acquire_any(dir.path(), "nested", "n", Some((2, &token)), 3).unwrap();
    assert_eq!(nested_slot, 2);
    assert!(!nested.owned());
    let forged = LongLock::acquire_any(dir.path(), "forged", "f", Some((2, "forged")), 3);
    assert!(forged.is_err());
    // A proof for another slot's file does not match.
    assert!(LongLock::acquire_any(dir.path(), "wrong", "w", Some((1, &token)), 3).is_err());
}

#[test]
fn one_slot_behaves_as_the_single_lock_did() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(slot_count(Some("1")).unwrap(), 1);
    let first = take(dir.path(), "a", 1).unwrap();
    let error = take(dir.path(), "b", 1).unwrap_err();
    assert_eq!(lock::exit_code(error.as_ref()), 75);
    assert!(error.to_string().contains("make a"));
    assert!(!slot_path(dir.path(), 1).exists());
    drop(first);
    take(dir.path(), "c", 1).unwrap();
}

#[test]
fn slot_count_defaults_to_three_and_rejects_out_of_range() {
    assert_eq!(slot_count(None).unwrap(), 3);
    assert_eq!(slot_count(Some(" ")).unwrap(), 3);
    assert_eq!(slot_count(Some("8")).unwrap(), 8);
    for bad in ["0", "9", "-1", "many"] {
        assert!(slot_count(Some(bad)).is_err(), "{bad}");
    }
}

#[test]
fn holder_text_without_a_held_flock_does_not_block() {
    let _serial = SERIAL.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    for slot in 0..3 {
        std::fs::write(
            slot_path(dir.path(), slot),
            "dead-proof\n`make coverage` on gone",
        )
        .unwrap();
    }
    assert!(lock::status(&slot_path(dir.path(), 0)).unwrap().is_none());
    assert!(!lock::verify(&slot_path(dir.path(), 1), "dead-proof").unwrap());
    assert_eq!(take(dir.path(), "a", 3).unwrap().1, 0);
}
