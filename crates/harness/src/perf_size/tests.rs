use super::*;

#[test]
fn size_baseline_rejects_wrong_identity_and_five_percent_regression() {
    let baseline = proposal(100);
    assert_eq!(baseline.toolchain, "rustc 1.99.1");
    assert_eq!(
        compare_observed(baseline, 105)
            .expect("at threshold")
            .verdict,
        Verdict::Pass
    );
    assert_eq!(
        compare_observed(proposal(100), 106)
            .expect("over threshold")
            .verdict,
        Verdict::Regression
    );
    let mut incompatible = proposal(100);
    incompatible.binaryen = "other".to_owned();
    assert!(compare_observed(incompatible, 100).is_err());
}

#[test]
fn size_artifact_must_exist_and_proposal_never_changes_baseline() {
    let temp = tempfile::tempdir().expect("directory");
    let artifact = temp.path().join("optimized.wasm.gz");
    let baseline = temp.path().join("baseline.json");
    let proposed = temp.path().join("proposal/wasm.json");
    assert!(observed_at(&artifact).is_err());
    fs::write(&artifact, []).expect("empty artifact");
    assert!(observed_at(&artifact).is_err());
    fs::write(&artifact, [7u8; 105]).expect("artifact");
    fs::write(&baseline, serde_json::to_vec(&proposal(100)).expect("JSON")).expect("baseline");
    propose_at(&artifact, &proposed).expect("proposal");
    assert_eq!(
        comparison_at(&baseline, &artifact)
            .expect("comparison")
            .verdict,
        Verdict::Pass
    );
    assert_eq!(
        serde_json::from_slice::<Baseline>(&fs::read(&proposed).expect("proposal"))
            .expect("JSON")
            .optimized_gzip_bytes,
        105
    );
    assert_eq!(
        serde_json::from_slice::<Baseline>(&fs::read(&baseline).expect("baseline"))
            .expect("JSON")
            .optimized_gzip_bytes,
        100
    );
}

#[test]
fn both_browser_apps_must_exist_and_fit_the_same_unchanged_cap() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("baselines/perf")).unwrap();
    fs::create_dir_all(root.path().join("reports/perf")).unwrap();
    fs::write(
        root.path().join("baselines/perf/wasm.json"),
        serde_json::to_vec(&proposal(100)).unwrap(),
    )
    .unwrap();
    let game = root.path().join("reports/perf/optimized.wasm.gz");
    let lab = root
        .path()
        .join("reports/perf/diagnostics-optimized.wasm.gz");
    fs::write(&game, [0; 100]).unwrap();
    assert!(check_at(root.path()).is_err());
    fs::write(&lab, [0; 105]).unwrap();
    check_at(root.path()).unwrap();
    fs::write(&lab, [0; 106]).unwrap();
    assert!(check_at(root.path()).is_err());
    fs::write(&lab, [0; 100]).unwrap();
    fs::write(&game, [0; 106]).unwrap();
    assert!(check_at(root.path()).is_err());
}
