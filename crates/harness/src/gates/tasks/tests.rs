use super::*;
mod context;
use task::{Artifact, ArtifactKind, Status};
fn registry() -> Registry {
    Registry::parse(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../gates/registry.json"
    )))
    .unwrap()
}
fn task() -> Task {
    Task::parse(&serde_json::to_vec(&serde_json::json!({
        "version":1,"id":"movement-fix","kind":"bug","candidate":"a".repeat(40),"base":"b".repeat(40),
        "registry_hash":registry().fingerprint().unwrap(),"role":"tester","provider":"deep-seek-harness","status":"testing",
        "objective":"Retain deterministic movement after cancellation", "acceptance":["Replay fixture fails before fix and passes after"],
        "todo":["Add external regression", "Run mandatory preflight"], "artifacts":[],"rounds_remaining":8
    })).unwrap()).unwrap()
}
fn catalog() -> Catalog {
    Catalog::parse(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../gates/roles.json"
    )))
    .unwrap()
}
#[test]
fn positive_plan_binds_exact_task_registry_catalog_and_full_mandatory_preflight() {
    let reg = registry();
    let roles = catalog();
    let output = plan(
        task(),
        &reg,
        &roles,
        &["crates/simulation/src/lib.rs".into()],
        &[],
        adapters::Observation::Unavailable("Codex/pi absent; DSH hooks unmeasured".into()),
    )
    .unwrap();
    assert!(!output.authoritative);
    assert_eq!(output.task.candidate, "a".repeat(40));
    assert_eq!(output.registry_hash, reg.fingerprint().unwrap());
    assert_eq!(output.catalog_hash, roles.fingerprint().unwrap());
    assert_eq!(output.handoff.cadence, Cadence::Preflight);
    assert_eq!(
        output.handoff.gates,
        reg.plan(Cadence::Preflight, &BTreeSet::from(["everything".into()]))
            .unwrap()
            .gates
    );
    assert_eq!(output.fixture_namespace, "task-fixtures/movement-fix");
    assert_eq!(output.integrity.disposition, "REVIEW_REQUIRED");
    assert!(output.integrity.independent_review_required);
    assert!(serde_json::to_vec(&output.context).unwrap().len() <= 16384);
    assert!(!output.semantic_handoffs.is_empty());
}
#[test]
fn closed_roles_never_subtract_pr_or_preflight_checks() {
    let reg = registry();
    let roles = catalog();
    let paths = vec!["crates/protocol/src/lib.rs".into()];
    let suites = reg.classify(&paths).suites;
    let expected = reg.plan(Cadence::Pr, &suites).unwrap().gates;
    for (role, status) in [
        (Role::Coordinator, Status::Planned),
        (Role::Implementer, Status::Implementing),
        (Role::Maintainer, Status::Maintaining),
        (Role::Tester, Status::Testing),
        (Role::Reviewer, Status::Reviewing),
        (Role::Qa, Status::Qa),
    ] {
        let mut task = task();
        task.role = role;
        task.status = status;
        let result = plan(
            task,
            &reg,
            &roles,
            &paths,
            &[],
            adapters::Observation::Unavailable("not installed".into()),
        )
        .unwrap();
        assert_eq!(result.required.gates, expected);
        assert_eq!(
            result.handoff.gates,
            reg.plan(Cadence::Preflight, &BTreeSet::from(["everything".into()]))
                .unwrap()
                .gates
        );
    }
}
#[test]
fn ids_oids_hash_namespace_controls_and_role_pairs_do_not_accept_shell_or_stale_task() {
    let hash = registry().fingerprint().unwrap();
    for id in ["../escape", "foo;publish", "Main", "", "foo/bar"] {
        let mut value = task();
        value.id = id.into();
        assert!(value.validate(&hash).is_err());
    }
    for oid in [
        "HEAD",
        "--upload-pack=x",
        "aaaa",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        let mut value = task();
        value.candidate = oid.into();
        assert!(value.validate(&hash).is_err());
    }
    let mut value = task();
    value.candidate = "a".repeat(64);
    assert!(value.validate(&hash).is_ok());
    let mut value = task();
    value.registry_hash = "different".into();
    assert!(value.validate(&hash).is_err());
    let mut value = task();
    value.role = Role::Reviewer;
    assert!(value.validate(&hash).is_err());
    for path in [
        "task-artifacts/other/review.json",
        "task-artifacts/movement-fix/../escape",
        "task-artifacts/movement-fix/.git/config",
        "task-artifacts/movement-fix/c:\\secret",
    ] {
        let mut value = task();
        value.artifacts.push(Artifact {
            kind: ArtifactKind::Review,
            path: path.into(),
            blake3: None,
        });
        assert!(value.validate(&hash).is_err());
    }
    let mut value = task();
    value.objective = "inject\0tool".into();
    assert!(value.validate(&hash).is_err());
    let mut value = task();
    value.artifacts.push(Artifact {
        kind: ArtifactKind::TaskState,
        path: "task-artifacts/movement-fix/state.json".into(),
        blake3: Some("a".repeat(64)),
    });
    assert!(value.validate(&hash).is_ok());
    value.artifacts.push(value.artifacts[0].clone());
    assert!(value.validate(&hash).is_err());
}
#[test]
fn strict_task_parser_rejects_unknown_fields_providers_statuses_kinds_and_structural_bounds() {
    let bytes = serde_json::to_vec(&task()).unwrap();
    for (field, bad) in [
        ("trusted", "true"),
        ("role", "publisher"),
        ("role", "merger"),
        ("role", "root"),
        ("provider", "claude"),
        ("kind", "shell"),
        ("status", "approved"),
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value[field] = bad.into();
        assert!(Task::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["candidate"] = "HEAD".into();
    assert!(Task::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(Task::parse(&vec![b' '; 32769]).is_err());
    assert!(Task::parse(b"\xff").is_err());
    let hash = registry().fingerprint().unwrap();
    let mut exhausted = task();
    exhausted.rounds_remaining = 0;
    assert!(exhausted.validate(&hash).is_err());
    exhausted.status = Status::Blocked;
    assert!(exhausted.validate(&hash).is_ok());
    exhausted.rounds_remaining = 257;
    assert!(exhausted.validate(&hash).is_err());
    let mut oversized = task();
    oversized.objective = "x".repeat(2049);
    assert!(oversized.validate(&hash).is_err());
    oversized = task();
    oversized.acceptance = vec!["x".repeat(513)];
    assert!(oversized.validate(&hash).is_err());
    oversized = task();
    oversized.todo.clear();
    assert!(oversized.validate(&hash).is_err());
    oversized = task();
    oversized.acceptance = vec!["valid".into(); 17];
    assert!(oversized.validate(&hash).is_err());
}
#[test]
fn semantic_transitions_reject_inappropriate_skips_and_exhaustion_without_claiming_proof() {
    let mut value = task();
    value.status = Status::Planned;
    value.role = Role::Coordinator;
    assert!(
        value
            .transition(Role::Coordinator, Status::ReadyForHuman)
            .is_err()
    );
    assert!(
        value
            .transition(Role::Reviewer, Status::Implementing)
            .is_err()
    );
    value
        .transition(Role::Implementer, Status::Implementing)
        .unwrap();
    value.transition(Role::Tester, Status::Testing).unwrap();
    value.transition(Role::Reviewer, Status::Reviewing).unwrap();
    value.transition(Role::Qa, Status::Qa).unwrap();
    value
        .transition(Role::Coordinator, Status::ReadyForHuman)
        .unwrap();
    assert_eq!(value.role, Role::Coordinator);
    assert_eq!(value.rounds_remaining, 8);
    value.status = Status::Blocked;
    value.rounds_remaining = 0;
    assert!(
        value
            .transition(Role::Coordinator, Status::Planned)
            .is_err()
    );
    value
        .transition(Role::Coordinator, Status::Blocked)
        .unwrap();
    value.rounds_remaining = 1;
    value
        .transition(Role::Coordinator, Status::Planned)
        .unwrap();
    value
        .transition(Role::Maintainer, Status::Maintaining)
        .unwrap();
    value.transition(Role::Tester, Status::Testing).unwrap();
}
#[test]
fn catalog_hash_normalizes_set_order_but_binds_guidance_and_rejects_widening() {
    let original = catalog();
    let hash = original.fingerprint().unwrap();
    let mut json = serde_json::to_value(&original).unwrap();
    json["roles"].as_array_mut().unwrap().reverse();
    for role in json["roles"].as_array_mut().unwrap() {
        role["guides"].as_array_mut().unwrap().reverse();
    }
    assert_eq!(
        Catalog::parse(&serde_json::to_vec(&json).unwrap())
            .unwrap()
            .fingerprint()
            .unwrap(),
        hash
    );
    let mut changed = serde_json::to_value(&original).unwrap();
    changed["roles"][0]["guides"] = serde_json::json!(["docs/testing.md"]);
    assert_ne!(
        Catalog::parse(&serde_json::to_vec(&changed).unwrap())
            .unwrap()
            .fingerprint()
            .unwrap(),
        hash
    );
    for bad in [
        serde_json::json!(["publish"]),
        serde_json::json!(["plan", "maintain"]),
    ] {
        let mut changed = serde_json::to_value(&original).unwrap();
        changed["roles"][0]["activities"] = bad;
        assert!(Catalog::parse(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    let mut changed = serde_json::to_value(&original).unwrap();
    changed["roles"][1] = changed["roles"][0].clone();
    assert!(Catalog::parse(&serde_json::to_vec(&changed).unwrap()).is_err());
    let mut changed = serde_json::to_value(&original).unwrap();
    changed["commands"] = serde_json::json!(["shell"]);
    assert!(Catalog::parse(&serde_json::to_vec(&changed).unwrap()).is_err());
    let mut changed = serde_json::to_value(&original).unwrap();
    changed["roles"][0]["guides"] = serde_json::json!(["../escape"]);
    assert!(Catalog::parse(&serde_json::to_vec(&changed).unwrap()).is_err());
    assert!(Catalog::parse(&vec![b' '; 16385]).is_err());
}
#[test]
fn adapters_report_unavailable_without_claiming_invocation_isolation_or_identity() {
    for provider in [
        adapters::Provider::Codex,
        adapters::Provider::PiDev,
        adapters::Provider::DeepSeekHarness,
    ] {
        let descriptor = adapters::describe(
            provider,
            adapters::Observation::Unavailable(
                "task planning never launches provider binaries".into(),
            ),
        )
        .unwrap();
        let value = serde_json::to_value(descriptor).unwrap();
        assert_eq!(value["availability"], "UNAVAILABLE");
        assert!(value["version"].is_null());
        assert!(!value.as_object().unwrap().contains_key("invocation"));
        assert_eq!(value["authoritative_role_identity"], false);
        assert!(
            value["pre_tool_interception"]
                .as_str()
                .unwrap()
                .starts_with("UNAVAILABLE")
        );
        assert!(
            value["filesystem_isolation"]
                .as_str()
                .unwrap()
                .starts_with("UNAVAILABLE")
        );
    }
    assert!(
        adapters::describe(
            adapters::Provider::Codex,
            adapters::Observation::Unavailable("x".repeat(2049))
        )
        .is_err()
    );
}
#[test]
fn no_heuristics_is_never_test_integrity_pass_and_suspicious_additions_removals_require_review() {
    let reg = registry();
    let finding = integrity::inspect(&reg, &["docs/testing.md".into()], &[]).unwrap();
    assert_eq!(finding.disposition, "REVIEW_NOT_ASSESSED");
    assert!(!finding.authoritative);
    assert!(finding.independent_review_required);
    let hunk = integrity::Hunk {
        path: "docs/testing.md".into(),
        removed: vec!["assert_eq!(old, new);".into()],
        added: vec!["#[ignore]".into()],
    };
    let finding = integrity::inspect(
        &reg,
        std::slice::from_ref(&hunk.path),
        std::slice::from_ref(&hunk),
    )
    .unwrap();
    assert_eq!(finding.disposition, "REVIEW_REQUIRED");
    assert!(finding.reasons.len() >= 2);
}
#[test]
fn numeric_campaign_counter_decreases_deleted_tests_and_ownership_trigger_independent_review() {
    let reg = registry();
    for (removed, added) in [
        ("cases: 128", "cases: 1"),
        ("coverage_threshold = 0.90", "coverage_threshold = 0.85"),
        ("assertions = 12", "assertions = 0"),
        ("campaign_runs: 100_000", "campaign_runs: 10"),
    ] {
        let hunk = integrity::Hunk {
            path: "docs/testing.md".into(),
            removed: vec![removed.into()],
            added: vec![added.into()],
        };
        let finding = integrity::inspect(
            &reg,
            std::slice::from_ref(&hunk.path),
            std::slice::from_ref(&hunk),
        )
        .unwrap();
        assert!(
            finding
                .reasons
                .iter()
                .any(|reason| reason.contains("numeric"))
        );
    }
    let finding =
        integrity::inspect(&reg, &["crates/example/tests/deleted.rs".into()], &[]).unwrap();
    assert!(
        finding
            .reasons
            .iter()
            .any(|reason| reason.contains("ownership"))
    );
    for added in [
        "test.only('narrow', action)",
        "#[should_panic]",
        "proptest_config(ProptestConfig { cases: 1 })",
    ] {
        let hunk = integrity::Hunk {
            path: "docs/testing.md".into(),
            removed: vec![],
            added: vec![added.into()],
        };
        assert_eq!(
            integrity::inspect(
                &reg,
                std::slice::from_ref(&hunk.path),
                std::slice::from_ref(&hunk)
            )
            .unwrap()
            .disposition,
            "REVIEW_REQUIRED"
        );
    }
}
#[test]
fn integrity_rejects_unresolved_hunks_oversized_counts_lines_and_control_bytes() {
    let reg = registry();
    let paths = vec!["docs/testing.md".into()];
    let hunk = integrity::Hunk {
        path: "other.rs".into(),
        removed: vec![],
        added: vec![],
    };
    assert!(integrity::inspect(&reg, &paths, &[hunk]).is_err());
    assert!(integrity::inspect(&reg, &vec!["docs/testing.md".into(); 4097], &[]).is_err());
    for line in ["x".repeat(4097), "x\0y".into()] {
        let hunk = integrity::Hunk {
            path: paths[0].clone(),
            removed: vec![],
            added: vec![line],
        };
        assert!(integrity::inspect(&reg, &paths, &[hunk]).is_err());
    }
    let hunk = integrity::Hunk {
        path: paths[0].clone(),
        removed: vec![],
        added: vec!["line".into(); 4097],
    };
    assert!(integrity::inspect(&reg, &paths, &[hunk]).is_err());
}
