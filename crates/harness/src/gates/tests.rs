use super::*;
use std::sync::OnceLock;

fn registry() -> Registry {
    Registry::parse(include_bytes!("../../../../gates/registry.json")).expect("registry")
}
fn rules() -> Rules {
    Rules::parse(
        include_bytes!("../../../../gates/ci-rules.json"),
        Some(&registry()),
    )
    .expect("ci rules")
}
/// The real crate graph, without sources: these tests do not exercise the mention rule.
fn graph() -> &'static Graph {
    static GRAPH: OnceLock<Graph> = OnceLock::new();
    GRAPH.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Graph::load(&root, &rules(), false).expect("crate graph")
    })
}
fn classify(paths: &[String]) -> Impact {
    super::classify(&registry(), paths)
}
fn selection(revision: String, base: Option<String>, paths: Vec<String>) -> Selection {
    let pull_request = base.is_some();
    super::selection(
        &registry(),
        &rules(),
        graph(),
        revision,
        base,
        paths,
        pull_request,
    )
    .expect("selection")
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
        "browser":"skipped", "e2e":"skipped",
        "target-performance":"skipped", "fuzz-smoke":"skipped",
        "qualification":"skipped", "exhaustive":"skipped"
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
    fs::write(
        root.join("gates/ci-rules.json"),
        include_bytes!("../../../../gates/ci-rules.json"),
    )
    .expect("fixture ci rules");
    let registry =
        parse(&fs::read(root.join("gates/registry.json")).expect("registry")).expect("parsed");
    fs::write(root.join("docs/gates.md"), table(&registry)).expect("generated docs");
    fs::write(
        root.join("docs/nested/guide.md"),
        "[gates](../gates.md) [web](https://example.com) [heading](#top)",
    )
    .expect("guide");
    fs::write(root.join("gates/flaky.json"), "[]").expect("quarantine");
    docs_check(root).expect("valid docs");
    fs::write(root.join("gates/ci-rules.json"), "{}").expect("broken rules");
    assert!(
        docs_check(root).is_err(),
        "broken ci rules fail the documentation gate"
    );
    fs::write(
        root.join("gates/ci-rules.json"),
        include_bytes!("../../../../gates/ci-rules.json"),
    )
    .expect("repaired rules");
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
        // Every pull-request job: a request with a base is a pull request even
        // when the base cannot be resolved, so only coverage (push and nightly)
        // and the post-merge qualification tier stay out.
        for (job, selected) in &manifest.jobs {
            let after_merge =
                ["native-coverage", "qualification", "exhaustive"].contains(&job.as_str());
            assert_eq!(*selected, !after_merge, "{job}");
        }
        assert!(manifest.gates.contains(&"fuzz-smoke".to_owned()));
    }
}

#[test]
fn impact_accepts_git_base_and_rejects_unknown_ref() {
    impact(Some("HEAD"), Vec::new()).expect("known base");
    assert!(impact(Some("this-ref-does-not-exist"), Vec::new()).is_err());
}

#[test]
fn pull_requests_skip_coverage_and_pushes_keep_it() {
    let code = vec!["crates/map/src/terrain.rs".to_owned()];
    let pull_request = selection("a".repeat(40), Some("b".repeat(40)), code.clone());
    assert_eq!(pull_request.jobs.get("native-coverage"), Some(&false));
    for job in ["static", "browser", "target-performance"] {
        assert_eq!(pull_request.jobs.get(job), Some(&true), "{job}");
    }
    let push = selection("a".repeat(40), None, code);
    assert_eq!(push.jobs.get("native-coverage"), Some(&true));
    assert_eq!(push.jobs.get("browser"), Some(&true));
}

#[test]
fn a_pull_request_whose_base_cannot_be_resolved_still_skips_coverage() {
    // No resolved base and no paths: everything else is selected, as on a push.
    let unresolved = super::selection(
        &registry(),
        &rules(),
        graph(),
        "a".repeat(40),
        None,
        Vec::new(),
        true,
    )
    .expect("selection");
    assert_eq!(unresolved.jobs.get("native-coverage"), Some(&false));
    for job in ["static", "browser", "target-performance", "fuzz-smoke"] {
        assert_eq!(unresolved.jobs.get(job), Some(&true), "{job}");
    }
}

#[test]
fn the_e2e_job_is_selected_exactly_when_test_e2e_is() {
    let registry = registry();
    assert_eq!(registry.jobs["e2e"], ["test-e2e"]);
    for paths in [
        vec!["docs/testing.md".to_owned()],
        vec!["web/index.html".to_owned()],
        vec!["crates/map/src/terrain.rs".to_owned()],
        Vec::new(),
    ] {
        for (base, pull_request) in [(Some("b".repeat(40)), true), (None, false)] {
            let manifest = super::selection(
                &registry,
                &rules(),
                graph(),
                "a".repeat(40),
                base,
                paths.clone(),
                pull_request,
            )
            .expect("selection");
            let e2e = manifest.gates.contains(&"test-e2e".to_owned());
            assert_eq!(manifest.jobs["e2e"], e2e, "{paths:?}");
        }
    }
}

#[test]
fn the_aggregate_requires_the_e2e_job_and_all_its_shards_to_succeed() {
    let selected = selection("a".repeat(40), None, vec!["web/index.html".into()]);
    assert!(selected.jobs["e2e"]);
    let manifest = serde_json::to_string(&selected).expect("manifest");
    let mut results = BTreeMap::from([("select".to_owned(), "success".to_owned())]);
    results.extend(selected.jobs.iter().map(|(job, runs)| {
        let outcome = if *runs { "success" } else { "skipped" };
        (job.clone(), outcome.to_owned())
    }));
    let accepted = |results: &BTreeMap<String, String>| {
        check_selection(
            &manifest,
            &serde_json::to_string(results).expect("results"),
            &selected,
        )
    };
    accepted(&results).expect("every shard passed");
    for outcome in ["failure", "cancelled", "skipped"] {
        let mut broken = results.clone();
        broken.insert("e2e".to_owned(), outcome.to_owned());
        assert!(accepted(&broken).is_err(), "e2e {outcome}");
    }
    let mut missing = results.clone();
    missing.remove("e2e");
    assert!(accepted(&missing).is_err());
    let docs = selection(
        "a".repeat(40),
        Some("b".repeat(40)),
        vec!["docs/testing.md".into()],
    );
    assert!(!docs.jobs["e2e"]);
}

#[test]
fn a_failed_select_job_is_named_not_reported_as_a_parse_error() {
    let expected = selection("a".repeat(40), None, Vec::new());
    for result in ["failure", "cancelled"] {
        let results = serde_json::json!({"select": result}).to_string();
        // A failed select publishes no manifest: an empty string, as in run 39346082654.
        let error = check_selection("", &results, &expected)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(&format!("select job ended `{result}`")),
            "{error}"
        );
        assert!(!error.contains("EOF while parsing"), "{error}");
    }
}

fn gate_members(registry: &Registry, selection: &Selection) -> BTreeSet<String> {
    registry
        .jobs
        .iter()
        .filter(|(job, _)| selection.jobs.get(*job) == Some(&true))
        .flat_map(|(_, members)| members.iter().cloned())
        .collect()
}

#[test]
fn the_manifest_lists_only_gates_that_a_selected_job_executes() {
    let registry = registry();
    for paths in [
        vec!["docs/testing.md"],
        vec!["ai/aoe_ai/client.py"],
        vec!["crates/client/src/lib.rs"],
        vec!["crates/sdk-python/src/lib.rs"],
        vec!["deny.toml"],
        vec!["Cargo.lock"],
        Vec::new(),
    ] {
        let paths: Vec<String> = paths.into_iter().map(str::to_owned).collect();
        let manifest = selection("a".repeat(40), Some("b".repeat(40)), paths.clone());
        let members = gate_members(&registry, &manifest);
        assert!(
            manifest.gates.iter().all(|gate| members.contains(gate)),
            "{paths:?}"
        );
        let unique: BTreeSet<_> = manifest.gates.iter().collect();
        assert_eq!(unique.len(), manifest.gates.len(), "{paths:?}");
    }
}

#[test]
fn a_change_without_rust_code_lists_only_the_checks_that_compile_nothing() {
    let docs = selection(
        "a".repeat(40),
        Some("b".repeat(40)),
        vec!["docs/guide-without-readers.md".into()],
    );
    assert_eq!(
        docs.gates,
        [
            "architecture-check",
            "docs-check",
            "fmt-check",
            "structure-check"
        ]
    );
    assert_eq!(docs.static_parts, ["light"]);
    let client = selection(
        "a".repeat(40),
        Some("b".repeat(40)),
        vec!["crates/client/src/lib.rs".into()],
    );
    for gate in [
        "lint",
        "deny",
        "build-wasm",
        "test-unit",
        "browser-check",
        "test-wasm",
        "test-e2e",
        "perf-ci",
        "perf-instructions",
        "perf-wasm-size",
    ] {
        assert!(client.gates.contains(&gate.to_owned()), "{gate}");
    }
    assert!(!client.gates.contains(&"perf-smoke".to_owned()));
    assert!(!client.gates.contains(&"fuzz-smoke".to_owned()));
    let position = |gate: &str| client.gates.iter().position(|g| g == gate);
    assert!(
        position("lint") < position("test-unit"),
        "{:?}",
        client.gates
    );
    assert!(
        position("test-unit") < position("test-wasm"),
        "{:?}",
        client.gates
    );
}

#[test]
fn the_manifest_names_every_job_with_its_reason_and_rejects_a_forged_one() {
    let expected = selection(
        "a".repeat(40),
        Some("b".repeat(40)),
        vec!["docs/guide.md".into()],
    );
    assert_eq!(
        expected.jobs.keys().collect::<Vec<_>>(),
        expected.reasons.keys().collect::<Vec<_>>()
    );
    let mut forged = serde_json::to_value(&expected).expect("manifest");
    forged["reasons"]["e2e"] = serde_json::json!("runs because someone said so");
    let results: BTreeMap<_, _> = std::iter::once(("select".to_owned(), "success"))
        .chain(
            expected
                .jobs
                .iter()
                .map(|(job, runs)| (job.clone(), if *runs { "success" } else { "skipped" })),
        )
        .collect();
    let results = serde_json::to_string(&results).expect("results");
    let genuine = serde_json::to_string(&expected).expect("manifest");
    check_selection(&genuine, &results, &expected).expect("the genuine manifest");
    assert!(check_selection(&forged.to_string(), &results, &expected).is_err());
}

#[test]
fn explain_prints_one_line_per_job_and_the_summary_is_a_table() {
    let manifest = selection(
        "a".repeat(40),
        Some("b".repeat(40)),
        vec!["crates/geodata/src/lib.rs".into()],
    );
    let text = explain(&manifest, false);
    for job in manifest.jobs.keys() {
        assert!(
            text.lines()
                .any(|line| line.starts_with(&format!("{job}: "))),
            "{job}: {text}"
        );
    }
    assert!(text.contains("test-unit: 1 of"), "{text}");
    let markdown = explain(&manifest, true);
    assert!(markdown.starts_with("## CI selection"));
    assert!(markdown.contains("| `e2e` | skip:"), "{markdown}");
    assert!(markdown.contains("| `static` | **run**:"), "{markdown}");
}
