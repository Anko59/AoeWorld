//! Resource knowledge stays silent until remembered amounts change.
use super::*;

#[tokio::test]
async fn unchanged_resource_memory_sends_nothing_when_observers_change() {
    use aoe_core::{PlayerId, TileCoord, TileRect, WorldPosition};
    use aoe_protocol::GameplayServerMessage;
    let directory = tempfile::tempdir().unwrap();
    let service = GameplayService::from_stored_map(
        package(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let (id, _) = service.register(None, tx).await;
    let region = TileRect::from_xywh(0, 0, 64, 64);
    service.subscribe(id, 1, region).await;
    service.publish_resources().await;
    let initial = resource_message(&mut rx);
    assert!(
        !initial.changes.is_empty(),
        "fixture needs remembered starting nodes"
    );
    let known = service
        .world
        .read()
        .await
        .known_resource_amounts(PlayerId(0));
    let vision_before = service.world.read().await.vision_revision();
    {
        let mut world = service.world.write().await;
        let at = world
            .unit(service.slots[0].primary_unit_id)
            .unwrap()
            .position;
        // An overlapping observer changes fog but learns no resource amounts.
        world.spawn_villager(PlayerId(0), at).unwrap();
    }
    assert_ne!(service.world.read().await.vision_revision(), vision_before);
    service.publish_resources().await;
    assert_eq!(
        service
            .world
            .read()
            .await
            .known_resource_amounts(PlayerId(0)),
        known
    );
    while let Ok(message) = rx.try_recv() {
        assert!(
            !matches!(message, GameplayServerMessage::ResourceState(_)),
            "unchanged memory was resent"
        );
    }

    // A visible change still sends a delta after a silent fog-only update.
    let starting = service
        .world
        .read()
        .await
        .starting_resources()
        .into_iter()
        .find(|node| initial.changes.iter().any(|known| known.id == node.id))
        .expect("an observed starting node");
    service
        .deplete_resource_persisted(starting.id, 1)
        .await
        .unwrap();
    service.publish_resources().await;
    let changed = resource_message(&mut rx);
    assert_eq!(changed.from_revision, Some(initial.revision));
    assert!(changed.changes.iter().any(|c| c.id == starting.id));

    // Discover a previously hidden modified node at the same global revision.
    let (hidden, vantage) = {
        let world = service.world.read().await;
        let generator = package()
            .generator_with_page_provider(Arc::new(Flat))
            .unwrap();
        (0..2)
            .flat_map(|y| (0..2).map(move |x| (x, y)))
            .flat_map(|(x, y)| generator.chunk(x, y).unwrap().resources)
            .filter(|node| {
                world.visibility(PlayerId(0), node.tile)
                    == aoe_simulation::TileVisibility::Unexplored
                    && world.resource_remaining(node.id) == Some(node.initial_amount)
            })
            .find_map(|node| {
                (-4..=4)
                    .flat_map(|dy| (-4..=4).map(move |dx| (dx, dy)))
                    .filter(|(dx, dy)| dx * dx + dy * dy <= 16)
                    .map(|(dx, dy)| TileCoord::new(node.tile.x + dx, node.tile.y + dy))
                    .find(|tile| world.tile_passable(*tile))
                    .map(|tile| (node, WorldPosition::from_tile_center(tile).unwrap()))
            })
            .expect("an unmodified hidden resource with a scout vantage")
    };
    service
        .deplete_resource_persisted(hidden.id, 1)
        .await
        .unwrap();
    service.publish_resources().await;
    while let Ok(message) = rx.try_recv() {
        assert!(
            !matches!(message, GameplayServerMessage::ResourceState(_)),
            "hidden changes leaked a payload"
        );
    }
    service
        .deplete_resource_persisted(starting.id, 1)
        .await
        .unwrap();
    service.publish_resources().await;
    let after_hidden = resource_message(&mut rx);
    assert_eq!(
        after_hidden.from_revision,
        Some(changed.revision),
        "silent updates must not advance the client's delivered revision"
    );
    assert!(after_hidden.changes.iter().all(|c| c.id != hidden.id));
    let revision = service.world.read().await.resource_revision();
    service
        .world
        .write()
        .await
        .spawn_villager(PlayerId(0), vantage)
        .unwrap();
    service.publish_resources().await;
    let revealed = resource_message(&mut rx);
    assert_eq!(Some(revealed.revision), revision);
    assert_eq!(
        revealed.from_revision, None,
        "new knowledge at an already delivered revision needs a reset"
    );
    assert!(
        revealed
            .changes
            .iter()
            .any(|c| c.id == hidden.id && c.remaining == hidden.initial_amount - 1)
    );
    // A new subscription still resets the complete remembered state.
    service.subscribe(id, 2, region).await;
    service.publish_resources().await;
    assert_eq!(resource_message(&mut rx).from_revision, None);
}
