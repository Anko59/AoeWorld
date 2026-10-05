use super::*;

fn registry() -> Registry {
    Registry::parse(include_bytes!("../../../../gates/registry.json")).expect("registry")
}
fn classify(paths: &[String]) -> Impact {
    super::classify(&registry(), paths)
}
fn selection(revision: String, base: Option<String>, paths: Vec<String>) -> Selection {
    super::selection(&registry(), revision, base, paths).expect("selection")
}

#[test]
fn unknown_and_shared_paths_select_relevant_suites() {
    let static_only = BTreeSet::from(["static".into(), "docs".into()]);
    for path in ["docs/testing.md", "README.md", "LICENSE", "THIRD_PARTY.md"] {
        assert_eq!(classify(&[path.into()]).suites, static_only, "{path}");
    }
    assert_eq!(
        classify(&["Cargo.lock".into()]).suites.len(),
        registry().suites.len()
    );
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
        let mut expected = runtime.clone();
        if path.starts_with("crates/")
            && !path.starts_with("crates/client/")
            && !path.starts_with("crates/rendering/")
        {
            expected.insert("gameplay".into());
        }
        assert_eq!(classify(&[path.into()]).suites, expected, "{path}");
    }
    assert_eq!(classify(&[]).suites.len(), registry().suites.len());
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
fn documentation_gate_rejects_drift_and_missing_local_links() {
    let temp = tempfile::tempdir().expect("directory");
    let root = temp.path();
    fs::create_dir_all(root.join("gates")).expect("gates");
    fs::create_dir_all(root.join("docs/nested")).expect("docs");
    fs::create_dir_all(root.join("crates/example")).expect("crates");
    fs::write(
        root.join("gates/registry.json"),
        include_bytes!("../../../../gates/registry.json"),
    )
    .expect("fixture registry");
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
fn unknown_markdown_and_protected_instructions_select_everything() {
    for path in [
        "unknown/notes.md",
        "AGENTS.md",
        "crates/map/AGENTS.md",
        "skills/harness-ci/SKILL.md",
        "crates/harness/notes.md",
        ".github/instructions.md",
    ] {
        assert_eq!(
            classify(&[path.into()]).suites.len(),
            registry().suites.len(),
            "{path}"
        );
    }
}

#[test]
fn selection_manifest_rejects_unknown_fields() {
    let expected = selection("a".repeat(40), None, Vec::new());
    let mut manifest = serde_json::to_value(&expected).expect("manifest");
    let mut results = BTreeMap::from([("select".to_owned(), "success")]);
    results.extend(expected.jobs.keys().map(|job| (job.clone(), "success")));
    let results = serde_json::to_string(&results).expect("complete results");
    check_selection(&manifest.to_string(), &results, &expected).expect("valid control");
    manifest["forged"] = serde_json::json!(true);
    assert!(check_selection(&manifest.to_string(), &results, &expected).is_err());
}

#[test]
fn unavailable_ci_base_selects_all_and_retains_requested_identity() {
    for reference in ["missing-harness-comparison-base", "--help"] {
        let manifest = current_selection(Some(reference)).expect("conservative selection");
        assert_eq!(manifest.base, None);
        assert_eq!(manifest.requested_base.as_deref(), Some(reference));
        assert!(manifest.jobs.values().all(|selected| *selected));
        assert!(manifest.gates.contains(&"fuzz-smoke".to_owned()));
    }
}

#[test]
fn impact_accepts_git_base_and_rejects_unknown_ref() {
    impact(Some("HEAD"), Vec::new()).expect("known base");
    assert!(impact(Some("this-ref-does-not-exist"), Vec::new()).is_err());
}

fn selected_job_members(registry: &Registry, selection: &Selection) -> BTreeSet<String> {
    registry
        .jobs
        .iter()
        .filter(|(job, _)| selection.jobs.get(*job) == Some(&true))
        .flat_map(|(_, members)| members.iter().cloned())
        .collect()
}

#[test]
fn ci_selection_rejects_unexecuted_nightly_and_weekly_dependencies() {
    let original = registry();
    let docs = vec!["docs/testing.md".to_owned()];
    let control = super::selection(&original, "a".repeat(40), None, docs.clone())
        .expect("normal docs selection");
    assert!(control.jobs["static"]);
    assert!(!control.jobs["target-performance"]);
    let executed = selected_job_members(&original, &control);
    assert!(control.gates.iter().all(|gate| executed.contains(gate)));
    for dependency in ["perf-full", "perf-stress"] {
        let mut changed = original.clone();
        changed
            .gates
            .iter_mut()
            .find(|gate| gate.id == "docs-check")
            .expect("docs gate")
            .requires = vec![dependency.into()];
        let suites = changed.classify(&docs).suites;
        let plan = changed
            .plan(Cadence::Ci, &suites)
            .expect("valid dependency graph");
        assert!(plan.gates.contains(&dependency.to_owned()));
        let error = super::selection(&changed, "a".repeat(40), None, docs.clone())
            .expect_err("unexecuted prerequisite must not be promised")
            .to_string();
        assert!(error.contains("without selected executing jobs"), "{error}");
        assert!(error.contains(dependency), "{error}");
    }
}

#[test]
fn ci_selection_execution_coverage_is_the_union_of_selected_jobs() {
    let mut changed = registry();
    changed
        .gates
        .iter_mut()
        .find(|gate| gate.id == "docs-check")
        .expect("docs gate")
        .requires = vec!["perf-instructions".into(), "browser-check".into()];
    let docs = vec!["docs/testing.md".to_owned()];
    let selected = super::selection(&changed, "a".repeat(40), None, docs.clone())
        .expect("covered prerequisites select their executing jobs");
    for job in ["static", "browser", "target-performance"] {
        assert!(selected.jobs[job], "{job}");
    }
    let executed = selected_job_members(&changed, &selected);
    assert!(selected.gates.iter().all(|gate| executed.contains(gate)));
    let plan = changed
        .plan(Cadence::Ci, &changed.classify(&docs).suites)
        .unwrap();
    assert_eq!(
        selected.gates, plan.gates,
        "coverage validation must not drop gates"
    );
    assert_eq!(selected.jobs, plan.jobs);
    for path in [
        None,
        Some("docs/testing.md"),
        Some("crates/assets/src/slp.rs"),
        Some("web/index.html"),
        Some("unknown/file"),
        Some("AGENTS.md"),
    ] {
        let paths = path.into_iter().map(str::to_owned).collect();
        let selected = super::selection(&changed, "a".repeat(40), None, paths)
            .expect("all selected gates have executing coverage");
        let executed = selected_job_members(&changed, &selected);
        assert!(
            selected.gates.iter().all(|gate| executed.contains(gate)),
            "{path:?}"
        );
    }
}
