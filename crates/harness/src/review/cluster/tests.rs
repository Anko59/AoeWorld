//! Real stored reviews (fixtures: the `findings` of reports under
//! `.git/aoe-ship/reviews`) pin that duplicates merge and distinct defects
//! stay apart.
use super::merge;
use crate::review::protocol::{Cast, Finding, Severity, Status, Vote, cap, status};

fn load(json: &str) -> Vec<Finding> {
    serde_json::from_str(json).unwrap()
}

fn merged(json: &str) -> Vec<Finding> {
    let mut findings = load(json);
    merge(&mut findings);
    findings
}

fn ids(finding: &Finding) -> Vec<&str> {
    std::iter::once(finding.id.as_str())
        .chain(finding.members.iter().map(|m| m.id.as_str()))
        .collect()
}

#[test]
fn five_reports_of_one_ci_defect_become_one_finding() {
    let findings = merged(include_str!("fixtures/duplicates.json"));
    assert_eq!(findings.len(), 1);
    assert_eq!(ids(&findings[0]), ["F1", "F2", "F3", "F4", "F5"]);
    let reporters: Vec<usize> = std::iter::once(findings[0].reporter)
        .chain(findings[0].members.iter().map(|m| m.reporter))
        .collect();
    assert_eq!(reporters, [0, 1, 2, 3, 4]);
    // Nothing a reviewer said is dropped.
    assert!(
        findings[0].members[3]
            .reported
            .claim
            .contains("three workers")
    );
}

#[test]
fn the_cluster_counts_at_the_highest_severity_of_its_members() {
    let before = load(include_str!("fixtures/severities.json"));
    assert_eq!(before[0].reported.severity, Severity::Major);
    assert_eq!(before[1].reported.severity, Severity::Critical);
    let findings = merged(include_str!("fixtures/severities.json"));
    assert_eq!(
        findings.len(),
        2,
        "the dispatch-base defect and the other file"
    );
    assert_eq!(ids(&findings[0]), ["F1", "F2"]);
    assert_eq!(findings[0].reported.severity, Severity::Critical);
    assert_eq!(findings[0].members[0].reported.severity, Severity::Critical);
    assert_eq!(findings[1].reported.file, before[2].reported.file);
    // Reporting categories differ (safety, test_integrity): the cluster keeps
    // the test-integrity cap of its member.
    assert!(findings[0].test_integrity());
}

#[test]
fn distinct_defects_on_one_file_stay_apart() {
    let before = load(include_str!("fixtures/distinct.json"));
    let findings = merged(include_str!("fixtures/distinct.json"));
    let all: usize = findings.iter().map(|f| 1 + f.members.len()).sum();
    assert_eq!(all, before.len(), "no finding is dropped");
    // The overflow (F1), the 35% grade rule (F3) and the seed dependence
    // (F4) are three defects; so is the other file's wet-tile one (F5).
    for lone in ["F1", "F3", "F4", "F5"] {
        let cluster = findings.iter().find(|f| f.id == lone).unwrap();
        assert!(cluster.members.is_empty(), "{lone} was merged: {cluster:?}");
    }
}

#[test]
fn findings_far_apart_or_in_other_files_never_merge() {
    let mut findings = load(include_str!("fixtures/duplicates.json"));
    findings[1].reported.line = Some(124 + 40);
    findings[2].reported.file = "other.yml".into();
    merge(&mut findings);
    assert_eq!(
        findings.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
        ["F1", "F2", "F3"]
    );
    assert_eq!(ids(&findings[0]), ["F1", "F4", "F5"]);
}

#[test]
fn a_finding_without_a_line_is_not_merged() {
    let mut findings = load(include_str!("fixtures/duplicates.json"));
    findings[0].reported.line = None;
    findings[1].reported.line = None;
    merge(&mut findings);
    assert!(findings[0].members.is_empty());
}

#[test]
fn nobody_votes_on_a_defect_they_reported() {
    let mut findings = merged(include_str!("fixtures/duplicates.json"));
    let finding = &mut findings[0];
    assert!(finding.reported_by(4) && finding.reported_by(0));
    assert!(!finding.reported_by(5));
    let cast = |reviewer, vote| Cast {
        reviewer,
        round: 2,
        vote,
        evidence: String::new(),
    };
    // Reporters 3 and 4 refuting their own cluster changes nothing.
    finding.votes = vec![
        cast(3, Vote::Refuted),
        cast(4, Vote::Refuted),
        cast(5, Vote::Upheld),
    ];
    assert_eq!(status(finding, 6), Status::Confirmed);
}

#[test]
fn a_merged_major_cap_is_that_of_one_finding_not_five() {
    let mut findings = merged(include_str!("fixtures/duplicates.json"));
    findings[0].status = Status::Confirmed;
    assert_eq!(cap(&findings), 7);
    findings[0].reported.severity = Severity::Critical;
    assert_eq!(cap(&findings), 4);
}

#[test]
fn votes_on_merged_findings_carry_over_to_the_cluster() {
    let mut findings = load(include_str!("fixtures/duplicates.json"));
    findings[1].votes.push(Cast {
        reviewer: 6,
        round: 2,
        vote: Vote::Upheld,
        evidence: "yes".into(),
    });
    let total: usize = findings.iter().map(|f| f.votes.len()).sum();
    merge(&mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].votes.len(),
        total,
        "every vote moves to the cluster"
    );
    assert!(findings[0].votes.iter().any(|v| v.reviewer == 6));
}

#[test]
fn merging_is_idempotent() {
    let mut findings = merged(include_str!("fixtures/duplicates.json"));
    let again = {
        let mut copy = findings.clone();
        merge(&mut copy);
        copy
    };
    merge(&mut findings);
    assert_eq!(findings, again);
}

#[test]
fn the_report_lists_every_merged_member_under_its_cluster() {
    let findings = merged(include_str!("fixtures/duplicates.json"));
    let mut report = crate::review::tests::sample_report();
    report.findings = findings;
    let text = report.markdown();
    assert!(text.contains("Confirmed findings (1)"), "{text}");
    for id in ["F2", "F3", "F4", "F5"] {
        assert!(text.contains(&format!("also reported as **{id}**")), "{id}");
    }
    assert!(text.contains("1 confirmed"), "{}", report.headline());
}
