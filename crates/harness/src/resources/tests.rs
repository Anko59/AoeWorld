use super::measure::{available_kib, cgroup_directory, parse_job};
use std::path::Path;

#[test]
fn container_observer_preserves_failure_exit_status() {
    assert_eq!(super::measure::container(&["false".into()]).unwrap(), 1);
    assert!(super::measure::container(&[]).is_err());
}

#[test]
fn measurement_refuses_an_empty_workload() {
    assert!(
        super::measure::run(Vec::new())
            .unwrap_err()
            .to_string()
            .contains("--job")
    );
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

#[test]
fn memory_probe_resolves_only_unified_cgroups() {
    assert_eq!(
        cgroup_directory("0::/system.slice/docker-abc.scope\n").unwrap(),
        Path::new("/sys/fs/cgroup/system.slice/docker-abc.scope")
    );
    assert!(cgroup_directory("2:memory:/docker/abc\n").is_err());
    assert!(cgroup_directory("0::/../../etc\n").is_err());
}

#[test]
fn measurement_jobs_are_single_make_targets_in_explicit_worktrees() {
    let (root, target) = parse_job("/tmp/tree=build-wasm").unwrap();
    assert_eq!(root, Path::new("/tmp/tree"));
    assert_eq!(target, "build-wasm");
    for job in [
        "lint",
        "=lint",
        "/tmp=",
        "/tmp=--eval=x",
        "/tmp=lint build",
        "/tmp=AOE_X=1",
    ] {
        assert!(parse_job(job).is_err(), "{job}");
    }
}
