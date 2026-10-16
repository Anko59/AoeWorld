#![cfg(unix)]
use super::super::tests::cid;
use super::*;
fn output(root: &tempfile::TempDir, excluded: &tempfile::TempDir) -> PrivateOutput {
    PrivateOutput::new(root.path(), &[excluded.path().into()]).unwrap()
}
#[test]
fn actual_local_fsync_journal_retains_intent_cid_and_no_authority() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let out = output(&root, &source);
    let nonce = "c".repeat(32);
    let mut journal = Journal::new(
        &out,
        nonce.clone(),
        &"1".repeat(64),
        &"2".repeat(64),
        &Operation::FmtCheck,
    )
    .unwrap();
    journal.append(Event::Intent, None).unwrap();
    journal.append(Event::Created, Some(&cid())).unwrap();
    journal.append(Event::VerifiedAbsent, Some(&cid())).unwrap();
    journal.verify().unwrap();
    let bytes = fs::read(root.path().join(format!("worker-{nonce}.json"))).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["authoritative"], false);
    assert_eq!(value["domain"], "LOCAL_WORKER_LIFECYCLE_V1");
    assert_eq!(value["cid"], cid().value());
    assert_eq!(
        value["events"],
        serde_json::json!(["INTENT", "CREATED", "VERIFIED_ABSENT"])
    );
    // No recovery API accepts these bytes as an owned lease.
    assert!(
        Journal::new(
            &out,
            nonce,
            &"1".repeat(64),
            &"2".repeat(64),
            &Operation::FmtCheck
        )
        .is_err()
    );
}
#[test]
fn replaced_changed_or_preexisting_journal_quarantines_local_history() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let out = output(&root, &source);
    let nonce = "c".repeat(32);
    let path = root.path().join(format!("worker-{nonce}.json"));
    let mut journal = Journal::new(
        &out,
        nonce,
        &"1".repeat(64),
        &"2".repeat(64),
        &Operation::FmtCheck,
    )
    .unwrap();
    journal.append(Event::Intent, None).unwrap();
    fs::write(&path, b"{\"authoritative\":true}").unwrap();
    assert!(journal.verify().is_err());
    assert!(journal.append(Event::Created, Some(&cid())).is_err());
}
#[test]
fn failure_to_publish_intent_cannot_be_reinterpreted_as_persisted() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let out = output(&root, &source);
    let nonce = "c".repeat(32);
    let path = root.path().join(format!("worker-{nonce}.json"));
    let mut journal = Journal::new(
        &out,
        nonce,
        &"1".repeat(64),
        &"2".repeat(64),
        &Operation::FmtCheck,
    )
    .unwrap();
    fs::create_dir(&path).unwrap();
    assert!(journal.append(Event::Intent, None).is_err());
}
