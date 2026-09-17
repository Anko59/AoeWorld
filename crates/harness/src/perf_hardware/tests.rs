use super::*;

fn environment() -> Environment {
    Environment {
        version: 1,
        cpu_model: "fixture-cpu".into(),
        gpu_model: "fixture-gpu".into(),
        gpu_driver: "fixture-driver".into(),
        os: "Linux".into(),
        kernel: "fixture-kernel".into(),
        browser: "Chromium".into(),
        browser_version: "fixture-browser".into(),
        rustc: "rustc 1.93.1".into(),
        container_digests: BTreeMap::from([(
            "runtime".into(),
            format!("sha256:{}", "a".repeat(64)),
        )]),
        memory_limit_bytes: 16 * 1024 * 1024 * 1024,
        cpu_limit_millicores: 8_000,
        cpu_governor: "performance".into(),
        gpu_power_mode: "maximum-performance".into(),
        concurrent_jobs: 1,
        viewport_width: 1920,
        viewport_height: 1080,
    }
}

fn samples() -> Samples {
    Samples {
        version: 1,
        revision: "a".repeat(40),
        workload_hash: TARGET_HOTSPOT.workload_hash(),
        duration_seconds: 60,
        total_entities: TARGET_HOTSPOT.entities,
        resident_entities: TARGET_HOTSPOT.entities,
        visible_entities: 10_007,
        tick_ns: vec![35_000_000; 1_200],
        frame_interval_ns: vec![15_000_000; 3_600],
        cpu_submission_ns: vec![4_000_000; 3_600],
        gpu_frame_ns: Some(vec![8_000_000; 3_600]),
        missed_tick_deadlines: 0,
        dropped_frames: 0,
    }
}

fn baseline(environment: Environment) -> Baseline {
    Baseline {
        version: 1,
        environment,
        workload_hash: TARGET_HOTSPOT.workload_hash(),
        runs: (1..=3)
            .map(|id| BaselineRun {
                sample_hash: format!("blake3:{id:064x}"),
                tick_p99_ns: 35_000_000,
                frame_p99_ns: 15_000_000,
            })
            .collect(),
    }
}

#[test]
fn qualification_requires_complete_workload_and_compatible_stable_environment() {
    let environment = environment();
    let samples = samples();
    assert_eq!(
        compare(&environment, &samples, None)
            .expect("unbaselined report")
            .verdict,
        Verdict::Unbaselined
    );
    assert_eq!(
        compare(&environment, &samples, Some(&baseline(environment.clone())))
            .expect("qualified report")
            .verdict,
        Verdict::Pass
    );

    let mut changed_environment = environment.clone();
    changed_environment.gpu_driver = "different".into();
    assert!(
        compare(
            &changed_environment,
            &samples,
            Some(&baseline(environment.clone()))
        )
        .is_err()
    );
    changed_environment = environment.clone();
    changed_environment.concurrent_jobs = 2;
    assert!(compare(&changed_environment, &samples, None).is_err());

    let mut incomplete = samples.clone();
    incomplete.visible_entities = 9_999;
    assert!(compare(&environment, &incomplete, None).is_err());
    incomplete = samples.clone();
    incomplete.frame_interval_ns.pop();
    assert!(compare(&environment, &incomplete, None).is_err());

    let mut unstable = baseline(environment.clone());
    unstable.runs[2].tick_p99_ns = 45_000_000;
    assert!(compare(&environment, &samples, Some(&unstable)).is_err());
    unstable = baseline(environment.clone());
    unstable.runs.pop();
    assert!(compare(&environment, &samples, Some(&unstable)).is_err());
    unstable = baseline(environment.clone());
    unstable.runs[2].sample_hash = unstable.runs[1].sample_hash.clone();
    assert!(compare(&environment, &samples, Some(&unstable)).is_err());
}

#[test]
fn sustained_deadline_or_baseline_regression_fails() {
    let environment = environment();
    let baseline = baseline(environment.clone());
    let mut candidate = samples();
    candidate.tick_ns[0] = 60_000_000;
    candidate.missed_tick_deadlines = 1;
    assert_eq!(
        compare(&environment, &candidate, Some(&baseline))
            .expect("deadline report")
            .verdict,
        Verdict::Regression
    );
    candidate = samples();
    candidate.frame_interval_ns.fill(16_000_000);
    assert_eq!(
        compare(&environment, &candidate, Some(&baseline))
            .expect("baseline report")
            .verdict,
        Verdict::Regression
    );
}
