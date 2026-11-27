use super::super::policy;

#[test]
fn memory_admission_refuses_at_once_below_three_gib_and_names_the_measurement() {
    let error = policy::admit("MemFree: 9000000 kB\nMemAvailable: 3145727 kB\n")
        .unwrap_err()
        .to_string();
    assert!(error.contains("MemAvailable 3145727 KiB"), "{error}");
    assert!(error.contains("3145728 KiB"), "{error}");
    assert!(error.contains("refusing immediately"), "{error}");
    assert!(policy::admit("MemAvailable: 3145728 kB\n").is_ok());
    assert!(policy::admit("MemFree: 9000000 kB\n").is_err());
    assert!(policy::admit("MemAvailable: invalid kB\n").is_err());
}

#[test]
fn a_missing_meminfo_stays_an_error_on_linux_and_never_admits_work_elsewhere() {
    let temp = tempfile::tempdir().unwrap();
    let meminfo = temp.path().join("meminfo");
    assert!(policy::meminfo_at(&meminfo, true).is_err());
    assert_eq!(policy::meminfo_at(&meminfo, false).unwrap(), None);
    let error = policy::admit_measured(None).unwrap_err().to_string();
    assert!(error.contains("Linux VM"), "{error}");
    std::fs::write(&meminfo, "MemAvailable: 3145728 kB\n").unwrap();
    for linux in [true, false] {
        policy::admit_measured(policy::meminfo_at(&meminfo, linux).unwrap()).unwrap();
    }
    std::fs::write(&meminfo, "MemAvailable: 1 kB\n").unwrap();
    for linux in [true, false] {
        assert!(policy::admit_measured(policy::meminfo_at(&meminfo, linux).unwrap()).is_err());
    }
}

#[cfg(not(target_os = "linux"))]
#[test]
fn a_status_without_meminfo_points_to_the_vm_instead_of_failing() {
    let nowhere = std::path::Path::new("/no-such-checkout");
    crate::jobs::print_status(nowhere, &crate::jobs::JobEnv::default()).unwrap();
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
