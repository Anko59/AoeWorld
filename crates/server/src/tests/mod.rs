use crate::{AppState, Config};
use aoe_scenario::SMOKE;
use std::{net::SocketAddr, time::Duration};

#[tokio::test]
async fn records_tick_delayed_by_world_lock_contention() {
    let state = AppState::new(
        &Config {
            bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            scenario: SMOKE,
            tick_hz: 20,
            asset_pack: None,
            map_package_directory: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "tick-deadline-test",
    )
    .expect("state");
    let period = state.tick_period;
    assert_eq!(period, Duration::from_millis(50));

    let held_world = state.world.write().await;
    let ticker = tokio::spawn(state.clone().run_ticks());
    tokio::task::yield_now().await;
    tokio::time::sleep(period * 4).await;
    drop(held_world);

    let observed = tokio::time::timeout(Duration::from_secs(5), async {
        while state.tick_deadline_misses() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    ticker.abort();
    let _ = ticker.await;

    assert!(observed.is_ok(), "the delayed tick was not recorded");
    assert!(state.tick_deadline_misses() >= 1);
}
