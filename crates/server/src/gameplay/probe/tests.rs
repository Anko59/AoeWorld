use super::*;
use aoe_core::Seed;
use aoe_protocol::GameplayServerMessage;
use std::sync::Arc;

#[tokio::test]
async fn retired_world_is_released_after_its_service_and_sessions_are_dropped() {
    let old = GameplayService::new(Seed(1));
    let replacement = GameplayService::new(Seed(2));
    let world = Arc::downgrade(&old.world);
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    old.register(None, tx).await;
    old.retire(replacement.world_id()).await;
    assert!(matches!(
        rx.recv().await,
        Some(GameplayServerMessage::WorldReset { .. })
    ));
    assert!(old.sessions.lock().await.is_empty());
    drop(old);
    assert!(
        world.upgrade().is_none(),
        "the retired service must not pin its world"
    );
}

#[tokio::test]
async fn every_tick_counts_one_unit_frame_build_over_every_unit() {
    let service = GameplayService::new(Seed(1));
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    service.register(None, tx).await;
    assert_eq!(service.unit_frame_passes(), (0, 0));
    for _ in 0..3 {
        service.tick().await;
    }
    let units = service.world.read().await.unit_count() as u64;
    assert_eq!(service.unit_frame_passes(), (3, 3 * units));
}
