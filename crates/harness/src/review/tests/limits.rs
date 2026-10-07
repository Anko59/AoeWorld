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

#[test]
fn a_huge_task_or_file_list_is_cut_not_fatal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let subject = Subject {
        head: "a".repeat(40),
        merge_base: "b".repeat(40),
        task: "task line\n".repeat(30_000),
        stat: " assets/a.png | Bin\n".repeat(20_000),
        diff: "+x\n".into(),
        facts: "facts".into(),
    };
    let prompt = prompt::first_round(&root, "correctness", &subject).unwrap();
    assert!(prompt.len() <= PROMPT_LIMIT, "{}", prompt.len());
    assert!(prompt.contains("[cut here]"));
}

#[test]
fn a_disputed_finding_still_counts_one_severity_lower() {
    use Vote::{Refuted, Upheld};
    let mut split = finding(
        "F1",
        0,
        Severity::Critical,
        "safety",
        &[(1, 2, Upheld), (2, 2, Refuted)],
    );
    split.status = status(&split, 3);
    assert_eq!(split.status, Status::Disputed);
    assert_eq!(
        cap(&[split]),
        7,
        "one dissenter cannot erase a critical finding"
    );
    let mut refuted = finding("F2", 0, Severity::Critical, "safety", &[(1, 2, Refuted)]);
    refuted.status = status(&refuted, 2);
    assert_eq!(cap(&[refuted]), 10);
}

#[test]
fn each_runtime_launches_its_reviewer_read_only_with_the_tier_model() {
    use super::super::{config::Model, runner::command};
    use crate::agents::Runtime;
    // Codex reads the repository's hook file; dsh writes under .cache.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let model = |name: &str, effort: &str| Model {
        model: name.into(),
        effort: effort.into(),
    };
    let args = |runtime, m: &Model| -> Vec<String> {
        let (command, _) = command(runtime, &root, m, "PROMPT").unwrap();
        assert_eq!(
            command
                .get_envs()
                .find(|(k, _)| *k == "AOE_AGENT_ROLE")
                .and_then(|(_, v)| v),
            Some(std::ffi::OsStr::new("reviewer"))
        );
        command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    };
    let claude = args(Runtime::Claude, &model("claude-sonnet-5-5", "high")).join(" ");
    assert!(
        claude.contains("--model claude-sonnet-5-5 --effort high"),
        "{claude}"
    );
    assert!(
        claude.contains("--allowedTools=Read,Grep,Glob,Bash"),
        "{claude}"
    );
    assert!(claude.ends_with("PROMPT"));
    let codex = args(Runtime::Codex, &model("gpt-6-luna", "xhigh")).join(" ");
    assert!(codex.contains("-s read-only"), "{codex}");
    assert!(
        codex.contains("-m gpt-6-luna -c model_reasoning_effort=\"xhigh\""),
        "{codex}"
    );
    let pi = args(Runtime::Pi, &model("litellm/glm-5.3", "high")).join(" ");
    assert!(pi.contains("--model litellm/glm-5.3:high"), "{pi}");
    assert!(pi.contains("--tools read,grep,find,ls,bash"), "{pi}");
    let (_, patch) = command(Runtime::Dsh, &root, &model("litellm/glm-5.3", "high"), "P").unwrap();
    let body = std::fs::read_to_string(patch.expect("a dsh patch")).unwrap();
    assert_eq!(body.matches("agent-default-model").count(), 1, "{body}");
    assert!(
        body.contains("\"litellm\"") && body.contains("\"glm-5.3\"") && body.contains("\"high\""),
        "{body}"
    );
    assert!(command(Runtime::Dsh, &root, &model("no-provider", "high"), "P").is_err());
}
