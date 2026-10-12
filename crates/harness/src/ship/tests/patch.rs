use super::{fixture, run};
use crate::ship::git;
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn rev(root: &Path) -> String {
    git::git(root, &["rev-parse", "HEAD"]).unwrap()
}

fn identity(root: &Path, base: &str) -> Vec<u8> {
    git::change_identity_from(root, base, &rev(root)).unwrap().1
}

fn commit(root: &Path, data: &[u8]) {
    fs::write(root.join("patch.txt"), data).unwrap();
    run(root, &["add", "patch.txt"]);
    run(root, &["commit", "-q", "-m", "own patch"]);
}

#[test]
fn context_insertions_can_split_a_hunk_without_changing_the_patch() {
    let (_temp, root) = fixture("true");
    run(&root, &["checkout", "-q", "dev"]);
    commit(&root, b"before\nA\nB\nafter\n");
    let old_base = rev(&root);
    run(&root, &["checkout", "-q", "-B", "feature", &old_base]);
    commit(&root, b"before\nC\nD\nafter\n");
    let approved = identity(&root, &old_base);
    run(&root, &["checkout", "-q", "dev"]);
    commit(
        &root,
        b"new context\nbefore\nA\ninserted context\nB\nafter\n",
    );
    let new_base = rev(&root);
    run(&root, &["checkout", "-q", "-B", "feature", &new_base]);
    commit(
        &root,
        b"new context\nbefore\nC\ninserted context\nD\nafter\n",
    );
    assert_eq!(approved, identity(&root, &new_base));
}

#[test]
fn patch_identity_preserves_semantic_whitespace_and_eof_newlines() {
    let (_temp, root) = fixture("true");
    let base = git::git(&root, &["rev-parse", "origin/dev"]).unwrap();
    for (first, second) in [
        (
            b"const S: &str = \"a b\";\n".as_slice(),
            b"const S: &str = \"ab\";\n".as_slice(),
        ),
        (b"line\n".as_slice(), b"line".as_slice()),
        (
            b"+++body\n---body\n".as_slice(),
            b"+++different\n---body\n".as_slice(),
        ),
    ] {
        commit(&root, first);
        let approved = identity(&root, &base);
        commit(&root, second);
        assert_ne!(approved, identity(&root, &base));
    }
}

#[test]
fn mode_changes_and_binary_content_are_part_of_patch_identity() {
    let (_temp, root) = fixture("true");
    let base = git::git(&root, &["rev-parse", "origin/dev"]).unwrap();
    commit(&root, b"\0binary a\n");
    let approved = identity(&root, &base);
    commit(&root, b"\0binary b\n");
    assert_ne!(approved, identity(&root, &base));
    let binary = identity(&root, &base);
    fs::set_permissions(root.join("patch.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    run(&root, &["commit", "-q", "-am", "change mode"]);
    assert_ne!(binary, identity(&root, &base));
}

#[test]
fn a_mode_only_change_survives_unrelated_content_changes_in_its_base() {
    let (_temp, root) = fixture("true");
    run(&root, &["checkout", "-q", "dev"]);
    commit(&root, b"parent content\n");
    let old_base = rev(&root);
    run(&root, &["checkout", "-q", "-B", "feature", &old_base]);
    fs::set_permissions(root.join("patch.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    run(&root, &["commit", "-q", "-am", "own mode change"]);
    let approved = identity(&root, &old_base);
    run(&root, &["checkout", "-q", "dev"]);
    commit(&root, b"changed parent content\n");
    let new_base = rev(&root);
    run(&root, &["checkout", "-q", "-B", "feature", &new_base]);
    fs::set_permissions(root.join("patch.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    run(&root, &["commit", "-q", "-am", "own mode change"]);
    assert_eq!(approved, identity(&root, &new_base));
}
