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
    // The cap check is its own gate after the measurement, so perf-wasm-size
    // stays a measurement that perf-baseline-propose can run over the cap.
    let cap = client.gates.iter().position(|g| g == "perf-wasm-cap");
    assert!(cap.is_some() && size < cap, "{:?}", client.gates);
    let docs = plan("docs/testing.md");
    assert!(!docs.gates.contains(&"perf-wasm-size".into()));
    assert!(!docs.gates.contains(&"perf-wasm-cap".into()));
    assert!(docs.gates.contains(&"build-wasm".into()));
}
