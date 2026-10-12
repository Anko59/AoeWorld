use super::unstored;
use crate::{
    review::{self, Report, Tier},
    ship::{
        Options, git, judge, review_gate, ship_with,
        tests::{fixture, offline, run},
    },
};
use std::{fs, path::Path, process::Command};

const PARENT: &str = "parent context\na\nb\nc\nd\ne\nf\nx\ng\nh\ni\nj\nk\n";

fn parent_child(gate: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let (temp, root) = fixture(gate);
    run(&root, &["checkout", "-q", "-b", "parent", "dev"]);
    fs::write(root.join("shared.txt"), PARENT).unwrap();
    fs::write(root.join("dependency.rs"), "pub fn value() {}\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "parent"]);
    run(&root, &["push", "-q", "origin", "parent"]);
    run(&root, &["checkout", "-q", "-B", "feature", "parent"]);
    fs::write(root.join("shared.txt"), PARENT.replace("\nx\n", "\ny\n")).unwrap();
    fs::write(
        root.join("child.rs"),
        "mod dependency; pub fn child() { dependency::value(); }\n",
    )
    .unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "child patch"]);
    (temp, root)
}

fn approval(root: &Path) -> Report {
    let _base = review::base::scope("parent");
    let report = unstored(root, Tier::Low, 9);
    report.store(root).unwrap();
    report
}

fn update_parent(root: &Path, text: &str) {
    run(root, &["checkout", "-q", "parent"]);
    fs::write(root.join("shared.txt"), text).unwrap();
    run(root, &["commit", "-q", "-am", "parent fixes"]);
    run(root, &["push", "-q", "origin", "parent"]);
    run(root, &["checkout", "-q", "feature"]);
}

#[test]
fn parent_context_and_line_shifts_keep_the_childs_approval() {
    let (_temp, root) = parent_child("true");
    let source = approval(&root);
    update_parent(
        &root,
        &format!(
            "new leading context\n{}",
            PARENT.replace("parent context", "fixed parent context")
        ),
    );
    run(&root, &["rebase", "-q", "parent"]);
    let _base = review::base::scope("parent");
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let reused = Report::reuse_for_change(&root, &head, "feature", &["low"], "")
        .unwrap()
        .expect("same changed lines");
    assert_eq!(reused.reused_from.as_deref(), Some(source.head.as_str()));
    assert_eq!(
        reused.base,
        git::git(&root, &["rev-parse", "parent"]).unwrap()
    );
    assert!(reused.markdown().contains("same patch"));
}

#[test]
fn conflict_resolution_that_changes_the_child_patch_requires_review() {
    let (_temp, root) = parent_child("true");
    approval(&root);
    update_parent(&root, &PARENT.replace("\nx\n", "\nz\n"));
    assert!(
        !Command::new("git")
            .current_dir(&root)
            .args(["rebase", "parent"])
            .status()
            .unwrap()
            .success()
    );
    fs::write(root.join("shared.txt"), PARENT.replace("\nx\n", "\ny\n")).unwrap();
    run(&root, &["add", "shared.txt"]);
    assert!(
        Command::new("git")
            .current_dir(&root)
            .env("GIT_EDITOR", "true")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "rebase",
                "--continue"
            ])
            .status()
            .unwrap()
            .success()
    );
    let _base = review::base::scope("parent");
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "")
            .unwrap()
            .is_none()
    );
}

#[test]
fn deleting_a_parent_dependency_cannot_bypass_gates_even_with_cached_approval() {
    let (_temp, root) = parent_child("rustc --crate-type lib child.rs --out-dir ..");
    let source = approval(&root);
    run(&root, &["checkout", "-q", "parent"]);
    run(&root, &["rm", "dependency.rs"]);
    run(&root, &["commit", "-q", "-m", "delete dependency"]);
    run(&root, &["push", "-q", "origin", "parent"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "parent"]);
    let _base = review::base::scope("parent");
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let reused = Report::reuse_for_change(&root, &head, "feature", &["low"], "")
        .unwrap()
        .expect("the child patch itself is unchanged");
    assert_eq!(reused.reused_from.as_deref(), Some(source.head.as_str()));
    let options = Options {
        base: "parent".into(),
        no_review: false,
        ..offline()
    };
    let error = ship_with(&root, &options, &|_, _, _, _, _| {
        panic!("gates must refuse first")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("FAIL"), "{error}");
    assert!(
        git::git(&root, &["ls-remote", "origin", "refs/heads/feature"])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn another_branch_cannot_use_an_exact_head_cached_approval() {
    let (_temp, root) = fixture("true");
    unstored(&root, Tier::Low, 9).store(&root).unwrap();
    run(&root, &["checkout", "-q", "-b", "other"]);
    let evidence = judge(&root, &offline()).unwrap();
    let mut calls = 0;
    let count = std::cell::Cell::new(0);
    review_gate::require(
        &root,
        &evidence,
        &Options {
            no_review: false,
            ..offline()
        },
        &|root, tier, _, _, _| {
            count.set(count.get() + 1);
            let mut report = unstored(root, tier, 9);
            report.branch = "other".into();
            Ok(report)
        },
    )
    .unwrap();
    calls += count.get();
    assert_eq!(calls, 1);
}

#[test]
fn cached_exact_head_approval_requires_the_current_policy() {
    let (_temp, root) = fixture("true");
    let mut source = unstored(&root, Tier::Low, 9);
    source.policy_fingerprint = Some("not the current policy".into());
    source.store(&root).unwrap();
    assert!(
        Report::load_passing(&root, &source.head, &["low"], "")
            .unwrap()
            .is_none()
    );
}
