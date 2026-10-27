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

#[test]
fn empty_perf_window_has_finite_zero_rate_and_no_phases() {
    let stats = TickStats::default();
    let summary = stats.summary(Duration::from_millis(50));
    assert_eq!(summary["samples"], 0);
    assert_eq!(summary["tick_rate"], 0.0);
    assert_eq!(summary["tick_hz"], 20.0);
    assert_eq!(summary["missed_ticks"], 0);
    assert_eq!(summary["missed_total"], 0);
    assert_eq!(summary["phases"], serde_json::json!({}));
    assert_eq!(summary["top_phases"], serde_json::json!([]));
}

#[test]
fn stopped_perf_window_expires_without_new_ticks_but_keeps_raw_history() {
    let stats = TickStats::default();
    let probe = TickProbe::default();
    probe.phase("movement", Duration::from_micros(300));
    stats.record(
        1,
        Duration::from_millis(60),
        Duration::ZERO,
        Duration::from_millis(50),
        &probe,
        12,
    );
    assert_eq!(stats.summary(Duration::from_millis(50))["samples"], 1);
    // Age the stored observation rather than sleeping: a stopped server must
    // stop reporting an old window even when record() is never called again.
    stats.inner.lock().unwrap().times[0] = std::time::Instant::now() - Duration::from_secs(61);
    let summary = stats.summary(Duration::from_millis(50));
    assert_eq!(summary["samples"], 0);
    assert_eq!(summary["tick_rate"], 0.0);
    assert_eq!(summary["missed_ticks"], 0);
    assert_eq!(summary["missed_total"], 1);
    assert_eq!(summary["phases"], serde_json::json!({}));
    assert_eq!(summary["top_phases"], serde_json::json!([]));
    let page = stats.page(0);
    assert_eq!(page.next, 1);
    assert_eq!(page.ticks.len(), 1);
    assert_eq!(page.missed_total, 1);
}

#[test]
fn replacing_match_clears_old_perf_window_and_preserves_cursor_and_misses() {
    let stats = TickStats::default();
    let probe = TickProbe::default();
    for tick in [10, 11] {
        probe.phase("movement", Duration::from_micros(900));
        stats.record(
            tick,
            Duration::from_millis(60),
            Duration::ZERO,
            Duration::from_millis(50),
            &probe,
            100,
        );
    }
    let old_cursor = stats.page(0).next;
    assert_eq!(old_cursor, 2);
    for tick in [0, 1] {
        probe.phase("movement", Duration::from_micros(7));
        stats.record(
            tick,
            Duration::from_micros(10),
            Duration::ZERO,
            Duration::from_millis(50),
            &probe,
            4,
        );
    }
    let summary = stats.summary(Duration::from_millis(50));
    assert_eq!(summary["samples"], 2);
    assert_eq!(summary["missed_ticks"], 0);
    assert_eq!(summary["missed_total"], 2);
    assert_eq!(summary["phases"]["movement"]["max_us"], 7);
    let page = stats.page(old_cursor);
    assert_eq!(page.next, 4, "the public cursor never restarts at zero");
    assert_eq!(page.missed_total, 2);
    assert_eq!(
        page.ticks.iter().map(|row| row.0).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(
        stats.page(0).ticks.len(),
        2,
        "old match samples are dropped"
    );
    assert!(stats.page(page.next).ticks.is_empty());
}

#[test]
fn direct_benchmark_reads_the_real_server_phases_once_per_tick() {
    let probe = TickProbe::default();
    probe.phase("movement", Duration::from_micros(31));
    probe.phase("session_views", Duration::from_micros(7));
    let first = probe.take_phase_micros();
    assert_eq!(first["movement"], 31);
    assert_eq!(first["session_views"], 7);
    assert_eq!(first.len(), 30);
    assert!(probe.take_phase_micros().values().all(|value| *value == 0));
}
