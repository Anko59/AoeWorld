use super::*;

#[test]
fn an_empty_reship_removes_only_its_section_and_comments_with_the_current_revision() {
    for findings in [
        vec![],
        vec![finding(
            "fixed.rs",
            Some(1),
            Severity::Minor,
            Status::Refuted,
        )],
        vec![finding("fixed.rs", Some(1), Severity::Nit, Status::Refuted)],
    ] {
        let mut stub = Stub::new(&serde_json::json!([]));
        let first = minor("fixed.rs", 1);
        assert!(file(&stub.root, PR, &first).is_empty());
        let first_body = stub.bodies().split("\n===\n").next().unwrap().to_owned();
        assert!(first_body.contains("## PR #42"));
        assert!(first_body.contains("- [ ] fixed.rs:1"));

        let mut other_report = minor("other.rs", 2);
        other_report.head = "fedcba9876543210fedcba9876543210fedcba9876".into();
        let other = plan(43, OTHER_PR, &other_report).section.unwrap();
        let other_body = other.render().replace("- [ ]", "- [x]");
        let current =
            standing_body(&first_body, &other).replace("- [ ] other.rs", "- [x] other.rs");
        let (listing, _) = listed_with_standing(&[], &other);
        stub.set("GH_LIST_JSON", &listing.to_string());
        stub.set("GH_VIEW_BODY", &current);
        stub.clear();
        let mut later = report(findings);
        later.head = "abcdef0123456789abcdef0123456789abcdef0123".into();
        assert!(file(&stub.root, PR, &later).is_empty());
        let calls = stub.calls();
        assert!(calls.contains("<issue>\n<edit>\n<200>"), "{calls}");
        assert!(calls.contains("<pr>\n<comment>\n<42>"), "{calls}");
        assert!(!calls.contains("<create>"), "{calls}");
        let bodies = stub.bodies();
        let (standing, comment) = bodies.split_once("\n===\n").unwrap();
        assert!(!standing.contains("## PR #42"), "{standing}");
        assert!(!standing.contains("fixed.rs"), "{standing}");
        assert!(!standing.contains(&first.head[..12]), "{standing}");
        assert!(standing.contains(&other_body), "{standing}");
        assert_eq!(standing.matches(MARKER).count(), 1, "{standing}");
        assert!(
            comment.contains("No open minor or nit findings"),
            "{comment}"
        );
        assert!(comment.contains("abcdef012345"), "{comment}");
        assert!(!comment.contains("- [ ]"), "{comment}");
        println!("re-ship: PR #42 section removed; PR #43 checked items preserved\n{comment}");
    }
}

#[test]
fn an_empty_review_without_a_standing_issue_creates_none_and_still_comments() {
    let stub = Stub::new(&serde_json::json!([]));
    assert!(file(&stub.root, PR, &report(vec![])).is_empty());
    let calls = stub.calls();
    assert!(
        !calls.contains("<create>") && !calls.contains("<edit>"),
        "{calls}"
    );
    assert!(calls.contains("<pr>\n<comment>\n<42>"), "{calls}");
    assert!(stub.bodies().contains("No open minor or nit findings"));
}
