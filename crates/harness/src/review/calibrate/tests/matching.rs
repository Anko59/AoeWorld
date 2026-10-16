use super::{Outcome, Summary, repo, repo_root, reviewer, shipped};
use crate::review::{Report, protocol::Severity};

fn queue_report() -> (super::Case, Report) {
    let case = shipped()
        .into_iter()
        .find(|c| c.id == "hard-queue-limit-and-counter")
        .unwrap();
    let dir = repo::materialise(&repo_root(), &case).unwrap();
    let mut report = reviewer("major", "none")(dir.path(), &case).unwrap();
    assert_eq!(report.personas.len(), 1);
    assert_eq!(report.findings.len(), 2);
    assert_eq!(report.findings[0].reported.line, Some(39));
    assert_eq!(report.findings[1].reported.line, Some(38));
    report.findings[1].reported.severity = Severity::Minor;
    (case, report)
}

#[test]
fn one_persona_reports_both_queue_defects() {
    let (case, report) = queue_report();
    let outcome = Outcome::from_report(&case, &report);
    assert_eq!((outcome.defects, outcome.found), (2, 2));
    let summary = Summary::new("low", "claude", "m", 8, vec![outcome]);
    assert_eq!(summary.areas[0].recall, Some(1.0));
    assert_eq!(summary.personas[0].found, 2);
    assert!(
        summary
            .markdown()
            .contains(&format!("| {} | 2 | 0 |", report.personas[0]))
    );
    let mut partial = report.clone();
    partial.findings.pop();
    let outcome = Outcome::from_report(&case, &partial);
    assert_eq!(outcome.found, 1);
    let summary = Summary::new("low", "claude", "m", 8, vec![outcome]);
    assert_eq!(summary.personas[0].found, 1);
}

#[test]
fn overlapping_defects_need_distinct_findings_and_can_reassign_a_match() {
    let (mut case, mut report) = queue_report();
    let super::seeds::Expect::Defects(defects) = &mut case.expect else {
        panic!("queue case must plant defects");
    };
    defects[0].severity = Severity::Minor;
    defects[0].lines = [38, 39];
    defects[1].severity = Severity::Major;
    defects[1].lines = [39, 39];
    let second = report.findings.pop().unwrap();
    // Line 39 matches both defects, but is only one finding.
    let outcome = Outcome::from_report(&case, &report);
    assert_eq!(outcome.found, 1);
    // The minor finding at 38 frees the major one at 39 for the second defect.
    report.findings.push(second);
    assert_eq!(Outcome::from_report(&case, &report).found, 2);
    report.findings.reverse();
    assert_eq!(Outcome::from_report(&case, &report).found, 2);
}

#[test]
fn duplicate_reports_and_other_personas_count_each_defect_once_per_persona() {
    let (case, mut report) = queue_report();
    report.findings.push(report.findings[0].clone());
    let outcome = Outcome::from_report(&case, &report);
    let summary = Summary::new("low", "claude", "m", 8, vec![outcome]);
    assert_eq!(summary.personas[0].found, 2);
    report.personas.push("spec".into());
    let mut other = report.findings[0].clone();
    other.reporter = 1;
    report.findings.push(other);
    let outcome = Outcome::from_report(&case, &report);
    assert_eq!(outcome.found, 2);
    let summary = Summary::new("low", "claude", "m", 8, vec![outcome]);
    assert_eq!(summary.personas[0].found, 2);
    assert_eq!(summary.personas[1].found, 1);
}

#[test]
fn only_standing_findings_on_the_right_file_lines_and_severity_match() {
    let (case, report) = queue_report();
    for fault in ["file", "line", "severity", "refuted"] {
        let mut report = report.clone();
        for f in &mut report.findings {
            match fault {
                "file" => f.reported.file = "src/other.rs".into(),
                "line" => f.reported.line = Some(1),
                "severity" => f.reported.severity = Severity::Nit,
                "refuted" => f.status = crate::review::protocol::Status::Refuted,
                _ => unreachable!(),
            }
        }
        assert_eq!(Outcome::from_report(&case, &report).found, 0, "{fault}");
    }
}

#[test]
fn several_clean_findings_from_one_persona_are_one_false_alarm() {
    let (mut case, mut report) = queue_report();
    for finding in &mut report.findings {
        finding.reported.severity = Severity::Major;
    }
    case.area = super::CLEAN.into();
    case.expect = super::seeds::Expect::Clean(true);
    let outcome = Outcome::from_report(&case, &report);
    assert_eq!((outcome.defects, outcome.found), (0, 0));
    let summary = Summary::new("low", "claude", "m", 8, vec![outcome]);
    assert_eq!(summary.personas[0].found, 0);
    assert_eq!(summary.personas[0].false_alarms, 1);
}
