use super::super::{
    Report,
    closing::{BUDGET, CARRIED, CLOSING_BUDGET, Next, Plan, next},
    protocol::{Severity, Status, Vote},
};
use super::super::{Tier, review_with};
use super::{
    finding,
    flow::{Script, fixture, git},
};
use crate::agents::Runtime;
use std::{fs, process::Command, sync::Mutex};

/// "Now" in these tests: long after every stored review.
const NOW: u64 = 1_000_000;

pub(super) fn report(head: &str, finished: u64, grade: u8, blocking: &[Severity]) -> Report {
    Report {
        version: 1,
        head: head.into(),
        branch: "feature".into(),
        base: String::new(),
        merge_base: String::new(),
        tier: "high".into(),
        floor: "high".into(),
        runtime: "claude".into(),
        runtime_fallback: false,
        model: "m".into(),
        effort: "high".into(),
        personas: vec!["a".into(), "b".into()],
        rounds: 2,
        findings: blocking
            .iter()
            .enumerate()
            .map(|(i, severity)| {
                let mut f = finding(&format!("F{i}"), 0, *severity, "correctness", &[]);
                f.reported.claim = format!("{head} claim {i}");
                f.status = Status::Confirmed;
                f
            })
            .collect(),
        written_grade: grade,
        grade,
        summary: String::new(),
        failures: vec![],
        merge_grade: 8,
        started: finished,
        finished,
        closing: false,
        change_fingerprint: None,
        policy_fingerprint: None,
        reused_from: None,
        increment: None,
        task_fingerprint: None,
        note: None,
        decision: None,
        base_branch: "dev".into(),
    }
}

#[test]
fn full_reviews_run_until_the_budget_is_spent() {
    assert_eq!(BUDGET, 1);
    assert!(matches!(next(&[], "a", NOW), Next::Review(Plan::Full)));
    let mut incomplete = report("a", 1, 9, &[]);
    incomplete.failures.push("no answer".into());
    let mut history = vec![incomplete];
    assert!(matches!(next(&history, "a", NOW), Next::Review(Plan::Full)));
    history.push(report("a", 2, 7, &[Severity::Major]));
    assert!(matches!(
        next(&history, "b", NOW),
        Next::Review(Plan::Closing { .. })
    ));
    assert!(
        matches!(next(&history, "a", NOW), Next::FixFirst(why) if why.contains("already failed")),
        "a failed commit is never reviewed again: that would only fish for a grade"
    );
}

#[test]
fn one_failing_non_severe_full_review_carries_its_blockers_into_closing() {
    let history = [report("c", 1, 7, &[Severity::Major, Severity::Major])];
    assert!(
        matches!(next(&history, "c", NOW), Next::FixFirst(why) if why.contains("fix and commit"))
    );
    let Next::Review(Plan::Closing { since, prior }) = next(&history, "d", NOW) else {
        panic!("a closing review");
    };
    assert_eq!(
        since, "c",
        "it audits the fixes since the last reviewed commit"
    );
    let ids: Vec<&str> = prior.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(
        ids,
        ["P1", "P2"],
        "every blocking finding from the full review is re-verified"
    );
    for (carried, original) in prior.iter().zip(&history[0].findings) {
        assert_eq!(carried.reported.claim, original.reported.claim);
    }
    assert!(
        prior
            .iter()
            .all(|f| f.reporter == CARRIED && f.votes.is_empty())
    );
    assert!(prior.iter().all(|f| f.status == Status::Disputed));
}

fn closing(head: &str, finished: u64, carried: &[Severity], found: &[Severity]) -> Report {
    let mut r = report(head, finished, 6, carried);
    for f in &mut r.findings {
        f.reporter = CARRIED;
    }
    r.findings.extend(
        report(head, finished, 6, found)
            .findings
            .into_iter()
            .map(|mut f| {
                f.id = format!("N{}", f.id);
                f
            }),
    );
    r.closing = true;
    r
}

fn failed_full() -> Vec<Report> {
    vec![report("c", 3, 7, &[Severity::Major])]
}

#[test]
fn a_diverging_branch_is_split_without_a_person() {
    let mut reopened = failed_full();
    reopened.push(closing("d", 4, &[Severity::Major], &[]));
    assert!(
        matches!(next(&reopened, "e", NOW), Next::Split(why) if why.contains("not shown fixed")),
        "a fix that did not fix splits the branch"
    );
}

#[test]
fn a_severe_full_review_goes_to_a_closing_review_that_must_show_it_fixed() {
    // A critical finding or a weakened test is usually a small fix: the closing
    // review carries it and splits only if it is still there.
    let critical = [report("c", 1, 4, &[Severity::Critical])];
    assert!(matches!(
        next(&critical, "d", NOW),
        Next::Review(Plan::Closing { prior, .. }) if prior.len() == 1
    ));
    let mut weakened = report("c", 1, 4, &[Severity::Major]);
    weakened.findings[0].reported.category = "test-integrity".into();
    assert!(matches!(
        next(&[weakened], "d", NOW),
        Next::Review(Plan::Closing { prior, .. }) if prior.len() == 1
    ));
}

#[test]
fn a_passing_review_resets_the_history() {
    for passed in [report("d", 4, 9, &[]), closing("d", 4, &[], &[])] {
        let mut history = failed_full();
        history.push(passed);
        assert!(matches!(next(&history, "e", NOW), Next::Review(Plan::Full)));
        history.push(report("e", 5, 6, &[]));
        let Next::Review(Plan::Closing { since, prior }) = next(&history, "f", NOW) else {
            panic!("the new full review spends the reset budget");
        };
        assert_eq!(since, "e");
        assert!(prior.is_empty(), "findings before the pass are settled");
    }
}

#[test]
fn an_incomplete_closing_review_neither_counts_nor_moves_the_audit_base() {
    let mut history = failed_full();
    let mut incomplete = closing("d", 4, &[Severity::Critical], &[]);
    incomplete.failures.push("no answer".into());
    history.push(incomplete);
    let Next::Review(Plan::Closing { since, prior }) = next(&history, "d", NOW) else {
        panic!("the incomplete closing review may run again at the same commit");
    };
    assert_eq!(since, "c");
    assert_eq!(prior.len(), 1);
    assert_eq!(prior[0].reported.claim, "c claim 0");
    assert_eq!(prior[0].reported.severity, Severity::Major);
}

#[test]
fn a_closing_review_passes_on_no_blocking_finding_not_on_the_grade() {
    let mut closing = report("d", 4, 6, &[Severity::Minor]);
    closing.closing = true;
    assert!(
        closing.passes(),
        "a 6/10 with only minor findings left passes"
    );
    assert!(
        closing
            .markdown()
            .contains("Closing review after one full review")
    );
    closing.findings = report("d", 4, 9, &[Severity::Major]).findings;
    assert!(
        !closing.passes(),
        "a blocking finding fails it whatever the grade"
    );
    closing.findings.clear();
    closing.failures.push("round 1: no answer".into());
    assert!(!closing.passes(), "an incomplete closing review fails");
}

#[test]
fn a_closing_review_verifies_carried_findings_against_the_fixes() {
    let temp = fixture("README.md");
    let root = temp.path();
    let since = {
        let out = Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    fs::write(root.join("README.md"), "fixed\n").unwrap();
    git(root, &["commit", "-q", "-am", "fix P1"]);
    let mut carried = finding("P1", CARRIED, Severity::Major, "correctness", &[]);
    carried.round = 0;
    let plan = Plan::Closing {
        since,
        prior: vec![carried],
    };
    let fixed = Script {
        first: |_| unreachable!("a closing review has no blind round"),
        cross: |p| {
            assert!(p.contains("Closing review"), "{p}");
            assert!(p.contains("+fixed"), "the prompt shows the fixes");
            r#"{"verdicts": [{"id": "P1", "verdict": "refuted", "evidence": "README.md:1 fixed"}], "findings": []}"#.into()
        },
        grade: r#"{"grade": 6, "summary": "fixed"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(root, Tier::Low, Runtime::Claude, "", &plan, &fixed).unwrap();
    assert!(report.closing);
    assert_eq!(report.findings[0].status, Status::Refuted);
    assert!(report.passes(), "{}", report.markdown());
    assert_eq!(
        report.model, "claude-opus-5-5",
        "closing reviews use the strong models"
    );
    let open = Script {
        first: |_| unreachable!(),
        cross: |_| {
            r#"{"verdicts": [{"id": "P1", "verdict": "upheld", "evidence": "still there"}], "findings": []}"#.into()
        },
        grade: r#"{"grade": 9, "summary": "x"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(root, Tier::Low, Runtime::Claude, "", &plan, &open).unwrap();
    assert!(
        !report.passes(),
        "a finding still present blocks whatever the grade"
    );
}

#[test]
fn reviewers_unavailable_lasts_an_hour_not_forever() {
    use super::super::closing::UNAVAILABLE_SECONDS;
    let incomplete = |head: &str, finished: u64| {
        let mut r = report(head, finished, 9, &[]);
        r.failures.push("no answer".into());
        r
    };
    let outage = [
        incomplete("a", NOW - 30),
        incomplete("a", NOW - 20),
        incomplete("a", NOW - 10),
    ];
    assert!(matches!(next(&outage, "a", NOW), Next::Unavailable(_)));
    assert!(
        matches!(
            next(&outage, "a", NOW + UNAVAILABLE_SECONDS),
            Next::Review(Plan::Full)
        ),
        "after the outage the same commit is reviewed again"
    );
}

#[test]
fn an_undecided_carried_finding_fails_the_closing_review() {
    let mut r = closing("d", 4, &[Severity::Major], &[]);
    r.findings[0].status = Status::Disputed;
    assert!(!r.passes(), "nobody showed it fixed");
    r.findings[0].status = Status::Refuted;
    assert!(r.passes(), "refuted: shown fixed");
}

#[test]
fn closing_reviews_then_a_split_end_a_converging_branch() {
    assert_eq!(CLOSING_BUDGET, 3);
    let mut history = failed_full();
    for (head, next_head, finished) in [("d", "e", 4), ("e", "f", 5), ("f", "g", 6)] {
        let Next::Review(Plan::Closing { since, prior }) = next(&history, head, NOW) else {
            panic!("each fix gets a closing review within the budget");
        };
        assert_eq!(since, history.last().unwrap().head);
        assert_eq!(
            prior.len(),
            1,
            "only the latest unsettled blocker is carried"
        );
        assert_eq!(prior[0].reported.claim, format!("{since} claim 0"));
        let mut review = closing(head, finished, &[], &[Severity::Major]);
        review.findings.extend(prior.into_iter().map(|mut f| {
            f.status = Status::Refuted;
            f
        }));
        history.push(review);
        assert!(matches!(next(&history, head, NOW), Next::FixFirst(_)));
        if head == "f" {
            assert!(
                matches!(next(&history, next_head, NOW), Next::Split(why) if why.contains("3 closing reviews")),
                "the third failing closing review ends in a split, never unreviewed code"
            );
        } else {
            assert!(matches!(
                next(&history, next_head, NOW),
                Next::Review(Plan::Closing { .. })
            ));
        }
    }
}

#[test]
fn an_undecided_carried_finding_counts_as_open() {
    let mut history = failed_full();
    let mut undecided = closing("d", 4, &[Severity::Major], &[]);
    undecided.findings[0].status = Status::Disputed;
    history.push(undecided);
    assert!(
        matches!(next(&history, "e", NOW), Next::Split(why) if why.contains("not shown fixed"))
    );
}

#[test]
fn a_carried_finding_confirmed_only_as_minor_no_longer_blocks() {
    // Partial votes confirm a carried major one severity lower: a minor
    // finding never blocks a full review, so it does not block a closing one.
    let partial = || {
        let mut f = finding(
            "P1",
            CARRIED,
            Severity::Major,
            "correctness",
            &[
                (0, 2, Vote::Partial),
                (1, 2, Vote::Partial),
                (2, 2, Vote::Partial),
                (3, 2, Vote::Refuted),
                (4, 2, Vote::Refuted),
            ],
        );
        f.status = Status::Confirmed;
        f
    };
    let mut r = closing("d", 4, &[], &[]);
    r.findings.push(partial());
    assert!(
        r.passes(),
        "a carried finding left at minor passes the closing review"
    );
    let mut history = failed_full();
    history.push(r);
    assert!(
        !matches!(next(&history, "e", NOW), Next::Split(_)),
        "a minor carried finding is not left open"
    );

    let mut upheld = closing("d", 4, &[Severity::Major], &[]);
    upheld.findings[0].status = Status::Confirmed;
    assert!(!upheld.passes(), "a carried major still confirmed blocks");
    let mut history = failed_full();
    history.push(upheld);
    assert!(
        matches!(next(&history, "e", NOW), Next::Split(why) if why.contains("not shown fixed"))
    );
}

#[test]
fn a_carried_finding_settled_as_minor_is_not_carried_again() {
    // The first closing review confirms carried P1 only as minor (partial
    // votes) but fails on a new major: the next plan must not reopen P1.
    let mut history = failed_full();
    let mut first = closing("d", 4, &[Severity::Major], &[Severity::Major]);
    first.findings[0].votes = finding(
        "P1",
        CARRIED,
        Severity::Major,
        "correctness",
        &[
            (0, 2, Vote::Partial),
            (1, 2, Vote::Partial),
            (2, 2, Vote::Refuted),
        ],
    )
    .votes;
    first.findings[0].status = Status::Confirmed;
    // P1 is the last full review's major finding, carried into this review.
    first.findings[0].reported = history[0].findings[0].reported.clone();
    assert!(
        !first.passes(),
        "the new major still fails the first closing review"
    );
    let settled = first.findings[0].reported.claim.clone();
    history.push(first);
    let Next::Review(Plan::Closing { prior, .. }) = next(&history, "e", NOW) else {
        panic!("a branch with closing reviews left gets another");
    };
    assert!(
        prior.iter().all(|f| f.reported.claim != settled),
        "a finding settled as minor is not carried again"
    );
}

#[test]
fn settling_a_finding_never_hides_another_at_a_different_line() {
    // Closing review 1 settles carried P1 as minor and confirms a new major N
    // with the same file and claim at another line: N is carried next time.
    let mut history = failed_full();
    let mut first = closing("d", 4, &[Severity::Major], &[Severity::Major]);
    first.findings[0].reported = history[0].findings[0].reported.clone();
    first.findings[0].votes = finding(
        "P1",
        CARRIED,
        Severity::Major,
        "correctness",
        &[
            (0, 2, Vote::Partial),
            (1, 2, Vote::Partial),
            (2, 2, Vote::Refuted),
        ],
    )
    .votes;
    first.findings[0].status = Status::Confirmed;
    first.findings[1].reported = first.findings[0].reported.clone();
    first.findings[1].reported.line = Some(80);
    history.push(first);
    let Next::Review(Plan::Closing { prior, .. }) = next(&history, "e", NOW) else {
        panic!("a branch with closing reviews left gets another");
    };
    assert!(
        prior.iter().any(|f| f.reported.line == Some(80)),
        "the new major at line 80 is carried"
    );
}

#[test]
fn a_new_disputed_critical_finding_fails_a_closing_review() {
    let mut r = closing("d", 4, &[], &[Severity::Critical]);
    r.findings[0].status = Status::Disputed;
    assert!(!r.passes(), "a disputed critical still caps below 8");
}

#[test]
fn the_closing_audit_shows_a_fix_that_reverts_a_file_to_dev() {
    let temp = fixture("README.md");
    let root = temp.path();
    fs::write(root.join("added.txt"), "branch file\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "add a file"]);
    let since = {
        let out = Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    // The fix removes the file the branch added: back to dev's content.
    git(root, &["rm", "-q", "added.txt"]);
    git(root, &["commit", "-q", "-m", "drop it"]);
    let plan = Plan::Closing {
        since,
        prior: vec![],
    };
    let script = Script {
        first: |_| unreachable!(),
        cross: |p| {
            assert!(
                p.contains("-branch file"),
                "the reverting fix is shown: {p}"
            );
            r#"{"verdicts": [], "findings": []}"#.into()
        },
        grade: r#"{"grade": 9, "summary": "x"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(root, Tier::Low, Runtime::Claude, "", &plan, &script).unwrap();
    assert!(report.passes(), "{}", report.markdown());
}
