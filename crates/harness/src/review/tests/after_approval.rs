//! A review of the commits added after an approval judges only their diff: a
//! finding outside it is a note and cannot block; one inside it can.
use super::super::{Ask, Increment, Plan, Tier, increment::Scope, protocol::Status, review_with};
use super::flow::{fixture, git};
use crate::agents::Runtime;
use std::{fs, path::Path, sync::Mutex};

const OUTSIDE: &str = r#"{"file": "README.md", "line": 1, "severity": "critical", "category": "correctness", "claim": "approved code is wrong", "trigger": "t", "expected_vs_actual": "e", "evidence": "README.md:1"}"#;
const INSIDE: &str = r#"{"file": "docs/follow.md", "line": 2, "severity": "major", "category": "correctness", "claim": "the new line is wrong", "trigger": "t", "expected_vs_actual": "e", "evidence": "docs/follow.md:2"}"#;

/// Answers every reviewer; the persona `reporter` alone reports `findings`.
struct Stub {
    reporter: &'static str,
    findings: Vec<&'static str>,
    prompts: Mutex<Vec<String>>,
    fired: Mutex<bool>,
}

impl Ask for Stub {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        self.prompts.lock().unwrap().extend(prompts.clone());
        prompts
            .iter()
            .map(|p| {
                Ok(if p.contains("# Grader") {
                    r#"{"grade": 9, "summary": "Fine.\nNothing else."}"#.into()
                } else {
                    let upheld = ["F1"]
                        .iter()
                        .filter(|id| p.contains(&format!("### {id} ")))
                        .map(|id| {
                            format!(r#"{{"id": "{id}", "verdict": "upheld", "evidence": "x"}}"#)
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let mut fired = self.fired.lock().unwrap();
                    let own = if p.contains(self.reporter) && !*fired {
                        *fired = true;
                        self.findings.join(",")
                    } else {
                        String::new()
                    };
                    format!(r#"{{"verdicts": [{upheld}], "findings": [{own}]}}"#)
                })
            })
            .collect()
    }
}

/// The fixture's one commit is approved; `docs/follow.md` is the 3-line increment.
fn increment(root: &Path, closing: bool) -> Plan {
    let approved = super::super::git::git(root, &["rev-parse", "HEAD"]).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/follow.md"), "one\ntwo\nthree\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "follow"]);
    Plan::Increment(Increment {
        approved: approved.clone(),
        grade: 9,
        tier: "medium".into(),
        scope: Scope::Since(approved),
        lines: 3,
        closing,
        outside: vec![],
    })
}

fn run(closing: bool, findings: Vec<&'static str>) -> (super::super::Report, Vec<String>) {
    let temp = fixture("README.md");
    let plan = increment(temp.path(), closing);
    let stub = Stub {
        reporter: "Correctness",
        findings,
        prompts: Mutex::new(vec![]),
        fired: Mutex::new(false),
    };
    let report = review_with(
        temp.path(),
        Tier::Medium,
        Runtime::Claude,
        "task",
        &plan,
        &stub,
    )
    .unwrap();
    let prompts = stub.prompts.into_inner().unwrap();
    (report, prompts)
}

#[test]
fn a_finding_outside_the_increment_is_a_note_that_cannot_block() {
    for closing in [true, false] {
        let (report, prompts) = run(closing, vec![OUTSIDE]);
        assert!(report.findings.is_empty(), "closing={closing}");
        let increment = report
            .increment
            .as_ref()
            .expect("the report names the increment");
        assert_eq!(increment.outside.len(), 1);
        assert_eq!(increment.outside[0].claim, "approved code is wrong");
        assert!(report.passes(), "closing={closing}: {}", report.markdown());
        assert_eq!(report.closing, closing);
        assert!(report.markdown().contains("Notes outside the increment"));
        assert!(
            prompts.iter().all(|p| !p.contains("+changed")),
            "the approved diff is not shown as the thing to judge"
        );
        assert!(
            prompts
                .iter()
                .all(|p| p.contains("+three") && p.contains("read-only context")),
            "every reviewer gets the increment and the scope"
        );
    }
}

#[test]
fn a_finding_inside_the_increment_blocks() {
    for closing in [true, false] {
        let (report, _) = run(closing, vec![INSIDE, OUTSIDE]);
        assert_eq!(report.findings.len(), 1, "closing={closing}");
        assert_eq!(report.findings[0].status, Status::Confirmed);
        assert!(!report.passes(), "closing={closing}");
        assert_eq!(report.increment.unwrap().outside.len(), 1);
    }
}

#[test]
fn a_closing_style_increment_reviews_on_the_strong_model_without_a_blind_round() {
    let (closing, _) = run(true, vec![]);
    let (full, _) = run(false, vec![]);
    assert!(closing.closing && !full.closing);
    assert_eq!(closing.model, "claude-opus-5-5");
    assert_eq!(full.model, "claude-sonnet-5-5");
}
