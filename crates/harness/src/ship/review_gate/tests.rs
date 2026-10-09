use super::super::{
    Options, git,
    github::description,
    judge,
    review_gate::publish_calls,
    ship_with,
    tests::{fixture, offline, run},
};

mod reuse;
mod reuse_policy;
use crate::review::{Plan, Report, Tier};
use std::{cell::Cell, path::Path};

/// A stored review of HEAD with this grade, as `review::review` leaves it.
fn graded(root: &Path, tier: Tier, grade: u8) -> Report {
    let report = unstored(root, tier, grade);
    report.store(root).unwrap();
    report
}

/// A review of HEAD with this grade, not stored yet.
fn unstored(root: &Path, tier: Tier, grade: u8) -> Report {
    Report {
        version: 1,
        head: git::git(root, &["rev-parse", "HEAD"]).unwrap(),
        branch: "feature".into(),
        base: String::new(),
        merge_base: String::new(),
        tier: tier.name().into(),
        floor: "low".into(),
        runtime: "claude".into(),
        model: "scripted".into(),
        effort: "medium".into(),
        personas: vec!["quick".into()],
        rounds: 1,
        findings: vec![],
        written_grade: grade,
        grade,
        summary: "scripted".into(),
        failures: vec![],
        merge_grade: 8,
        started: 0,
        finished: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
        closing: false,
        change_fingerprint: None,
        policy_fingerprint: Report::policy_fingerprint(root, tier.name()).ok(),
        reused_from: None,
        // An empty task: the reviewers saw the commit log.
        task_fingerprint: Report::effective_task(root, "HEAD", "")
            .ok()
            .map(|task| Report::task_fingerprint(&task)),
    }
}

fn reviewed() -> Options {
    Options {
        no_review: false,
        ..offline()
    }
}

fn pushed(root: &Path) -> bool {
    !git::git(root, &["ls-remote", "origin", "refs/heads/feature"])
        .unwrap()
        .is_empty()
}

#[test]
fn a_review_below_the_merge_grade_pushes_nothing() {
    let (_temp, root) = fixture("true");
    let error = ship_with(&root, &reviewed(), &|root, tier, _, _, _| {
        Ok(graded(root, tier, 7))
    })
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("review 7/10 (needs 8); nothing was pushed"),
        "{error}"
    );
    assert!(!pushed(&root));
}

#[test]
fn an_incomplete_review_pushes_nothing_whatever_its_grade() {
    let (_temp, root) = fixture("true");
    let error = ship_with(&root, &reviewed(), &|root, tier, _, _, _| {
        let mut report = graded(root, tier, 9);
        report.failures.push("round 1, quick: no answer".into());
        report.store(root).unwrap();
        Ok(report)
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("the review was incomplete"), "{error}");
    assert!(!pushed(&root));
}

#[test]
fn a_passing_review_is_pushed_and_reused_for_the_same_commit() {
    let (_temp, root) = fixture("true");
    let calls = Cell::new(0);
    let review = |root: &Path, tier, _, _: &str, _: &Plan| {
        calls.set(calls.get() + 1);
        Ok(graded(root, tier, 9))
    };
    ship_with(&root, &reviewed(), &review).expect("ship");
    assert!(pushed(&root));
    ship_with(&root, &reviewed(), &review).expect("ship again");
    assert_eq!(calls.get(), 1, "the stored passing review was reused");
}

#[test]
fn the_tier_defaults_to_the_floor_and_never_goes_below_it() {
    let (_temp, root) = fixture("true");
    let asked = Cell::new(None);
    ship_with(&root, &reviewed(), &|root, tier, _, _, _| {
        asked.set(Some(tier));
        Ok(graded(root, tier, 9))
    })
    .expect("ship");
    assert_eq!(
        asked.get(),
        Some(Tier::Low),
        "a README change floors at low"
    );
    std::fs::create_dir_all(root.join("gates/x")).unwrap();
    std::fs::write(root.join("gates/x/rule.txt"), "new\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "gate change"]);
    let options = Options {
        tier: Some(Tier::Low),
        ..reviewed()
    };
    let error = ship_with(&root, &options, &|_, _, _, _, _| {
        panic!("no review below the floor")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("needs at least a high review"), "{error}");
}

#[test]
fn a_commit_that_moves_during_the_review_is_not_pushed() {
    let (_temp, root) = fixture("true");
    let error = ship_with(&root, &reviewed(), &|root, tier, _, _, _| {
        // Another session commits while the reviewers work.
        run(root, &["commit", "-q", "--allow-empty", "-m", "racing"]);
        Ok(graded(root, tier, 9))
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("HEAD moved while the review ran"), "{error}");
    assert!(!pushed(&root));
}

#[test]
fn a_passing_review_posts_its_status_and_arms_auto_merge() {
    let (_temp, root) = fixture("true");
    let report = graded(&root, Tier::Low, 9);
    let calls = publish_calls("o/r", "https://github.com/o/r/pull/7", &report).unwrap();
    let status = calls[0].join(" ");
    assert!(
        status.starts_with("api --method POST repos/o/r/statuses/"),
        "{status}"
    );
    assert!(status.contains(&report.head));
    assert!(status.contains("state=success") && status.contains("context=harness/review"));
    assert!(
        status.contains("description=grade 9/10 · low tier · 1 rounds"),
        "{status}"
    );
    assert_eq!(
        calls[1],
        [
            "pr",
            "merge",
            "https://github.com/o/r/pull/7",
            "--repo",
            "o/r",
            "--auto",
            "--squash",
            "--match-head-commit",
            report.head.as_str()
        ]
    );
    let failing = graded(&root, Tier::Low, 7);
    assert!(
        publish_calls("o/r", "u", &failing).is_err(),
        "a failing review publishes nothing"
    );
}

#[test]
fn the_review_is_rendered_into_a_private_description() {
    let (_temp, root) = fixture("true");
    let evidence = judge(&root, &offline()).unwrap();
    let mut report = graded(&root, Tier::Low, 9);
    report.reused_from = Some("a".repeat(40));
    let body = root.join("body.md");
    std::fs::write(
        &body,
        "<!-- level: low -->\n## Why\n> \"I don't review code\" — the user\nBecause.\n## What\nA README edit.\n",
    )
    .unwrap();
    let options = Options {
        body_file: Some(body.clone()),
        ..offline()
    };
    let template = std::fs::read_to_string(&body).unwrap();
    std::fs::write(&body, "mutated after validation").unwrap();
    let path = description(&root, &evidence, &options, &report, &template).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# 🧑 For humans"), "{text}");
    assert!(
        text.contains("> \"I don't review code\" — the user"),
        "{text}"
    );
    assert!(text.contains("Because."), "Why prose was lost: {text}");
    assert!(text.contains("A README edit."), "{text}");
    assert!(text.contains("9/10"), "{text}");
    assert!(text.contains("### ✅ How\n\n"), "{text}");
    assert!(
        text.contains("## Test-first report\n\n") && text.contains("- test-first: not applicable"),
        "{text}"
    );
    assert!(
        text.contains("Review of `aaaaaaaaaaaa` reused (same change, identical file blobs)"),
        "{text}"
    );
    assert!(
        text.contains("# 🤖 For AI") && text.contains("same change, identical file blobs"),
        "{text}"
    );
    assert!(text.contains("## Adversarial review"), "{text}");
    let common = git::git(
        &root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .unwrap();
    assert!(
        path.starts_with(std::path::Path::new(&common).join("aoe-ship/bodies")),
        "{}",
        path.display()
    );
}

#[test]
fn a_reviewed_ship_needs_a_description() {
    let (_temp, root) = fixture("true");
    let options = Options {
        no_pr: false,
        ..reviewed()
    };
    let error = ship_with(&root, &options, &|_, _, _, _, _| {
        panic!("no review without SHIP_BODY")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("SHIP_BODY is required"), "{error}");
}

fn commit(root: &Path, message: &str) {
    run(root, &["commit", "-q", "--allow-empty", "-m", message]);
}

/// A failing closing review that did answer: one new major finding in the fixes.
fn failed_closing(root: &Path, tier: Tier) -> Report {
    let mut report = unstored(root, tier, 6);
    report.closing = true;
    report.findings = finding(crate::review::protocol::Severity::Major);
    report.store(root).unwrap();
    report
}

fn finding(severity: crate::review::protocol::Severity) -> Vec<crate::review::protocol::Finding> {
    use crate::review::protocol::{Finding, Reported, Status};
    vec![Finding {
        id: "F1".into(),
        reporter: 0,
        round: 1,
        reported: Reported {
            file: "x.rs".into(),
            line: Some(1),
            severity,
            category: "correctness".into(),
            claim: "c".into(),
            trigger: "t".into(),
            expected_vs_actual: "e".into(),
            evidence: "v".into(),
        },
        votes: vec![],
        status: Status::Confirmed,
    }]
}

/// Three failed full reviews whose blocking findings do not grow.
fn three_failed(root: &Path) {
    for attempt in 1..=3 {
        commit(root, &format!("try {attempt}"));
        let _ = ship_with(root, &reviewed(), &|root, tier, _, _, plan| {
            assert_eq!(*plan, Plan::Full);
            Ok(graded(root, tier, 5))
        });
    }
}

#[test]
fn the_review_budget_always_ends_without_a_person() {
    let (_temp, root) = fixture("true");
    three_failed(&root);
    let error = ship_with(&root, &reviewed(), &|_, _, _, _, _| panic!("fix first"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("already failed a review; fix and commit"),
        "{error}"
    );
    for (closing, expected) in [(1, "closing review of your fixes"), (2, "Split the change")] {
        commit(&root, &format!("fixes {closing}"));
        let error = ship_with(&root, &reviewed(), &|root, tier, _, _, plan| {
            assert!(matches!(plan, Plan::Closing { .. }), "{plan:?}");
            Ok(failed_closing(root, tier))
        })
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("closing review failed (1 blocking finding(s) left)")
                && error.contains(expected),
            "closing {closing}: {error}"
        );
    }
    commit(&root, "more fixes");
    let error = ship_with(&root, &reviewed(), &|_, _, _, _, _| {
        panic!("no more reviews")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("Split the change"), "{error}");
    assert!(!pushed(&root));
}
#[test]
fn incomplete_reviews_do_not_spend_the_budget() {
    let (_temp, root) = fixture("true");
    let incomplete = |root: &Path, tier, _, _: &str, _: &Plan| {
        let mut report = unstored(root, tier, 9);
        report.failures.push("no answer".into());
        report.store(root).unwrap();
        Ok(report)
    };
    for attempt in 1..=2 {
        let error = ship_with(&root, &reviewed(), &incomplete)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("does not count"),
            "attempt {attempt}: {error}"
        );
    }
    // Same commit, reviewed again: an incomplete review checked nothing.
    let error = ship_with(&root, &reviewed(), &incomplete)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not count"), "{error}");
    let error = ship_with(&root, &reviewed(), &|_, _, _, _, _| panic!("unavailable"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("reviewers are unavailable"), "{error}");
}

#[test]
fn a_diverging_branch_is_told_to_split() {
    let (_temp, root) = fixture("true");
    for _ in 0..3 {
        commit(&root, "try");
        let _ = ship_with(&root, &reviewed(), &|root, tier, _, _, _| {
            let mut report = unstored(root, tier, 4);
            report.findings = finding(crate::review::protocol::Severity::Critical);
            report.store(root).unwrap();
            Ok(report)
        });
    }
    commit(&root, "more");
    let error = ship_with(&root, &reviewed(), &|_, _, _, _, _| panic!("no review"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Split the change"), "{error}");
    assert!(!pushed(&root));
}

#[test]
fn rerunning_a_failed_commit_is_refused_and_every_attempt_is_kept() {
    let (_temp, root) = fixture("true");
    let _ = ship_with(&root, &reviewed(), &|root, tier, _, _, _| {
        Ok(graded(root, tier, 5))
    });
    let error = ship_with(&root, &reviewed(), &|_, _, _, _, _| {
        panic!("no second review of a failed commit")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("already failed"), "{error}");
    graded(&root, Tier::Low, 5);
    assert_eq!(crate::review::history(&root, "feature").unwrap().len(), 2);
}

#[test]
fn a_clean_pull_request_is_merged_directly_and_still_pinned() {
    use super::super::github::direct_merge;
    let call: Vec<String> = [
        "pr",
        "merge",
        "u",
        "--repo",
        "o/r",
        "--auto",
        "--squash",
        "--match-head-commit",
        "abc",
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect();
    let refused = "gh pr merge …: GraphQL: Pull request Pull request is in clean status (enablePullRequestAutoMerge)";
    let direct = direct_merge(&call, refused).expect("a clean PR is merged directly");
    assert!(!direct.iter().any(|a| a == "--auto"));
    assert!(
        direct
            .windows(2)
            .any(|w| w[0] == "--match-head-commit" && w[1] == "abc")
    );
    assert!(
        direct_merge(&call, "some other failure").is_none(),
        "other errors still fail"
    );
    let unpinned: Vec<String> = call
        .iter()
        .filter(|a| *a != "--match-head-commit" && *a != "abc")
        .cloned()
        .collect();
    assert!(
        direct_merge(&unpinned, refused).is_none(),
        "never merge an unpinned call directly"
    );
    let status: Vec<String> = ["api", "--method", "POST"]
        .iter()
        .map(|a| (*a).to_owned())
        .collect();
    assert!(direct_merge(&status, refused).is_none());
}
