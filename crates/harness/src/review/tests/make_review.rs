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

#[test]
fn explaining_reuse_does_not_store_an_approval_or_launch_a_reviewer() {
    use super::super::{Report, Reuse};
    let temp = fixture("README.md");
    let root = temp.path();
    let head = crate::ship::git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let decision =
        Report::inspect_reuse(root, &head, "feature", &["medium"], "task", None).unwrap();
    let Reuse::Declined(why) = decision else {
        panic!("no report exists");
    };
    assert_eq!(why, "no passing review of this branch");
    assert!(!root.join(".git/aoe-ship/reviews").exists());
    let report = review_with(
        root,
        Tier::Medium,
        Runtime::Claude,
        "task",
        &super::super::Plan::Full,
        &Quiet,
    )
    .unwrap();
    let directory = root.join(".git/aoe-ship/reviews");
    let before: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let decision =
        Report::inspect_reuse(root, &head, "feature", &["medium"], "task", None).unwrap();
    let Reuse::Reused(reused) = decision else {
        panic!("the report passed");
    };
    assert_eq!(reused.reused_from.as_deref(), Some(report.head.as_str()));
    let after: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(before, after, "a diagnostic never saves an approval");
}
