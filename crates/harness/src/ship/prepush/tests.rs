use super::reusable;
use crate::ship::evidence::{self, Evidence, Verdict};
use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["config", "user.email", "t@example.com"]);
    git(root.path(), &["config", "user.name", "t"]);
    std::fs::write(root.path().join("a"), "a\n").unwrap();
    git(root.path(), &["add", "a"]);
    git(root.path(), &["commit", "-q", "-m", "a"]);
    root
}

fn record(root: &Path, verdict: Verdict, cadence: &str) -> String {
    let head = git(root, &["rev-parse", "HEAD"]);
    evidence::write(
        root,
        &Evidence {
            version: 1,
            cadence: cadence.into(),
            head: head.clone(),
            tree: git(root, &["rev-parse", "HEAD^{tree}"]),
            branch: "feature".into(),
            base: "dev".into(),
            merge_base: head.clone(),
            changed: Vec::new(),
            gates: Vec::new(),
            verdict,
            started: 0,
            finished: 0,
        },
    )
    .unwrap();
    head
}

#[test]
fn passing_preflight_evidence_for_the_clean_pushed_head_is_reused() {
    let root = repository();
    let head = record(root.path(), Verdict::Pass, "preflight");
    assert!(reusable(root.path(), Some(&head)).unwrap());
}

#[test]
fn anything_else_runs_the_preflight() {
    let root = repository();
    let head = git(root.path(), &["rev-parse", "HEAD"]);
    // No claim, a malformed claim, or no evidence at all.
    assert!(!reusable(root.path(), None).unwrap());
    assert!(!reusable(root.path(), Some("HEAD")).unwrap());
    assert!(!reusable(root.path(), Some(&head)).unwrap());
    // Failing, incomplete or other-cadence evidence.
    for (verdict, cadence) in [
        (Verdict::Fail, "preflight"),
        (Verdict::Incomplete, "preflight"),
        (Verdict::Pass, "pr"),
    ] {
        record(root.path(), verdict, cadence);
        assert!(
            !reusable(root.path(), Some(&head)).unwrap(),
            "{verdict:?} {cadence}"
        );
    }
    // Passing evidence, but the claim is not HEAD or the tree is dirty.
    record(root.path(), Verdict::Pass, "preflight");
    std::fs::write(root.path().join("a"), "changed\n").unwrap();
    assert!(!reusable(root.path(), Some(&head)).unwrap());
    git(root.path(), &["commit", "-qam", "b"]);
    assert!(!reusable(root.path(), Some(&head)).unwrap());
}
