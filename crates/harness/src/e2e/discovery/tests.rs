use super::*;

fn fixture() -> tempfile::TempDir {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/tmp");
    std::fs::create_dir_all(&cache).unwrap();
    tempfile::Builder::new()
        .prefix("playwright-discovery-")
        .tempdir_in(cache)
        .unwrap()
}

#[test]
fn collection_hang_fails_before_any_test_timeout_can_start() {
    let root = fixture();
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30"]);
    let started = std::time::Instant::now();
    let error = run(
        command,
        &root.path().join("list.json"),
        Duration::from_millis(100),
        &Cancellation::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("playwright-discovery"), "{error}");
    assert!(error.contains("Deadline"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn failed_discovery_never_becomes_a_named_test_failure() {
    let root = fixture();
    let mut command = Command::new("sh");
    command.args(["-c", "printf '{\"errors\":[]}'; exit 1"]);
    let error = run(
        command,
        &root.path().join("list.json"),
        Duration::from_secs(2),
        &Cancellation::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("playwright-discovery"), "{error}");
    assert!(error.contains("Failed(Some(1))"), "{error}");
}

#[test]
fn a_second_discovery_in_the_same_checkout_replaces_the_first_listing() {
    let root = fixture();
    let report = root.path().join("list.json");
    for _ in 0..2 {
        let mut command = Command::new("sh");
        command.args(["-c", "printf '{\"suites\":[]}'"]);
        run(
            command,
            &report,
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap_or_else(|e| panic!("a repeated filtered run must list again: {e}"));
    }
}
