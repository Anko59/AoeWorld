use super::super::tests::repository;
use super::*;
use crate::gates::scopes::Kind;

#[test]
fn both_direction_alias_policy_normal_missing_suffix_and_symlink_rejection() {
    assert!(!disjoint(
        Path::new("/tmp/source"),
        Path::new("/tmp/source/child")
    ));
    assert!(!disjoint(
        Path::new("/tmp/source/child"),
        Path::new("/tmp/source")
    ));
    assert!(!disjoint(
        Path::new("/tmp/source"),
        Path::new("/tmp/source")
    ));
    assert!(disjoint(
        Path::new("/tmp/source"),
        Path::new("/tmp/source-extra")
    ));
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        projection(&root.path().join("not-yet-created/sub")).unwrap(),
        root.path().join("not-yet-created/sub")
    );
    assert!(projection(&root.path().join("../escape")).is_err());
    fs::write(root.path().join("file"), b"not directory").unwrap();
    assert!(projection(&root.path().join("file/child")).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.path(), root.path().join("alias")).unwrap();
        assert!(projection(&root.path().join("alias/child")).is_err());
    }
}
#[test]
fn actual_fresh_root_identity_changes_reject_and_other_allocations_are_disjoint() {
    let root = repository();
    let snapshot = Snapshot::prepare_independent(
        root.path(),
        Kind::Commit(scopes::resolved_head(root.path()).unwrap()),
    )
    .unwrap();
    let storage = Storage::new(root.path(), &snapshot).unwrap();
    let other = Storage::new(root.path(), &snapshot).unwrap();
    assert!(disjoint(storage.raw(), other.raw()));
    assert!(disjoint(storage.target(), other.target()));
    storage.verify().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = storage.raw();
        assert_eq!(path.canonicalize().unwrap(), path);
        let mode = fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        assert_eq!(
            fs::metadata(storage.target()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o020)).unwrap();
        assert!(storage.verify().is_err());
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
        storage.verify().unwrap();
    }
}
#[test]
fn temporary_root_under_source_is_rejected_before_any_scanner() {
    const MARKER: &str = "AOE_MUTATION_STORAGE_ENV_CHILD";
    if let Some(root) = std::env::var_os(MARKER) {
        let root = PathBuf::from(root);
        let snapshot = Snapshot::prepare_independent(
            &root,
            Kind::Commit(scopes::resolved_head(&root).unwrap()),
        )
        .unwrap();
        assert!(Storage::new(&root, &snapshot).is_err());
        return;
    }
    let root = repository();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mutation::storage::tests::temporary_root_under_source_is_rejected_before_any_scanner",
            "--nocapture",
        ])
        .env(MARKER, root.path())
        .env("TMPDIR", root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}
