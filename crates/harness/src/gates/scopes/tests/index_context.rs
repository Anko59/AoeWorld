use super::*;

#[test]
fn effective_source_index_is_distinct_from_default_and_never_leaks_into_private_git() {
    let repo = repo();
    let root = repo.path();
    put(root, "code.rs", "pub fn staged_default() {}\n");
    run(root, &["add", "code.rs"]);
    let alternate = root.join(".git/alternate-index");
    fs::copy(root.join(".git/index"), &alternate).unwrap();
    put(root, "code.rs", "pub fn pending_commit() {}\n");
    git_at(root, Some(&alternate), &["add", "code.rs"], None).unwrap();
    put(root, "code.rs", "pub fn working_only() {}\n");
    let before = index_bytes(root);
    let alternate_before = fs::read(&alternate).unwrap();
    let snapshot =
        Snapshot::prepare_index(root, Kind::Index, Some(Path::new(".git/alternate-index")))
            .unwrap();
    assert_eq!(
        fs::read(snapshot.root().join("code.rs")).unwrap(),
        b"pub fn pending_commit() {}\n"
    );
    assert_eq!(
        snapshot.identity.effective_index,
        fs::canonicalize(&alternate).unwrap()
    );
    snapshot.run_checked(|_| Ok(())).unwrap();
    assert_eq!(index_bytes(root), before);
    assert_eq!(fs::read(&alternate).unwrap(), alternate_before);
    assert!(
        command(snapshot.root())
            .get_envs()
            .any(|(name, value)| name == "GIT_INDEX_FILE" && value.is_none())
            || std::env::var_os("GIT_INDEX_FILE").is_none()
    );
    put(root, "code.rs", "pub fn new_pending_commit() {}\n");
    git_at(root, Some(&alternate), &["add", "code.rs"], None).unwrap();
    assert!(snapshot.verify_source().is_err());
}

#[test]
fn missing_external_or_symlink_index_never_falls_back_to_default() {
    let repo = repo();
    let outside = tempfile::tempdir().unwrap();
    let external = outside.path().join("index");
    fs::copy(repo.path().join(".git/index"), &external).unwrap();
    for index in [
        Path::new(".git/absent-index"),
        Path::new(".git"),
        external.as_path(),
    ] {
        assert!(Snapshot::prepare_index(repo.path(), Kind::Index, Some(index)).is_err());
    }
    #[cfg(unix)]
    {
        let alias = repo.path().join(".git/alias-index");
        std::os::unix::fs::symlink("index", &alias).unwrap();
        assert!(Snapshot::prepare_index(repo.path(), Kind::Index, Some(&alias)).is_err());
    }
}

#[test]
fn source_builder_alone_applies_effective_index() {
    let repo = repo();
    let mut child = command(repo.path());
    child.env("GIT_INDEX_FILE", repo.path().join(".git/index"));
    assert!(
        child
            .get_envs()
            .any(|(name, value)| name == "GIT_INDEX_FILE" && value.is_some())
    );
    assert!(
        !command(repo.path())
            .get_envs()
            .any(|(name, value)| name == "GIT_INDEX_FILE" && value.is_some())
    );
    let make = include_str!("../../../../../../Makefile");
    assert!(
        make.lines()
            .find(|line| line.starts_with("DOCKER_RUN :="))
            .unwrap()
            .contains("-e GIT_INDEX_FILE")
    );
}
