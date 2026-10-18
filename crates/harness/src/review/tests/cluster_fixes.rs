//! Review fixes for clustering: numbering after a merge, carried members, and a
//! defect every reviewer reported.
use super::super::{
    closing::{CARRIED, Next, Plan, next},
    next_number,
    protocol::{Member, Severity, Status, status},
};
use super::{closing::report, finding};

fn member(id: &str, reporter: usize) -> Member {
    let base = finding(id, reporter, Severity::Major, "correctness", &[]);
    Member {
        id: base.id,
        reporter,
        round: base.round,
        reported: base.reported,
    }
}

#[test]
fn a_finding_added_after_a_merge_takes_a_free_number() {
    // F2 and F3 were merged into F1, so only two findings remain in the list.
    let mut clustered = finding("F1", 0, Severity::Major, "correctness", &[]);
    clustered.members = vec![member("F2", 1), member("F3", 2)];
    let findings = vec![clustered, finding("F4", 3, Severity::Minor, "spec", &[])];
    assert_eq!(
        next_number(&findings),
        5,
        "neither the list length nor a member's id"
    );
    assert_eq!(next_number(&[]), 1);
}

#[test]
fn a_defect_every_reviewer_reported_is_confirmed_without_votes() {
    let mut unanimous = finding("F1", 0, Severity::Major, "correctness", &[]);
    unanimous.members = vec![member("F2", 1), member("F3", 2)];
    assert_eq!(status(&unanimous, 3), Status::Confirmed);
    let alone = finding("F4", 0, Severity::Major, "correctness", &[]);
    assert_eq!(
        status(&alone, 3),
        Status::Disputed,
        "one reporter and no votes is still unsettled"
    );
}

#[test]
fn a_carried_cluster_lets_every_closing_reviewer_vote() {
    let mut failed = report("c", 1, 7, &[Severity::Major]);
    failed.findings[0].members = vec![member("F2", 1)];
    let Next::Review(Plan::Closing { prior, .. }) = next(&[failed], "d", 1_000_000) else {
        panic!("a closing review");
    };
    assert_eq!(prior[0].reporter, CARRIED);
    assert!(prior[0].members.iter().all(|m| m.reporter == CARRIED));
    assert!(
        !prior[0].reported_by(1),
        "reviewer 1 of the closing review did not report it"
    );
}
