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

fn report(head: &str, finished: u64, grade: u8, blocking: &[Severity]) -> Report {
    Report {
        version: 1,
        head: head.into(),
        branch: "feature".into(),
        base: String::new(),
        merge_base: String::new(),
        tier: "high".into(),
        floor: "high".into(),
        runtime: "claude".into(),
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
    }
}

#[test]
fn full_reviews_run_until_the_budget_is_spent() {
    assert_eq!(BUDGET, 3);
    let two = [
        report("a", 1, 4, &[Severity::Major]),
        report("b", 2, 7, &[Severity::Major]),
    ];
    assert!(matches!(next(&two, "c", NOW), Next::Review(Plan::Full)));
    assert!(
        matches!(next(&two, "b", NOW), Next::FixFirst(why) if why.contains("already failed")),
        "a failed commit is never reviewed again: that would only fish for a grade"
    );
    // A passing review settles everything before it.
    let mut settled = vec![
        report("a", 1, 4, &[]),
        report("b", 2, 4, &[]),
        report("c", 3, 4, &[]),
        report("d", 4, 9, &[]),
    ];
    settled.push(report("e", 5, 6, &[]));
    assert!(matches!(next(&settled, "f", NOW), Next::Review(Plan::Full)));
}

#[test]
fn a_converging_branch_gets_one_closing_review_of_its_fixes() {
    let history = [
        report("a", 1, 4, &[Severity::Major, Severity::Major]),
        report("b", 2, 7, &[Severity::Major]),
        report("c", 3, 4, &[Severity::Major]),
    ];
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
        ["P1", "P2", "P3", "P4"],
        "every earlier blocking finding is re-verified"
    );
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

fn three_converging() -> Vec<Report> {
    vec![
        report("a", 1, 4, &[Severity::Major, Severity::Major]),
        report("b", 2, 7, &[Severity::Major]),
        report("c", 3, 4, &[Severity::Major]),
    ]
}

#[test]
fn a_diverging_branch_is_split_without_a_person() {
    let growing = [
        report("a", 1, 4, &[Severity::Major]),
        report("b", 2, 7, &[Severity::Major]),
        report("c", 3, 6, &[Severity::Major, Severity::Major]),
    ];
    assert!(matches!(next(&growing, "d", NOW), Next::Split(why) if why.contains("from 1 to 2")));
    let critical = [
        report("a", 1, 4, &[Severity::Major]),
        report("b", 2, 7, &[Severity::Major]),
        report("c", 3, 4, &[Severity::Critical]),
    ];
    assert!(matches!(next(&critical, "d", NOW), Next::Split(why) if why.contains("critical")));
    let mut reopened = three_converging();
    reopened.push(closing("d", 4, &[Severity::Major], &[]));
    assert!(
        matches!(next(&reopened, "e", NOW), Next::Split(why) if why.contains("not shown fixed")),
        "a fix that did not fix splits the branch"
    );
}

#[test]
fn a_closing_review_passes_on_no_blocking_finding_not_on_the_grade() {
    let mut closing = report("d", 4, 6, &[Severity::Minor]);
    closing.closing = true;
    assert!(
        closing.passes(),
        "a 6/10 with only minor findings left passes"
    );
    assert!(closing.markdown().contains("Closing review"));
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
    assert_eq!(CLOSING_BUDGET, 2);
    let mut history = three_converging();
    let mut first = closing("d", 4, &[Severity::Major], &[Severity::Major]);
    first.findings[0].status = Status::Refuted; // the carried one: shown fixed
    history.push(first);
    let Next::Review(Plan::Closing { since, prior }) = next(&history, "e", NOW) else {
        panic!("a closing review that only finds new issues in the fixes earns another");
    };
    assert_eq!(since, "d");
    assert!(
        prior.iter().all(|f| f.reported.claim != "d claim 0"),
        "a finding shown fixed is not carried again"
    );
    history.push(closing("e", 5, &[], &[Severity::Major]));
    assert!(
        matches!(next(&history, "f", NOW), Next::Split(why) if why.contains("2 closing reviews")),
        "no unreviewed ending: the branch is split"
    );
}

#[test]
fn an_undecided_carried_finding_counts_as_open() {
    let mut history = three_converging();
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
    let mut history = three_converging();
    history.push(r);
    assert!(
        !matches!(next(&history, "e", NOW), Next::Split(_)),
        "a minor carried finding is not left open"
    );

    let mut upheld = closing("d", 4, &[Severity::Major], &[]);
    upheld.findings[0].status = Status::Confirmed;
    assert!(!upheld.passes(), "a carried major still confirmed blocks");
    let mut history = three_converging();
    history.push(upheld);
    assert!(
        matches!(next(&history, "e", NOW), Next::Split(why) if why.contains("not shown fixed"))
    );
}

#[test]
fn a_carried_finding_settled_as_minor_is_not_carried_again() {
    // The first closing review confirms carried P1 only as minor (partial
    // votes) but fails on a new major: the next plan must not reopen P1.
    let mut history = three_converging();
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
    first.findings[0].reported = history[2].findings[0].reported.clone();
    assert!(
        !first.passes(),
        "the new major still fails the first closing review"
    );
    let settled = first.findings[0].reported.claim.clone();
    history.push(first);
    let Next::Review(Plan::Closing { prior, .. }) = next(&history, "e", NOW) else {
        panic!("a converging branch with one closing review left gets it");
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
    let mut history = three_converging();
    let mut first = closing("d", 4, &[Severity::Major], &[Severity::Major]);
    first.findings[0].reported = history[2].findings[0].reported.clone();
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
        panic!("a converging branch with one closing review left gets it");
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
