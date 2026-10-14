use super::super::{
    BODY_LIMIT, MARKER, contains_marker_prefix, fingerprint_marker, marked_body, parse_issue_pages,
    test_stub::Stub,
};
use super::*;
use crate::review::protocol::Reported;

const PR: &str = "https://github.com/project/checkout/pull/42";
const OTHER_PR: &str = "https://github.com/project/checkout/pull/43";

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
        increment: None,
        task_fingerprint: None,
        note: None,
        decision: None,
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

/// A review of one confirmed minor finding.
fn minor(file: &str, line: u32) -> Report {
    report(vec![finding(
        file,
        Some(line),
        Severity::Minor,
        Status::Confirmed,
    )])
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

/// `listed` plus the open standing issue holding `section`, as GitHub would list it.
fn listed_with_standing(planned: &[Planned], section: &Section) -> (serde_json::Value, String) {
    let body = marked_body(&standing_body("", section), STANDING_TITLE).unwrap();
    let mut issues = listed(planned, "someone");
    issues.as_array_mut().unwrap().push(serde_json::json!({
        "number": 200, "title": STANDING_TITLE, "body": body,
        "labels": [{"name": "agent-found"}, {"name": "review-follow-up"}],
        "user": {"login": "someone"},
        "html_url": "https://github.com/project/checkout/issues/200"
    }));
    (issues, body)
}

#[test]
fn serious_leftovers_are_issues_and_small_ones_one_section_and_refuted_ones_nothing() {
    let plan = plan(42, PR, &mixed());
    assert_eq!(plan.issues.len(), 2, "{:?}", plan.issues);
    assert!(
        plan.issues[0].title.contains("a.rs:1"),
        "{}",
        plan.issues[0].title
    );
    assert!(
        plan.issues[0]
            .labels
            .contains(&"priority:critical".to_owned())
    );
    assert!(
        plan.issues[1].title.contains("b.rs:2"),
        "{}",
        plan.issues[1].title
    );
    assert!(plan.issues[1].labels.contains(&"priority:high".to_owned()));
    assert!(plan.issues[1].body.contains("disputed major"));
    let section = plan.section.expect("the minor and nit section");
    assert_eq!(section.pr, 42);
    assert_eq!(section.url, PR);
    assert!(
        section
            .items
            .contains(&"- [ ] d.rs:4 — claim about d.rs (minor)".to_owned())
    );
    assert!(
        section
            .items
            .contains(&"- [ ] e.rs — claim about e.rs (nit)".to_owned())
    );
    for item in &plan.issues {
        assert!(item.labels.contains(&"agent-found".to_owned()));
        assert!(item.labels.contains(&"review-follow-up".to_owned()));
        for refuted in ["c.rs", "f.rs"] {
            assert!(!item.title.contains(refuted) && !item.body.contains(refuted));
        }
    }
    let rendered = section.render();
    assert!(
        !rendered.contains("c.rs") && !rendered.contains("f.rs"),
        "{rendered}"
    );
}

#[test]
fn a_review_with_only_refuted_findings_files_nothing() {
    let only_refuted = report(vec![
        finding("c.rs", Some(3), Severity::Critical, Status::Refuted),
        finding("f.rs", Some(6), Severity::Nit, Status::Refuted),
    ]);
    let plan = plan(42, PR, &only_refuted);
    assert!(plan.issues.is_empty() && plan.section.is_none());
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
    let plan = plan(7, PR, &report(vec![forged]));
    let item = &plan.issues[0];
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
    let plan = plan(1, PR, &report(vec![big]));
    assert_eq!(plan.issues[0].title.chars().count(), TITLE_LIMIT);
    assert!(marked_body(&plan.issues[0].body, &plan.issues[0].title).is_ok());
}

#[test]
fn a_section_over_its_limit_keeps_the_marker_and_counts_what_it_left_out() {
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
    let section = plan(42, PR, &report(many)).section.expect("section");
    assert!(
        section
            .items
            .last()
            .unwrap()
            .contains("more finding(s) are in the review on #42")
    );
    let body = standing_body("", &section);
    let marked = marked_body(&body, STANDING_TITLE).unwrap();
    assert!(marked.chars().count() <= BODY_LIMIT);
    assert!(marked.contains("more finding(s)"));
}

#[test]
fn the_same_leftover_twice_in_one_review_is_filed_once() {
    let twice = report(vec![
        finding("a.rs", Some(1), Severity::Major, Status::Confirmed),
        finding("a.rs", Some(1), Severity::Major, Status::Disputed),
        finding("d.rs", Some(4), Severity::Nit, Status::Confirmed),
        finding("d.rs", Some(4), Severity::Nit, Status::Confirmed),
    ]);
    let plan = plan(3, OTHER_PR, &twice);
    assert_eq!(plan.issues.len(), 1);
    let section = plan.section.expect("section");
    assert_eq!(
        section
            .items
            .iter()
            .filter(|i| i.contains("d.rs:4"))
            .count(),
        1
    );
}

#[test]
fn the_first_follow_up_opens_the_standing_issue_and_no_issue_per_pull_request() {
    let stub = Stub::new(&serde_json::json!([]));
    assert!(file(&stub.root, PR, &mixed()).is_empty());
    let calls = stub.calls();
    // Two serious findings and the standing issue: three creates, and no
    // issue named after the pull request.
    assert_eq!(calls.matches("<issue>\n<create>").count(), 3, "{calls}");
    assert!(
        calls.contains("<--title>\n<Review follow-ups>\n"),
        "{calls}"
    );
    assert!(!calls.contains("for #42"), "{calls}");
    assert!(!calls.contains("<edit>"), "{calls}");
    let bodies = stub.bodies();
    assert!(bodies.contains("## PR #42 — https://github.com/project/checkout/pull/42"));
    assert!(bodies.contains("- [ ] d.rs:4 — claim about d.rs (minor)"));
    // The same list is posted as a pull request comment.
    assert!(calls.contains("<pr>\n<comment>\n<42>"), "{calls}");
}

#[test]
fn a_second_ship_comments_on_its_issues_and_a_decoy_marker_is_not_a_duplicate() {
    let planned = plan(42, PR, &mixed());
    // A decoy carries the standing issue's marker under another title.
    let decoy = serde_json::json!([{
        "number": 5, "title": "Something else",
        "body": format!("{MARKER}{} -->", fingerprint(STANDING_TITLE)),
        "labels": [{"name": "agent-found"}], "html_url": "https://github.com/project/checkout/issues/5"
    }]);
    let stub = Stub::new(&decoy);
    assert!(file(&stub.root, PR, &mixed()).is_empty());
    let calls = stub.calls();
    assert_eq!(calls.matches("<create>\n<--title>").count(), 3, "{calls}");
    assert!(!calls.contains("<issue>\n<comment>"), "{calls}");
    assert!(calls.contains("<--repo>\n<github.com/project/checkout>"));
    assert!(!calls.contains("wrong/override"));
    let bodies = stub.bodies();
    assert_eq!(bodies.matches(MARKER).count(), 3, "{bodies}");
    assert!(bodies.contains("- [ ] d.rs:4"));

    // Both serious issues are open and the standing issue too: comments and
    // one edit of the standing body, never a new issue.
    drop(stub);
    let (listing, marked) =
        listed_with_standing(&planned.issues, planned.section.as_ref().unwrap());
    let mut stub = Stub::new(&listing);
    stub.set("GH_VIEW_BODY", &marked);
    assert!(file(&stub.root, PR, &mixed()).is_empty());
    let calls = stub.calls();
    assert!(!calls.contains("<create>"), "{calls}");
    assert_eq!(calls.matches("<issue>\n<comment>").count(), 2, "{calls}");
    assert!(calls.contains("Reported again by the passing review of #42 at 0123456789ab."));
    assert!(calls.contains("<issue>\n<edit>\n<200>"), "{calls}");

    // A later review's new minor finding reaches the standing issue.
    stub.clear();
    let mut later = mixed();
    later
        .findings
        .push(finding("g.rs", Some(7), Severity::Minor, Status::Confirmed));
    assert!(file(&stub.root, PR, &later).is_empty());
    let calls = stub.calls();
    assert!(!calls.contains("<create>"), "{calls}");
    assert!(
        stub.bodies()
            .contains("- [ ] g.rs:7 — claim about g.rs (minor)"),
        "{calls}"
    );
    assert_eq!(stub.bodies().matches("## PR #42").count(), 1);
}

#[test]
fn a_second_pull_request_appends_its_section_to_the_standing_issue() {
    let first = plan(42, PR, &mixed());
    let (listing, marked) = listed_with_standing(&first.issues, first.section.as_ref().unwrap());
    let mut stub = Stub::new(&listing);
    stub.set("GH_VIEW_BODY", &marked);
    assert!(file(&stub.root, OTHER_PR, &minor("h.rs", 8)).is_empty());
    let calls = stub.calls();
    assert!(!calls.contains("<create>"), "{calls}");
    assert!(calls.contains("<issue>\n<edit>\n<200>"), "{calls}");
    let body = stub.bodies();
    assert!(
        body.contains("## PR #42 — https://github.com/project/checkout/pull/42"),
        "{body}"
    );
    assert!(
        body.contains("## PR #43 — https://github.com/project/checkout/pull/43"),
        "{body}"
    );
    assert!(
        body.contains("- [ ] h.rs:8 — claim about h.rs (minor)"),
        "{body}"
    );
    assert!(
        body.contains("- [ ] d.rs:4 — claim about d.rs (minor)"),
        "{body}"
    );
    // The pull request comment is posted on the second pull request.
    assert!(calls.contains("<pr>\n<comment>\n<43>"), "{calls}");
}

#[test]
fn a_pull_request_shipped_again_replaces_its_own_section() {
    let current = standing_body("", &plan(42, PR, &mixed()).section.unwrap());
    let again = plan(42, PR, &minor("k.rs", 2));
    let next = standing_body(&current, again.section.as_ref().unwrap());
    assert_eq!(next.matches("## PR #42").count(), 1, "{next}");
    assert!(next.contains("- [ ] k.rs:2"));
    assert!(!next.contains("d.rs:4"));
}

#[test]
fn the_standing_issue_folds_its_oldest_sections_past_the_bound() {
    let mut body = String::new();
    for pr in 1..=MAX_SECTIONS as u64 + 1 {
        let finding = finding(
            &format!("f{pr}.rs"),
            Some(1),
            Severity::Minor,
            Status::Confirmed,
        );
        let section = plan(pr, PR, &report(vec![finding])).section.unwrap();
        body = standing_body(&body, &section);
    }
    assert_eq!(body.matches("## PR #").count(), MAX_SECTIONS, "{body}");
    assert!(
        body.contains("> Folded 1 earlier review section, up to #1."),
        "{body}"
    );
    assert!(!body.contains("## PR #1 "), "{body}");
    assert!(body.contains("## PR #2 "));
    // Folding again keeps one summary line, counting on.
    let section = plan(99, PR, &minor("z.rs", 1)).section.unwrap();
    let body = standing_body(&body, &section);
    assert_eq!(body.matches("> Folded").count(), 1);
    assert!(
        body.contains("> Folded 2 earlier review sections, up to #2."),
        "{body}"
    );
    assert_eq!(body.matches("## PR #").count(), MAX_SECTIONS);
}

#[test]
fn the_standing_body_folds_to_fit_the_limit_whatever_the_section_sizes() {
    let mut body = String::new();
    for pr in 1..=12u64 {
        let claims: Vec<_> = (0..40)
            .map(|n| {
                let mut item = finding(
                    &format!("p{pr}-{n}.rs"),
                    Some(n),
                    Severity::Minor,
                    Status::Confirmed,
                );
                item.reported.claim = "界".repeat(300);
                item
            })
            .collect();
        let section = plan(pr, PR, &report(claims)).section.unwrap();
        body = standing_body(&body, &section);
        let marked = marked_body(&body, STANDING_TITLE).expect("within the limit");
        assert!(marked.chars().count() <= BODY_LIMIT);
    }
    assert!(body.contains("> Folded "), "{body}");
    assert!(body.contains("## PR #12 "));
}

#[test]
fn failing_to_file_is_reported_and_never_fatal() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("GH_FAIL", "create");
    let problems = file(&stub.root, PR, &mixed());
    // Two serious issues and the standing issue; the comment still goes out.
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].contains("stub failure"), "{problems:?}");
    assert!(stub.calls().contains("<pr>\n<comment>"), "{}", stub.calls());
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
