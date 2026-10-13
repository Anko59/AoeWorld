//! Commits added after a passing review are reviewed on their own diff
//! (docs/review.md): the approved code is never judged again.
use super::{commit, failed_closing, graded, reviewed, unstored};
use crate::{
    review::{
        self, Increment, Plan, Report, Tier,
        closing::Next,
        increment::{SMALL, Scope},
    },
    ship::{
        git, ship_with,
        tests::{fixture, run},
    },
};
use std::{cell::RefCell, fs, path::Path};

fn write(root: &Path, name: &str, text: &str) {
    fs::write(root.join(name), text).unwrap();
    run(root, &["add", "-A"]);
    run(root, &["commit", "-q", "-m", name]);
}

fn head(root: &Path) -> String {
    git::git(root, &["rev-parse", "HEAD"]).unwrap()
}

fn next(root: &Path) -> Next {
    let history = review::history(root, "feature").unwrap();
    review::increment::next(root, &history, &head(root), u64::MAX / 2)
}

fn increment(root: &Path) -> Increment {
    match next(root) {
        Next::Review(Plan::Increment(increment)) => increment,
        other => panic!("an increment review was expected: {other:?}"),
    }
}

/// The review of an increment as the engine leaves it: the plan's increment
/// on a report that passes (closing-style) or fails with one major finding.
fn judged(root: &Path, tier: Tier, plan: &Plan, pass: bool) -> Report {
    let Plan::Increment(increment) = plan else {
        panic!("an increment plan was expected: {plan:?}");
    };
    let mut report = if pass {
        unstored(root, tier, 9)
    } else {
        failed_closing(root, tier)
    };
    report.closing = increment.closing;
    report.increment = Some(increment.clone());
    report.store(root).unwrap();
    report
}

fn advance_dev(root: &Path, file: &str, text: &str) {
    run(root, &["checkout", "-q", "dev"]);
    write(root, file, text);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
}

fn plans() -> RefCell<Vec<Plan>> {
    RefCell::new(Vec::new())
}

#[test]
fn a_small_commit_after_an_approval_is_a_closing_review_of_its_own_diff() {
    let (_temp, root) = fixture("true");
    let seen = plans();
    let review = |root: &Path, tier, _, _: &str, plan: &Plan| {
        seen.borrow_mut().push(plan.clone());
        match plan {
            Plan::Full => Ok(graded(root, tier, 9)),
            plan => Ok(judged(root, tier, plan, true)),
        }
    };
    ship_with(&root, &reviewed(), &review).expect("the full review passes");
    let approved = head(&root);
    // The 12-line follow-up of the report this rule answers (PR #336).
    write(&root, "follow.txt", &"line\n".repeat(12));
    ship_with(&root, &reviewed(), &review).expect("the follow-up ships");
    let report = review::history(&root, "feature").unwrap().pop().unwrap();
    let plans = seen.borrow();
    assert_eq!(plans.len(), 2, "the approved code is not reviewed again");
    assert_eq!(plans[0], Plan::Full);
    let Plan::Increment(increment) = &plans[1] else {
        panic!("not a start-over: {:?}", plans[1]);
    };
    assert_eq!(increment.scope, Scope::Since(approved.clone()));
    assert_eq!(increment.lines, 12);
    assert!(increment.closing, "a small increment gets a closing review");
    assert_eq!(
        (increment.approved.as_str(), increment.grade),
        (approved.as_str(), 9)
    );
    let headline = report.headline();
    assert!(
        headline.contains(&format!(
            "closing review of 12 lines since `{}`, approved 9/10",
            &approved[..12]
        )),
        "{headline}"
    );
    assert!(report.passes() && report.closing);
}

#[test]
fn a_large_commit_after_an_approval_is_a_full_review_of_the_increment_only() {
    let (_temp, root) = fixture("true");
    graded(&root, Tier::Low, 9);
    let approved = head(&root);
    write(&root, "big.txt", &"line\n".repeat(SMALL + 1));
    let increment = increment(&root);
    assert_eq!(increment.scope, Scope::Since(approved));
    assert_eq!(increment.lines, SMALL + 1);
    assert!(!increment.closing, "over {SMALL} lines it is a full review");
    write(&root, "small.txt", "x\n");
    let two = increment_lines(&root);
    assert_eq!(
        two,
        SMALL + 2,
        "still everything since the approval, nothing before it"
    );
}

fn increment_lines(root: &Path) -> usize {
    increment(root).lines
}

#[test]
fn a_commit_that_raises_the_floor_above_the_passed_tier_gets_a_full_review() {
    let (_temp, root) = fixture("true");
    graded(&root, Tier::Low, 9);
    // `gates/**` is the fixture's `everything` suite: floor high.
    fs::create_dir_all(root.join("gates")).unwrap();
    write(&root, "gates/note.txt", "x\n");
    let increment = increment(&root);
    assert!(
        !increment.closing,
        "one line, but its floor (high) is above the low tier that passed"
    );
    assert_eq!(increment.lines, 1);
    let tiers = RefCell::new(Vec::new());
    ship_with(&root, &reviewed(), &|root, tier, _, _, plan| {
        tiers.borrow_mut().push((tier, plan.closing_style()));
        Ok(judged(root, tier, plan, true))
    })
    .expect("ships");
    assert_eq!(*tiers.borrow(), [(Tier::High, false)]);
}

#[test]
fn a_clean_rebase_keeps_the_approval_and_a_later_commit_is_reviewed_alone() {
    let (_temp, root) = fixture("true");
    let source = graded(&root, Tier::Low, 9);
    advance_dev(&root, "dev-only.txt", "dev\n");
    run(&root, &["rebase", "-q", "dev"]);
    let rebased = head(&root);
    assert_ne!(rebased, source.head);
    let history = review::history(&root, "feature").unwrap();
    // Unchanged patch: reuse (as before), nothing to review.
    assert!(
        Report::reuse_for_change(&root, &rebased, "feature", &["low"], "")
            .unwrap()
            .is_some()
    );
    drop(history);
    write(&root, "follow.txt", "one\ntwo\n");
    let increment = increment(&root);
    assert_eq!(
        increment.scope,
        Scope::Since(rebased),
        "the rebased approved commit"
    );
    assert_eq!(increment.approved, source.head);
    assert_eq!(increment.lines, 2);
}

#[test]
fn a_rebase_that_changed_the_approved_patch_puts_the_changed_files_in_the_increment() {
    let (_temp, root) = fixture("true");
    advance_dev(&root, "shared.txt", "base\n");
    run(&root, &["rebase", "-q", "dev"]);
    write(&root, "kept.txt", "kept\n");
    write(&root, "shared.txt", "mine\n");
    let source = graded(&root, Tier::Low, 9);
    advance_dev(&root, "shared.txt", "theirs\n");
    // What a rebase onto the moved dev leaves after a conflict resolved by
    // hand: kept.txt as approved, shared.txt resolved differently.
    run(&root, &["checkout", "-q", "-B", "feature", "dev"]);
    write(&root, "kept.txt", "kept\n");
    write(&root, "shared.txt", "mine and theirs\n");
    let head = head(&root);
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"], "")
            .unwrap()
            .is_none(),
        "the changed patch is not the approved one"
    );
    let increment = increment(&root);
    assert_eq!(increment.approved, source.head);
    assert_eq!(
        increment.scope,
        Scope::Files(vec!["shared.txt".into()]),
        "only the file whose changed lines differ is unapproved; kept.txt stays approved"
    );
}

#[test]
fn nothing_approved_is_a_full_review_of_everything() {
    let (_temp, root) = fixture("true");
    assert!(matches!(next(&root), Next::Review(Plan::Full)));
    let mut failed = unstored(&root, Tier::Low, 5);
    failed.findings = vec![];
    failed.store(&root).unwrap();
    write(&root, "more.txt", "x\n");
    assert!(
        matches!(next(&root), Next::Review(Plan::Closing { .. })),
        "a failed review is not an approval: its fixes are closing-reviewed"
    );
}

#[test]
fn an_unreadable_stored_report_approves_nothing() {
    let (_temp, root) = fixture("true");
    let report = graded(&root, Tier::Low, 9);
    let common = git::git(
        &root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .unwrap();
    let stored = Path::new(&common).join("aoe-ship/reviews");
    for entry in fs::read_dir(&stored).unwrap() {
        let path = entry.unwrap().path();
        assert!(path.to_string_lossy().contains(&report.head));
        fs::write(path, "{ not a report").unwrap();
    }
    write(&root, "follow.txt", "x\n");
    assert!(
        matches!(next(&root), Next::Review(Plan::Full)),
        "an unreadable report is no approval: the whole diff is reviewed"
    );
    // An old report that lacks today's fields reads as unreadable too.
    let mut value = serde_json::to_value(&report).unwrap();
    value.as_object_mut().unwrap().remove("branch");
    fs::write(
        stored.join(format!("{}-low-9.json", report.head)),
        value.to_string(),
    )
    .unwrap();
    assert!(matches!(next(&root), Next::Review(Plan::Full)));
}

#[test]
fn the_budget_of_an_increment_starts_afresh_and_still_ends_in_a_split() {
    let (_temp, root) = fixture("true");
    graded(&root, Tier::Low, 9);
    write(&root, "follow.txt", "x\n");
    // The increment's first review fails without a severe finding.
    let error = ship_with(&root, &reviewed(), &|root, tier, _, _, plan| {
        assert!(matches!(plan, Plan::Increment(_)), "{plan:?}");
        Ok(judged(root, tier, plan, false))
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("closing review of your fixes"), "{error}");
    for closing in 1..=3 {
        commit(&root, &format!("fix {closing}"));
        let error = ship_with(&root, &reviewed(), &|root, tier, _, _, plan| {
            let Plan::Closing { prior, .. } = plan else {
                panic!("fixes of an increment are closing reviews: {plan:?}");
            };
            assert!(!prior.is_empty());
            let report = failed_closing(root, tier);
            report.store(root).unwrap();
            Ok(report)
        })
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("closing review failed"),
            "{closing}: {error}"
        );
        assert_eq!(error.contains("Split the change"), closing == 3, "{error}");
    }
    commit(&root, "more");
    let error = ship_with(&root, &reviewed(), &|_, _, _, _, _| panic!("no review"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Split the change"), "{error}");
}
