use super::{browser_run_deadline, server_binary};
use std::time::Duration;

#[test]
fn the_instrumented_debug_server_gets_its_own_deadline() {
    assert_eq!(browser_run_deadline(None), Duration::from_secs(1200));
    assert_eq!(browser_run_deadline(Some("1")), Duration::from_secs(2400));
}

#[test]
fn coverage_mode_cannot_silently_select_an_uninstrumented_server() {
    assert_eq!(server_binary(None), Ok("target/release/aoe-server"));
    assert_eq!(
        server_binary(Some("1")),
        Ok("target/llvm-cov-target/debug/aoe-server")
    );
    for invalid in ["", "0", "true", "release"] {
        assert!(server_binary(Some(invalid)).is_err());
    }
}
