use super::policy::available_kib;
mod cpu;
mod limits;
mod makefile;

#[test]
fn container_observer_preserves_failure_exit_status() {
    assert_eq!(super::observer::container(&["false".into()]).unwrap(), 1);
    assert!(super::observer::container(&[]).is_err());
}

#[test]
fn available_memory_uses_available_not_free_and_rejects_missing_metrics() {
    assert_eq!(
        available_kib("MemFree: 12 kB\nMemAvailable: 3456789 kB\n").unwrap(),
        3456789
    );
    assert!(available_kib("MemFree: 9000000 kB\n").is_err());
    assert!(available_kib("MemAvailable: invalid kB\n").is_err());
}
