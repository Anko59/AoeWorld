use super::{
    config::{Config, Tier, bump},
    protocol::{Cast, Finding, Reported, Severity, Status, Vote, cap, converged, extract, status},
    report::Report,
};
use crate::agents::Runtime;
use std::collections::BTreeSet;

fn finding(
    id: &str,
    reporter: usize,
    severity: Severity,
    category: &str,
    votes: &[(usize, u32, Vote)],
) -> Finding {
    Finding {
        id: id.into(),
        reporter,
        round: 1,
        reported: Reported {
            file: "crates/x.rs".into(),
            line: Some(3),
            severity,
            category: category.into(),
            claim: "c".into(),
            trigger: "t".into(),
            expected_vs_actual: "e".into(),
            evidence: "v".into(),
        },
        votes: votes
            .iter()
            .map(|&(reviewer, round, vote)| Cast {
                reviewer,
                round,
                vote,
                evidence: String::new(),
            })
            .collect(),
        status: Status::Disputed,
    }
}

#[test]
fn findings_stand_or_fall_on_the_other_reviewers_latest_votes() {
    use Vote::*;
    // A single reviewer cannot be cross-checked: its findings stand.
    assert_eq!(
        status(&finding("F1", 0, Severity::Major, "", &[]), 1),
        Status::Confirmed
    );
    assert_eq!(
        status(&finding("F1", 0, Severity::Major, "", &[]), 3),
        Status::Disputed
    );
    assert_eq!(
        status(
            &finding(
                "F1",
                0,
                Severity::Major,
                "",
                &[(1, 2, Upheld), (2, 2, Partial)]
            ),
            3
        ),
        Status::Confirmed
    );
    assert_eq!(
        status(
            &finding(
                "F1",
                0,
                Severity::Major,
                "",
                &[(1, 2, Refuted), (2, 2, Unverifiable)]
            ),
            3
        ),
        Status::Refuted
    );
    assert_eq!(
        status(
            &finding(
                "F1",
                0,
                Severity::Major,
                "",
                &[(1, 2, Upheld), (2, 2, Refuted)]
            ),
            3
        ),
        Status::Disputed
    );
    // A later round's vote replaces the same reviewer's earlier one.
    assert_eq!(
        status(
            &finding(
                "F1",
                0,
                Severity::Major,
                "",
                &[(1, 2, Upheld), (1, 3, Refuted)]
            ),
            2
        ),
        Status::Refuted
    );
    // The reporter cannot vote for itself.
    assert_eq!(
        status(&finding("F1", 0, Severity::Major, "", &[(0, 2, Upheld)]), 3),
        Status::Disputed
    );
}

#[test]
fn confirmed_findings_cap_the_grade() {
    let mut findings = vec![
        finding("F1", 0, Severity::Minor, "correctness", &[]),
        finding("F2", 1, Severity::Critical, "safety", &[]),
    ];
    assert_eq!(cap(&findings), 7, "a disputed critical counts as a major");
    findings[1].status = Status::Refuted;
    assert_eq!(cap(&findings), 10, "refuted findings do not cap");
    findings[0].status = Status::Confirmed;
    assert_eq!(cap(&findings), 10);
    findings[1].status = Status::Confirmed;
    assert_eq!(cap(&findings), 4);
    let mut weakened = vec![finding("F3", 0, Severity::Major, "test_integrity", &[])];
    weakened[0].status = Status::Confirmed;
    assert_eq!(
        cap(&weakened),
        4,
        "a confirmed test weakening caps like a critical"
    );
    let mut gap = vec![finding("F5", 0, Severity::Minor, "test_integrity", &[])];
    gap[0].status = Status::Confirmed;
    assert_eq!(cap(&gap), 10, "a minor coverage gap counts by its severity");
    let mut major = vec![finding("F4", 0, Severity::Major, "spec", &[])];
    major[0].status = Status::Confirmed;
    assert_eq!(cap(&major), 7);
}

#[test]
fn a_round_converges_when_nothing_changes() {
    let mut findings = vec![finding("F1", 0, Severity::Major, "", &[])];
    let before = vec![("F1".to_owned(), Status::Disputed)];
    assert!(converged(&before, &findings));
    findings[0].status = Status::Confirmed;
    assert!(!converged(&before, &findings));
    findings[0].status = Status::Disputed;
    findings.push(finding("F2", 1, Severity::Critical, "", &[]));
    assert!(!converged(&before, &findings));
}

#[test]
fn answers_are_found_in_any_runtime_output() {
    let answer = "{\"findings\": []}";
    let plain = format!("thinking...\nAOE-REVIEW-BEGIN\n{answer}\nAOE-REVIEW-END\n");
    assert_eq!(extract(&plain).as_deref(), Some(answer));
    let fenced = format!("AOE-REVIEW-BEGIN\n```json\n{answer}\n```\nAOE-REVIEW-END");
    assert_eq!(extract(&fenced).as_deref(), Some(answer));
    // Claude's JSON result and Codex's JSONL events carry the text escaped.
    let claude = serde_json::json!({"type": "result", "result": plain}).to_string();
    assert_eq!(extract(&claude).as_deref(), Some(answer));
    let codex = format!(
        "{}\n{}\n",
        serde_json::json!({"type": "item.completed", "item": {"type": "agent_message", "text": "AOE-REVIEW-BEGIN {\"findings\": [1]} AOE-REVIEW-END"}}),
        serde_json::json!({"type": "item.completed", "item": {"type": "agent_message", "text": plain}})
    );
    assert_eq!(
        extract(&codex).as_deref(),
        Some(answer),
        "the last answer wins"
    );
    assert_eq!(extract("no markers here"), None);
    // The answer shapes in the prompts are not JSON: an echo is no answer.
    let echoed = "AOE-REVIEW-BEGIN\n{\"grade\": <1-10>, \"summary\": \"<at most two plain lines>\"}\nAOE-REVIEW-END";
    assert!(serde_json::from_str::<serde_json::Value>(&extract(echoed).unwrap()).is_err());
    // A reviewer of this very code may quote the markers inside its JSON.
    let quoting = r#"{"findings": [{"claim": "stops at AOE-REVIEW-END inside a string"}]}"#;
    let text = format!("AOE-REVIEW-BEGIN\n{quoting}\nAOE-REVIEW-END\n");
    assert_eq!(extract(&text).as_deref(), Some(quoting));
    let begun = r#"{"findings": [{"claim": "AOE-REVIEW-BEGIN appears here"}]}"#;
    let text = format!("AOE-REVIEW-BEGIN\n{begun}\nAOE-REVIEW-END\n");
    assert_eq!(extract(&text).as_deref(), Some(begun));
}

#[test]
fn the_committed_config_is_strict_and_sets_floors_and_models() {
    let text = include_str!("../../../../gates/review.json");
    let config = Config::parse(text).expect("committed config");
    let suites = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    assert_eq!(config.floor(&suites(&["static", "docs"])), Tier::Low);
    assert_eq!(
        config.floor(&suites(&["static", "gameplay", "native"])),
        Tier::Medium
    );
    // Map and geodata changes select every product suite but not the harness floor.
    assert_eq!(
        config.floor(&suites(&["map", "static", "native", "gameplay", "browser"])),
        Tier::Medium
    );
    assert_eq!(config.floor(&suites(&["static", "everything"])), Tier::High);
    assert_eq!(
        config.floor(&suites(&["static", "everything", "release"])),
        Tier::High
    );
    assert_eq!(
        config.model_for(Tier::Medium, Runtime::Claude, false).model,
        "claude-sonnet-5-5"
    );
    assert_eq!(
        config.model_for(Tier::Xhigh, Runtime::Claude, false).model,
        "claude-opus-5-5"
    );
    // High uses the average models one reasoning step higher.
    assert_eq!(
        config.model_for(Tier::High, Runtime::Claude, false).effort,
        "high"
    );
    // Codex's highest effort is xhigh: the bump never invents one it refuses.
    assert_eq!(
        config.model_for(Tier::High, Runtime::Codex, false).effort,
        "xhigh"
    );
    assert_eq!(config.tiers[&Tier::Max].personas.len(), 5);
    assert_eq!(bump("claude", "max"), "max");
    assert_eq!(bump("codex", "high"), "xhigh");
    assert_eq!(bump("dsh", "high"), "max");
    let mut wrong: serde_json::Value = serde_json::from_str(text).unwrap();
    wrong["models"]["codex"]["average"]["effort"] = serde_json::json!("max");
    assert!(
        Config::parse(&wrong.to_string()).is_err(),
        "an effort the runtime refuses is refused"
    );
    let mut broken: serde_json::Value = serde_json::from_str(text).unwrap();
    broken["surprise"] = serde_json::json!(true);
    assert!(
        Config::parse(&broken.to_string()).is_err(),
        "unknown fields are refused"
    );
    let mut broken: serde_json::Value = serde_json::from_str(text).unwrap();
    broken["tiers"]["low"]["personas"] = serde_json::json!(["flatterer"]);
    assert!(
        Config::parse(&broken.to_string()).is_err(),
        "unknown personas are refused"
    );
    let mut broken: serde_json::Value = serde_json::from_str(text).unwrap();
    broken["models"].as_object_mut().unwrap().remove("pi");
    assert!(
        Config::parse(&broken.to_string()).is_err(),
        "every runtime needs models"
    );
}

#[test]
fn a_review_passes_only_complete_and_at_the_merge_grade() {
    let mut report = Report {
        version: 1,
        head: "a".repeat(40),
        branch: "feature".into(),
        base: "b".repeat(40),
        merge_base: "c".repeat(40),
        tier: "medium".into(),
        floor: "medium".into(),
        runtime: "claude".into(),
        model: "claude-sonnet-5-5".into(),
        effort: "medium".into(),
        personas: vec!["correctness".into(), "spec".into(), "test-integrity".into()],
        rounds: 2,
        findings: vec![],
        written_grade: 9,
        grade: 9,
        summary: "Adds X.\nWell tested.".into(),
        failures: vec![],
        merge_grade: 8,
        started: 0,
        finished: 0,
        closing: false,
        change_fingerprint: None,
        policy_fingerprint: None,
        reused_from: None,
        task_fingerprint: None,
        base_branch: "dev".into(),
    };
    assert!(report.passes());
    assert!(
        report
            .headline()
            .starts_with("🟢 **Review: 9/10** (medium tier · 3 reviewers · 2 rounds")
    );
    report.grade = 7;
    assert!(!report.passes());
    report.grade = 9;
    report.failures.push("round 1, spec: timeout".into());
    assert!(!report.passes(), "an incomplete review never passes");
    assert!(report.markdown().contains("Incomplete review"));
}

mod closing;
mod flow;
mod limits;
mod pinned;
