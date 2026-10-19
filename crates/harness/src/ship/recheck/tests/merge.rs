//! The merge fallback: a rebase replays every commit, so it can conflict where a merge of the
//! same base is clean. The reviewed patch must be the same either way.
use super::{start::fetching, *};

/// A branch that adds `list.txt` in one commit and removes it in the next, so replaying the
/// first commit conflicts with a base that also adds it, while the merged result is clean.
fn add_then_remove(root: &Path) {
    fs::write(root.join("list.txt"), "a\n").unwrap();
    run(root, &["add", "list.txt"]);
    run(root, &["commit", "-q", "-m", "add list"]);
    run(root, &["rm", "-q", "list.txt"]);
    run(root, &["commit", "-q", "-m", "remove list"]);
}

fn parents(root: &Path) -> usize {
    git(root, &["rev-list", "--parents", "-n", "1", "HEAD"])
        .split_whitespace()
        .count()
        - 1
}

fn own_patch(root: &Path, base: &str, commit: &str) -> Vec<u8> {
    git::change_identity_from(root, base, commit).unwrap().1
}

#[test]
fn a_conflicting_rebase_falls_back_to_a_clean_merge_at_the_start() {
    let (temp, root) = fixture(LOGGED);
    add_then_remove(&root);
    let (old_base, old_head) = (git(&root, &["rev-parse", "dev"]), head(&root));
    let reviewed = own_patch(&root, &old_base, &old_head);
    let moved = advance(&temp, "dev", "list.txt", "d\n");
    let shipped = ship(&root, &fetching()).unwrap();
    assert_eq!(parents(&root), 2, "a merge commit, not a rebase");
    assert!(contains(&root, &old_head), "the branch commits are kept");
    assert!(contains(&root, &moved));
    assert_eq!(fs::read_to_string(root.join("list.txt")).unwrap(), "d\n");
    assert_eq!(shipped.base, moved);
    assert_eq!(shipped.changed, ["README.md"]);
    assert_eq!(own_patch(&root, &moved, &shipped.head), reviewed);
    assert_eq!(remote_tip(&root, "feature"), shipped.head);
    assert_eq!(runs(&temp), 1);
}

#[test]
fn a_conflicting_rebase_falls_back_to_a_clean_merge_during_the_ship() {
    let (temp, root) = fixture(LOGGED);
    add_then_remove(&root);
    let (old_base, old_head) = (git(&root, &["rev-parse", "dev"]), head(&root));
    let reviewed = own_patch(&root, &old_base, &old_head);
    let moved = advance(&temp, "dev", "list.txt", "d\n");
    let shipped = ship(&root, &offline()).unwrap();
    let recheck = shipped.recheck.as_ref().expect("recheck recorded");
    assert_eq!(
        (recheck.rounds, recheck.commits.clone()),
        (1, vec![moved.clone()])
    );
    assert_eq!(parents(&root), 2);
    assert!(contains(&root, &old_head) && contains(&root, &moved));
    assert_eq!(own_patch(&root, &moved, &shipped.head), reviewed);
    assert_eq!(remote_tip(&root, "feature"), shipped.head);
}

#[test]
fn a_real_conflict_in_both_still_stops_with_the_files() {
    let (temp, root) = fixture("true");
    add_then_remove(&root);
    let before = head(&root);
    advance(&temp, "dev", "README.md", "z\n");
    let error = ship(&root, &offline()).unwrap_err().to_string();
    assert!(error.contains("conflicts in README.md"), "{error}");
    assert!(error.contains("nothing was pushed"), "{error}");
    assert_eq!(head(&root), before);
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    let merging = git(&root, &["rev-parse", "--git-path", "MERGE_HEAD"]);
    assert!(!root.join(merging).exists(), "no merge left half-done");
}
