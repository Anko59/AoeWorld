use super::super::{
    Ask, Plan, Tier,
    protocol::{ARBITER, Cast, Severity, Status, Vote, cap},
    review_with,
};
use super::{finding, flow::fixture};
use crate::agents::Runtime;
use std::sync::Mutex;

const MAJOR: &str = r#"{"findings": [{"file": "README.md", "line": 1, "severity": "major", "category": "correctness", "claim": "wrong text", "trigger": "read it", "expected_vs_actual": "base vs changed", "evidence": "README.md:1"}]}"#;
const MINOR: &str = r#"{"findings": [{"file": "README.md", "line": 1, "severity": "minor", "category": "correctness", "claim": "small", "trigger": "x", "expected_vs_actual": "y", "evidence": "z"}]}"#;

/// The correctness reviewer reports F1; the spec reviewer upholds it and the
/// hostile-input reviewer refutes it (the others abstain): it stays disputed
/// until the arbiter's session, which answers `ruling`.
struct Arbitrated {
    first: &'static str,
    ruling: &'static str,
    strong: Mutex<Vec<String>>,
}

impl Ask for Arbitrated {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        prompts
            .iter()
            .map(|p| {
                Ok(if p.contains("# Grader") {
                    r#"{"grade": 9, "summary": "ok"}"#.into()
                } else if p.contains("verify the other reviewers' findings") {
                    let vote = if p.contains("# Spec and scope auditor") {
                        "upheld"
                    } else if p.contains("# Hostile-input and safety adversary") {
                        "refuted"
                    } else {
                        "unverifiable"
                    };
                    format!(
                        r#"{{"verdicts": [{{"id": "F1", "verdict": "{vote}", "evidence": "README.md:1"}}], "findings": []}}"#
                    )
                } else if p.contains("# Correctness breaker") {
                    self.first.into()
                } else {
                    r#"{"findings": []}"#.into()
                })
            })
            .collect()
    }

    fn strong(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        self.strong.lock().unwrap().extend(prompts.clone());
        prompts.iter().map(|_| Ok(self.ruling.into())).collect()
    }
}

fn ask(first: &'static str, ruling: &'static str) -> Arbitrated {
    Arbitrated {
        first,
        ruling,
        strong: Mutex::new(Vec::new()),
    }
}

fn run(tier: Tier, ask: &Arbitrated) -> super::super::Report {
    let temp = fixture("README.md");
    review_with(temp.path(), tier, Runtime::Claude, "", &Plan::Full, ask).unwrap()
}

#[test]
fn the_arbiter_upholds_a_disputed_major_and_it_counts_in_full() {
    let ask = ask(
        MAJOR,
        r#"{"rulings": [{"id": "F1", "ruling": "upheld", "evidence": "README.md:1 changed"}]}"#,
    );
    let report = run(Tier::High, &ask);
    let finding = &report.findings[0];
    assert_eq!(finding.status, Status::Confirmed);
    assert_eq!(finding.votes.last().unwrap().reviewer, ARBITER);
    assert_eq!(
        report.grade, 7,
        "an upheld major caps at 7, not as a disputed one"
    );
    assert!(
        report
            .markdown()
            .contains("arbiter: Upheld — README.md:1 changed")
    );
    let prompts = ask.strong.lock().unwrap();
    assert_eq!(
        prompts.len(),
        1,
        "one session rules on every disputed finding"
    );
    assert!(prompts[0].contains("# Arbiter") && prompts[0].contains("### F1"));
}

#[test]
fn the_arbiter_can_refute_a_disputed_major() {
    let ask = ask(
        MAJOR,
        r#"{"rulings": [{"id": "F1", "ruling": "refuted", "evidence": "README.md:1 is fine"}]}"#,
    );
    let report = run(Tier::High, &ask);
    assert_eq!(report.findings[0].status, Status::Refuted);
    assert!(report.passes(), "{}", report.markdown());
    assert!(report.markdown().contains("arbiter: Refuted"));
}

#[test]
fn without_a_usable_ruling_the_finding_stays_disputed() {
    for answer in [
        "not json",
        r#"{"rulings": [{"id": "F1", "ruling": "maybe"}]}"#,
    ] {
        let ask = ask(MAJOR, answer);
        let report = run(Tier::High, &ask);
        assert_eq!(report.findings[0].status, Status::Disputed);
        assert!(report.failures.is_empty(), "the review is still complete");
        assert_eq!(
            report.grade, 9,
            "a disputed major counts one severity lower"
        );
        assert_eq!(
            ask.strong.lock().unwrap().len(),
            2,
            "one retry, then give up"
        );
    }
}

#[test]
fn rulings_on_unknown_findings_change_nothing() {
    let ask = ask(
        MAJOR,
        r#"{"rulings": [{"id": "F9", "ruling": "refuted", "evidence": "x"}]}"#,
    );
    let report = run(Tier::High, &ask);
    assert_eq!(report.findings[0].status, Status::Disputed);
}

#[test]
fn lower_tiers_and_minor_findings_never_reach_the_arbiter() {
    let ruling = r#"{"rulings": [{"id": "F1", "ruling": "upheld", "evidence": "x"}]}"#;
    let medium = ask(MAJOR, ruling);
    assert_eq!(
        run(Tier::Medium, &medium).findings[0].status,
        Status::Confirmed
    );
    assert!(medium.strong.lock().unwrap().is_empty());
    let minor = ask(MINOR, ruling);
    assert_eq!(run(Tier::High, &minor).findings[0].status, Status::Disputed);
    assert!(minor.strong.lock().unwrap().is_empty());
}

#[test]
fn an_upheld_ruling_is_not_lowered_by_the_split_votes_it_settled() {
    let mut f = finding(
        "F1",
        0,
        Severity::Major,
        "spec",
        &[(1, 2, Vote::Upheld), (2, 2, Vote::Refuted)],
    );
    assert_eq!(
        cap(std::slice::from_ref(&f)),
        10,
        "disputed: one severity lower"
    );
    f.votes.push(Cast {
        reviewer: ARBITER,
        round: 3,
        vote: Vote::Upheld,
        evidence: "x".into(),
    });
    f.status = Status::Confirmed;
    assert_eq!(cap(&[f]), 7);
}
