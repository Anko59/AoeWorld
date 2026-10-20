use super::*;
use aoe_map::ResourceOverlay;

#[derive(Debug)]
struct Flat;
impl EnvironmentPageProvider for Flat {
    fn page(
        &self,
        key: aoe_map::EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<aoe_map::EnvironmentPage>, aoe_map::EnvironmentPageError> {
        if cancelled() {
            return Err(aoe_map::EnvironmentPageError::Cancelled);
        }
        Ok(Arc::new(aoe_map::EnvironmentPage::Elevation(elevation(
            key.level,
        ))))
    }
}
fn elevation(level: u8) -> aoe_map::ElevationPage {
    let side = if level == 0 { 2 } else { 1 };
    aoe_map::ElevationPage {
        level,
        x: 0,
        y: 0,
        width: side,
        height: side,
        geographic_height_centimeters: vec![0; usize::from(side).pow(2)],
    }
}
fn package() -> MapPackage {
    use aoe_map::*;
    MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: 3_840,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 1_920_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: (0..2)
                    .map(|level| PyramidLevel {
                        samples_per_axis: if level == 0 { 2 } else { 1 },
                        ordered_page_root: ordered_page_root(&[elevation(level)]).unwrap(),
                    })
                    .collect(),
            },
            water: None,
            vegetation: None,
            historical_land_use: None,
            hydrology_evidence: None,
        },
    )
    .unwrap()
}
fn node(package: &MapPackage) -> aoe_map::ResourceNode {
    let generator = package
        .generator_with_page_provider(Arc::new(Flat))
        .unwrap();
    (0..2)
        .find_map(|y| (0..2).find_map(|x| generator.chunk(x, y).ok()?.resources.into_iter().next()))
        .expect("fixture resource")
}

#[tokio::test]
async fn persisted_depletion_survives_service_recreation_and_failed_save_is_atomic() {
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let node = node(&package);
    let service = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let result = service
        .deplete_resource_persisted(node.id, node.initial_amount)
        .await
        .unwrap();
    assert!(result.directory_synced);
    assert!(result.depletion.became_nonblocking);
    let snapshot = service.resource_snapshot().await.unwrap();
    let restored = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    assert_eq!(restored.resource_snapshot().await.unwrap(), snapshot);
    let failed_directory = directory.path().join("not-a-directory");
    std::fs::write(&failed_directory, b"sentinel").unwrap();
    let mut failed =
        GameplayService::from_stored_map(package, Some(Arc::new(Flat)), None, &|| false)
            .unwrap()
            .unwrap();
    failed.resource_directory = Some(failed_directory);
    let before = failed.resource_snapshot().await.unwrap();
    assert!(failed.deplete_resource_persisted(node.id, 1).await.is_err());
    assert_eq!(failed.resource_snapshot().await.unwrap(), before);
}

#[tokio::test]
async fn concurrent_changes_serialize_and_corrupt_restore_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let node = node(&package);
    let service = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let (first, second) = tokio::join!(
        service.deplete_resource_persisted(node.id, 1),
        service.deplete_resource_persisted(node.id, 1)
    );
    first.unwrap();
    second.unwrap();
    assert_eq!(service.resource_snapshot().await.unwrap().revision, 2);
    let overlays = directory.path().join("resource-overlays");
    let saved = std::fs::read_dir(&overlays)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(saved, b"{broken").unwrap();
    assert!(
        GameplayService::from_stored_map(
            package,
            Some(Arc::new(Flat)),
            Some(directory.path().to_owned()),
            &|| { false }
        )
        .is_err()
    );
}

#[test]
fn bounded_storage_rejects_oversized_json_and_wrong_map_on_restore() {
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let overlays = directory.path().join("resource-overlays");
    let snapshot = ResourceOverlay::default().snapshot(package.content_hash);
    store::save(&overlays, &snapshot, 0).unwrap();
    let saved = std::fs::read_dir(&overlays)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&saved)
        .unwrap();
    file.set_len(4 * 1024 * 1024 + 1).unwrap();
    assert!(matches!(
        store::load(&overlays, package.content_hash),
        Err(ResourceLifecycleError::TooLarge)
    ));
    let mut wrong = snapshot;
    wrong.map_content_hash = [7; 32];
    std::fs::write(saved, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert!(
        GameplayService::from_stored_map(
            package,
            Some(Arc::new(Flat)),
            Some(directory.path().to_owned()),
            &|| { false }
        )
        .is_err()
    );
}

#[tokio::test]
async fn stale_service_cannot_overwrite_a_newer_persisted_revision() {
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let node = node(&package);
    let first = GameplayService::from_stored_map(
        package.clone(),
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let stale = GameplayService::from_stored_map(
        package,
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    let before = stale.resource_snapshot().await.unwrap();
    first.deplete_resource_persisted(node.id, 1).await.unwrap();
    assert!(matches!(
        stale.deplete_resource_persisted(node.id, 2).await,
        Err(ResourceLifecycleError::StaleRevision)
    ));
    assert_eq!(stale.resource_snapshot().await.unwrap(), before);
    assert_eq!(first.resource_snapshot().await.unwrap().revision, 1);
}

#[test]
fn interrupted_temporary_snapshot_is_recovered_without_accumulating_files() {
    let directory = tempfile::tempdir().unwrap();
    let snapshot = ResourceOverlay::default().snapshot([5; 32]);
    let temporary = directory
        .path()
        .join(format!("{}.resource-tmp", "05".repeat(32)));
    std::fs::write(&temporary, b"incomplete prior write").unwrap();
    assert!(store::save(directory.path(), &snapshot, 0).unwrap());
    assert!(!temporary.exists());
    assert_eq!(
        store::load(directory.path(), [5; 32]).unwrap(),
        Some(snapshot)
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

fn resource_message(
    receiver: &mut tokio::sync::mpsc::Receiver<aoe_protocol::GameplayServerMessage>,
) -> aoe_protocol::ResourceState {
    while let Ok(message) = receiver.try_recv() {
        if let aoe_protocol::GameplayServerMessage::ResourceState(state) = message {
            return state;
        }
    }
    panic!("resource message was not published");
}

#[tokio::test]
async fn subscribers_receive_snapshot_delta_and_reconnect_snapshot() {
    use aoe_protocol::GameplayServerMessage;
    let directory = tempfile::tempdir().unwrap();
    let package = package();
    let node = node(&package);
    let service = GameplayService::from_stored_map(
        package,
        Some(Arc::new(Flat)),
        Some(directory.path().to_owned()),
        &|| false,
    )
    .unwrap()
    .unwrap();
    service.set_fog_disabled(true).await;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    let (id, welcome) = service.register(None, sender).await;
    let GameplayServerMessage::Welcome { resume_token, .. } = welcome else {
        panic!("welcome");
    };
    service
        .subscribe(id, 1, aoe_core::TileRect::from_xywh(0, 0, 64, 64))
        .await;
    service.tick().await;
    let initial = resource_message(&mut receiver);
    assert_eq!(initial.from_revision, None);
    assert_eq!(initial.revision, 0);
    let kit = service.world.read().await.starting_resources();
    assert!(
        !kit.is_empty(),
        "fixture needs supplemental starting resources"
    );
    let expected: Vec<_> = kit
        .iter()
        .map(|node| aoe_protocol::ResourceAmount {
            id: node.id,
            remaining: node.initial_amount,
        })
        .collect();
    assert_eq!(initial.changes, expected);
    let frame = GameplayServerMessage::ResourceState(initial.clone());
    assert_eq!(
        aoe_protocol::decode_gameplay_server(
            &aoe_protocol::encode_gameplay_server(&frame).unwrap()
        )
        .unwrap(),
        frame
    );
    service
        .deplete_resource_persisted(node.id, node.initial_amount)
        .await
        .unwrap();
    service.tick().await;
    let delta = resource_message(&mut receiver);
    assert_eq!(delta.from_revision, Some(0));
    assert_eq!(delta.revision, 1);
    assert_eq!(
        delta.changes,
        vec![aoe_protocol::ResourceAmount {
            id: node.id,
            remaining: 0
        }]
    );
    service.disconnect(id).await;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    let (id, _) = service.register(resume_token, sender).await;
    service
        .subscribe(id, 1, aoe_core::TileRect::from_xywh(0, 0, 64, 64))
        .await;
    service.tick().await;
    let restored = resource_message(&mut receiver);
    assert_eq!(restored.from_revision, None);
    assert_eq!(restored.revision, 1);
    let mut expected = initial.changes;
    expected.extend(delta.changes);
    expected.sort_by_key(|change| change.id);
    assert_eq!(restored.changes, expected);
}

#[tokio::test]
async fn resource_backpressure_disconnects_without_advancing_delivery_state() {
    let service =
        GameplayService::from_stored_map(package(), Some(Arc::new(Flat)), None, &|| false)
            .unwrap()
            .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let (id, _) = service.register(None, sender).await;
    service
        .subscribe(id, 1, aoe_core::TileRect::from_xywh(0, 0, 64, 64))
        .await;
    service.tick().await;
    assert!(service.sessions.lock().await.get(&id).is_none());
    assert!(receiver.try_recv().is_ok());
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
    ));
}

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
        let at = world.unit(service.primary_unit_id).unwrap().position;
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
