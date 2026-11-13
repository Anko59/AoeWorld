use super::*;
use aoe_scenario::SMOKE;

#[tokio::test]
async fn arena_steps_record_every_tick_in_the_public_timing_stream() {
    let state = AppState::new(
        &Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            scenario: SMOKE,
            tick_hz: 20,
            asset_pack: None,
            map_package_directory: None,
            open_map_package: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "arena-timing-test",
    )
    .unwrap();
    let gameplay = state.gameplay.read().await.clone();
    advance_recorded(&state, &gameplay, 3).await;
    let page = serde_json::to_value(state.tick_stats.page(0)).unwrap();
    let records = page["ticks"].as_array().unwrap();
    assert_eq!(records.len(), 3);
    for (i, record) in records.iter().enumerate() {
        assert_eq!(record[0], i + 1);
        assert!(record[1].as_u64().is_some());
        assert!(record[2].as_u64().is_some());
        assert_eq!(record[3], 0, "lockstep has no wall-clock schedule lateness");
    }
    assert_eq!(page["next"], 3);
}
