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
