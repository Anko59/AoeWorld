use super::super::{MARKER, marked_body, parse_issue_pages, test_stub::Stub};
use super::*;

fn needs(pairs: &[(&str, &str)]) -> String {
    let map: serde_json::Map<_, _> = pairs
        .iter()
        .map(|(job, result)| {
            (
                (*job).to_owned(),
                serde_json::json!({"result": result, "outputs": {}}),
            )
        })
        .collect();
    serde_json::Value::Object(map).to_string()
}

/// An issue as GitHub lists it, with the body's last marker parsed.
fn listed(number: u64, title: &str, author: &str, labels: &[&str], body: &str) -> Issue {
    let page = serde_json::json!([{
        "number": number, "title": title, "body": body, "user": {"login": author},
        "labels": labels.iter().map(|name| serde_json::json!({"name": name})).collect::<Vec<_>>(),
        "html_url": format!("https://github.com/project/checkout/issues/{number}")
    }]);
    parse_issue_pages(page.to_string().as_bytes())
        .unwrap()
        .remove(0)
}

fn genuine(number: u64, job: &str) -> Issue {
    let title = title(job);
    listed(
        number,
        &title,
        BOT,
        &[NIGHTLY_LABEL],
        &marked_body("failed", &title).unwrap(),
    )
}

fn as_json(issue: &Issue, body: &str) -> serde_json::Value {
    serde_json::json!({
        "number": issue.number, "title": issue.title, "body": body,
        "user": {"login": issue.author},
        "labels": issue.labels.iter().map(|l| serde_json::json!({"name": l.name})).collect::<Vec<_>>(),
        "html_url": issue.url
    })
}

#[test]
fn the_needs_context_is_bounded_and_strict() {
    let parsed = parse(&needs(&[
        ("full-functional", "failure"),
        ("_soak", "success"),
    ]))
    .unwrap();
    assert_eq!(parsed["full-functional"], Outcome::Failure);
    assert_eq!(parsed["_soak"], Outcome::Success);
    let refused = [
        "[]".to_owned(),
        "{}".to_owned(),
        "not json".to_owned(),
        r#"{"a": {"result": "failure", "extra": 1}}"#.to_owned(),
        r#"{"a": {"result": "exploded"}}"#.to_owned(),
        r#"{"a": {}}"#.to_owned(),
        r#"{"a": {"result": "failure", "outputs": {"k": 1}}}"#.to_owned(),
        needs(&[("bad job", "failure")]),
        needs(&[("-dash", "failure")]),
        needs(&[("", "failure")]),
        needs(&[(&"j".repeat(65), "failure")]),
        format!(
            r#"{{"a": {{"result": "failure", "outputs": {{"k": "{}"}}}}}}"#,
            "x".repeat(RESULTS_LIMIT)
        ),
    ];
    for raw in &refused {
        assert!(parse(raw).is_err(), "{}", &raw[..raw.len().min(80)]);
    }
    let jobs: Vec<String> = (0..=JOB_CAP).map(|n| format!("job{n}")).collect();
    let too_many: Vec<(&str, &str)> = jobs.iter().map(|job| (job.as_str(), "success")).collect();
    assert!(parse(&needs(&too_many)).is_err());
    let outputs: serde_json::Map<_, _> = (0..=OUTPUT_CAP)
        .map(|n| (format!("k{n}"), serde_json::json!("v")))
        .collect();
    let crowded = serde_json::json!({"a": {"result": "success", "outputs": outputs}});
    assert!(parse(&crowded.to_string()).is_err());
}

#[test]
fn only_the_bots_labelled_marked_issue_with_the_generated_title_is_owned() {
    let job = "parser-fuzz";
    let title = title(job);
    let marked = marked_body("failed", &title).unwrap();
    assert!(owned(&genuine(1, job), job));
    // A person forging every other property is still not the bot.
    assert!(!owned(
        &listed(2, &title, "mallory", &[NIGHTLY_LABEL], &marked),
        job
    ));
    assert!(!owned(&listed(3, &title, BOT, &[], &marked), job));
    assert!(!owned(
        &listed(4, &title, BOT, &[NIGHTLY_LABEL], "failed"),
        job
    ));
    assert!(
        !owned(&genuine(5, "network-soak"), job),
        "another job's issue"
    );
    let retitled = format!("{title}!");
    assert!(!owned(
        &listed(6, &retitled, BOT, &[NIGHTLY_LABEL], &marked),
        job
    ));
    // A decoy marker earlier in the body does not count; the last one does.
    let decoy = format!(
        "{MARKER}{} -->\n{}",
        fingerprint(&title),
        marked_body("x", "Other").unwrap()
    );
    assert!(!owned(
        &listed(7, &title, BOT, &[NIGHTLY_LABEL], &decoy),
        job
    ));
}

#[test]
fn failures_open_or_comment_passes_close_and_other_results_do_nothing() {
    let forged = listed(
        8,
        &title("b"),
        "mallory",
        &[NIGHTLY_LABEL],
        &marked_body("x", &title("b")).unwrap(),
    );
    let issues = [genuine(7, "a"), forged.clone(), genuine(9, "c")];
    let results = parse(&needs(&[
        ("a", "success"),
        ("b", "failure"),
        ("c", "failure"),
        ("d", "cancelled"),
        ("e", "skipped"),
        ("f", "failure"),
    ]))
    .unwrap();
    let actions = plan(&results, &issues, "RUN");
    assert_eq!(actions.len(), 4, "{actions:?}");
    assert_eq!(
        actions[0],
        Action::Close {
            number: 7,
            comment: "Passing again in RUN.".into()
        }
    );
    assert!(
        matches!(&actions[1], Action::Open { title, body } if title == "Nightly failure: b" && body.contains("`b` failed in RUN"))
    );
    assert_eq!(
        actions[2],
        Action::Comment {
            number: 9,
            body: "Still failing in RUN.".into()
        }
    );
    assert!(matches!(&actions[3], Action::Open { title, .. } if title == "Nightly failure: f"));
    // The forged issue is never closed even when its job passes.
    let passing = parse(&needs(&[("b", "success")])).unwrap();
    assert!(plan(&passing, &[forged], "RUN").is_empty());
}

#[test]
fn triage_acts_on_origin_and_leaves_forged_issues_alone() {
    let forged = listed(8, &title("b"), "mallory", &[NIGHTLY_LABEL], "");
    let forged_body = marked_body("x", &title("b")).unwrap();
    let listing = serde_json::json!([
        as_json(&genuine(7, "a"), &marked_body("x", &title("a")).unwrap()),
        as_json(&forged, &forged_body),
        as_json(&genuine(9, "c"), &marked_body("x", &title("c")).unwrap()),
    ]);
    let stub = Stub::new(&listing);
    let raw = needs(&[("a", "success"), ("b", "failure"), ("c", "failure")]);
    triage(&stub.root, &raw, Some("123")).unwrap();
    let calls = stub.calls();
    let run = "https://github.com/project/checkout/actions/runs/123";
    assert!(
        calls.contains(&format!(
            "<close>\n<7>\n<--comment>\n<Passing again in {run}.>"
        )),
        "{calls}"
    );
    assert!(calls.contains(&format!(
        "<comment>\n<9>\n<--body>\n<Still failing in {run}.>"
    )));
    assert!(calls.contains("<--title>\n<Nightly failure: b>"), "{calls}");
    assert!(calls.contains("<--label>\n<nightly-failure>"));
    assert!(
        !calls.contains("<8>"),
        "the forged issue was touched: {calls}"
    );
    assert!(!calls.contains("wrong/override"));
    assert!(calls.contains("<--repo>\n<github.com/project/checkout>"));
    let bodies = stub.bodies();
    assert!(bodies.contains(&format!("`b` failed in {run}")));
    assert!(bodies.contains(&format!("{MARKER}{} -->", fingerprint(&title("b")))));
}

#[test]
fn a_failed_github_call_fails_the_triage_after_trying_every_job() {
    let listing = serde_json::json!([as_json(
        &genuine(7, "a"),
        &marked_body("x", &title("a")).unwrap()
    )]);
    let mut stub = Stub::new(&listing);
    stub.set("GH_FAIL", "close");
    let raw = needs(&[("a", "success"), ("b", "failure")]);
    let error = triage(&stub.root, &raw, Some("not-a-number"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("stub failure"), "{error}");
    let calls = stub.calls();
    assert!(
        calls.contains("<create>"),
        "the other job still ran: {calls}"
    );
    assert!(stub.bodies().contains("a run without a GITHUB_RUN_ID"));
    stub.clear();
    assert!(triage(&stub.root, "{}", None).is_err());
    assert!(
        stub.calls().is_empty(),
        "a bad context makes no GitHub call"
    );
}

#[test]
fn only_the_main_session_triages_and_the_results_are_required() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("AOE_AGENT_ROLE", "tester");
    stub.set("NIGHTLY_RESULTS", &needs(&[("a", "failure")]));
    let error = execute(&stub.root).unwrap_err().to_string();
    assert!(error.contains("only to the main session"), "{error}");
    stub.unset("AOE_AGENT_ROLE");
    stub.unset("NIGHTLY_RESULTS");
    let error = execute(&stub.root).unwrap_err().to_string();
    assert!(error.contains("set NIGHTLY_RESULTS"), "{error}");
    stub.set("NIGHTLY_RESULTS", &needs(&[("a", "skipped")]));
    stub.unset("GITHUB_RUN_ID");
    execute(&stub.root).unwrap();
    assert!(!stub.calls().contains("<issue>"));
}

#[test]
fn the_route_make_target_and_workflow_reach_the_triage() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
    assert!(
        read(".agents/hooks/harness.sh")
            .contains("nightly-triage) probe=crates/harness/src/ship/issues/triage.rs")
    );
    let make = read("make/ship.mk");
    assert!(make.contains("nightly-triage:\n\t@.agents/hooks/harness.sh exec nightly-triage"));
    assert!(make.contains(" NIGHTLY_RESULTS"));
    let workflow = read(".github/workflows/nightly.yml");
    let (_, jobs) = workflow.split_once("\njobs:\n").unwrap();
    let (jobs, triage) = jobs.split_once("\n  triage:\n").unwrap();
    for line in [
        "if: always()",
        "issues: write",
        "run: make nightly-triage",
        "NIGHTLY_RESULTS: ${{ toJSON(needs) }}",
        "GH_TOKEN: ${{ github.token }}",
    ] {
        assert!(triage.contains(line), "{line}");
    }
    let needs = triage
        .lines()
        .find_map(|line| line.trim().strip_prefix("needs: ["))
        .unwrap();
    let jobs: Vec<_> = jobs
        .lines()
        .filter_map(|line| line.strip_prefix("  ")?.strip_suffix(':'))
        .filter(|name| !name.starts_with(' '))
        .collect();
    assert!(jobs.len() >= 5, "{jobs:?}");
    for job in jobs {
        assert!(
            needs
                .trim_end_matches(']')
                .split(", ")
                .any(|need| need == job),
            "triage does not wait for {job}"
        );
    }
}
