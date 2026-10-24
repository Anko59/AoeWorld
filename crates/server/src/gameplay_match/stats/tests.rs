use super::*;

#[test]
fn timing_records_include_and_drain_planner_work_per_tick() {
    let probe = TickProbe::default();
    probe.record(Duration::from_micros(60), 3);
    probe.planning_micros.store(37, Ordering::Relaxed);
    let stats = TickStats::default();
    for tick in 1..=2 {
        stats.record(
            tick,
            Duration::from_micros(100),
            Duration::ZERO,
            Duration::from_millis(50),
            &probe,
            12,
        );
    }
    let value = serde_json::to_value(stats.page(0)).unwrap();
    assert_eq!(
        value["ticks"][0],
        serde_json::json!([1, 100, 60, 0, 3, 12, 0, 37, phases::Micros::default()])
    );
    assert_eq!(
        value["ticks"][1],
        serde_json::json!([2, 100, 60, 0, 3, 12, 0, 0, phases::Micros::default()])
    );
}

#[test]
fn detailed_phases_are_named_and_drained_between_ticks() {
    let probe = TickProbe::default();
    probe.phase("orders", Duration::from_micros(19));
    probe.phase("orders", Duration::from_micros(7));
    probe.phase("movement", Duration::from_micros(31));
    let stats = TickStats::default();
    for tick in 1..=2 {
        stats.record(
            tick,
            Duration::from_micros(100),
            Duration::ZERO,
            Duration::from_millis(50),
            &probe,
            12,
        );
    }
    let value = serde_json::to_value(stats.page(0)).unwrap();
    assert_eq!(value["ticks"][0][8]["orders"], 26);
    assert_eq!(value["ticks"][0][8]["movement"], 31);
    assert_eq!(value["ticks"][1][8]["orders"], 0);
    assert_eq!(value["ticks"][1][8]["movement"], 0);
}
