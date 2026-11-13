//! After a review fix, a ship reruns only the gates the fix can affect.
use super::{Evidence, Verdict, split};
use crate::gates::registry::{Cadence, Registry};
use crate::ship::run::{GateResult, GateVerdict};
use std::path::Path;

fn result(gate: &str, verdict: GateVerdict) -> GateResult {
    GateResult {
        gate: gate.into(),
        verdict,
        seconds: 12.0,
        summary: "ok".into(),
        tail: String::new(),
        attempts: Vec::new(),
    }
}

fn prior(gates: Vec<GateResult>) -> Evidence {
    Evidence {
        version: 1,
        cadence: "preflight".into(),
        head: "a".repeat(40),
        tree: "b".repeat(40),
        branch: "fix".into(),
        base: "origin/dev".into(),
        base_branch: "dev".into(),
        merge_base: "c".repeat(40),
        changed: Vec::new(),
        gates,
        verdict: Verdict::Pass,
        level: None,
        recheck: None,
        started: 0,
        finished: 0,
    }
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn only_passed_gates_the_fix_does_not_select_are_reused() {
    let prior = prior(vec![
        result("lint", GateVerdict::Pass),
        result("build-wasm", GateVerdict::Pass),
        result("deny", GateVerdict::Fail),
    ]);
    let planned = ids(&["fmt-check", "lint", "build-wasm", "deny"]);
    let (run, reused) = split(&planned, &ids(&["fmt-check", "lint"]), &prior);
    // fmt-check had no prior result, lint is selected again, deny failed before.
    assert_eq!(run, ids(&["fmt-check", "lint", "deny"]));
    assert_eq!(reused.len(), 1);
    assert_eq!(reused[0].gate, "build-wasm");
    assert_eq!(reused[0].verdict, GateVerdict::Pass);
    // A reviewer sees which gate was not rerun and the commit it passed at.
    assert_eq!(
        reused[0].summary,
        format!("reused from {}: ok", "a".repeat(12))
    );
}

/// The preflight gates a change since passing evidence makes a ship rerun.
fn preflight(paths: &[&str]) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry = Registry::load(&root).unwrap();
    let planned = registry
        .plan(Cadence::Preflight, &registry.classify(&ids(paths)).suites)
        .unwrap()
        .gates;
    let affected = registry.affected(&registry.classify(&ids(paths)).suites);
    planned
        .into_iter()
        .filter(|gate| affected.contains(gate))
        .collect()
}

#[test]
fn a_python_ai_fix_reruns_only_the_static_gates() {
    let gates = preflight(&["ai/aoe_ai/play/management.py"]);
    assert_eq!(gates, ids(&["docs-check", "fmt-check", "structure-check"]));
}

#[test]
fn every_fix_reruns_the_repository_wide_static_gates() {
    // Line and directory limits and docs links read the whole tree, so any change
    // reruns them; a Rust change also reruns the native gates.
    let gates = preflight(&["crates/server/src/gameplay.rs"]);
    for gate in [
        "fmt-check",
        "structure-check",
        "docs-check",
        "architecture-check",
        "lint",
    ] {
        assert!(
            gates.contains(&gate.to_owned()),
            "{gate} not rerun: {gates:?}"
        );
    }
}
