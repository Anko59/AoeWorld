use super::super::{
    increment::{
        Increment, Scope,
        history::{Step, slice},
    },
    protocol::Severity,
};
use super::closing::report;

#[test]
fn a_report_counts_once_towards_the_cumulative_rule() {
    let increment = |closing| Increment {
        approved: "a".into(),
        grade: 9,
        tier: "high".into(),
        scope: Scope::Since("a".into()),
        lines: 80,
        closing,
        outside: Vec::new(),
    };
    let mut passed = report("h", 1, 9, &[]);
    passed.increment = Some(increment(true));
    assert_eq!(slice(&passed), Step::Small(80));
    // A reused copy describes the same approval and adds nothing.
    let mut copy = passed.clone();
    copy.reused_from = Some("h".into());
    assert_eq!(slice(&copy), Step::Other);
    let mut failed = report("h", 1, 5, &[Severity::Major]);
    failed.increment = Some(increment(true));
    assert_eq!(slice(&failed), Step::FailedSmall(80));
    failed.increment = Some(increment(false));
    assert_eq!(slice(&failed), Step::FailedFull);
    // A closing review of fixes (no increment report) settles what failed.
    let mut fixes = report("h", 2, 9, &[]);
    fixes.closing = true;
    assert_eq!(slice(&fixes), Step::Fixes);
    assert_eq!(slice(&report("h", 2, 9, &[])), Step::Full);
}
