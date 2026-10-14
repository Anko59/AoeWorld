//! The carry-over under the real adapter's pinning: `.agents/hooks/harness.sh`
//! exports `AOE_JUDGE_REV` as the origin/dev the ship started from, and the
//! re-check fetches a newer dev. The judge revision is passed explicitly here
//! (the process environment is shared by parallel tests).
use super::*;
use crate::review::Report;

fn dev(root: &Path) -> String {
    git(root, &["rev-parse", "refs/remotes/origin/dev"])
}

fn reviewing() -> Options {
    Options {
        no_review: false,
        ..offline()
    }
}

/// The ship up to the review: evidence, a stored passing review, the judge
/// revision it was pinned to.
fn reviewed(root: &Path) -> (Evidence, Report, String) {
    let judged = dev(root);
    let evidence = judge(root, &offline()).unwrap();
    let report = graded(root, Tier::Low, 9);
    (evidence, report, judged)
}

#[test]
fn the_passed_review_is_carried_under_the_pinned_judge_without_a_reviewer() {
    let (temp, root) = fixture("true");
    let (before, report, judged) = reviewed(&root);
    let moved = advance(&temp, "dev", "other.txt", "o\n");
    let rebased = before_push_judged(&root, &before, Some(&report), &reviewing(), Some(&judged))
        .unwrap()
        .expect("dev moved");
    assert_eq!(
        dev(&root),
        moved,
        "the fetch advanced origin/dev past the judge"
    );
    assert_ne!(dev(&root), judged);
    let carried = rebased.review.expect("carried");
    assert_eq!(carried.head, rebased.evidence.head);
    assert_eq!(carried.reused_from.as_deref(), Some(before.head.as_str()));
    assert_eq!(carried.base, moved);
    assert!(carried.passes());
    let recheck = rebased.evidence.recheck.as_ref().unwrap();
    assert_eq!(recheck.judge.as_deref(), Some(judged.as_str()));
    assert_eq!(recheck.from_head, before.head);
    // Stored: the next ship of this branch finds it by the rebased head.
    assert!(Report::has_complete(&root, &rebased.evidence.head, &["low"]).unwrap());
}

#[test]
fn the_carried_review_says_it_was_carried_not_the_original_decision() {
    let (temp, root) = fixture("true");
    let (before, mut report, judged) = reviewed(&root);
    report.decision = Some("review: full review — no passing review of this branch".into());
    report.note = Some("description text changed after approval; code unchanged".into());
    advance(&temp, "dev", "other.txt", "o\n");
    let rebased = before_push_judged(&root, &before, Some(&report), &reviewing(), Some(&judged))
        .unwrap()
        .expect("dev moved");
    let carried = rebased.review.expect("carried");
    assert_eq!(
        carried.decision,
        Some(format!(
            "review: carried 9/10 of {} over the re-check rebase (patch and policy unchanged)",
            &before.head[..12]
        ))
    );
    assert_eq!(carried.note, None);
    assert!(carried.markdown().contains("review: carried 9/10 of"));
}

#[test]
fn a_changed_review_policy_on_dev_stops_with_the_policy_message() {
    let (temp, root) = fixture("true");
    let (before, report, judged) = reviewed(&root);
    let moved = advance(&temp, "dev", "gates/review/preamble.md", "new rules\n");
    let error = before_push_judged(&root, &before, Some(&report), &reviewing(), Some(&judged))
        .err()
        .expect("stopped")
        .to_string();
    assert!(
        error.contains(&format!(
            "the review policy on dev changed in {}",
            &moved[..12]
        )),
        "{error}"
    );
    assert!(
        error.contains("ship again for a review under the new policy"),
        "{error}"
    );
    assert!(!error.contains("changed your patch"), "{error}");
    assert_eq!(remote_tip(&root, "feature"), "");
}

#[test]
fn a_rebase_that_changes_the_own_patch_stops_with_the_patch_message() {
    let (temp, root) = fixture("true");
    let (before, report, judged) = reviewed(&root);
    // Dev gained exactly the branch's own change: the rebase drops the commit.
    advance(&temp, "dev", "README.md", "y\n");
    let error = before_push_judged(&root, &before, Some(&report), &reviewing(), Some(&judged))
        .err()
        .expect("stopped")
        .to_string();
    assert!(
        error.contains("the re-check rebase changed your patch"),
        "{error}"
    );
    assert!(!error.contains("review policy"), "{error}");
    assert_eq!(remote_tip(&root, "feature"), "");
}

#[test]
fn a_rebase_that_cannot_start_reports_its_own_error_not_a_failed_abort() {
    let (temp, root) = fixture("true");
    let before = head(&root);
    let moved = advance(&temp, "dev", "other.txt", "o\n");
    run(&root, &["fetch", "-q", "origin"]);
    let merge_base = git(&root, &["merge-base", &moved, "HEAD"]);
    fs::write(root.join("README.md"), "dirty\n").unwrap();
    let error = rebase(&root, &offline(), &moved, &merge_base, &before, 1)
        .unwrap_err()
        .to_string();
    assert!(error.contains("could not run"), "{error}");
    assert!(error.contains("The branch is as it was"), "{error}");
    assert!(!error.contains("abort did not restore"), "{error}");
    assert_eq!(head(&root), before);
}
