use super::*;

#[test]
fn historical_commit_fingerprint_binds_tree_paths_with_same_blob_set() {
    let repo = repo();
    let historical = line(run(repo.path(), &["rev-parse", "HEAD"])).unwrap();
    let old_tree = line(run(repo.path(), &["rev-parse", "HEAD^{tree}"])).unwrap();
    run(repo.path(), &["mv", "code.rs", "renamed.rs"]);
    run(
        repo.path(),
        &["commit", "--quiet", "-m", "same blobs new paths"],
    );
    let new_tree = line(run(repo.path(), &["rev-parse", "HEAD^{tree}"])).unwrap();
    let snapshot = Snapshot::prepare(repo.path(), Kind::Commit(historical)).unwrap();
    let index = index_bytes(repo.path());
    assert!(snapshot.fingerprints().is_ok());
    let path = |oid: &str| {
        repo.path()
            .join(".git/objects")
            .join(&oid[..2])
            .join(&oid[2..])
    };
    let original = path(&old_tree);
    assert_eq!(fs::canonicalize(&original).unwrap(), original);
    fs::remove_file(&original).unwrap();
    fs::copy(path(&new_tree), &original).unwrap();
    assert_eq!(index_bytes(repo.path()), index);
    assert!(snapshot.verify_source().is_err());
    assert!(snapshot.fingerprints().is_err());
}

#[test]
fn fingerprint_binds_referenced_blob_bytes_not_only_index_object_names() {
    let repo = repo();
    let snapshot = Snapshot::prepare(repo.path(), Kind::Index).unwrap();
    let fingerprints = snapshot.fingerprints().unwrap();
    assert_eq!(snapshot.fingerprints().unwrap(), fingerprints);
    let original_index = index_bytes(repo.path());
    let original = line(run(repo.path(), &["rev-parse", "HEAD:code.rs"])).unwrap();
    let other = line(
        git(
            repo.path(),
            &["hash-object", "-w", "--stdin"],
            Some(b"pub fn altered_object() {}\n"),
        )
        .unwrap(),
    )
    .unwrap();
    let object_path = |oid: &str| {
        repo.path()
            .join(".git/objects")
            .join(&oid[..2])
            .join(&oid[2..])
    };
    let target = object_path(&original);
    assert_eq!(fs::canonicalize(&target).unwrap(), target);
    fs::remove_file(&target).unwrap();
    fs::copy(object_path(&other), &target).unwrap();
    assert_eq!(index_bytes(repo.path()), original_index);
    assert!(snapshot.verify_source().is_err());
    assert!(snapshot.fingerprints().is_err());
}
