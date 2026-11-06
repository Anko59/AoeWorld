//! Pause stops the match clock and resuming continues it.
use super::*;

#[tokio::test]
async fn paused_match_does_not_advance_until_resumed() {
    let state = AppState::new(
        &Config {
            bind: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            scenario: SMOKE,
            tick_hz: 20,
            asset_pack: None,
            map_package_directory: None,
            open_map_package: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "pause-test",
    )
    .expect("state");
    let gameplay = state.gameplay.read().await.clone();
    assert!(!gameplay.paused());
    let ticker = tokio::spawn(state.clone().run_ticks());
    let running = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while gameplay.tick_and_units().await.0 < 3 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
    };
    assert!(running.await.is_ok(), "the match never advanced");

    gameplay.set_paused(true);
    // A tick already in flight may finish; after that the clock holds still.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let held = gameplay.tick_and_units().await.0;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        gameplay.tick_and_units().await.0,
        held,
        "paused match advanced"
    );

    gameplay.set_paused(false);
    let resumed = tokio::time::timeout(Duration::from_secs(5), async {
        while gameplay.tick_and_units().await.0 <= held {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    ticker.abort();
    assert!(resumed.is_ok(), "the resumed match did not advance");
}
