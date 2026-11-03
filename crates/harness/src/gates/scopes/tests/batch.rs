use super::super::git::Batch;
use super::*;

#[test]
fn batch_reads_binary_empty_large_and_packed_objects_without_desynchronizing() {
    let repo = repo();
    let values = [
        Vec::new(),
        b"\0blob\ncommit\xff\n".to_vec(),
        vec![b'x'; 200_000],
    ];
    let objects: Vec<_> = values
        .iter()
        .map(|bytes| {
            line(git(repo.path(), &["hash-object", "-w", "--stdin"], Some(bytes)).unwrap()).unwrap()
        })
        .collect();
    for (i, bytes) in values.iter().enumerate() {
        put(repo.path(), &format!("blob-{i}"), bytes);
    }
    run(repo.path(), &["add", "."]);
    run(repo.path(), &["commit", "--quiet", "-m", "binary blobs"]);
    run(repo.path(), &["repack", "-ad"]);
    run(repo.path(), &["prune-packed"]);
    let mut batch = Batch::new(repo.path()).unwrap();
    for _ in 0..2 {
        for (object, bytes) in objects.iter().zip(&values) {
            assert_eq!(batch.read(object, "blob").unwrap(), *bytes);
        }
    }
    batch.finish().unwrap();
}

#[test]
fn batch_rejects_non_oid_missing_object_and_wrong_type() {
    let repo = repo();
    let head = resolve(repo.path(), "HEAD", "commit").unwrap();
    for (object, kind) in [
        ("HEAD", "commit"),
        (&"0".repeat(40), "blob"),
        (&head, "blob"),
    ] {
        let mut batch = Batch::new(repo.path()).unwrap();
        assert!(batch.read(object, kind).is_err());
    }
}

#[test]
fn bulk_export_preserves_newline_quotes_binary_and_duplicate_blob_paths() {
    let repo = repo();
    for name in ["with\nnewline", "with\"quote", "duplicate"] {
        put(repo.path(), name, b"\0\xffbinary\n");
    }
    run(repo.path(), &["add", "."]);
    let snapshot = Snapshot::prepare(repo.path(), Kind::Index).unwrap();
    snapshot
        .run_checked(|root| {
            assert_eq!(fs::read(root.join("with\nnewline"))?, b"\0\xffbinary\n");
            Ok(())
        })
        .unwrap();
}

#[test]
fn sha256_index_and_commit_export_keep_object_format() {
    let repo = tempfile::tempdir().unwrap();
    run(
        repo.path(),
        &["init", "--quiet", "--template=", "--object-format=sha256"],
    );
    run(repo.path(), &["config", "user.name", "Fixture"]);
    run(
        repo.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    run(repo.path(), &["config", "commit.gpgsign", "false"]);
    put(repo.path(), "file", b"\0binary\n");
    run(repo.path(), &["add", "."]);
    run(repo.path(), &["commit", "--quiet", "-m", "base"]);
    let head = resolve(repo.path(), "HEAD", "commit").unwrap();
    for kind in [Kind::Index, Kind::Commit(head)] {
        let snapshot = Snapshot::prepare(repo.path(), kind).unwrap();
        assert_eq!(snapshot.identity.tree.as_ref().unwrap().len(), 64);
        snapshot.content_witness().unwrap();
    }
}
