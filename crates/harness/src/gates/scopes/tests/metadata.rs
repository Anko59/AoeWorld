use super::*;

#[test]
fn changed_private_metadata_fails_even_when_head_tree_is_unchanged() {
    let repo = repo();
    for name in ["HEAD", "config", "info/attributes"] {
        let snapshot = Snapshot::prepare(repo.path(), Kind::Index).unwrap();
        let original_index = index_bytes(repo.path());
        let path = snapshot.root().join(".git").join(name);
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend_from_slice(b"\n");
        fs::write(&path, bytes).unwrap();
        assert!(
            snapshot
                .run_checked(|_| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("private Git metadata")
        );
        assert_eq!(original_index, index_bytes(repo.path()));
    }
}

#[cfg(unix)]
#[test]
fn private_git_alias_rejection_never_writes_into_source_metadata() {
    for gitdir_file in [false, true] {
        let repo = repo();
        let snapshot = Snapshot::prepare(repo.path(), Kind::Index).unwrap();
        let index_before = index_bytes(repo.path());
        let refs_before = run(
            repo.path(),
            &["for-each-ref", "--format=%(refname) %(objectname)"],
        );
        let source_head = fs::read(repo.path().join(".git/HEAD")).unwrap();
        let private = snapshot.root().join(".git");
        assert_eq!(fs::canonicalize(&private).unwrap(), private);
        let saved = snapshot.root().join(".saved-private-metadata");
        assert_eq!(saved.parent().unwrap(), snapshot.root());
        fs::rename(&private, &saved).unwrap();
        if gitdir_file {
            fs::write(
                &private,
                format!("gitdir: {}\n", repo.path().join(".git").display()),
            )
            .unwrap();
        } else {
            std::os::unix::fs::symlink(repo.path().join(".git"), &private).unwrap();
        }
        assert!(
            snapshot
                .run_checked(|_| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("private Git metadata")
        );
        assert_eq!(index_before, index_bytes(repo.path()));
        assert_eq!(
            source_head,
            fs::read(repo.path().join(".git/HEAD")).unwrap()
        );
        assert_eq!(
            refs_before,
            run(
                repo.path(),
                &["for-each-ref", "--format=%(refname) %(objectname)"]
            )
        );
    }
}
