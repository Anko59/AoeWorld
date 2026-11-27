use super::*;
use crate::{Config, gameplay_match::TickProbe};

#[tokio::test]
async fn minute_is_appended_with_phase_costs_and_isolated_by_world() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 10,
        asset_pack: None,
        map_package_directory: Some(dir.path().to_path_buf()),
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: dir.path().join("geodata"),
    };
    let state = AppState::new(&config, "test").unwrap();
    let active = state.gameplay.read().await.clone();
    let world = active.world_id();
    let start = Instant::now() - MINUTE;
    active.telemetry.started.set(start).unwrap();
    let probe = TickProbe::default();

    for tick in 1..=3 {
        probe.phase("movement", Duration::from_micros(35));
        state.tick_stats.record(
            (world, tick),
            Duration::from_millis(10),
            Duration::ZERO,
            Duration::from_millis(100),
            &probe,
            88,
        );
    }
    let end = Instant::now();
    write_minute(&state, &active, start, end).await.unwrap();
    let path = directory(dir.path(), world).join("telemetry.jsonl");
    let first = std::fs::read_to_string(&path).unwrap();
    let row: MatchTelemetryRow = serde_json::from_str(&first).unwrap();
    let MatchTelemetryRow::Server {
        window_ms,
        ticks,
        units,
        phases_us,
        ..
    } = row
    else {
        panic!("expected server row");
    };
    assert!(window_ms >= 60_000);
    assert_eq!(ticks, 3);
    assert_eq!(units, 88);
    assert_eq!(phases_us["movement"]["p95_us"], 35);
    assert_eq!(active.telemetry.latest().unwrap()["ticks"], 3);

    let replacement = GameplayService::new(config.scenario.seed);
    replacement.telemetry.started.set(start).unwrap();
    write_minute(&state, &replacement, start, end)
        .await
        .unwrap();
    assert_eq!(replacement.telemetry.latest().unwrap()["ticks"], 0);
    assert_eq!(std::fs::read_to_string(path).unwrap(), first);
    assert_ne!(active.world_id(), replacement.world_id());
}
