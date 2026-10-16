use super::*;

#[test]
fn preflight_runs_the_wasm_size_check_for_client_changes_not_docs() {
    let registry = registry();
    let plan = |changed: &str| {
        let impact = registry.classify(&paths(&[changed]));
        registry.plan(Cadence::Preflight, &impact.suites).unwrap()
    };
    let client = plan("crates/client/src/lib.rs");
    assert!(client.gates.contains(&"perf-wasm-size".into()));
    let (wasm, size) = (
        client.gates.iter().position(|g| g == "build-wasm"),
        client.gates.iter().position(|g| g == "perf-wasm-size"),
    );
    assert!(wasm < size);
    let docs = plan("docs/testing.md");
    assert!(!docs.gates.contains(&"perf-wasm-size".into()));
    assert!(docs.gates.contains(&"build-wasm".into()));
}
