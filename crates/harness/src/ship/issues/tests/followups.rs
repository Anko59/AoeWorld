use super::super::test_stub::Stub;
use super::*;
use crate::review::protocol::Reported;

fn report(findings: Vec<Finding>) -> Report {
    Report {
        version: 1,
        head: "0123456789abcdef0123456789abcdef01234567".into(),
        branch: "feature".into(),
        base: String::new(),
        merge_base: String::new(),
        tier: "low".into(),
        floor: "low".into(),
        runtime: "claude".into(),
        model: "scripted".into(),
        effort: "medium".into(),
        personas: vec!["quick".into()],
        rounds: 1,
        findings,
        written_grade: 9,
        grade: 9,
        summary: "scripted".into(),
        failures: vec![],
        merge_grade: 8,
        started: 0,
        finished: 0,
        closing: false,
        change_fingerprint: None,
        policy_fingerprint: None,
        reused_from: None,
        increment: None,
        task_fingerprint: None,
        note: None,
        decision: None,
        base_branch: "dev".into(),
    }
}

#[test]
fn reviewer_text_is_safe_and_findings_keep_severity_and_location() {
    let finding = Finding {
        id: "one".into(),
        reporter: 0,
        round: 1,
        reported: Reported {
            file: "crates/simulation/a.rs".into(),
            line: Some(12),
            severity: Severity::Major,
            category: "correctness".into(),
            claim: "bad\n<!-- injection\u{202e}".into(),
            trigger: String::new(),
            expected_vs_actual: String::new(),
            evidence: String::new(),
        },
        votes: vec![],
        status: Status::Disputed,
        members: vec![],
    };
    let line = item(&finding);
    assert!(line.contains("crates/simulation/a.rs:12"));
    assert!(line.contains("major, disputed"));
    assert!(!line.contains("<!--"));
    assert!(!line.contains("bad\n"));
    assert!(!line.contains('\u{202e}'));
    assert_eq!(pr_number("https://github.com/a/b/pull/42"), Some(42));
    assert_eq!(pr_number("https://github.com/a/b/issues/42"), None);
}

#[test]
fn ship_routes_findings_by_path_and_never_creates() {
    let stub = Stub::new(&serde_json::json!([]));
    let finding = |path: &str, status| Finding {
        id: path.into(),
        reporter: 0,
        round: 1,
        reported: Reported {
            file: path.into(),
            line: Some(4),
            severity: Severity::Major,
            category: "correctness".into(),
            claim: "broken".into(),
            trigger: String::new(),
            expected_vs_actual: String::new(),
            evidence: String::new(),
        },
        votes: vec![],
        status,
        members: vec![],
    };
    let review = report(vec![
        finding("crates/server/src/lib.rs", Status::Confirmed),
        finding("docs/issues.md", Status::Disputed),
        finding("crates/client/src/lib.rs", Status::Refuted),
    ]);
    file(
        &stub.root,
        "https://github.com/project/checkout/pull/42",
        &review,
    )
    .unwrap();
    let calls = stub.calls();
    assert!(calls.contains("<issue>\n<edit>\n<575>"), "{calls}");
    assert!(calls.contains("<issue>\n<edit>\n<576>"), "{calls}");
    assert!(!calls.contains("<create>"));
    assert!(!calls.contains("<close>"));
    assert!(!calls.contains("<--jq>"));
    assert!(calls.contains("<--repo>\n<github.com/project/checkout>"));
    assert!(!calls.contains("wrong/override"));
    let bodies = stub.bodies();
    let (game, other) = bodies.split_once("===").unwrap();
    assert!(
        game.find("#### Game findings").unwrap() < game.find("crates/server/src/lib.rs").unwrap()
    );
    assert!(!game.contains("docs/issues.md"));
    assert!(other.contains("docs/issues.md"));
    assert!(!bodies.contains("crates/client/src/lib.rs"));
}

#[test]
fn matching_findings_keep_the_highest_severity_and_untrusted_details_are_sanitised() {
    let stub = Stub::new(&serde_json::json!([]));
    let finding = |severity, claim: &str| Finding {
        id: format!("{severity:?}"),
        reporter: 0,
        round: 1,
        reported: Reported {
            file: "crates/server/a.rs".into(),
            line: Some(8),
            severity,
            category: "correctness".into(),
            claim: claim.into(),
            trigger: "<tag>\ntrigger".into(),
            expected_vs_actual: "expected".into(),
            evidence: "proof".into(),
        },
        votes: vec![],
        status: Status::Confirmed,
        members: vec![],
    };
    let review = report(vec![
        finding(Severity::Minor, "Broken!"),
        finding(Severity::Critical, "broken"),
    ]);
    file(
        &stub.root,
        "https://github.com/project/checkout/pull/42",
        &review,
    )
    .unwrap();
    let body = stub.bodies();
    assert_eq!(body.matches("crates/server/a.rs:8").count(), 1);
    assert!(body.contains("critical, confirmed"));
    assert!(body.contains("&lt;tag> trigger"));
    assert!(body.contains("Expected vs actual: expected"));
    assert!(body.contains("Evidence: proof"));
}

#[test]
fn ship_fails_loudly_for_a_closed_destination() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set(
        "GH_VIEW_JSON",
        r#"{"state":"CLOSED","body":"","updatedAt":"t"}"#,
    );
    let error = file(
        &stub.root,
        "https://github.com/project/checkout/pull/42",
        &report(vec![]),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("gates/issues.json") && error.contains("575"),
        "{error}"
    );
    assert!(!stub.calls().contains("<edit>"));
    assert!(!stub.calls().contains("<create>"));
}
