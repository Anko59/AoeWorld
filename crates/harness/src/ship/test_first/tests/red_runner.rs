use crate::ship::{git, tests::run};
use std::process::Command;

#[test]
fn linked_worktree_red_mounts_external_common_git_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("main");
    std::fs::create_dir(&main).unwrap();
    run(&main, &["init", "-q", "-b", "feature"]);
    run(&main, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let linked = temp.path().join("linked");
    run(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked.to_str().unwrap(),
        ],
    );
    assert!(linked.join(".git").is_file());
    let common = git::git(
        &linked,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .unwrap();
    let mut command = Command::new("docker");
    super::mount_git(&mut command, &linked).unwrap();
    let args: Vec<_> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, ["-v", &format!("{common}:{common}")]);
    let mut ordinary = Command::new("docker");
    super::mount_git(&mut ordinary, &main).unwrap();
    assert_eq!(
        ordinary.get_args().count(),
        0,
        "the root bind already covers ordinary .git"
    );
}
