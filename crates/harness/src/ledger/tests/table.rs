use super::{CLOSING, FULL, ONCE, pr};
use crate::ledger::table::*;
use std::collections::BTreeSet;

fn fixture() -> Vec<crate::ledger::record::Pr> {
    vec![
        pr(1, FULL, &[]),    // Haiku + Opus, 2 rounds
        pr(2, ONCE, &[]),    // GPT, 9/10 once
        pr(3, CLOSING, &[]), // Sonnet, approved 8 then closing
        pr(4, "No review here.\n- Written by Claude Haiku 5.5.", &[]),
        pr(5, "Bare.", &[]), // unattributed
    ]
}

#[test]
fn rows_aggregate_per_implementer_model() {
    let prs = fixture();
    let refs: Vec<_> = prs.iter().collect();
    let rows = by_implementer(&refs, &BTreeSet::from([2]));
    let haiku = &rows["Claude Haiku 5.5"];
    assert_eq!(
        (haiku.prs, haiku.reviewed, haiku.graded, haiku.first_pass),
        (2, 1, 0, 0)
    );
    assert_eq!(rows["Claude Opus 5.5"].prs, 1);
    let gpt = &rows["GPT-6.1 Sol"];
    assert_eq!(
        (
            gpt.prs,
            gpt.grade_sum,
            gpt.first_pass,
            gpt.confirmed_sum,
            gpt.reverted
        ),
        (1, 9, 1, 1, 1)
    );
    let sonnet = &rows["Claude Sonnet 5.5"];
    assert_eq!(
        (
            sonnet.grade_sum,
            sonnet.first_pass,
            sonnet.closing,
            sonnet.counted
        ),
        (8, 1, 1, 0)
    );
    assert_eq!(rows["(unattributed)"].prs, 1);
}

#[test]
fn rows_aggregate_per_reviewer_model() {
    let prs = fixture();
    let refs: Vec<_> = prs.iter().collect();
    let rows = by_reviewer(&refs, &BTreeSet::new());
    assert_eq!(rows["Claude Sonnet 5.5"].prs, 1);
    assert_eq!(rows["Claude Opus 5.5"].prs, 1);
    assert_eq!(rows["GPT-6.1 Sol"].prs, 1);
}

#[test]
fn means_that_have_no_data_say_unknown() {
    let prs = fixture();
    let refs: Vec<_> = prs.iter().collect();
    let text = render("T", "implementer", &by_implementer(&refs, &BTreeSet::new()));
    let haiku = text
        .lines()
        .find(|l| l.starts_with("Claude Haiku 5.5"))
        .unwrap();
    assert!(haiku.contains("unknown"), "{haiku}");
    let gpt = text.lines().find(|l| l.starts_with("GPT-6.1 Sol")).unwrap();
    assert!(gpt.contains("9.0") && gpt.contains("100%"), "{gpt}");
    assert!(render("T", "x", &Default::default()).contains("(no pull requests)"));
}
