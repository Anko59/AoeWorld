use super::*;
use serde_json::{Value, json};
mod job_mapping;

fn proposal() -> Value {
    serde_json::from_slice(include_bytes!("../../../../../gates/registry.json"))
        .expect("draft JSON")
}

fn parse(value: &Value) -> Result<Registry> {
    Registry::parse(&serde_json::to_vec(value).expect("encode fixture"))
}

fn registry() -> Registry {
    parse(&proposal()).expect("valid v2 control")
}

fn paths(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| (*s).into()).collect()
}

fn suite_set(names: &[&str]) -> BTreeSet<String> {
    paths(names).into_iter().collect()
}

fn gate_mut<'a>(value: &'a mut Value, id: &str) -> &'a mut Value {
    value["gates"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|gate| gate["id"] == id)
        .unwrap()
}

#[test]
fn valid_proposal_roundtrips_and_keeps_entire_v1_catalog() {
    let registry = registry();
    Registry::parse(&serde_json::to_vec(&registry).unwrap()).expect("strict roundtrip");
    // Draft-only characterization fixture; integration should preserve a v1
    // fixture in tests instead of pointing this assertion at the migrated file.
    let original: Value = serde_json::from_slice(include_bytes!(
        "../../../../../gates/tests/registry-v1.json"
    ))
    .unwrap();
    for old in original["gates"].as_array().unwrap() {
        let gate = registry
            .gates
            .iter()
            .find(|g| g.id == old["id"].as_str().unwrap())
            .unwrap();
        assert_eq!(gate.command, old["command"].as_str().unwrap());
        assert_eq!(gate.select, old["select"].as_str().unwrap());
        assert_eq!(gate.evidence, old["evidence"].as_str().unwrap());
        for dep in old["requires"].as_array().unwrap() {
            assert!(gate.requires.contains(&dep.as_str().unwrap().to_owned()));
        }
    }
    assert_eq!(
        registry.jobs.keys().cloned().collect::<BTreeSet<_>>(),
        suite_set(&[
            "static",
            "native-coverage",
            "browser",
            "target-performance",
            "fuzz-smoke"
        ])
    );
}

#[test]
fn load_is_root_relative_and_missing_or_v1_registry_fails() {
    let temp = tempfile::tempdir().unwrap();
    assert!(Registry::load(temp.path()).is_err());
    fs::create_dir(temp.path().join("gates")).unwrap();
    fs::write(
        temp.path().join("gates/registry.json"),
        serde_json::to_vec(&proposal()).unwrap(),
    )
    .unwrap();
    assert_eq!(Registry::load(temp.path()).unwrap().version, 2);
    let mut value = proposal();
    value["version"] = json!(1);
    assert!(parse(&value).is_err());
}

#[test]
fn serde_rejects_unknown_fields_and_enum_values_at_every_schema_layer() {
    for location in ["root", "suite", "gate"] {
        let mut value = proposal();
        match location {
            "root" => {
                value["surprise"] = json!(true);
            }
            "suite" => {
                value["suites"][0]["surprise"] = json!(true);
            }
            _ => {
                value["gates"][0]["surprise"] = json!(true);
            }
        }
        assert!(
            parse(&value)
                .unwrap_err()
                .to_string()
                .contains("unknown field")
        );
    }
    for (field, invalid) in [
        ("cadences", "publish"),
        ("blocks", "merge"),
        ("capabilities", "network"),
    ] {
        let mut value = proposal();
        value["gates"][0][field] = json!([invalid]);
        assert!(parse(&value).is_err(), "{field}");
    }
    let mut value = proposal();
    value["gates"][0]
        .as_object_mut()
        .unwrap()
        .remove("budget_s");
    assert!(parse(&value).is_err());
    value = proposal();
    value["gates"][0]["budget_s"] = json!(-1);
    assert!(parse(&value).is_err());
}

#[test]
fn aggregate_semantic_errors_not_first_failure_only() {
    let mut value = proposal();
    gate_mut(&mut value, "fmt-check")["command"] = json!("make fmt-check; true");
    gate_mut(&mut value, "lint")["requires"] = json!(["missing-gate"]);
    gate_mut(&mut value, "deny")["suites"] = json!(["missing-suite"]);
    gate_mut(&mut value, "deny")["budget_s"] = json!(0);
    value["jobs"]["browser"] = json!(["missing-job-gate"]);
    let error = parse(&value).unwrap_err().to_string();
    for expected in [
        "command mismatch",
        "missing-gate",
        "missing-suite",
        "zero budget",
        "missing-job-gate",
    ] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn cycles_self_references_and_duplicate_ids_or_members_fail() {
    let mut value = proposal();
    gate_mut(&mut value, "fmt-check")["requires"] = json!(["lint"]);
    assert!(parse(&value).unwrap_err().to_string().contains("cyclic"));
    value = proposal();
    value["suites"][1]["implies"] = json!(["docs"]); // docs -> static -> docs
    assert!(parse(&value).unwrap_err().to_string().contains("cyclic"));
    value = proposal();
    gate_mut(&mut value, "lint")["requires"] = json!(["lint"]);
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("self dependency")
    );
    value = proposal();
    let duplicate = value["gates"][0].clone();
    value["gates"].as_array_mut().unwrap().push(duplicate);
    assert!(parse(&value).is_err());
    value = proposal();
    value["suites"][2]["implies"] = json!(["static", "static"]);
    assert!(parse(&value).is_err());
    value = proposal();
    value["suites"][2]["implies"] = json!(["absent"]);
    assert!(parse(&value).is_err());
}

#[test]
fn everything_is_mandatory_and_must_cover_every_suite() {
    let mut value = proposal();
    value["suites"].as_array_mut().unwrap().remove(0);
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("missing mandatory suite everything")
    );
    value = proposal();
    value["suites"][0]["implies"] = json!(["static", "docs"]);
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("everything must imply")
    );
}

#[test]
fn docs_do_not_select_code_via_static_flag_or_dependency_broadening() {
    let registry = registry();
    let impact = registry.classify(&paths(&["docs/testing.md", "README.md"]));
    assert_eq!(impact.suites, suite_set(&["docs", "static"]));
    let plan = registry.plan(Cadence::Pr, &impact.suites).unwrap();
    assert_eq!(
        plan.gates,
        paths(&["docs-check", "fmt-check", "structure-check"])
    );
    assert!(plan.jobs["static"]);
    for name in [
        "native-coverage",
        "browser",
        "target-performance",
        "fuzz-smoke",
    ] {
        assert!(!plan.jobs[name]);
    }
    assert!(!plan.gates.contains(&"release-source-check".into()));
}

#[test]
fn protected_instructions_precede_docs_even_if_proposal_omits_pattern() {
    let mut value = proposal();
    value["suites"][0]["paths"] = json!([]);
    let registry = parse(&value).unwrap();
    for path in [
        "docs/agent-engineering.md",
        "AGENTS.md",
        "docs/nested/AGENTS.md",
    ] {
        let impact = registry.classify(&paths(&[path]));
        assert!(impact.suites.contains("everything"));
        assert!(
            impact.reasons["everything"]
                .iter()
                .any(|r| r.starts_with("protected:"))
        );
    }
}

#[test]
fn unknown_invalid_empty_map_and_policy_paths_select_everything() {
    let registry = registry();
    for path in [
        "unknown/guide.md",
        "Cargo.lock",
        "Makefile",
        "crates/map/src/lib.rs",
        "crates/geodata/src/lib.rs",
        "gates/registry.json",
        "crates/harness/src/gates.rs",
        "skills/testing/SKILL.md",
        "../docs/testing.md",
        "/docs/testing.md",
        "docs//a.md",
    ] {
        assert!(
            registry
                .classify(&paths(&[path]))
                .suites
                .contains("everything"),
            "{path}"
        );
    }
    assert!(registry.classify(&[]).suites.contains("everything"));
}

#[test]
fn rename_union_keeps_old_code_when_new_name_is_docs_and_reverse() {
    let registry = registry();
    // These are both sides emitted by PR1 --no-renames path extraction.
    let old_new = paths(&["crates/simulation/src/state.rs", "docs/state.md"]);
    let mut new_old = old_new.clone();
    new_old.reverse();
    let impact = registry.classify(&old_new);
    assert_eq!(impact, registry.classify(&new_old));
    for suite in ["native", "browser", "performance", "docs"] {
        assert!(impact.suites.contains(suite));
    }
    assert!(!impact.suites.contains("everything"));
}

#[test]
fn suite_transitivity_and_registry_globs_are_executable() {
    let mut value = proposal();
    value["suites"][2]["paths"] = json!(["guide/**/note?.md"]);
    value["suites"][2]["implies"] = json!(["assets"]);
    let registry = parse(&value).unwrap();
    let impact = registry.classify(&paths(&["guide/deep/nested/note雪.md"]));
    assert!(!impact.suites.contains("everything"));
    for suite in [
        "docs",
        "assets",
        "browser",
        "native",
        "performance",
        "static",
    ] {
        assert!(impact.suites.contains(suite));
    }
    assert!(
        impact.reasons["performance"]
            .iter()
            .any(|r| r.starts_with("implied by"))
    );
}

#[test]
fn segment_glob_controls_zero_segments_unicode_and_boundaries() {
    for (pattern, path, expected) in [
        ("docs/*", "docs/a.md", true),
        ("docs/*", "docs/x/a.md", false),
        ("docs/**", "docs/x/a.md", true),
        ("**/AGENTS.md", "AGENTS.md", true),
        ("**/AGENTS.md", "crates/x/AGENTS.md", true),
        ("docs/**/a?.md", "docs/a雪.md", true),
        ("docs/**/a?.md", "docs/x/a雪.md", true),
        ("docs/**/a?.md", "docs/ab雪.md", false),
        ("docs/a*.md", "docs/a.md", true),
        ("docs/a*.md", "docs/A.md", false),
        ("docs/**/a.md", "docs/../a.md", false),
        ("docs/a**.md", "docs/abc.md", false),
    ] {
        assert_eq!(glob(pattern, path), expected, "{pattern} {path}");
    }
    let mut value = proposal();
    value["suites"][2]["paths"] = json!(["docs/[ab].md"]);
    assert!(parse(&value).is_err());
}

#[test]
fn dependency_first_plan_is_deterministic_and_suites_do_not_expand_from_dependencies() {
    let mut value = proposal();
    gate_mut(&mut value, "docs-check")["requires"] = json!(["deny"]);
    let registry = parse(&value).unwrap();
    let impact = registry.classify(&paths(&["docs/testing.md"]));
    let plan = registry.plan(Cadence::Pr, &impact.suites).unwrap();
    assert!(plan.gates.contains(&"deny".into()));
    assert_eq!(plan.suites, impact.suites);
    assert!(!plan.gates.contains(&"test-unit".into()));
    for gate in &plan.gates {
        let spec = registry.gates.iter().find(|g| &g.id == gate).unwrap();
        for dep in &spec.requires {
            assert!(
                plan.gates.iter().position(|g| g == dep)
                    < plan.gates.iter().position(|g| g == gate)
            );
        }
    }
    let mut reordered = registry.clone();
    reordered.gates.reverse();
    reordered.suites.reverse();
    assert_eq!(plan, reordered.plan(Cadence::Pr, &impact.suites).unwrap());
    assert!(
        registry
            .plan(Cadence::Pr, &suite_set(&["missing"]))
            .is_err()
    );
}

#[test]
fn preflight_remains_full_for_docs_and_source_checks_do_not_leak_into_pr() {
    let registry = registry();
    let docs = registry.classify(&paths(&["docs/testing.md"]));
    let plan = registry.plan(Cadence::Preflight, &docs.suites).unwrap();
    for gate in [
        "fmt-check",
        "structure-check",
        "architecture-check",
        "docs-check",
        "lint",
        "deny",
        "build-wasm",
        "test-unit",
        "perf-smoke",
    ] {
        assert!(plan.gates.contains(&gate.into()), "{gate}");
    }
    let everything = registry.classify(&[]);
    let pr = registry.plan(Cadence::Pr, &everything.suites).unwrap();
    assert!(!pr.gates.iter().any(|g| g.starts_with("release-")));
    assert!(!pr.gates.contains(&"test-creator-source".into()));
    for gate in ["perf-instructions", "perf-wasm-size", "build-wasm"] {
        assert!(pr.gates.contains(&gate.into()));
    }
    let qualification = registry
        .plan(Cadence::Qualification, &everything.suites)
        .unwrap();
    assert!(qualification.gates.contains(&"test-creator-source".into()));
    assert!(qualification.gates.contains(&"perf-hardware-check".into()));
}

#[test]
fn unsafe_ids_duplicate_jobs_and_regex_like_globs_fail() {
    for id in ["-lint", "lint;true", "lint x", "LINT", "lïnt", "lint/other"] {
        let mut value = proposal();
        gate_mut(&mut value, "lint")["id"] = json!(id);
        assert!(parse(&value).is_err(), "{id}");
        value = proposal();
        value["suites"][2]["id"] = json!(id);
        assert!(parse(&value).is_err(), "{id}");
        value = proposal();
        value["jobs"][id] = json!(["lint"]);
        assert!(parse(&value).is_err(), "{id}");
    }
    for pattern in [
        "docs/(a|b).md",
        "^docs/**",
        "docs/a+.md",
        "docs/**x",
        "docs/../*",
    ] {
        let mut value = proposal();
        value["suites"][2]["paths"] = json!([pattern]);
        assert!(
            parse(&value)
                .unwrap_err()
                .to_string()
                .contains("invalid glob")
        );
    }
    let raw = serde_json::to_string(&proposal()).unwrap();
    let duplicate = raw.replacen("\"jobs\":{", "\"jobs\":{\"static\":[\"fmt-check\"],", 1);
    assert!(
        Registry::parse(duplicate.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("duplicate job")
    );
}

#[test]
fn new_ci_gates_require_a_job_and_privileged_roots_are_rejected() {
    let mut value = proposal();
    let mut extra = gate_mut(&mut value, "fmt-check").clone();
    extra["id"] = json!("extra-check");
    extra["command"] = json!("make extra-check");
    extra["cadences"] = json!(["ci"]);
    extra["blocks"] = json!(["ci"]);
    value["gates"].as_array_mut().unwrap().push(extra);
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("no job coverage")
    );
    value["jobs"]["static"]
        .as_array_mut()
        .unwrap()
        .push(json!("extra-check"));
    parse(&value).expect("job covers new gate");
    value["jobs"]["static"]
        .as_array_mut()
        .unwrap()
        .push(json!("release-publish"));
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("not a CI gate")
    );
}

#[test]
fn publication_cannot_be_reached_by_generic_cadence_or_hidden_dependency() {
    let mut value = proposal();
    gate_mut(&mut value, "release-publish")["cadences"] = json!(["pr"]);
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("privileged dispatch")
    );
    value = proposal();
    gate_mut(&mut value, "docs-check")["requires"] = json!(["release-publish"]);
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("reaches privileged")
    );
}
