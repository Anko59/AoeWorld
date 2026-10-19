//! The long Make lists are written one item per line, so two branches that each add a different
//! item merge without a conflict (they used to edit the same single line).
use super::makefile::{definitions, joined_continuations, repository};
use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The repository's own `make/heavy.mk`, as it is on disk (continuations intact).
fn heavy() -> String {
    fs::read_to_string(repository().join("make/heavy.mk")).unwrap()
}

fn commit_variant(root: &Path, branch: &str, after: &str, add: &str) {
    git(root, &["checkout", "-q", "-b", branch, "main"]);
    let text = fs::read_to_string(root.join("heavy.mk")).unwrap();
    let anchor = format!("  {after} \\\n");
    assert!(text.contains(&anchor), "{after} is a SLOT_ENV item");
    let text = text.replacen(&anchor, &format!("{anchor}  {add} \\\n"), 1);
    fs::write(root.join("heavy.mk"), text).unwrap();
    git(root, &["commit", "-q", "-am", branch]);
}

#[test]
fn two_branches_adding_different_slot_env_variables_merge_without_a_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("heavy.mk"), heavy()).unwrap();
    git(root, &["add", "heavy.mk"]);
    git(root, &["commit", "-q", "-m", "base"]);
    commit_variant(root, "a", "-e HARNESS_TEST_FILTER", "-e AOE_ADDED_BY_A");
    commit_variant(root, "b", "-e AOE_E2E_SPECS", "-e AOE_ADDED_BY_B");
    git(root, &["checkout", "-q", "a"]);
    git(root, &["merge", "-q", "--no-edit", "b"]);
    let merged = fs::read_to_string(root.join("heavy.mk")).unwrap();
    assert!(merged.contains("-e AOE_ADDED_BY_A") && merged.contains("-e AOE_ADDED_BY_B"));
    // The audit reads the merged file the way make does: both variables are in SLOT_ENV.
    let files = vec![("make/heavy.mk".to_owned(), joined_continuations(&merged))];
    let slot_env = definitions(&files)["SLOT_ENV"].clone();
    assert!(slot_env.contains("-e AOE_ADDED_BY_A") && slot_env.contains("-e AOE_ADDED_BY_B"));
}
