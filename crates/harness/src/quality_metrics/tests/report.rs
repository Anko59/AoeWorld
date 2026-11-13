use crate::quality_metrics::report::*;

#[test]
fn history_is_revision_keyed_and_never_overwrites_a_run() {
    let directory = tempfile::tempdir().unwrap();
    let report = serde_json::json!({"revision": "abc123", "metrics": []});
    let first = write_history(directory.path(), "abc123", "idle", &report).unwrap();
    let second = write_history(directory.path(), "abc123", "idle", &report).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.file_name().unwrap(), "report.json");
    assert_eq!(first.parent().unwrap().file_name().unwrap(), "idle-001");
    assert_eq!(
        first
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .file_name()
            .unwrap(),
        "abc123"
    );
    assert_eq!(
        std::fs::read(first).unwrap(),
        std::fs::read(second).unwrap()
    );
}

#[test]
fn history_rejects_path_components_as_run_labels() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        write_history(
            directory.path(),
            "abc123",
            "../escape",
            &serde_json::json!({})
        )
        .is_err()
    );
}

#[test]
fn repeatability_compares_values_and_scenario_identity_not_runtime_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reference.json");
    std::fs::write(
        &path,
        r#"{"metrics":[{"scenario":"march","arrived":1}],"workers":1}"#,
    )
    .unwrap();
    assert!(
        compare(
            &path,
            &serde_json::json!([{"scenario":"march","arrived":1}])
        )
        .is_ok()
    );
    assert!(
        compare(
            &path,
            &serde_json::json!([{"scenario":"march","arrived":0}])
        )
        .is_err()
    );
    assert!(compare(&path, &serde_json::json!([])).is_err());
}

#[test]
fn source_content_identity_survives_staging_and_excludes_generated_history() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .current_dir(root)
                .args(args)
                .status()
                .unwrap()
                .success()
        )
    };
    git(&["init", "-q"]);
    std::fs::create_dir(root.join("crates")).unwrap();
    std::fs::write(root.join("crates/probe.rs"), "first").unwrap();
    let first = source_digest(root).unwrap();
    git(&["add", "crates/probe.rs"]);
    assert_eq!(first, source_digest(root).unwrap());
    std::fs::create_dir_all(root.join("reports/quality")).unwrap();
    std::fs::write(root.join("reports/quality/run.json"), "{}").unwrap();
    assert_eq!(first, source_digest(root).unwrap());
    std::fs::write(root.join("crates/probe.rs"), "second").unwrap();
    assert_ne!(first, source_digest(root).unwrap());
}

#[test]
fn history_keeps_many_runs_within_the_repository_directory_file_limit() {
    let directory = tempfile::tempdir().unwrap();
    for _ in 0..30 {
        write_history(
            directory.path(),
            "abc123",
            "run",
            &serde_json::json!({"metrics": []}),
        )
        .unwrap();
    }
    let mut pending = vec![directory.path().to_owned()];
    while let Some(path) = pending.pop() {
        let mut files = 0;
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                files += 1;
            }
        }
        assert!(
            files <= 14,
            "history must remain valid after many builds and repeats"
        );
    }
}
