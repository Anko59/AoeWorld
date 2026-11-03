use super::super::policy;
use std::time::{Duration, Instant};

#[test]
fn memory_admission_refuses_at_once_below_three_gib_and_names_the_measurement() {
    let begun = Instant::now();
    let error = policy::admit("MemFree: 9000000 kB\nMemAvailable: 3145727 kB\n")
        .unwrap_err()
        .to_string();
    assert!(begun.elapsed() < Duration::from_secs(1));
    assert!(error.contains("MemAvailable 3145727 KiB"), "{error}");
    assert!(error.contains("3145728 KiB"), "{error}");
    assert!(error.contains("refusing immediately"), "{error}");
    assert!(policy::admit("MemAvailable: 3145728 kB\n").is_ok());
    assert!(policy::admit("MemFree: 9000000 kB\n").is_err());
    assert!(policy::admit("MemAvailable: invalid kB\n").is_err());
}

#[test]
fn containers_have_memory_swap_and_relative_cpu_limits_without_cpu_quotas() {
    let args = policy::cap_args();
    assert!(args.iter().any(|arg| arg.starts_with("--memory=")));
    assert!(args.iter().any(|arg| arg.starts_with("--memory-swap=")));
    assert!(args.iter().any(|arg| arg.starts_with("--cpu-shares=")));
    assert!(!args.iter().any(|arg| arg.starts_with("--cpus")));
    let compiler_jobs: u32 = args
        .iter()
        .find_map(|arg| arg.strip_prefix("CARGO_BUILD_JOBS="))
        .expect("explicit compiler parallelism")
        .parse()
        .unwrap();
    assert!(
        compiler_jobs >= 4,
        "global compiler parallelism must not serialize cold builds"
    );
}
