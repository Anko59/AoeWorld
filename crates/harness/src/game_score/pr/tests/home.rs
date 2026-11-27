use super::*;

#[test]
fn the_home_is_beside_the_cargo_cache_in_the_common_git_directory() {
    let common = Path::new("/repo/.git");
    assert_eq!(
        in_directory(None, common),
        Path::new("/repo/.git/aoe-game-score")
    );
    assert_eq!(
        in_directory(Some(OsString::new()), common),
        Path::new("/repo/.git/aoe-game-score"),
        "an empty variable names nothing"
    );
}

#[test]
fn a_named_home_wins() {
    assert_eq!(
        in_directory(Some(OsString::from("/elsewhere")), Path::new("/repo/.git")),
        Path::new("/elsewhere")
    );
}

#[test]
fn the_default_package_directory_is_the_homes_except_in_ci_where_the_workflows_cache_it() {
    let home = Some(PathBuf::from("/repo/.git/aoe-game-score"));
    assert_eq!(
        packages_default(false, home.clone()),
        Path::new("/repo/.git/aoe-game-score/packages")
    );
    assert_eq!(
        packages_default(true, home),
        Path::new(".cache/game-map/packages")
    );
    assert_eq!(
        packages_default(false, None),
        Path::new(".cache/game-score/packages"),
        "outside a Git checkout"
    );
}

#[test]
fn the_folders_of_the_home() {
    let home = Path::new("/h");
    assert_eq!(packages(home), Path::new("/h/packages"));
    assert_eq!(results(home), Path::new("/h/results"));
    assert_eq!(base_checkout(home), Path::new("/h/base"));
}

#[test]
fn a_worktree_and_its_main_checkout_share_one_home() {
    let repository = tempfile::tempdir().expect("repository");
    let git = |directory: &Path, args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(directory)
            .args(args)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(repository.path(), &["init", "-q"]);
    git(
        repository.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(repository.path(), &["config", "user.name", "Fixture"]);
    git(repository.path(), &["config", "commit.gpgsign", "false"]);
    git(
        repository.path(),
        &["commit", "-q", "--allow-empty", "-m", "one"],
    );
    let linked = repository.path().join("linked");
    git(
        repository.path(),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &linked.to_string_lossy(),
        ],
    );
    let main = crate::hooks::common_directory(repository.path()).expect("main");
    let from_linked = crate::hooks::common_directory(&linked).expect("linked");
    assert_eq!(
        std::fs::canonicalize(&main).expect("main"),
        std::fs::canonicalize(&from_linked).expect("linked")
    );
    assert_eq!(
        in_directory(None, &from_linked)
            .file_name()
            .and_then(|n| n.to_str()),
        Some("aoe-game-score")
    );
}
