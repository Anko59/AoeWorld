use super::{
    super::{Plan, Tier, protocol::Status, review_with},
    flow::{Script, fixture},
};
use crate::agents::Runtime;
use std::sync::Mutex;

const CORRECTNESS: &str = r#"{"findings": [{"file": "README.md", "line": 1, "severity": "major", "category": "correctness", "claim": "The README text says AOE_E2E_WORKERS is forwarded but it never is", "trigger": "t", "expected_vs_actual": "e", "evidence": "README.md:1"}]}"#;
const SPEC: &str = r#"{"findings": [{"file": "README.md", "line": 2, "severity": "critical", "category": "spec", "claim": "AOE_E2E_WORKERS is never forwarded, so the README text is wrong", "trigger": "t", "expected_vs_actual": "e", "evidence": "README.md:2"}, {"file": "README.md", "line": 90, "severity": "minor", "category": "spec", "claim": "A paragraph far below is stale", "trigger": "t", "expected_vs_actual": "e", "evidence": "README.md:90"}]}"#;

#[test]
fn one_defect_reported_by_two_personas_is_one_finding_in_the_stored_report() {
    let temp = fixture("README.md");
    let script = Script {
        first: |p| {
            if p.contains("# Correctness breaker") {
                CORRECTNESS.into()
            } else if p.contains("# Spec") {
                SPEC.into()
            } else {
                r#"{"findings": []}"#.into()
            }
        },
        // Everyone upholds whichever of F1 and F3 they are shown.
        cross: |_| {
            r#"{"verdicts": [{"id": "F1", "verdict": "upheld", "evidence": "x"}, {"id": "F3", "verdict": "upheld", "evidence": "x"}], "findings": []}"#.into()
        },
        grade: r#"{"grade": 9, "summary": "Changes the README.\nBroken."}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(
        temp.path(),
        Tier::Medium,
        Runtime::Claude,
        "Change the README",
        &Plan::Full,
        &script,
    )
    .unwrap();
    let ids: Vec<&str> = report.findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["F1", "F3"], "F2 is a member of F1: {ids:?}");
    let cluster = &report.findings[0];
    assert_eq!(cluster.members.len(), 1);
    assert_eq!(cluster.members[0].id, "F2");
    assert_eq!(
        cluster.reported.severity,
        super::super::protocol::Severity::Critical
    );
    assert_eq!(cluster.status, Status::Confirmed);
    // Only the third persona is neither reporter: one vote, from it.
    assert_eq!(cluster.votes.len(), 1);
    assert_eq!(cluster.votes[0].reviewer, 2);
    assert_eq!(report.grade, 4, "the critical cluster caps the grade");
    let text = report.markdown();
    assert!(text.contains("also reported as **F2**"), "{text}");
    assert!(text.contains("2 confirmed"), "{}", report.headline());
}
