use aoe_core::{Seed, TileCoord, TileRect};
use aoe_protocol::GameplayServerMessage;
use aoe_server::GameplayService;
use aoe_simulation::GameWorld;
use tokio::sync::mpsc;

#[tokio::test]
async fn a_prebuilt_world_serves_a_subscribed_session() {
    let (world, primary) = GameWorld::default_with_village(Seed(3));
    let units = world.unit_count();
    let service = GameplayService::from_scale_world(world, primary);
    let (sender, mut receiver) = mpsc::channel(64);
    let (session, welcome) = service.register(None, sender).await;
    let GameplayServerMessage::Welcome {
        primary_unit_id,
        map_content_hash,
        ..
    } = welcome
    else {
        panic!("expected a welcome");
    };
    assert_eq!(primary_unit_id, primary);
    assert_eq!(map_content_hash, None, "a scale world has no map identity");
    let region = TileRect::new(TileCoord::new(0, 0), TileCoord::new(200, 200));
    service.subscribe(session, 1, region).await;
    service.tick().await;
    let mut snapshot_units = None;
    while let Ok(message) = receiver.try_recv() {
        if let GameplayServerMessage::Snapshot { units, .. } = message {
            snapshot_units = Some(units.len());
        }
    }
    assert!(snapshot_units.is_some_and(|count| count > 0 && count <= units));
}
