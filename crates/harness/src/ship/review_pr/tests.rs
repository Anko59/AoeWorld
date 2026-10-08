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
        change_fingerprint: None,
        policy_fingerprint: None,
        reused_from: None,
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
            "--squash",
            "--match-head-commit",
            "abc123"
        ]
    );
    assert_eq!(&calls[2][..2], ["pr", "comment"]);
    assert!(calls[2][6].contains("clean"));
}

#[test]
fn a_major_bump_in_a_workspace_member_manifest_raises_the_floor() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .current_dir(root)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q", "-b", "dev"]);
    std::fs::create_dir_all(root.join("crates/x")).unwrap();
    std::fs::write(
        root.join("crates/x/Cargo.toml"),
        "[dependencies]\nsyn = \"2.0.1\"\n",
    )
    .unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    std::fs::write(
        root.join("crates/x/Cargo.toml"),
        "[dependencies]\nsyn = \"3.0.6\"\n",
    )
    .unwrap();
    git(&["commit", "-q", "-am", "bump"]);
    assert!(dependency_major_bump(root, "HEAD~1", "HEAD").unwrap());
}

#[test]
fn only_dependabot_dependency_updates_are_reviewed() {
    let mut human = metadata("dev", false);
    human.author = serde_json::json!({"login": "someone"});
    assert!(
        validate_metadata(&human)
            .unwrap_err()
            .to_string()
            .contains("Dependabot only")
    );
    let ok: Vec<String> = [
        "Cargo.lock",
        "crates/x/Cargo.toml",
        "browser/package.json",
        "browser/package-lock.json",
        "docker/rust-tools.Dockerfile",
        ".github/workflows/ci.yml",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    assert!(dependency_paths_only(&ok).is_ok());
    for bad in [
        ".claude/settings.json",
        ".agents/hooks/harness.sh",
        "crates/harness/src/main.rs",
        "gates/review.json",
        "AGENTS.md",
    ] {
        let error = dependency_paths_only(&[bad.to_owned()])
            .unwrap_err()
            .to_string();
        assert!(error.contains(bad), "{error}");
    }
}

#[test]
fn auto_merge_is_pinned_to_the_reviewed_commit() {
    let report: review::Report = serde_json::from_value(serde_json::json!({
        "version": 1, "head": "b".repeat(40), "branch": "dependabot/x", "base": "", "merge_base": "",
        "tier": "low", "floor": "low", "runtime": "codex", "model": "m", "effort": "high",
        "personas": ["quick"], "rounds": 1, "findings": [], "written_grade": 9, "grade": 9,
        "summary": "", "failures": [], "merge_grade": 8, "started": 0, "finished": 0
    })).unwrap();
    let calls = publication_calls("o/r", "https://github.com/o/r/pull/1", &report).unwrap();
    let merge = calls
        .iter()
        .find(|c| c[0] == "pr" && c[1] == "merge")
        .unwrap();
    assert!(
        merge
            .windows(2)
            .any(|w| w[0] == "--match-head-commit" && w[1] == "b".repeat(40)),
        "{merge:?}"
    );
}

#[test]
fn a_docker_image_major_bump_counts() {
    let before = image_dependencies("FROM rust:1.93.1-bookworm@sha256:abc AS build\nRUN x\n");
    let after = image_dependencies("FROM rust:2.0.0-bookworm@sha256:def AS build\n");
    assert_eq!(
        before.get("rust").map(String::as_str),
        Some("1.93.1-bookworm")
    );
    assert!(has_major_bump(&before, &after));
    let docker = image_dependencies("FROM docker:29.1.3-cli@sha256:x\n");
    assert!(!has_major_bump(
        &docker,
        &image_dependencies("FROM docker:29.8.2-cli\n")
    ));
}
