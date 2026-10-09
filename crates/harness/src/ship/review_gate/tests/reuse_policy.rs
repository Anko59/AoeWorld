use super::{fixture, run, unstored};
use crate::review::{Report, Tier};
use crate::ship::git;
use std::path::Path;

fn advance_dev_with_policy(root: &Path, update: impl FnOnce(&mut serde_json::Value)) {
    run(root, &["checkout", "-q", "dev"]);
    let path = root.join("gates/review.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    update(&mut config);
    std::fs::write(path, config.to_string()).unwrap();
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(root, &["add", "gates/review.json", "BASE.md"]);
    run(root, &["commit", "-q", "-m", "change review policy"]);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["fetch", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
    run(root, &["rebase", "-q", "origin/dev"]);
}

#[test]
fn reuse_requires_the_current_review_config_fingerprint() {
    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::Low, 9);
    source.store(&root).unwrap();
    advance_dev_with_policy(&root, |config| {
        config["tiers"]["low"]["personas"] = serde_json::json!(["quick", "test-integrity"]);
    });
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "")
            .unwrap()
            .is_none()
    );

    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::Low, 9);
    source.store(&root).unwrap();
    run(&root, &["checkout", "-q", "dev"]);
    std::fs::write(
        root.join("gates/review/personas/quick.md"),
        "# Updated quick reviewer\n",
    )
    .unwrap();
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(&root, &["add", "gates/review/personas/quick.md", "BASE.md"]);
    run(&root, &["commit", "-q", "-m", "change reviewer prompt"]);
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["fetch", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "")
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_rust_prompt_change_on_origin_dev_prevents_reuse() {
    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::Low, 9);
    source.store(&root).unwrap();
    run(&root, &["checkout", "-q", "dev"]);
    let prompt = root.join("crates/harness/src/review/prompt.rs");
    let text = std::fs::read_to_string(&prompt).unwrap();
    std::fs::write(
        &prompt,
        format!("{text}\n// changed reviewer instruction\n"),
    )
    .unwrap();
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(
        &root,
        &["add", "crates/harness/src/review/prompt.rs", "BASE.md"],
    );
    run(
        &root,
        &["commit", "-q", "-m", "change Rust reviewer prompt"],
    );
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["fetch", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "")
            .unwrap()
            .is_none(),
        "a changed Rust prompt means the review engine fingerprint changed"
    );
}

#[test]
fn reuse_requires_the_current_runtime_model_policy() {
    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::Low, 9);
    source.store(&root).unwrap();
    advance_dev_with_policy(&root, |config| {
        config["models"]["claude"]["average"]["model"] = serde_json::json!("claude-next-model");
    });
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "")
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_report_without_a_policy_fingerprint_is_never_reused() {
    let (_temp, root) = fixture("true");
    let mut source = unstored(&root, Tier::Low, 9);
    source.policy_fingerprint = None;
    source.store(&root).unwrap();
    advance_dev_with_policy(&root, |_| {});
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "")
            .unwrap()
            .is_none()
    );
}

#[test]
fn another_branchs_closing_review_is_never_reused() {
    let (_temp, root) = fixture("true");
    let mut closing = unstored(&root, Tier::Low, 6);
    closing.closing = true;
    assert!(closing.passes());
    closing.store(&root).unwrap();
    // Branch `other` failed its own review, then reached the same change.
    run(&root, &["checkout", "-q", "-b", "other", "origin/dev"]);
    std::fs::write(root.join("README.md"), "unresolved\n").unwrap();
    run(&root, &["commit", "-q", "-am", "other change"]);
    let mut failed = unstored(&root, Tier::Low, 3);
    failed.branch = "other".into();
    assert!(!failed.passes());
    failed.store(&root).unwrap();
    std::fs::write(root.join("README.md"), "y\n").unwrap();
    run(&root, &["commit", "-q", "-am", "same change as feature"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(
        git::change_identity(&root, "dev", &head).unwrap().1,
        git::change_identity(&root, "dev", &closing.head).unwrap().1,
        "the two branches hold the same change"
    );
    assert!(
        Report::reuse_for_change(&root, &head, "other", &["low"], "")
            .unwrap()
            .is_none(),
        "branch other must run its own closing review"
    );

    // The same branch after a rebase still reuses its own closing review.
    run(&root, &["checkout", "-q", "dev"]);
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(&root, &["add", "BASE.md"]);
    run(&root, &["commit", "-q", "-m", "move dev"]);
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["fetch", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let reused = Report::reuse_for_change(&root, &head, "feature", &["low"], "")
        .unwrap()
        .expect("the same branch reuses its closing review after a rebase");
    assert_eq!(reused.reused_from.as_deref(), Some(closing.head.as_str()));
    assert!(reused.closing);
}

/// Commit `contents` at `path` on origin/dev and rebase `feature` onto it.
fn advance_dev_with_file(root: &Path, path: &str, contents: &str) {
    run(root, &["checkout", "-q", "dev"]);
    std::fs::write(root.join(path), contents).unwrap();
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(root, &["add", path, "BASE.md"]);
    run(root, &["commit", "-q", "-m", "move dev"]);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["fetch", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
    run(root, &["rebase", "-q", "origin/dev"]);
}

fn all_tiers() -> Vec<&'static str> {
    vec!["low", "medium", "high", "xhigh", "max"]
}

#[test]
fn a_changed_description_gets_a_fresh_review() {
    let (temp, root) = fixture("true");
    let mut source = unstored(&root, Tier::Low, 9);
    source.task_fingerprint = Some(Report::task_fingerprint("first description\n"));
    source.store(&root).unwrap();
    advance_dev_with_file(&root, "BASE.md", "moved base\n");
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "changed description\n")
            .unwrap()
            .is_none(),
        "a review of one description never answers for another"
    );

    let body = temp.path().join("body.md");
    std::fs::write(&body, "changed description\n").unwrap();
    let evidence = crate::ship::judge(&root, &crate::ship::tests::offline()).unwrap();
    let options = crate::ship::Options {
        no_review: false,
        body_file: Some(body.clone()),
        ..crate::ship::tests::offline()
    };
    let calls = std::cell::Cell::new(0);
    let report =
        crate::ship::review_gate::require(&root, &evidence, &options, &|root, tier, _, task, _| {
            calls.set(calls.get() + 1);
            assert_eq!(task, "changed description\n");
            Ok(unstored(root, tier, 9))
        })
        .unwrap();
    assert_eq!(calls.get(), 1, "the changed description was reviewed");
    assert!(report.reused_from.is_none());

    // The same description still reuses the review after a rebase.
    let (temp, root) = fixture("true");
    let mut source = unstored(&root, Tier::Low, 9);
    source.task_fingerprint = Some(Report::task_fingerprint("first description\n"));
    source.store(&root).unwrap();
    advance_dev_with_file(&root, "BASE.md", "moved base\n");
    let body = temp.path().join("body.md");
    std::fs::write(&body, "first description\n").unwrap();
    let evidence = crate::ship::judge(&root, &crate::ship::tests::offline()).unwrap();
    let options = crate::ship::Options {
        no_review: false,
        body_file: Some(body),
        ..crate::ship::tests::offline()
    };
    let reused = crate::ship::review_gate::require(&root, &evidence, &options, &|_, _, _, _, _| {
        panic!("the same description should reuse")
    })
    .unwrap();
    assert_eq!(reused.reused_from.as_deref(), Some(source.head.as_str()));
}

#[test]
fn a_registry_change_on_origin_dev_prevents_reuse() {
    let (_temp, root) = fixture("true");
    let before = Report::policy_fingerprint(&root, "high").unwrap();
    let source = unstored(&root, Tier::High, 9);
    source.store(&root).unwrap();
    // Only the registry moves: README.md now belongs to the high-floor suite.
    let mut registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("gates/registry.json")).unwrap()).unwrap();
    registry["suites"][0]["paths"] = serde_json::json!(["gates/**", "README.md"]);
    advance_dev_with_file(&root, "gates/registry.json", &registry.to_string());
    assert_ne!(
        Report::policy_fingerprint(&root, "high").unwrap(),
        before,
        "the registry is part of the policy fingerprint"
    );
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &all_tiers(), "")
            .unwrap()
            .is_none()
    );
}

#[test]
fn reuse_refuses_a_tier_below_the_pinned_floor() {
    let (_temp, root) = fixture("true");
    // The floor of this change is high; a low review is offered as eligible.
    std::fs::write(root.join("gates/extra.md"), "gated\n").unwrap();
    run(&root, &["add", "gates/extra.md"]);
    run(&root, &["commit", "-q", "-m", "gated change"]);
    unstored(&root, Tier::Low, 9).store(&root).unwrap();
    advance_dev_with_file(&root, "BASE.md", "moved base\n");
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &all_tiers(), "")
            .unwrap()
            .is_none(),
        "reuse recomputes the floor from the pinned policy commit"
    );
}

#[test]
fn a_reused_report_records_the_real_floor() {
    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::High, 9);
    source.store(&root).unwrap();
    advance_dev_with_file(&root, "BASE.md", "moved base\n");
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let reused = Report::reuse_for_change(&root, &head, "feature", &["high", "xhigh", "max"], "")
        .unwrap()
        .expect("a high review reuses for a low-floor change");
    assert_eq!(reused.tier, "high");
    assert_eq!(
        reused.floor, "low",
        "the floor is the change's, not the request"
    );
}

#[test]
fn a_judge_behind_the_fetched_origin_dev_never_reuses() {
    let (_temp, root) = fixture("true");
    let judge = git::git(&root, &["rev-parse", "refs/remotes/origin/dev"]).unwrap();
    unstored(&root, Tier::Low, 9).store(&root).unwrap();
    // make ship fetched a newer origin/dev after the judge was chosen.
    advance_dev_with_file(&root, "BASE.md", "moved base\n");
    let fetched = git::git(&root, &["rev-parse", "refs/remotes/origin/dev"]).unwrap();
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change_judged(&root, &head, "feature", &["low"], "", Some(&judge))
            .unwrap()
            .is_none(),
        "a judge pinned to an older origin/dev never reuses"
    );
    assert!(
        Report::reuse_for_change_judged(&root, &head, "feature", &["low"], "", Some(&fetched))
            .unwrap()
            .is_some(),
        "the judge of the fetched origin/dev reuses the same change"
    );
}

#[test]
fn an_amended_commit_message_gets_a_fresh_review() {
    let (_temp, root) = fixture("true");
    unstored(&root, Tier::Low, 9).store(&root).unwrap();
    advance_dev_with_file(&root, "BASE.md", "moved base\n");
    // Without SHIP_BODY the reviewers read the commit log, which changed.
    run(&root, &["commit", "-q", "--amend", "-m", "reworded"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "")
            .unwrap()
            .is_none(),
        "a review of one commit log never answers for another"
    );
}

#[test]
fn the_same_commit_with_another_description_gets_a_fresh_review() {
    let (temp, root) = fixture("true");
    let mut first = unstored(&root, Tier::Low, 9);
    first.task_fingerprint = Some(Report::task_fingerprint("first description\n"));
    first.store(&root).unwrap();
    let head = first.head.clone();
    let mut unmarked = unstored(&root, Tier::Low, 9);
    unmarked.task_fingerprint = None;
    unmarked.store(&root).unwrap();
    assert!(
        Report::load_passing(&root, &head, &["low"], "first description\n")
            .unwrap()
            .is_some()
    );
    assert!(
        Report::load_passing(&root, &head, &["low"], "")
            .unwrap()
            .is_none(),
        "a report without the reviewers' task is never reused"
    );

    let body = temp.path().join("body.md");
    std::fs::write(&body, "second description\n").unwrap();
    let evidence = crate::ship::judge(&root, &crate::ship::tests::offline()).unwrap();
    let options = crate::ship::Options {
        no_review: false,
        body_file: Some(body),
        ..crate::ship::tests::offline()
    };
    let calls = std::cell::Cell::new(0);
    crate::ship::review_gate::require(&root, &evidence, &options, &|root, tier, _, task, _| {
        calls.set(calls.get() + 1);
        assert_eq!(task, "second description\n");
        Ok(unstored(root, tier, 9))
    })
    .unwrap();
    assert_eq!(calls.get(), 1, "the second description was reviewed");
}
