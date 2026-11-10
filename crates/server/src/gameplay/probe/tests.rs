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
async fn idle_ticks_visit_units_only_for_the_initial_frame() {
    let service = GameplayService::new(Seed(1));
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    service.register(None, tx).await;
    assert_eq!(service.unit_frame_passes(), (0, 0));
    for _ in 0..3 {
        service.tick().await;
    }
    let units = service.world.read().await.unit_count() as u64;
    assert_eq!(service.unit_frame_passes(), (3, units));
}

#[tokio::test]
async fn idle_units_are_converted_once_and_then_kept() {
    let service = GameplayService::new(Seed(1));
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    service.register(None, tx).await;
    for _ in 0..3 {
        service.tick().await;
    }
    let units = service.world.read().await.unit_count() as u64;
    assert!(units > 0);
    // An untouched world: the first frame converts every unit, later frames
    // keep every state, so nothing is marked changed after the first.
    assert_eq!(service.unit_frame_work(), (units, units));
}
