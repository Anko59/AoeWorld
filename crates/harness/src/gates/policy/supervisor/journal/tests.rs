use super::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
fn private_dir() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    directory
}
fn private_file(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn setup(root: &Path) {
    fs::create_dir(root.join("lease-journal")).unwrap();
    #[cfg(unix)]
    fs::set_permissions(
        root.join("lease-journal"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
}
#[test]
fn durable_pairs_reopen_and_reuse_without_minting_service_ownership() {
    let owner = private_dir();
    let first = persist_model(owner.path()).unwrap();
    assert_eq!(first["status"], "MODEL_ONLY");
    assert_eq!(first["authoritative"], false);
    assert_eq!(first["sequence"], 2);
    assert_eq!(first["fsync_calls_completed"], true);
    let path = owner.path().join("lease-journal/journal.json");
    let bytes = fs::read(&path).unwrap();
    let ledger = Ledger::parse(&bytes).unwrap();
    assert_eq!(ledger.entries[0].phase, Phase::CreatedIntent);
    assert_eq!(ledger.entries[0].cid, None);
    assert_eq!(ledger.entries[1].phase, Phase::ModelObservation);
    assert_eq!(ledger.entries[1].cid, Some("a".repeat(64)));
    assert_eq!(
        first["endpoint_blake3"],
        blake3::hash(&bytes).to_hex().to_string()
    );
    let second = persist_model(owner.path()).unwrap();
    assert_eq!(second["sequence"], 4);
    assert_eq!(second["previous_endpoint_blake3"], first["endpoint_blake3"]);
    let reopened = Ledger::parse(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(reopened.entries[2].lease_id, "model-lease-0002");
    assert_eq!(
        reopened.entries[2].previous_hash,
        ledger.entries[1].entry_hash
    );
    #[cfg(unix)]
    {
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(&path).unwrap().nlink(), 1);
        assert_eq!(
            fs::metadata(owner.path().join("lease-journal"))
                .unwrap()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(owner.path().join("lease-journal/lock"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn already_synced_model_intent_resumes_same_correlation_id() {
    let owner = private_dir();
    let store = io::Store::open(owner.path()).unwrap();
    let mut ledger = Ledger::empty();
    ledger.append().unwrap();
    store.replace(None, &ledger.bytes().unwrap()).unwrap();
    let persisted = store.read().unwrap().unwrap();
    drop(store);
    let report = persist_model(owner.path()).unwrap();
    assert_eq!(report["resumed_model_intent"], true);
    assert_eq!(report["entries"], 2);
    assert_eq!(report["authoritative"], false);
    let reopened =
        Ledger::parse(&fs::read(owner.path().join("lease-journal/journal.json")).unwrap()).unwrap();
    assert_eq!(reopened.entries[0].lease_id, reopened.entries[1].lease_id);
    assert_eq!(
        reopened.entries[0].entry_hash,
        Ledger::parse(&persisted).unwrap().entries[0].entry_hash
    );
}
#[test]
fn inconsistent_closed_history_is_quarantined_not_reset() {
    let mut ledger = Ledger::empty();
    ledger.append().unwrap();
    ledger.append().unwrap();
    let valid = serde_json::to_value(&ledger).unwrap();
    let cases = [
        ("/schema", json!(2)),
        ("/authoritative", json!(true)),
        ("/mode", json!("READY")),
        ("/entries/0/sequence", json!(0)),
        ("/entries/1/sequence", json!(1)),
        ("/entries/0/phase", json!("MODEL_OBSERVATION")),
        ("/entries/1/lease_id", json!("callerCID")),
        ("/entries/1/cid", json!("b".repeat(64))),
        ("/entries/1/identity/service", json!("foreign")),
        ("/entries/1/previous_hash", json!("0".repeat(64))),
        ("/entries/0/entry_hash", json!("bad")),
    ];
    for (pointer, value) in cases {
        let owner = private_dir();
        setup(owner.path());
        let mut bad = valid.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        let path = owner.path().join("lease-journal/journal.json");
        let raw = serde_json::to_vec(&bad).unwrap();
        private_file(&path, &raw);
        assert!(persist_model(owner.path()).is_err(), "{pointer}");
        assert_eq!(fs::read(&path).unwrap(), raw);
    }
    for bytes in [vec![255], b"not-json".to_vec(), vec![b' '; MAX_BYTES + 1]] {
        assert!(Ledger::parse(&bytes).is_err());
    }
    let mut unknown = valid.clone();
    unknown["verified"] = json!(true);
    assert!(Ledger::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    let mut unknown = valid;
    unknown["entries"][0]["command"] = json!("rm");
    assert!(Ledger::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    let mut duplicate = ledger;
    duplicate.append().unwrap();
    duplicate.entries[2].lease_id = "model-lease-0001".into();
    duplicate.entries[2].entry_hash = hash(&duplicate.entries[2]).unwrap();
    assert!(duplicate.validate().is_err());
}
#[test]
fn model_entry_budget_is_fail_closed_and_retains_last_valid_endpoint() {
    let owner = private_dir();
    for _ in 0..MAX_ENTRIES / 2 {
        persist_model(owner.path()).unwrap();
    }
    let path = owner.path().join("lease-journal/journal.json");
    let bytes = fs::read(&path).unwrap();
    assert_eq!(Ledger::parse(&bytes).unwrap().entries.len(), MAX_ENTRIES);
    assert!(persist_model(owner.path()).is_err());
    assert_eq!(fs::read(path).unwrap(), bytes);
    let mut ledger = Ledger::empty();
    for _ in 0..MAX_ENTRIES {
        ledger.append().unwrap();
    }
    assert!(ledger.append().is_err());
    ledger.entries.push(ledger.entries[0].clone());
    assert!(ledger.validate().is_err());
}
#[cfg(unix)]
#[test]
fn nonblocking_lock_serializes_cooperating_writers_and_expected_head_rejects_changes() {
    let owner = private_dir();
    let store = io::Store::open(owner.path()).unwrap();
    assert!(io::Store::open(owner.path()).is_err());
    assert!(persist_model(owner.path()).is_err());
    let mut ledger = Ledger::empty();
    ledger.append().unwrap();
    let bytes = ledger.bytes().unwrap();
    store.replace(None, &bytes).unwrap();
    assert!(store.replace(None, &bytes).is_err());
    assert!(
        store
            .replace(Some(&bytes), &vec![b'x'; MAX_BYTES + 1])
            .is_err()
    );
    private_file(
        &owner.path().join("lease-journal/journal.json"),
        b"external-change",
    );
    assert!(store.replace(Some(&bytes), &bytes).is_err());
    drop(store);
    assert!(io::Store::open(owner.path()).is_ok());
}
#[cfg(unix)]
#[test]
fn symlinks_hardlinks_permissions_and_nonregular_targets_fail_without_replacement() {
    for target in ["journal.json", "lock"] {
        let owner = private_dir();
        setup(owner.path());
        let outside = owner.path().join("outside");
        private_file(&outside, b"caller bytes");
        symlink(&outside, owner.path().join("lease-journal").join(target)).unwrap();
        assert!(persist_model(owner.path()).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"caller bytes");
        let owner = private_dir();
        setup(owner.path());
        let outside = owner.path().join("outside");
        private_file(&outside, b"caller bytes");
        fs::hard_link(&outside, owner.path().join("lease-journal").join(target)).unwrap();
        assert!(persist_model(owner.path()).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"caller bytes");
        let owner = private_dir();
        setup(owner.path());
        let path = owner.path().join("lease-journal").join(target);
        private_file(&path, b"{}");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(persist_model(owner.path()).is_err());
        assert_eq!(fs::read(path).unwrap(), b"{}");
        let owner = private_dir();
        setup(owner.path());
        fs::create_dir(owner.path().join("lease-journal").join(target)).unwrap();
        assert!(persist_model(owner.path()).is_err());
    }
    let owner = private_dir();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), owner.path().join("lease-journal")).unwrap();
    assert!(persist_model(owner.path()).is_err());
    assert!(!outside.path().join("journal.json").exists());
    let owner = private_dir();
    setup(owner.path());
    fs::set_permissions(
        owner.path().join("lease-journal"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(persist_model(owner.path()).is_err());
    let owner = private_dir();
    let alias = owner.path().join("alias");
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), &alias).unwrap();
    assert!(persist_model(&alias).is_err());
    let owner = private_dir();
    fs::set_permissions(owner.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(persist_model(owner.path()).is_err());
}
#[cfg(unix)]
#[test]
fn replaced_root_and_lock_inode_endpoints_never_overwrite_unverified_targets() {
    let owner = private_dir();
    let store = io::Store::open(owner.path()).unwrap();
    let root = owner.path().join("lease-journal");
    fs::rename(&root, owner.path().join("old-root")).unwrap();
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(store.replace(None, b"{}").is_err());
    assert!(!root.join("journal.json").exists());
    let owner = private_dir();
    let store = io::Store::open(owner.path()).unwrap();
    let lock = owner.path().join("lease-journal/lock");
    fs::rename(&lock, owner.path().join("old-lock")).unwrap();
    private_file(&lock, b"");
    assert!(store.read().is_err());
    assert!(store.replace(None, b"{}").is_err());
}
