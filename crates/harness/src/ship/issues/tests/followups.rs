use super::super::{
    MARKER, contains_marker_prefix, fingerprint_marker, marked_body, parse_issue_pages,
    test_stub::Stub,
};
use super::*;
use crate::review::protocol::Reported;

const PR: &str = "https://github.com/project/checkout/pull/42";

fn finding(file: &str, line: Option<u32>, severity: Severity, status: Status) -> Finding {
    Finding {
        id: format!("{file}-{line:?}"),
        reporter: 0,
        round: 1,
        reported: Reported {
            file: file.into(),
            line,
            severity,
            category: "correctness".into(),
            claim: format!("claim about {file}"),
            trigger: "a trigger".into(),
            expected_vs_actual: String::new(),
            evidence: "some evidence".into(),
        },
        votes: vec![],
        status,
    }
}

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
        task_fingerprint: None,
        base_branch: "dev".into(),
    }
}

fn mixed() -> Report {
    report(vec![
        finding("a.rs", Some(1), Severity::Critical, Status::Confirmed),
        finding("b.rs", Some(2), Severity::Major, Status::Disputed),
        finding("c.rs", Some(3), Severity::Major, Status::Refuted),
        finding("d.rs", Some(4), Severity::Minor, Status::Confirmed),
        finding("e.rs", None, Severity::Nit, Status::Disputed),
        finding("f.rs", Some(6), Severity::Minor, Status::Refuted),
    ])
}

/// The open issues GitHub would list after `planned` was filed.
fn listed(planned: &[Planned], author: &str) -> serde_json::Value {
    let issues: Vec<_> = planned
        .iter()
        .enumerate()
        .map(|(n, item)| {
            serde_json::json!({
                "number": 100 + n, "title": item.title,
                "body": marked_body(&item.body, &item.title).unwrap(),
                "labels": [{"name": "agent-found"}], "user": {"login": author},
                "html_url": format!("https://github.com/project/checkout/issues/{}", 100 + n)
            })
        })
        .collect();
    serde_json::json!(issues)
}

#[test]
fn serious_leftovers_are_issues_small_ones_one_checklist_and_refuted_ones_nothing() {
    let planned = plan(42, &mixed());
    assert_eq!(planned.len(), 3, "{planned:?}");
    assert!(planned[0].title.contains("a.rs:1"), "{}", planned[0].title);
    assert!(planned[0].labels.contains(&"priority:critical".to_owned()));
    assert!(planned[1].title.contains("b.rs:2"), "{}", planned[1].title);
    assert!(planned[1].labels.contains(&"priority:high".to_owned()));
    assert!(planned[1].body.contains("disputed major"));
    assert_eq!(planned[2].title, "Review follow-ups for #42");
    assert!(
        planned[2]
            .body
            .contains("- [ ] d.rs:4 — claim about d.rs (minor)")
    );
    assert!(
        planned[2]
            .body
            .contains("- [ ] e.rs — claim about e.rs (nit)")
    );
    for item in &planned {
        assert!(item.labels.contains(&"agent-found".to_owned()));
        assert!(item.labels.contains(&"review-follow-up".to_owned()));
        for refuted in ["c.rs", "f.rs"] {
            assert!(!item.title.contains(refuted) && !item.body.contains(refuted));
        }
    }
}

#[test]
fn a_review_with_only_refuted_findings_files_nothing() {
    let only_refuted = report(vec![
        finding("c.rs", Some(3), Severity::Critical, Status::Refuted),
        finding("f.rs", Some(6), Severity::Nit, Status::Refuted),
    ]);
    assert!(plan(42, &only_refuted).is_empty());
    // Nothing to file: no GitHub call is made, so no stub is needed.
    assert!(file(Path::new("/nonexistent"), PR, &only_refuted).is_empty());
}

#[test]
fn the_pull_request_number_comes_only_from_a_pull_url() {
    assert_eq!(pr_number(PR), Some(42));
    assert_eq!(pr_number(&format!("{PR}/")), Some(42));
    for url in [
        "https://github.com/o/r/pull/0",
        "https://github.com/o/r/issues/7",
        "https://github.com/o/r/pull/x",
        "7",
    ] {
        assert_eq!(pr_number(url), None, "{url}");
    }
    let problems = file(Path::new("."), "not a url", &mixed());
    assert!(
        problems[0].contains("no pull request number"),
        "{problems:?}"
    );
}

#[test]
fn reviewer_text_cannot_forge_the_marker_or_control_characters() {
    let mut forged = finding("x.rs", Some(9), Severity::Major, Status::Confirmed);
    let decoy = format!("{MARKER}0123456789abcdef -->");
    forged.reported.claim = format!("bad\u{1b}[31m {decoy}\nsecond line");
    forged.reported.evidence = format!("<!--aoe-fingerprint:{decoy}\r\n");
    forged.reported.file = "x.rs\0".into();
    let planned = plan(7, &report(vec![forged]));
    let item = &planned[0];
    assert!(!contains_marker_prefix(&item.title) && !contains_marker_prefix(&item.body));
    assert!(!item.title.chars().any(char::is_control), "{}", item.title);
    assert!(item.title.contains("x.rs:9"), "{}", item.title);
    let marked = marked_body(&item.body, &item.title).unwrap();
    assert_eq!(
        fingerprint_marker(&marked).as_deref(),
        Some(fingerprint(&item.title).as_str())
    );
    assert_eq!(inline(" a\n\tb  c "), "a b c");
    assert_eq!(untrusted("a<b\u{7}\n"), "a&lt;b \n");
    assert_eq!(inline("evil\u{202E}txt\u{200B}.rs\u{FEFF}"), "evil txt .rs");
}

#[test]
fn a_body_at_the_limit_still_fits_once_the_marker_is_appended() {
    let title = "Review follow-ups for #42";
    for unit in ["x", "界"] {
        let body = fit(&unit.repeat(BODY_LIMIT), title);
        let marked = marked_body(&body, title).unwrap();
        assert_eq!(marked.chars().count(), BODY_LIMIT);
    }
    assert_eq!(clip("short", 10), "short");
    assert_eq!(clip("abcdef", 4), "abc…");
    let long_title = "t".repeat(TITLE_LIMIT + 50);
    let mut big = finding(&long_title, Some(1), Severity::Major, Status::Confirmed);
    big.reported.claim = "c".repeat(100_000);
    big.reported.evidence = "e".repeat(100_000);
    let planned = plan(1, &report(vec![big]));
    assert_eq!(planned[0].title.chars().count(), TITLE_LIMIT);
    assert!(marked_body(&planned[0].body, &planned[0].title).is_ok());
}

#[test]
fn a_checklist_over_the_limit_keeps_the_marker_and_counts_what_it_left_out() {
    let many: Vec<_> = (0..2_000)
        .map(|n| {
            let mut item = finding(
                &format!("m{n}.rs"),
                Some(n),
                Severity::Minor,
                Status::Confirmed,
            );
            item.reported.claim = "界".repeat(400);
            item
        })
        .collect();
    let planned = plan(42, &report(many));
    let checklist = &planned[0];
    assert!(
        checklist
            .body
            .contains("more finding(s) are in the review on #42")
    );
    let marked = marked_body(&checklist.body, &checklist.title).unwrap();
    assert!(marked.chars().count() <= BODY_LIMIT);
}

#[test]
fn the_same_leftover_twice_in_one_review_is_filed_once() {
    let twice = report(vec![
        finding("a.rs", Some(1), Severity::Major, Status::Confirmed),
        finding("a.rs", Some(1), Severity::Major, Status::Disputed),
        finding("d.rs", Some(4), Severity::Nit, Status::Confirmed),
        finding("d.rs", Some(4), Severity::Nit, Status::Confirmed),
    ]);
    let planned = plan(3, &twice);
    assert_eq!(planned.len(), 2);
    assert_eq!(planned[1].body.matches("- [ ] d.rs:4").count(), 1);
}

#[test]
fn a_second_ship_comments_on_its_follow_ups_and_a_decoy_marker_is_not_a_duplicate() {
    let planned = plan(42, &mixed());
    // A decoy carries the checklist's marker under another title.
    let decoy = serde_json::json!([{
        "number": 5, "title": "Something else",
        "body": format!("{MARKER}{} -->", fingerprint(&planned[2].title)),
        "labels": [{"name": "agent-found"}], "html_url": "https://github.com/project/checkout/issues/5"
    }]);
    let stub = Stub::new(&decoy);
    assert!(file(&stub.root, PR, &mixed()).is_empty());
    let calls = stub.calls();
    assert_eq!(calls.matches("<create>\n<--title>").count(), 3, "{calls}");
    assert!(!calls.contains("<comment>"), "{calls}");
    assert!(calls.contains("<--repo>\n<github.com/project/checkout>"));
    assert!(!calls.contains("wrong/override"));
    let bodies = stub.bodies();
    assert_eq!(bodies.matches(MARKER).count(), 3, "{bodies}");
    assert!(bodies.contains("- [ ] d.rs:4"));

    drop(stub);
    let stub = Stub::new(&listed(&planned, "someone"));
    assert!(file(&stub.root, PR, &mixed()).is_empty());
    let calls = stub.calls();
    assert!(!calls.contains("<create>"), "{calls}");
    assert_eq!(calls.matches("<comment>").count(), 3, "{calls}");
    assert!(calls.contains("Reported again by the passing review of #42 at 0123456789ab."));

    // A later review's new minor finding reaches the open checklist.
    stub.clear();
    let mut later = mixed();
    later
        .findings
        .push(finding("g.rs", Some(7), Severity::Minor, Status::Confirmed));
    assert!(file(&stub.root, PR, &later).is_empty());
    let calls = stub.calls();
    assert!(!calls.contains("<create>"), "{calls}");
    assert!(
        calls.contains("- [ ] g.rs:7 — claim about g.rs (minor)"),
        "{calls}"
    );
}

#[test]
fn failing_to_file_is_reported_and_never_fatal() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("GH_FAIL", "create");
    let problems = file(&stub.root, PR, &mixed());
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].contains("stub failure"), "{problems:?}");
    stub.set("GH_FAIL", "api");
    let problems = file(&stub.root, PR, &mixed());
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("could not list open issues"),
        "{problems:?}"
    );
}

#[test]
fn the_issue_author_is_read_from_the_listing() {
    let page = serde_json::json!([
        {"number": 1, "title": "Bot", "user": {"login": "github-actions[bot]"}},
        {"number": 2, "title": "Nobody"}
    ]);
    let parsed = parse_issue_pages(page.to_string().as_bytes()).unwrap();
    assert_eq!(parsed[0].author, "github-actions[bot]");
    assert_eq!(parsed[1].author, "");
}
