//! An approved patch that is unchanged stays approved when only the
//! description text changed, and every ship says why a review runs
//! (docs/review.md). Reviewers are stubbed and fail where none may run.
use super::{fixture, run, unstored};
use crate::{
    review::{self, Plan, Report, Tier, increment::Scope},
    ship::{Options, git, judge, review_gate::require, tests::offline},
};
use std::{cell::RefCell, fs, path::Path};

const FIRST: &str = "first description\n";
const SECOND: &str = "second description\n";

/// Commit `files` on origin/dev, then rebase `feature` onto it.
fn advance_dev(root: &Path, files: &[(&str, &str)]) {
    run(root, &["checkout", "-q", "dev"]);
    for (path, text) in files {
        fs::write(root.join(path), text).unwrap();
        run(root, &["add", path]);
    }
    fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(root, &["add", "BASE.md"]);
    run(root, &["commit", "-q", "-m", "move dev"]);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["fetch", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
    run(root, &["rebase", "-q", "origin/dev"]);
}

fn policy_change(root: &Path) -> (String, String) {
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("gates/review.json")).unwrap()).unwrap();
    config["tiers"]["medium"]["personas"] = serde_json::json!(["quick", "test-integrity"]);
    ("gates/review.json".into(), config.to_string())
}

/// A stored passing review of HEAD for `task`.
fn approved(root: &Path, tier: Tier, task: &str) -> Report {
    let mut report = unstored(root, tier, 9);
    report.task_fingerprint = Some(Report::task_fingerprint(task));
    report.store(root).unwrap();
    report
}

fn options(temp: &Path, task: &str, tier: Option<Tier>) -> Options {
    let body = temp.join("body.md");
    fs::write(&body, task).unwrap();
    Options {
        no_review: false,
        body_file: Some(body),
        tier,
        ..offline()
    }
}

/// The review gate with a reviewer that records the plan and the announced
/// line, and answers 9/10.
fn ship(root: &Path, options: &Options) -> (Report, Vec<(Plan, String, String)>) {
    let evidence = judge(root, &offline()).unwrap();
    let seen = RefCell::new(Vec::new());
    let report = require(root, &evidence, options, &|root, tier, _, task, plan| {
        let line = review::take_announced().expect("the line is printed before the review");
        seen.borrow_mut()
            .push((plan.clone(), line.clone(), task.to_owned()));
        let mut report = unstored(root, tier, 9);
        report.decision = Some(line);
        report.store(root).unwrap();
        Ok(report)
    })
    .unwrap();
    (report, seen.into_inner())
}

fn no_reviewer(root: &Path, options: &Options) -> Report {
    let evidence = judge(root, &offline()).unwrap();
    require(root, &evidence, options, &|_, _, _, _, _| {
        panic!("no reviewer may run for an approved, unchanged patch")
    })
    .unwrap()
}

fn head(root: &Path) -> String {
    git::git(root, &["rev-parse", "HEAD"]).unwrap()
}

#[test]
fn an_edited_description_after_a_rebase_keeps_the_approval() {
    // PR #361: 9/10 for one description, a clean rebase, another description.
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Medium, FIRST);
    advance_dev(&root, &[]);
    assert_ne!(head(&root), source.head);
    let reused = no_reviewer(&root, &options(temp.path(), SECOND, None));
    assert_eq!(reused.reused_from.as_deref(), Some(source.head.as_str()));
    assert_eq!(reused.head, head(&root));
    assert_eq!(reused.grade, 9);
    assert_eq!(
        reused.task_fingerprint,
        Some(Report::task_fingerprint(SECOND)),
        "the report records the description it now stands under"
    );
    assert_eq!(
        reused.note.as_deref(),
        Some("description text changed after approval; code unchanged")
    );
    assert_eq!(
        reused.decision.as_deref(),
        Some(
            format!(
                "review: reusing 9/10 of {} (patch and policy unchanged; description text changed)",
                &source.head[..12]
            )
            .as_str()
        )
    );
    // It is stored: the next ship of this commit finds it, whatever the text.
    let stored = Report::load_passing(&root, &head(&root), &["medium"], SECOND)
        .unwrap()
        .expect("stored for the rebased head");
    assert_eq!(stored.decision, reused.decision);
    assert!(reused.markdown().contains("description text changed"));
    let status = crate::ship::review_gate::publish_calls("o/r", "u", &stored).unwrap()[0].join(" ");
    assert!(
        status.contains("reused (same patch); description text changed"),
        "{status}"
    );
}

#[test]
fn an_unedited_description_still_reuses_and_says_so() {
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Low, FIRST);
    advance_dev(&root, &[]);
    let reused = no_reviewer(&root, &options(temp.path(), FIRST, None));
    assert_eq!(reused.note, None);
    assert_eq!(
        reused.decision.as_deref(),
        Some(
            format!(
                "review: reusing 9/10 of {} (patch and policy unchanged)",
                &source.head[..12]
            )
            .as_str()
        )
    );
}

#[test]
fn an_unchanged_patch_under_a_changed_policy_gets_a_full_review() {
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Medium, FIRST);
    let (path, text) = policy_change(&root);
    advance_dev(&root, &[(&path, &text)]);
    let (report, seen) = ship(&root, &options(temp.path(), SECOND, None));
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, Plan::Full);
    assert_eq!(
        seen[0].1,
        format!(
            "review: full review — the review policy on dev changed since the 9/10 of {}",
            &source.head[..12]
        )
    );
    assert_eq!(report.decision.as_deref(), Some(seen[0].1.as_str()));
    assert!(report.reused_from.is_none());
}

#[test]
fn an_unchanged_patch_below_the_required_tier_gets_a_full_review_at_that_tier() {
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Low, FIRST);
    advance_dev(&root, &[]);
    let (report, seen) = ship(&root, &options(temp.path(), SECOND, Some(Tier::Medium)));
    assert_eq!(seen.len(), 1);
    assert_eq!(report.tier, "medium");
    assert_eq!(
        seen[0].1,
        format!(
            "review: full review — the 9/10 of {} is a low review and this ship needs at least medium",
            &source.head[..12]
        )
    );

    // The floor itself rose on dev (README.md now belongs to the high suite).
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Low, FIRST);
    let mut registry: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("gates/registry.json")).unwrap()).unwrap();
    registry["suites"][0]["paths"] = serde_json::json!(["gates/**", "README.md"]);
    advance_dev(&root, &[("gates/registry.json", &registry.to_string())]);
    let (report, seen) = ship(&root, &options(temp.path(), SECOND, None));
    assert_eq!(report.tier, "high", "reviewed at the floor");
    assert!(
        seen[0].1.contains(&format!(
            "the 9/10 of {} is a low review and this ship needs at least high",
            &source.head[..12]
        )),
        "{}",
        seen[0].1
    );
}

#[test]
fn one_changed_line_is_an_increment_not_a_reuse() {
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Low, FIRST);
    fs::write(root.join("follow.txt"), "one line\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "follow"]);
    let (_, seen) = ship(&root, &options(temp.path(), FIRST, None));
    assert_eq!(seen.len(), 1);
    let Plan::Increment(increment) = &seen[0].0 else {
        panic!("an increment was expected: {:?}", seen[0].0);
    };
    assert_eq!((increment.lines, increment.closing), (1, true));
    assert_eq!(increment.scope, Scope::Since(source.head.clone()));
    assert_eq!(
        seen[0].1,
        format!(
            "review: closing review of 1 lines since `{}`, approved 9/10",
            &source.head[..12]
        )
    );
}

#[test]
fn an_edited_description_and_one_changed_line_review_that_line() {
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Low, FIRST);
    advance_dev(&root, &[]);
    fs::write(root.join("follow.txt"), "one line\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "follow"]);
    let (report, seen) = ship(&root, &options(temp.path(), SECOND, None));
    assert_eq!(seen.len(), 1, "the changed code is reviewed");
    assert_eq!(seen[0].2, SECOND);
    let Plan::Increment(increment) = &seen[0].0 else {
        panic!("an increment was expected: {:?}", seen[0].0);
    };
    assert_eq!(increment.lines, 1);
    assert_eq!(increment.approved, source.head);
    assert!(report.reused_from.is_none());
}

#[test]
fn no_passing_review_is_a_full_review_that_says_so() {
    let (temp, root) = fixture("true");
    let (_, seen) = ship(&root, &options(temp.path(), FIRST, None));
    assert_eq!(seen[0].0, Plan::Full);
    assert_eq!(
        seen[0].1,
        "review: full review — no passing review of this branch"
    );
}

#[test]
fn an_unreadable_report_is_a_full_review_that_says_so() {
    let (temp, root) = fixture("true");
    let mut source = unstored(&root, Tier::Low, 9);
    source.task_fingerprint = Some(Report::task_fingerprint(FIRST));
    // The only report of the branch is damaged.
    fs::write(source.store(&root).unwrap(), b"{ not a report").unwrap();
    advance_dev(&root, &[]);
    let (_, seen) = ship(&root, &options(temp.path(), FIRST, None));
    assert_eq!(seen[0].0, Plan::Full);
    assert_eq!(
        seen[0].1,
        "review: full review — 1 stored review report(s) are unreadable and no readable passing review of this branch exists"
    );
    assert_ne!(head(&root), source.head);
}

#[test]
fn a_missing_base_commit_is_a_full_review_that_says_so() {
    let (temp, root) = fixture("true");
    let mut source = unstored(&root, Tier::Low, 9);
    source.task_fingerprint = Some(Report::task_fingerprint(FIRST));
    source.base = "0".repeat(40);
    source.store(&root).unwrap();
    advance_dev(&root, &[]);
    let (_, seen) = ship(&root, &options(temp.path(), FIRST, None));
    assert_eq!(
        seen[0].1,
        format!(
            "review: full review — the base commit of the 9/10 of {} is missing",
            &source.head[..12]
        )
    );
}

#[test]
fn the_same_commit_shipped_again_under_another_description_keeps_its_approval() {
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Medium, FIRST);
    let reused = no_reviewer(&root, &options(temp.path(), SECOND, None));
    assert_eq!(reused.head, source.head);
    assert_eq!(reused.grade, 9);
    assert_eq!(
        reused.task_fingerprint,
        Some(Report::task_fingerprint(SECOND))
    );
    assert_eq!(
        reused.note.as_deref(),
        Some("description text changed after approval; code unchanged")
    );
    // A second edit, after the reuse, is covered too, and so is the first text.
    for text in ["third description\n", SECOND, FIRST] {
        let again = no_reviewer(&root, &options(temp.path(), text, None));
        assert_eq!(again.head, source.head);
        assert_eq!(again.task_fingerprint, Some(Report::task_fingerprint(text)));
    }
}

#[test]
fn a_description_edit_after_a_rebase_reuse_keeps_the_approval_again() {
    let (temp, root) = fixture("true");
    approved(&root, Tier::Medium, FIRST);
    advance_dev(&root, &[]);
    no_reviewer(&root, &options(temp.path(), SECOND, None));
    let third = no_reviewer(&root, &options(temp.path(), "third description\n", None));
    assert_eq!(third.head, head(&root));
    assert_eq!(third.grade, 9);
}

#[test]
fn a_failed_review_of_this_commit_is_reviewed_again_under_a_new_description() {
    let (temp, root) = fixture("true");
    // On its own branch: on `feature` a failure forbids any review.
    run(&root, &["checkout", "-q", "-b", "candidate"]);
    let mut failed = unstored(&root, Tier::Medium, 7);
    failed.task_fingerprint = Some(Report::task_fingerprint(FIRST));
    failed.store(&root).unwrap();
    let (_, seen) = ship(&root, &options(temp.path(), SECOND, None));
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, Plan::Full);
}

#[test]
fn the_same_commit_under_a_changed_policy_gets_a_full_review_with_the_policy_reason() {
    let (temp, root) = fixture("true");
    let source = approved(&root, Tier::Medium, FIRST);
    let (path, text) = policy_change(&root);
    advance_dev_policy_only(&root, &path, &text);
    assert_eq!(head(&root), source.head, "the commit itself is unchanged");
    let (_, seen) = ship(&root, &options(temp.path(), SECOND, None));
    assert_eq!(seen[0].0, Plan::Full);
    assert_eq!(
        seen[0].1,
        format!(
            "review: full review — the review policy on dev changed since the 9/10 of {}",
            &source.head[..12]
        )
    );
}

/// Move origin/dev without touching `feature`, so its head and base stay.
fn advance_dev_policy_only(root: &Path, path: &str, text: &str) {
    run(root, &["checkout", "-q", "dev"]);
    fs::write(root.join(path), text).unwrap();
    run(root, &["add", path]);
    run(root, &["commit", "-q", "-m", "policy"]);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["fetch", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
}
