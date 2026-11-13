use super::*;

#[test]
fn fresh_compile_targets_never_reuse_or_delete_another_runs_inputs() {
    let root = tempfile::tempdir().unwrap();
    let sentinel = root.path().join("keep.txt");
    fs::write(&sentinel, "original input").unwrap();
    let first = fresh_target(root.path()).unwrap();
    let second = fresh_target(root.path()).unwrap();
    assert_ne!(first.path(), second.path());
    let first_path = first.path().to_owned();
    drop(first);
    assert!(!first_path.exists());
    assert!(second.path().is_dir());
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "original input");
}
