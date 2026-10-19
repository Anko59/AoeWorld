//! The merge bar of 7 (2026-10-19): a grade at the bar passes only while no major or
//! critical finding stands.
use super::{
    super::{Plan, Tier, review_with},
    flow::{Script, fixture},
};
use crate::agents::Runtime;
use std::sync::Mutex;

#[test]
fn a_seven_passes_only_without_a_blocking_finding() {
    let temp = fixture("README.md");
    let root = temp.path();
    let clean = Script {
        first: |_| r#"{"findings": []}"#.into(),
        cross: |_| unreachable!(),
        grade: r#"{"grade": 7, "summary": "fine"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(root, Tier::High, Runtime::Claude, "", &Plan::Full, &clean).unwrap();
    assert!(
        report.passes(),
        "7 with nothing confirmed passes at the bar of 7"
    );
    let mut major = report.clone();
    let mut confirmed = super::finding(
        "F1",
        0,
        crate::review::protocol::Severity::Major,
        "correctness",
        &[],
    );
    confirmed.status = crate::review::protocol::Status::Confirmed;
    major.findings.push(confirmed);
    assert!(
        !major.passes(),
        "a standing major finding still blocks at 7"
    );
}

#[test]
fn a_disputed_major_counts_as_minor_and_a_disputed_critical_still_blocks() {
    let temp = fixture("README.md");
    let root = temp.path();
    let clean = Script {
        first: |_| r#"{"findings": []}"#.into(),
        cross: |_| unreachable!(),
        grade: r#"{"grade": 7, "summary": "fine"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(root, Tier::High, Runtime::Claude, "", &Plan::Full, &clean).unwrap();
    for (severity, passes) in [
        (crate::review::protocol::Severity::Major, true),
        (crate::review::protocol::Severity::Critical, false),
    ] {
        let mut disputed = report.clone();
        let mut finding = super::finding("F1", 0, severity, "correctness", &[]);
        finding.status = crate::review::protocol::Status::Disputed;
        disputed.findings.push(finding);
        assert_eq!(disputed.passes(), passes, "disputed {severity:?}");
    }
}
