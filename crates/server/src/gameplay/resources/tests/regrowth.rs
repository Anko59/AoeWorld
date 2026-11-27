use super::*;

#[tokio::test]
async fn a_partly_elapsed_regrowth_timer_resumes_and_restores_its_committed_growth() {
    use aoe_core::{PlayerId, TileCoord, TileRect, WorldPosition};
    use aoe_map::ObjectKind;
    use aoe_protocol::ResourceAmount;

    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let reopen = || {
        GameplayService::from_stored_map(
            package.clone(),
            Some(Arc::new(Flat)),
            Some(directory.path().to_owned()),
            &|| false,
        )
        .unwrap()
        .unwrap()
    };
    let service = reopen();
    let generator = package
        .generator_with_page_provider(Arc::new(Flat))
        .unwrap();
    let village = service
        .world
        .read()
        .await
        .units()
        .next()
        .unwrap()
        .position
        .tile_floor();
    let tree = (0..2)
        .flat_map(|y| (0..2).map(move |x| (x, y)))
        .flat_map(|(x, y)| generator.chunk(x, y).unwrap().resources)
        .find(|node| {
            node.object == ObjectKind::Tree
                && node
                    .tile
                    .x
                    .abs_diff(village.x)
                    .max(node.tile.y.abs_diff(village.y))
                    > 12
                && [(0, -1), (-1, 0), (1, 0), (0, 1)].iter().any(|(dx, dy)| {
                    generator
                        .object_at(TileCoord::new(node.tile.x + dx, node.tile.y + dy))
                        .is_some_and(|neighbor| neighbor.object == ObjectKind::Tree)
                })
        })
        .expect("fixture has a seeded forest outside the village");

    service
        .deplete_resource_persisted(tree.id, tree.initial_amount)
        .await
        .unwrap();
    for _ in 0..3 {
        service.tick().await;
    }
    let cut = service.resource_snapshot().await.unwrap();
    assert_eq!(cut.recovery.len(), 1);
    assert_eq!(
        cut.recovery[0],
        (120 * u64::from(cut.recovery_hz) - 3, tree.id)
    );
    assert!(service.world.read().await.tile_passable(tree.tile));
    drop(service);

    let service = reopen();
    assert_eq!(service.resource_snapshot().await.unwrap(), cut);
    service
        .world
        .write()
        .await
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(tree.tile).unwrap(),
        )
        .unwrap();
    for _ in 1..cut.recovery[0].0 {
        service.tick().await;
    }
    // Interrupt persistence exactly when occupancy reschedules the timer.
    let overlays = directory.path().join("resource-overlays");
    let backed_up = directory.path().join("overlays-backup");
    std::fs::rename(&overlays, &backed_up).unwrap();
    std::fs::write(&overlays, b"temporarily unavailable").unwrap();
    service.tick().await;
    std::fs::remove_file(&overlays).unwrap();
    std::fs::rename(backed_up, &overlays).unwrap();
    // Persist the rescheduled timer on the next step, even though no work is due.
    service.tick().await;
    let deferred = service.resource_snapshot().await.unwrap();
    assert_eq!(deferred.revision, cut.revision);
    assert_eq!(deferred.changes, cut.changes);
    assert_eq!(
        deferred.recovery,
        [(120 * u64::from(deferred.recovery_hz) - 1, tree.id)]
    );
    drop(service);

    let service = reopen();
    assert_eq!(service.resource_snapshot().await.unwrap(), deferred);
    for _ in 2..deferred.recovery[0].0 {
        service.tick().await;
    }
    service.set_fog_disabled(true).await;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
    let (id, _) = service.register(None, sender).await;
    service
        .subscribe(id, 1, TileRect::from_xywh(0, 0, 64, 64), true)
        .await;
    service.tick().await;
    assert_eq!(resource_message(&mut receiver).revision, cut.revision);
    assert_eq!(
        service.world.read().await.resource_remaining(tree.id),
        Some(0)
    );
    service.tick().await;
    let delta = resource_message(&mut receiver);
    assert_eq!(delta.from_revision, Some(cut.revision));
    assert_eq!(delta.revision, cut.revision + 1);
    assert_eq!(
        delta.changes,
        [ResourceAmount {
            id: tree.id,
            remaining: tree.initial_amount
        }]
    );
    let grown = service.resource_snapshot().await.unwrap();
    assert_eq!(grown.revision, cut.revision + 1);
    assert_eq!(grown.replenished, u64::from(tree.initial_amount));
    assert!(grown.recovery.is_empty());
    assert!(!service.world.read().await.tile_passable(tree.tile));
    drop(service);

    let restarted = reopen();
    assert_eq!(restarted.resource_snapshot().await.unwrap(), grown);
    assert_eq!(
        restarted.world.read().await.resource_remaining(tree.id),
        Some(tree.initial_amount)
    );
    assert!(!restarted.world.read().await.tile_passable(tree.tile));
}

#[test]
fn recovery_clock_extends_only_its_checkpoint_revision() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    let hash = [6; 32];
    let mut snapshot = ResourceOverlay::default().snapshot(hash);
    snapshot.revision = 1;
    snapshot.recovery = vec![(20, 7)];
    store::save(path, &snapshot, 0, 10).unwrap();
    store::save_clock(path, hash, 1, 15).unwrap();
    let resumed = store::load(path, hash).unwrap().unwrap();
    assert_eq!(resumed.tick, 15);
    assert_eq!(resumed.snapshot.recovery, [(15, 7)]);
    assert!(matches!(
        store::save_clock(path, hash, 1, 14),
        Err(ResourceLifecycleError::StaleRevision)
    ));

    snapshot.revision = 2;
    snapshot.recovery = vec![(40, 8)];
    store::save(path, &snapshot, 1, 16).unwrap();
    // An older revision's clock cannot advance the replacement schedule.
    store::save_clock(path, hash, 1, 17).unwrap();
    let resumed = store::load(path, hash).unwrap().unwrap();
    assert_eq!(resumed.tick, 16);
    assert_eq!(resumed.snapshot, snapshot);
}
