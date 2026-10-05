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
fn cached_driver_target_is_prohibited_and_never_reused_for_scanner_storage() {
    const ROOT: &str = "AOE_MUTATION_DRIVER_TARGET_FIXTURE";
    if let Some(root) = std::env::var_os(ROOT) {
        let root = PathBuf::from(root);
        let driver = root.join(".cache/mutation/driver-target");
        let snapshot = Snapshot::prepare_independent(
            &root,
            Kind::Commit(scopes::resolved_head(&root).unwrap()),
        )
        .unwrap();
        let storage = Storage::new(&root, &snapshot).unwrap();
        assert!(storage.prohibited.contains(&driver));
        assert!(disjoint(storage.raw(), &driver));
        assert!(disjoint(storage.target(), &driver));
        assert!(disjoint(storage.target(), snapshot.root()));
        assert_eq!(
            fs::read(driver.join("cache-sentinel")).unwrap(),
            b"unqualified driver cache"
        );
        storage.verify().unwrap();
        return;
    }
    let root = repository();
    let driver = root.path().join(".cache/mutation/driver-target");
    fs::create_dir_all(&driver).unwrap();
    fs::write(driver.join("cache-sentinel"), b"unqualified driver cache").unwrap();
    let executable = std::env::current_exe().unwrap();
    let captured = crate::process::capture_in(
        root.path(),
        executable.to_str().unwrap(),
        &[
            "--exact",
            "mutation::storage::tests::cached_driver_target_is_prohibited_and_never_reused_for_scanner_storage",
            "--nocapture",
        ],
        &[
            (ROOT, root.path().to_str().unwrap()),
            ("CARGO_TARGET_DIR", driver.to_str().unwrap()),
        ],
        std::time::Duration::from_secs(30),
        &crate::process::Cancellation::default(),
    );
    assert!(matches!(
        captured.exit,
        crate::process::CaptureExit::Success
    ));
    assert!(!captured.truncated);
    assert!(
        std::str::from_utf8(&captured.stdout)
            .unwrap()
            .contains("test result: ok. 1 passed; 0 failed; 0 ignored;")
    );
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
