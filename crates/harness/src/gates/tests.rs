use super::*;

#[test]
fn unknown_and_shared_paths_select_relevant_suites() {
    let static_only = BTreeSet::from(["static".into()]);
    for path in ["docs/testing.md", "README.md", "LICENSE", "THIRD_PARTY.md"] {
        assert_eq!(classify(&[path.into()]).suites, static_only, "{path}");
    }
    assert_eq!(classify(&["Cargo.lock".into()]).suites.len(), 6);
    assert_eq!(
        classify(&["crates/assets/src/slp.rs".into()]).suites,
        BTreeSet::from([
            "static".into(),
            "native".into(),
            "assets".into(),
            "browser".into(),
            "performance".into()
        ])
    );
    let runtime = BTreeSet::from([
        "static".into(),
        "native".into(),
        "browser".into(),
        "performance".into(),
    ]);
    for path in [
        "browser/tests/lab.spec.ts",
        "web/index.html",
        "crates/client/src/web.rs",
        "crates/rendering/src/lib.rs",
        "crates/server/src/lib.rs",
        "crates/core/src/lib.rs",
        "crates/scenario/src/lib.rs",
        "crates/simulation/src/lib.rs",
        "crates/protocol/src/lib.rs",
    ] {
        assert_eq!(classify(&[path.into()]).suites, runtime, "{path}");
    }
    assert_eq!(classify(&[]).suites.len(), 6);
}

#[test]
fn ci_selection_requires_exact_manifest_and_job_outcomes() {
    let docs = selection(
        "a".repeat(40),
        Some("b".repeat(40)),
        vec!["docs/testing.md".into()],
    );
    assert_eq!(docs.jobs.get("static"), Some(&true));
    assert_eq!(docs.jobs.get("browser"), Some(&false));
    let assets = selection(
        "a".repeat(40),
        None,
        vec!["crates/assets/src/slp.rs".into()],
    );
    assert_eq!(assets.jobs.get("fuzz-smoke"), Some(&true));
    assert_eq!(assets.jobs.get("native-coverage"), Some(&true));
    assert_eq!(assets.jobs.get("browser"), Some(&true));
    let all = selection("a".repeat(40), None, Vec::new());
    assert!(all.jobs.values().all(|selected| *selected));
    let manifest = serde_json::to_string(&docs).expect("manifest");
    let results = serde_json::json!({
        "select":"success", "static":"success", "native-coverage":"skipped",
        "browser":"skipped", "target-performance":"skipped", "fuzz-smoke":"skipped"
    })
    .to_string();
    check_selection(&manifest, &results, &docs).expect("matching selection");
    assert!(check_selection(&manifest, &results, &assets).is_err());
    assert!(check_selection(&manifest, "{}", &docs).is_err());
    assert!(
        check_selection(
            &manifest,
            &results.replace("\"skipped\"", "\"success\""),
            &docs
        )
        .is_err()
    );
    assert!(
        check_selection(
            &manifest,
            &results.replace("\"select\":\"success\"", "\"select\":\"failure\""),
            &docs
        )
        .is_err()
    );
}

#[test]
fn invalid_registry_is_rejected() {
    assert!(parse(br#"{"version":1,"gates":[{"id":"x","command":"make x","requires":["missing"],"select":"all","evidence":"test"}]}"#).is_err());
    for payload in [
        r#"{"version":2,"gates":[]}"#,
        r#"{"version":1,"gates":[{"id":"x","command":"make x","requires":[],"select":"all","evidence":"test"},{"id":"x","command":"make x","requires":[],"select":"all","evidence":"test"}]}"#,
        r#"{"version":1,"gates":[{"id":"x","command":"cargo test","requires":[],"select":"all","evidence":"test"}]}"#,
        r#"{"version":1,"gates":[{"id":"x","command":"make y","requires":[],"select":"all","evidence":"test"}]}"#,
    ] {
        assert!(parse(payload.as_bytes()).is_err(), "{payload}");
    }
}

#[test]
fn documentation_gate_rejects_drift_and_missing_local_links() {
    let temp = tempfile::tempdir().expect("directory");
    let root = temp.path();
    fs::create_dir_all(root.join("gates")).expect("gates");
    fs::create_dir_all(root.join("docs/nested")).expect("docs");
    fs::create_dir_all(root.join("crates/example")).expect("crates");
    fs::write(
            root.join("gates/registry.json"),
            r#"{"version":1,"gates":[{"id":"fmt-check","command":"make fmt-check","requires":[],"select":"all","evidence":"exit"}]}"#,
        )
        .expect("registry");
    let registry =
        parse(&fs::read(root.join("gates/registry.json")).expect("registry")).expect("parsed");
    fs::write(root.join("docs/gates.md"), table(&registry)).expect("generated docs");
    fs::write(
        root.join("docs/nested/guide.md"),
        "[gates](../gates.md) [web](https://example.com) [heading](#top)",
    )
    .expect("guide");
    docs_check(root).expect("valid docs");
    fs::write(root.join("docs/nested/guide.md"), "[missing](gone.md)").expect("broken link");
    assert!(docs_check(root).is_err());
    fs::write(root.join("docs/nested/guide.md"), "[gates](../gates.md)").expect("repaired link");
    fs::write(root.join("docs/gates.md"), "stale").expect("drifted docs");
    assert!(docs_check(root).is_err());
}

#[test]
fn impact_accepts_git_base_and_rejects_unknown_ref() {
    impact(Some("HEAD"), Vec::new()).expect("known base");
    assert!(impact(Some("this-ref-does-not-exist"), Vec::new()).is_err());
}
