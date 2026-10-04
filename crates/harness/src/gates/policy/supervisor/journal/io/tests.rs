#![cfg(unix)]
use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
fn fixture() -> (tempfile::TempDir, Store) {
    let owner = tempfile::tempdir().unwrap();
    fs::set_permissions(owner.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = Store::open(owner.path()).unwrap();
    store.replace(None, b"old history").unwrap();
    (owner, store)
}
fn temp_path(store: &Store, prepared: &Prepared) -> PathBuf {
    let path = store.directory.join(&prepared.name);
    assert_eq!(
        path.canonicalize().unwrap().parent(),
        Some(store.directory.canonicalize().unwrap().as_path())
    );
    path
}
fn assert_unchanged(store: &Store) {
    assert_eq!(store.read().unwrap().unwrap(), b"old history");
}
#[test]
fn synced_temporary_correlates_handle_path_bytes_and_expected_history() {
    let (_owner, store) = fixture();
    let prepared = store.prepare(Some(b"old history"), b"new history").unwrap();
    store
        .publish(Some(b"old history"), b"new history", prepared)
        .unwrap();
    assert_eq!(store.read().unwrap().unwrap(), b"new history");
}
#[test]
fn same_bytes_replacement_inode_rejects_before_overwriting_history() {
    let (_owner, store) = fixture();
    let prepared = store.prepare(Some(b"old history"), b"new history").unwrap();
    let path = temp_path(&store, &prepared);
    let retained = store.directory.join("retained-original.tmp");
    assert_eq!(retained.parent(), Some(store.directory.as_path()));
    assert!(!retained.exists());
    fs::rename(&path, &retained).unwrap();
    fs::write(&path, b"new history").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        store
            .publish(Some(b"old history"), b"new history", prepared)
            .is_err()
    );
    assert_unchanged(&store);
    assert_eq!(fs::read(retained).unwrap(), b"new history");
    assert_eq!(fs::read(path).unwrap(), b"new history");
}
#[test]
fn same_inode_mutation_and_overflow_reject_before_publication() {
    for bytes in [b"bad history".to_vec(), vec![b'x'; MAX_BYTES + 1]] {
        let (_owner, store) = fixture();
        let prepared = store.prepare(Some(b"old history"), b"new history").unwrap();
        let path = temp_path(&store, &prepared);
        fs::write(&path, &bytes).unwrap();
        assert!(
            store
                .publish(Some(b"old history"), b"new history", prepared)
                .is_err()
        );
        assert_unchanged(&store);
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}
#[test]
fn hardlinked_temporary_and_symlink_substitution_are_not_renamed_into_ledger() {
    let (_owner, store) = fixture();
    let prepared = store.prepare(Some(b"old history"), b"new history").unwrap();
    let path = temp_path(&store, &prepared);
    let link = store.directory.join("temporary-alias");
    fs::hard_link(&path, &link).unwrap();
    assert!(
        store
            .publish(Some(b"old history"), b"new history", prepared)
            .is_err()
    );
    assert_unchanged(&store);
    assert_eq!(fs::read(link).unwrap(), b"new history");

    let (_owner, store) = fixture();
    let prepared = store.prepare(Some(b"old history"), b"new history").unwrap();
    let path = temp_path(&store, &prepared);
    let retained = store.directory.join("retained-original.tmp");
    assert_eq!(retained.parent(), Some(store.directory.as_path()));
    assert!(!retained.exists());
    fs::rename(&path, &retained).unwrap();
    symlink(store.directory.join("journal.json"), &path).unwrap();
    assert!(
        store
            .publish(Some(b"old history"), b"new history", prepared)
            .is_err()
    );
    assert_unchanged(&store);
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
}
#[test]
fn changed_history_after_preparation_rejects_and_retains_prepared_debt() {
    let (_owner, store) = fixture();
    let prepared = store.prepare(Some(b"old history"), b"new history").unwrap();
    let path = temp_path(&store, &prepared);
    store
        .replace(Some(b"old history"), b"other history")
        .unwrap();
    assert!(
        store
            .publish(Some(b"old history"), b"new history", prepared)
            .is_err()
    );
    assert_eq!(store.read().unwrap().unwrap(), b"other history");
    assert_eq!(fs::read(path).unwrap(), b"new history");
}
