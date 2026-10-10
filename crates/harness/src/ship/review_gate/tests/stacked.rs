//! Stacked reviews: auto-merge waits for `dev`, and a review recorded against
//! a parent branch is reused for the same branch after the parent merged and
//! the branch was rebased onto dev, only for the very same change.
use super::{graded, unstored};
use crate::{
    review::{self, Report, Tier},
    ship::{
        Options, git,
        github::description,
        judge, review_gate,
        stack::Parent,
        tests::{offline, run},
    },
};
use std::path::Path;

fn rev(root: &Path, name: &str) -> String {
    git::git(root, &["rev-parse", name]).unwrap()
}

/// The passing review of `feature` stacked on `parent`, as `make ship
/// SHIP_BASE=parent` stores it.
fn stacked_review(root: &Path) -> Report {
    let _base = review::base::scope("parent");
    let mut report = unstored(root, Tier::Low, 9);
    report.base_branch = "parent".into();
    report.base = rev(root, "refs/remotes/origin/parent");
    report.merge_base = report.base.clone();
    report.store(root).unwrap();
    report
}

/// `parent` squash-merges into dev; `feature` is rebased onto origin/dev.
fn merge_parent_and_restack(root: &Path) {
    run(root, &["checkout", "-q", "dev"]);
    run(root, &["merge", "-q", "--squash", "parent"]);
    run(root, &["commit", "-q", "-m", "parent change (#12)"]);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["fetch", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
    run(
        root,
        &["rebase", "-q", "--onto", "origin/dev", "parent", "feature"],
    );
}

fn reuse(root: &Path, branch: &str) -> Option<Report> {
    let head = rev(root, "HEAD");
    Report::reuse_for_change(root, &head, branch, &["low"], "").unwrap()
}

#[test]
fn a_stacked_review_is_reused_after_the_parent_merged() {
    let (_temp, root) = crate::ship::tests::stack::stacked_fixture();
    let reviewed = stacked_review(&root);
    merge_parent_and_restack(&root);
    assert!(reuse(&root, "other").is_none(), "only the same branch");
    let reused = reuse(&root, "feature").expect("identical change reuses");
    assert_eq!(reused.reused_from.as_deref(), Some(reviewed.head.as_str()));
    assert_eq!(reused.head, rev(&root, "HEAD"));
    assert_eq!(reused.base_branch, "dev");
    assert_eq!(reused.base, rev(&root, "refs/remotes/origin/dev"));
    assert!(reused.passes());

    // A later rebase reuses the original review, never the reuse report.
    run(&root, &["checkout", "-q", "dev"]);
    std::fs::write(root.join("BASE.md"), "moved\n").unwrap();
    run(&root, &["add", "BASE.md"]);
    run(&root, &["commit", "-q", "-m", "move dev"]);
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["fetch", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    let again = reuse(&root, "feature").expect("still the same change");
    assert_eq!(again.reused_from.as_deref(), Some(reviewed.head.as_str()));
}

#[test]
fn any_byte_change_after_the_restack_gets_a_fresh_review() {
    let (_temp, root) = crate::ship::tests::stack::stacked_fixture();
    stacked_review(&root);
    merge_parent_and_restack(&root);
    std::fs::write(root.join("README.md"), "y \n").unwrap();
    run(&root, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    assert!(reuse(&root, "feature").is_none());
}

#[test]
fn a_stacked_review_never_answers_for_the_same_commit_against_dev() {
    // Shipped onto dev before the parent merged: the change now carries the
    // parent's file too, so the stacked review of this very commit does not
    // answer for it, and does not block a fresh review either.
    let (_temp, root) = crate::ship::tests::stack::stacked_fixture();
    let reviewed = stacked_review(&root);
    assert!(
        Report::load_passing(&root, &reviewed.head, &["low"], "")
            .unwrap()
            .is_none()
    );
    assert!(!Report::has_complete(&root, &reviewed.head, &["low"]).unwrap());
    assert!(reuse(&root, "feature").is_none());
    let _base = review::base::scope("parent");
    let task = Report::effective_task(&root, &reviewed.head, "").unwrap();
    assert_eq!(task.trim(), "change");
    assert!(
        Report::load_passing(&root, &reviewed.head, &["low"], "")
            .unwrap()
            .is_some()
    );
}

#[test]
fn the_stacked_review_is_reused_on_the_same_parent_after_a_rebase() {
    let (_temp, root) = crate::ship::tests::stack::stacked_fixture();
    let reviewed = stacked_review(&root);
    run(&root, &["checkout", "-q", "parent"]);
    std::fs::write(root.join("OTHER.md"), "more\n").unwrap();
    run(&root, &["add", "OTHER.md"]);
    run(&root, &["commit", "-q", "-m", "parent fix"]);
    run(&root, &["push", "-q", "origin", "parent"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "parent"]);
    let _base = review::base::scope("parent");
    let reused = reuse(&root, "feature").expect("same change on the moved parent");
    assert_eq!(reused.reused_from.as_deref(), Some(reviewed.head.as_str()));
    assert_eq!(reused.base_branch, "parent");
}

#[test]
fn auto_merge_is_armed_only_onto_dev() {
    let (_temp, root) = crate::ship::tests::fixture("true");
    let report = graded(&root, Tier::Low, 9);
    let calls = review_gate::publish_calls_onto("o/r", "u", &report, "parent").unwrap();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].join(" ").contains("context=harness/review"));
    assert!(
        !calls
            .iter()
            .flatten()
            .any(|a| a == "merge" || a == "--auto")
    );
    let calls = review_gate::publish_calls_onto("o/r", "u", &report, "dev").unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls[1].iter().any(|a| a == "--auto"));
    assert_eq!(
        calls,
        review_gate::publish_calls("o/r", "u", &report).unwrap()
    );
}

#[test]
fn a_stacked_description_names_its_parent() {
    let (_temp, root) = crate::ship::tests::stack::stacked_fixture();
    let evidence = judge(
        &root,
        &Options {
            base: "parent".into(),
            ..offline()
        },
    )
    .unwrap();
    let report = graded(&root, Tier::Low, 9);
    let body = "<!-- level: low -->\n## Why\n> \"stack it\" — the user\nBecause.\n## What\nA README edit.\n";
    let options = Options {
        base: "parent".into(),
        parent: Some(Parent {
            number: 12,
            branch: "parent".into(),
            url: "https://github.com/o/r/pull/12".into(),
        }),
        ..offline()
    };
    let path = description(&root, &evidence, &options, &report, body).unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("Stacked on #12"), "{text}");
    assert!(text.contains("# 🧑 For humans"), "{text}");
}
