use super::super::{
    prompt::{self, PROMPT_LIMIT, Subject},
    protocol::{Severity, Status, Vote, cap, status},
};
use super::finding;
use std::path::Path;

#[test]
fn partial_votes_confirm_a_finding_one_severity_lower() {
    use Vote::{Partial, Refuted, Upheld};
    let mut inflated = finding(
        "F1",
        0,
        Severity::Critical,
        "safety",
        &[(1, 2, Partial), (2, 2, Partial)],
    );
    inflated.status = status(&inflated, 3);
    assert_eq!(inflated.status, Status::Confirmed);
    assert_eq!(
        cap(&[inflated]),
        7,
        "a critical only partially upheld counts as major"
    );
    let mut agreed = finding(
        "F2",
        0,
        Severity::Critical,
        "safety",
        &[(1, 2, Upheld), (2, 2, Partial)],
    );
    agreed.status = status(&agreed, 3);
    assert_eq!(cap(&[agreed]), 4, "one full upheld vote keeps the severity");
    let mut label = finding(
        "F3",
        0,
        Severity::Minor,
        "test_integrity",
        &[(1, 2, Partial), (2, 2, Refuted), (3, 2, Partial)],
    );
    label.status = status(&label, 4);
    assert_eq!(label.status, Status::Confirmed);
    assert_eq!(
        cap(&[label]),
        10,
        "a partially upheld minor test-integrity finding counts as a nit"
    );
    let mut weakening = finding(
        "F5",
        0,
        Severity::Critical,
        "test_integrity",
        &[(1, 2, Partial), (2, 2, Partial)],
    );
    weakening.status = status(&weakening, 3);
    assert_eq!(
        cap(&[weakening]),
        4,
        "a critical test weakening, partially upheld, is still a major one"
    );
    let mut major = finding("F4", 0, Severity::Major, "spec", &[(1, 2, Partial)]);
    major.status = status(&major, 2);
    assert_eq!(
        cap(&[major]),
        10,
        "a partially upheld major counts as minor"
    );
}

#[test]
fn prompts_stay_under_the_argument_limit_for_any_diff() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let subject = Subject {
        head: "a".repeat(40),
        merge_base: "b".repeat(40),
        task: "task".into(),
        stat: "stat".into(),
        diff: "+ changed line é\n".repeat(40_000),
        facts: "facts".into(),
    };
    let first = prompt::first_round(&root, "correctness", &subject).unwrap();
    assert!(first.len() <= PROMPT_LIMIT, "{}", first.len());
    assert!(first.contains("diff cut here"));
    let mut findings: Vec<_> = (0..40)
        .map(|i| {
            let mut f = finding(&format!("F{i}"), 1, Severity::Major, "spec", &[]);
            f.reported.claim = "long claim ".repeat(1_000);
            f.reported.evidence = "evidence ".repeat(1_000);
            f
        })
        .collect();
    findings[0].reporter = 0;
    let cross = prompt::cross_round(
        &root,
        "spec",
        &subject,
        &findings,
        0,
        prompt::Round {
            number: 2,
            last: false,
        },
    )
    .unwrap();
    assert!(cross.len() <= PROMPT_LIMIT, "{}", cross.len());
    let grade = prompt::grading(&root, &subject, &findings).unwrap();
    assert!(grade.len() <= PROMPT_LIMIT, "{}", grade.len());
}

#[test]
fn only_a_clean_session_on_stdout_answers() {
    use super::super::runner::answer;
    use crate::process::CaptureExit;
    let reply = "AOE-REVIEW-BEGIN\n{\"findings\": []}\nAOE-REVIEW-END";
    assert_eq!(
        answer(reply, "", &CaptureExit::Success).as_deref(),
        Ok("{\"findings\": []}")
    );
    assert!(answer(reply, "", &CaptureExit::Failed(Some(1))).is_err());
    assert!(answer(reply, "", &CaptureExit::Deadline).is_err());
    let echoed_on_stderr = answer("thinking", reply, &CaptureExit::Success).unwrap_err();
    assert!(
        echoed_on_stderr.contains("no AOE-REVIEW answer"),
        "{echoed_on_stderr}"
    );
}
