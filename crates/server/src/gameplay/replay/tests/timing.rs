use super::*;

#[test]
fn a_failed_nested_counter_read_invalidates_the_tick() {
    let probe = Probe::fixture();
    drop(Span {
        probe: probe.clone(),
        name: "sidestep",
        started: Err(std::io::Error::other("counter failed")),
    });
    assert!(probe.check().is_err());
}

#[test]
fn multiplexed_or_unavailable_counts_are_rejected_without_scaling() {
    for (enabled, running) in [(100, 99), (0, 0), (100, 0)] {
        assert!(validate(10, enabled, running).is_err());
    }
    assert_eq!(validate(1234, 100, 100).unwrap(), 1234);
}

#[test]
#[ignore = "requires a hardware PMU; run make save-replay-counter-check"]
fn hardware_counter_is_user_only_thread_local_and_fully_scheduled() {
    let probe = Probe::open().unwrap();
    let before = probe.read().unwrap();
    for value in 0_u64..100_000 {
        std::hint::black_box(std::hint::black_box(value).wrapping_mul(31));
    }
    let work = probe.read().unwrap() - before;
    let before = probe.read().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(40));
    let sleeping = probe.read().unwrap() - before;
    assert!(
        work > 0 && sleeping < work / 10,
        "work={work}, sleep={sleeping}"
    );
    let other = probe.clone();
    assert!(
        std::thread::spawn(move || other.read().is_err())
            .join()
            .unwrap()
    );
    assert!(probe.finish().unwrap() >= work);
}

#[test]
fn phase_samples_keep_instructions_and_cpu_time_in_separate_units() {
    let probe = Probe::fixture();
    probe.record(
        "sidestep",
        Sample {
            instructions: 900,
            cpu_ns: 9_000,
        },
    );
    let (instructions, cpu_ns) = probe.drain();
    assert_eq!(instructions["sidestep"], 900);
    assert_eq!(cpu_ns["sidestep"], 9_000);
    assert_eq!(instructions["plan_ahead"], 0);
    assert_eq!(cpu_ns["plan_ahead"], 0);
}

#[test]
fn thread_cpu_clock_does_not_charge_sleep() {
    let before = cpu().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(40));
    assert!(cpu().unwrap() - before < 20_000_000);
}
