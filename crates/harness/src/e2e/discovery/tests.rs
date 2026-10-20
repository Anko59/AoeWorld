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
fn a_load_failure_at_discovery_is_left_to_the_main_run() {
    // A spec that cannot load (a module the PR adds) makes `--list` exit 1; the main run's
    // report carries that error to the compile-red classifier, so discovery must not stop it.
    let root = fixture();
    let mut command = Command::new("sh");
    command.args(["-c", "printf '{\"errors\":[]}'; exit 1"]);
    run(
        command,
        &root.path().join("list.json"),
        Duration::from_secs(2),
        &Cancellation::default(),
    )
    .expect("an ordinary failing exit is deferred to the run");
    // Any other failure (a crash) still stops at discovery.
    let mut crashed = Command::new("sh");
    crashed.args(["-c", "exit 2"]);
    let error = run(
        crashed,
        &root.path().join("list.json"),
        Duration::from_secs(2),
        &Cancellation::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("playwright-discovery"), "{error}");
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
