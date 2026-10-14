//! `make review` on the default base announces and records the `review:` line
//! like `make ship` does, before any reviewer starts.
use super::super::{Ask, Tier, entry::review_current, review_with};
use super::flow::fixture;
use crate::agents::Runtime;
use std::cell::RefCell;

struct Quiet;

impl Ask for Quiet {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        prompts
            .iter()
            .map(|p| {
                Ok(if p.contains("# Grader") {
                    r#"{"grade": 9, "summary": "Fine.\nNothing else."}"#.into()
                } else {
                    r#"{"verdicts": [], "findings": []}"#.into()
                })
            })
            .collect()
    }
}

#[test]
fn make_review_announces_and_records_the_review_line() {
    let temp = fixture("README.md");
    let before_reviewer = RefCell::new(None);
    let report = review_current(
        temp.path(),
        Tier::Medium,
        Runtime::Claude,
        "task",
        &|root, tier, runtime, task, plan| {
            let line = crate::review::take_announced();
            if let Some(line) = &line {
                crate::review::announce(line);
            }
            *before_reviewer.borrow_mut() = Some(line);
            review_with(root, tier, runtime, task, plan, &Quiet)
        },
    )
    .unwrap();
    let line = "review: full review — requested with make review";
    assert_eq!(
        before_reviewer.into_inner().unwrap().as_deref(),
        Some(line),
        "announced before any reviewer starts"
    );
    assert_eq!(report.decision.as_deref(), Some(line));
    assert!(report.markdown().contains(line));
    assert_eq!(
        crate::review::take_announced(),
        None,
        "nothing is left over"
    );
}
