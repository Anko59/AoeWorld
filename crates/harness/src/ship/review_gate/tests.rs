use super::super::{
    Options, git,
    review_gate::{description, publish_calls},
    ship_with,
    tests::{fixture, offline, run},
};
use crate::review::{Report, Tier};
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
    let error = ship_with(&root, &reviewed(), &|root, tier, _, _| {
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
    let error = ship_with(&root, &reviewed(), &|root, tier, _, _| {
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
    let review = |root: &Path, tier, _, _: &str| {
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
    ship_with(&root, &reviewed(), &|root, tier, _, _| {
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
    let error = ship_with(&root, &options, &|_, _, _, _| {
        panic!("no review below the floor")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("needs at least a high review"), "{error}");
}

#[test]
fn a_commit_that_moves_during_the_review_is_not_pushed() {
    let (_temp, root) = fixture("true");
    let error = ship_with(&root, &reviewed(), &|root, tier, _, _| {
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
            "--squash"
        ]
    );
    let failing = graded(&root, Tier::Low, 7);
    assert!(
        publish_calls("o/r", "u", &failing).is_err(),
        "a failing review publishes nothing"
    );
}

#[test]
fn the_review_is_appended_to_the_description_privately() {
    let (_temp, root) = fixture("true");
    let report = graded(&root, Tier::Low, 9);
    let body = root.join("body.md");
    std::fs::write(&body, "## Why\nBecause.\n").unwrap();
    let options = Options {
        body_file: Some(body),
        ..offline()
    };
    let path = description(&root, &options, &report).unwrap().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("## Why\nBecause."));
    assert!(text.contains("## Adversarial review"));
    assert!(text.contains("9/10"));
    assert!(
        path.to_string_lossy().contains("aoe-ship/bodies"),
        "{}",
        path.display()
    );
    assert!(!path.starts_with(std::env::temp_dir()) || path.starts_with(&root));
}

#[test]
fn a_reviewed_ship_needs_a_description() {
    let (_temp, root) = fixture("true");
    let options = Options {
        no_pr: false,
        ..reviewed()
    };
    let error = ship_with(&root, &options, &|_, _, _, _| {
        panic!("no review without SHIP_BODY")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("SHIP_BODY is required"), "{error}");
}
