use super::*;

fn metadata(base_name: &str, cross_repository: bool) -> PrMetadata {
    PrMetadata {
        head_oid: "a".repeat(40),
        head_name: "dependabot/cargo/example-2.0.0".into(),
        base_name: base_name.into(),
        cross_repository,
        author: serde_json::json!({"login":"dependabot[bot]"}),
    }
}

#[test]
fn metadata_rejects_forks_and_non_dev_bases() {
    assert!(
        validate_metadata(&metadata("dev", true))
            .unwrap_err()
            .to_string()
            .contains("forks")
    );
    assert!(
        validate_metadata(&metadata("main", false))
            .unwrap_err()
            .to_string()
            .contains("based on dev")
    );
    assert!(validate_metadata(&metadata("dev", false)).is_ok());
}

#[test]
fn dependency_major_bumps_raise_low_floor_to_medium() {
    let before = BTreeMap::from([("example".into(), "^1.4.0".into())]);
    let bumped = BTreeMap::from([("example".into(), "^2.0.0".into())]);
    let minor = BTreeMap::from([("example".into(), "^1.5.0".into())]);
    assert!(has_major_bump(&before, &bumped));
    assert!(!has_major_bump(&before, &minor));
    assert_eq!(required_tier(Tier::Low, true), Tier::Medium);
    assert_eq!(required_tier(Tier::High, true), Tier::High);
    assert_eq!(required_tier(Tier::Low, false), Tier::Low);
}

#[test]
fn publication_calls_include_shared_status_merge_and_pr_comment() {
    let report = review::Report {
        version: 1,
        head: "abc123".into(),
        branch: "dependabot/update".into(),
        base: String::new(),
        merge_base: String::new(),
        tier: "low".into(),
        floor: "low".into(),
        runtime: "codex".into(),
        model: "scripted".into(),
        effort: "medium".into(),
        personas: vec!["quick".into()],
        rounds: 1,
        findings: vec![],
        written_grade: 9,
        grade: 9,
        summary: "clean".into(),
        failures: vec![],
        merge_grade: 8,
        started: 0,
        finished: 1,
        closing: false,
    };
    let calls = publication_calls(
        "owner/repo",
        "https://github.com/owner/repo/pull/161",
        &report,
    )
    .unwrap();
    assert_eq!(
        calls[0][0..4],
        [
            "api",
            "--method",
            "POST",
            "repos/owner/repo/statuses/abc123"
        ]
    );
    assert_eq!(
        calls[1],
        [
            "pr",
            "merge",
            "https://github.com/owner/repo/pull/161",
            "--repo",
            "owner/repo",
            "--auto",
            "--squash"
        ]
    );
    assert_eq!(&calls[2][..2], ["pr", "comment"]);
    assert!(calls[2][6].contains("clean"));
}
