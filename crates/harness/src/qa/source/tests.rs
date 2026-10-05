use super::*;
use serde_json::json;
use std::fs;
mod fixture;
use fixture::*;
fn options(path: PathBuf, base: &str, candidate: Option<&str>, index: bool) -> Options {
    Options {
        file: path,
        base: Some(base.into()),
        candidate: candidate.map(str::to_owned),
        index,
    }
}
#[test]
fn actual_commit_source_match_measures_whole_wrapper_and_never_runtime_or_approval() {
    let (owner, base_oid, candidate_oid) = fixture();
    let root = owner.path();
    let (base, candidate) = snapshots(root, &base_oid, &candidate_oid);
    let summary = review::source_summary(&base, &candidate).unwrap();
    let (path, evidence, _) = report(root, &summary);
    let raw = fs::read(&path).unwrap();
    let result = bind_at(
        &options(path.clone(), &base_oid, Some(&candidate_oid), false),
        root,
        &evidence,
    )
    .unwrap();
    assert_eq!(
        result.observation.report.raw_blake3,
        blake3::hash(&raw).to_hex().to_string()
    );
    assert_eq!(result.source_binding.summary, summary);
    assert_eq!(
        result.source_identity,
        "MEASURED_LOGICAL_CONTENT_NON_AUTHORITATIVE"
    );
    assert_eq!(result.served_build_binding, "UNAVAILABLE");
    assert!(!result.authoritative);
    assert_eq!(result.observation.source_identity, "UNAVAILABLE");
    assert!(
        observation::observe_file_at(&path, &evidence)
            .unwrap_err()
            .to_string()
            .contains("requires retained immutable source context")
    );
    let text = serde_json::to_string(&result).unwrap();
    assert!(!text.contains("unverified-health-claim"));
    assert_eq!(result.independent_qa, "NOT_ASSESSED");
    assert_eq!(result.journey_execution, "NOT_ASSESSED");
}
#[test]
fn intentional_index_scope_is_actual_pending_tree_with_unchanged_index_and_working_bytes() {
    let (owner, base_oid, candidate_oid) = fixture();
    let root = owner.path();
    fs::write(root.join("old.txt"), "staged-only").unwrap();
    git(root, &["add", "old.txt"], None);
    fs::write(root.join("old.txt"), "working-only").unwrap();
    let base = Snapshot::prepare_independent(root, Kind::Commit(base_oid.clone())).unwrap();
    let candidate = Snapshot::prepare(root, Kind::Index).unwrap();
    let summary = review::source_summary(&base, &candidate).unwrap();
    assert_eq!(summary.scope["kind"], "index");
    assert_eq!(summary.scope["captured_source_head"], candidate_oid);
    assert!(summary.scope.get("candidate").is_none());
    let (path, evidence, _) = report(root, &summary);
    let raw_index = fs::read(root.join(".git/index")).unwrap();
    let working = fs::read(root.join("old.txt")).unwrap();
    let result = bind_at(&options(path, &base_oid, None, true), root, &evidence).unwrap();
    assert_eq!(result.source_binding.summary, summary);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), raw_index);
    assert_eq!(fs::read(root.join("old.txt")).unwrap(), working);
    fs::write(root.join("old.txt"), "restaged").unwrap();
    git(root, &["add", "old.txt"], None);
    assert!(review::source_summary(&base, &candidate).is_err());
    let fresh_base = Snapshot::prepare_independent(root, Kind::Commit(base_oid)).unwrap();
    let fresh_candidate = Snapshot::prepare(root, Kind::Index).unwrap();
    assert_ne!(
        review::source_summary(&fresh_base, &fresh_candidate).unwrap(),
        summary
    );
}
#[test]
fn fake_digests_wrong_base_and_changed_candidate_cannot_echo_into_source_match() {
    let (owner, base_oid, candidate_oid) = fixture();
    let root = owner.path();
    let (base, candidate) = snapshots(root, &base_oid, &candidate_oid);
    let summary = review::source_summary(&base, &candidate).unwrap();
    let (path, evidence, value) = report(root, &summary);
    for key in ["candidate_content_witness_digest", "review_subject_digest"] {
        let mut bad = value.clone();
        bad["expected_source"][key] = json!("0".repeat(64));
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(
            bind_at(
                &options(path.clone(), &base_oid, Some(&candidate_oid), false),
                root,
                &evidence
            )
            .unwrap_err()
            .to_string()
            .contains("does not match generated immutable subject")
        );
    }
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(
        bind_at(
            &options(path.clone(), &candidate_oid, Some(&candidate_oid), false),
            root,
            &evidence
        )
        .is_err()
    );
    let lib = root.join("crates/demo/src/lib.rs");
    let old = fs::read_to_string(&lib).unwrap();
    fs::write(
        &lib,
        format!("// changed actual AST span and raw source\n{old}"),
    )
    .unwrap();
    let changed = commit(root, Some(&candidate_oid));
    assert!(
        bind_at(
            &options(path, &base_oid, Some(&changed), false),
            root,
            &evidence
        )
        .unwrap_err()
        .to_string()
        .contains("does not match generated immutable subject")
    );
}
#[test]
fn wrapper_is_closed_all_depths_and_legacy_v1_remains_unbound() {
    let (owner, base_oid, candidate_oid) = fixture();
    let root = owner.path();
    let (base, candidate) = snapshots(root, &base_oid, &candidate_oid);
    let summary = review::source_summary(&base, &candidate).unwrap();
    let (path, evidence, value) = report(root, &summary);
    for key in ["role", "algorithm", "base", "scope", "approved"] {
        let mut bad = value.clone();
        bad["expected_source"][key] = json!(true);
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(
            bind_at(
                &options(path.clone(), &base_oid, Some(&candidate_oid), false),
                root,
                &evidence
            )
            .is_err()
        );
    }
    let mut bad = value.clone();
    bad["approved"] = json!(true);
    fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(
        bind_at(
            &options(path.clone(), &base_oid, Some(&candidate_oid), false),
            root,
            &evidence
        )
        .is_err()
    );
    let text = serde_json::to_string(&value)
        .unwrap()
        .replace("\"version\":2", "\"version\":2,\"version\":2");
    fs::write(&path, text).unwrap();
    assert!(observation::retain_file_at(&path, &evidence, true).is_err());
    fs::write(&path, serde_json::to_vec(&value["report"]).unwrap()).unwrap();
    let legacy = observation::observe_file_at(&path, &evidence).unwrap();
    assert_eq!(legacy.source_identity, "UNAVAILABLE");
    assert!(
        bind_at(
            &options(path, &base_oid, Some(&candidate_oid), false),
            root,
            &evidence
        )
        .unwrap_err()
        .to_string()
        .contains("require a version-2 report")
    );
}
#[test]
fn retained_original_report_and_artifact_fds_detect_late_changes_after_measurement() {
    let (owner, base_oid, candidate_oid) = fixture();
    let root = owner.path();
    let (base, candidate) = snapshots(root, &base_oid, &candidate_oid);
    let summary = review::source_summary(&base, &candidate).unwrap();
    let (path, evidence, value) = report(root, &summary);
    let mut retained = observation::retain_file_at(&path, &evidence, true).unwrap();
    fs::write(
        evidence.join("screen.bin"),
        b"changed original artifact bytes",
    )
    .unwrap();
    assert!(retained.recheck_all().is_err());
    report(root, &summary);
    let mut retained = observation::retain_file_at(&path, &evidence, true).unwrap();
    let mut changed = value;
    changed["report"]["build"] = json!("new claim");
    fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(retained.recheck_all().is_err());
}
#[test]
fn cli_preserves_optional_legacy_file_and_rejects_incomplete_or_ambiguous_source_groups() {
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        options: Options,
    }
    let legacy = Cli::try_parse_from(["qa"]).unwrap();
    assert_eq!(
        legacy.options.file,
        PathBuf::from("reports/qa/session.json")
    );
    for args in [
        vec!["qa", "--base", "x"],
        vec!["qa", "--candidate", "x"],
        vec!["qa", "--index"],
        vec!["qa", "--base", "x", "--candidate", "y", "--index"],
    ] {
        assert!(Cli::try_parse_from(args).is_err());
    }
    assert!(Cli::try_parse_from(["qa", "report.json", "--base", "x", "--index"]).is_ok());
    assert!(full_oid("abc").is_err());
    assert!(full_oid(&"A".repeat(40)).is_err());
    assert!(full_oid(&"a".repeat(40)).is_ok());
    assert!(full_oid(&"a".repeat(64)).is_ok());
}
