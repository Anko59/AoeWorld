use super::*;
use aoe_core::{Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
use aoe_simulation::{GameWorld, RallyTarget};

#[path = "tests/navy.rs"]
mod navy;

#[test]
fn queued_destinations_are_private_to_the_owning_viewer_even_with_fog_disabled() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let unit = world.spawn_unit(PlayerId(1), at(10, 10)).unwrap();
    world.issue_queued_move(unit, at(14, 10)).unwrap();
    world.issue_queued_move(unit, at(14, 14)).unwrap();
    world.set_fog_disabled(true);
    assert!(move_queues(&world, PlayerId(0), std::iter::once(unit)).is_empty());
    let own = move_queues(&world, PlayerId(1), std::iter::once(unit));
    assert_eq!(own[0].destinations, [at(14, 10), at(14, 14)]);
}

#[test]
fn rally_is_private_in_fog_and_a_hundred_no_fog_viewers_share_one_building_map() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    world
        .place_town_center(PlayerId(0), TileCoord::new(64, 64))
        .unwrap();
    let enemy = world
        .place_town_center(PlayerId(1), TileCoord::new(128, 128))
        .unwrap();
    let target = WorldPosition::from_tile_center(TileCoord::new(140, 140)).unwrap();
    world
        .issue_set_rally(PlayerId(1), enemy, RallyTarget::Ground(target))
        .unwrap();
    world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(127, 127)).unwrap(),
        )
        .unwrap();
    world.advance();
    let views = (0..100_u16)
        .map(|player| View {
            session_id: u64::from(player),
            subscription: Subscription {
                revision: 1,
                region: TileRect::new(TileCoord::new(0, 0), TileCoord::new(256, 256)),
            },
            vision: None,
            fog_disabled: None,
            viewer: PlayerId(player),
            primary: EntityId(0),
            ai: true,
            rebuild: true,
        })
        .collect::<Vec<_>>();
    let ordinary = viewer_states(&world, &views[..1], &BTreeMap::new());
    let Buildings::Full(own_view) = &ordinary[&PlayerId(0)].buildings else {
        panic!("full view");
    };
    assert_eq!(
        own_view[&enemy].rally_point, None,
        "visible enemy buildings do not disclose rally targets through fog"
    );
    world.set_fog_disabled(true);
    let complete = viewer_states(&world, &views, &BTreeMap::new());
    let Buildings::Full(shared) = &complete[&PlayerId(0)].buildings else {
        panic!("full view");
    };
    assert_eq!(shared[&enemy].rally_point.unwrap().position, target);
    for view in complete.values() {
        let Buildings::Full(buildings) = &view.buildings else {
            panic!("full view");
        };
        assert!(
            Arc::ptr_eq(shared, buildings),
            "no per-viewer full-map clone at 100 players"
        );
    }
}

#[tokio::test]
async fn owned_buildings_stream_past_one_message_and_reconnect_without_oversized_frames() {
    use aoe_protocol::{GameplayServerMessage as M, MAX_ECONOMY_BUILDINGS, encode_gameplay_server};
    use aoe_simulation::WorldLimits;
    let mut world = GameWorld::new(WorldConfig::new(1024, 1024, Seed(0)).unwrap()).unwrap();
    assert!(WorldLimits::for_match(128, 5000).buildings <= aoe_protocol::MAX_RETAINED_BUILDINGS);
    world.set_limits(WorldLimits::for_match(100, 200));
    world.set_fog_disabled(true);
    for raw in 0..=MAX_ECONOMY_BUILDINGS {
        let at = TileCoord::new(4 + (raw % 200) as i32 * 5, 4 + (raw / 200) as i32 * 5);
        world
            // Owned buildings stay available off camera. Enemy buildings now
            // follow camera bounds while fog is disabled; the separate camera
            // regression below covers that intentional policy change.
            .place_town_center(PlayerId(0), at)
            .unwrap();
    }
    let primary = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(2, 2)).unwrap(),
        )
        .unwrap();
    let service = super::super::GameplayService::from_scale_world(world, primary);
    let region = TileRect::from_xywh(0, 0, 32, 32);
    for _ in 0..2 {
        let (sender, mut rx) = tokio::sync::mpsc::channel(64);
        let (id, _) = service.register(None, sender).await;
        let mut held = BTreeMap::new();
        for revision in 1..=2 {
            service.subscribe(id, revision, region).await;
            for _ in 0..4 {
                while let Ok(message) = rx.try_recv() {
                    encode_gameplay_server(&message)
                        .expect("every frame remains within its wire bound");
                    match message {
                        M::Snapshot { buildings, .. } => {
                            held = buildings.into_iter().map(|b| (b.id, b)).collect()
                        }
                        M::Tick {
                            changed_buildings,
                            building_removals,
                            ..
                        } => {
                            for b in changed_buildings {
                                held.insert(b.id, b);
                            }
                            for id in building_removals {
                                held.remove(&id);
                            }
                        }
                        _ => {}
                    }
                }
                service.tick().await;
            }
            assert_eq!(held.len(), MAX_ECONOMY_BUILDINGS + 1);
        }
        service.disconnect(id).await;
    }
}

#[tokio::test]
async fn fog_disabled_enemy_buildings_follow_camera_and_reenable_removes_hidden_entities() {
    use aoe_protocol::{GameplayServerMessage as M, encode_gameplay_server};
    let mut world = GameWorld::new(WorldConfig::new(1024, 1024, Seed(7)).unwrap()).unwrap();
    let own = world
        .place_town_center(PlayerId(0), TileCoord::new(12, 12))
        .unwrap();
    let near = world
        .place_town_center(PlayerId(1), TileCoord::new(40, 40))
        .unwrap();
    let far = world
        .place_town_center(PlayerId(1), TileCoord::new(900, 900))
        .unwrap();
    let primary = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(8, 8)).unwrap(),
        )
        .unwrap();
    let hidden = world
        .spawn_unit(
            PlayerId(1),
            WorldPosition::from_tile_center(TileCoord::new(44, 44)).unwrap(),
        )
        .unwrap();
    world.set_fog_disabled(true);
    let service = super::super::GameplayService::from_scale_world(world, primary);
    for _ in 0..2 {
        let (sender, mut rx) = tokio::sync::mpsc::channel(64);
        let (id, _) = service.register(None, sender).await;
        for (revision, region, visible, absent) in [
            (1, TileRect::from_xywh(32, 32, 32, 32), near, far),
            (2, TileRect::from_xywh(896, 896, 32, 32), far, near),
            (3, TileRect::from_xywh(32, 32, 32, 32), near, far),
        ] {
            service.subscribe(id, revision, region).await;
            let mut snapshot = false;
            while let Ok(message) = rx.try_recv() {
                encode_gameplay_server(&message).expect("camera snapshots are bounded");
                if let M::Snapshot { buildings, .. } = message {
                    assert_eq!(buildings.len(), 2, "owned plus camera enemy building");
                    assert!(buildings.iter().any(|b| b.id == own));
                    assert!(buildings.iter().any(|b| b.id == visible));
                    assert!(buildings.iter().all(|b| b.id != absent));
                    snapshot = true;
                }
            }
            assert!(snapshot);
        }
        service.set_fog_disabled(false).await;
        service.tick().await;
        let mut removed_unit = false;
        let mut removed_building = false;
        while let Ok(message) = rx.try_recv() {
            encode_gameplay_server(&message).expect("fog removals are bounded");
            if let M::Tick {
                removals,
                building_removals,
                ..
            } = message
            {
                removed_unit |= removals.contains(&hidden);
                removed_building |= building_removals.contains(&near);
                assert!(!building_removals.contains(&own));
            }
        }
        assert!(
            removed_unit && removed_building,
            "fog reenable removes entities never explored by the viewer"
        );
        service.disconnect(id).await;
        service.set_fog_disabled(true).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn pending_subscription_holds_all_revision_updates_until_snapshot() {
    use super::super::{GameplayService, deliver::deliver};
    use aoe_core::Tick;
    use aoe_protocol::{GameplayServerMessage as M, ProjectileKind, ProjectileState};

    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let primary = world.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
    let service = GameplayService::from_scale_world(world, primary);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    let (id, _) = service.register(None, sender.clone()).await;
    let region = TileRect::from_xywh(0, 0, 64, 64);

    // Pressure creates the retained worker. Drain it fully, leaving an empty
    // outgoing queue and an idle worker, as after a temporarily slow client.
    for _ in 0..8 {
        sender
            .try_send(M::Error {
                code: 429,
                message: "pressure fixture".to_owned(),
            })
            .unwrap();
    }
    service.subscribe(id, 1, region).await;
    for _ in 0..8 {
        assert!(matches!(receiver.try_recv().unwrap(), M::Error { .. }));
    }
    for _ in 0..4 {
        receiver.recv().await.unwrap();
    }
    tokio::task::yield_now().await;
    assert!(
        !service.sessions.lock().await[&id]
            .subscription_delivery
            .as_ref()
            .unwrap()
            .pending()
    );
    assert!(receiver.is_empty());

    // Every following await takes an uncontended lock and completes inline.
    // The current-thread runtime therefore cannot poll the worker between
    // queuing revision 2 and delivering changes captured after its snapshot.
    service.subscribe(id, 2, region).await;
    {
        let mut world = service.world.write().await;
        world.set_fog_disabled(true);
        world.spawn_unit(PlayerId(0), at(30, 20)).unwrap();
        world
            .spawn_animal(aoe_simulation::AnimalKind::Wolf, at(22, 20))
            .unwrap();
        world.advance();
    }
    let mut sessions = service.sessions.lock().await;
    let session = &sessions[&id];
    assert!(session.subscription_delivery.as_ref().unwrap().pending());
    let before_fog = session.resident_fog.clone();
    let before_vision = session.vision_revision;
    let before_units = session.resident.clone();
    let before_animals = session.resident_animals.clone();
    let before_projectiles = session.resident_projectiles.clone();
    let views = [View {
        session_id: id,
        subscription: session.subscription.unwrap(),
        vision: None,
        fog_disabled: None,
        viewer: session.viewer,
        primary,
        ai: false,
        rebuild: true,
    }];
    let world = service.world.read().await;
    let mut built = build(&world, &views, &BTreeMap::new()).unwrap();
    drop(world);
    // A projectile starting on this tick exercises its independent baseline
    // without depending on combat wind-up or attack-range timing.
    built.sessions[0].projectiles = vec![ProjectileState {
        id: 1,
        from: at(20, 20),
        to: at(22, 20),
        launched: built.tick,
        impact: Tick(built.tick.0 + 5),
        kind: ProjectileKind::Arrow,
    }];
    let fresh = &built.sessions[0];
    let (expected_fog, expected_vision) = fresh.fog.clone().unwrap();
    assert_ne!(before_fog.as_ref().unwrap().tiles, expected_fog.tiles);
    assert_ne!(before_animals, fresh.animals);
    assert_ne!(before_projectiles, fresh.projectiles);
    assert!(deliver(&mut sessions, &built, BTreeMap::new()).is_empty());
    assert!(
        receiver.is_empty(),
        "fog, wildlife, projectiles and ticks must wait for their snapshot"
    );
    let session = &sessions[&id];
    assert_eq!(session.resident_fog, before_fog);
    assert_eq!(session.vision_revision, before_vision);
    assert_eq!(session.resident, before_units);
    assert_eq!(session.resident_animals, before_animals);
    assert_eq!(session.resident_projectiles, before_projectiles);
    assert!(session.rebuild_buildings);
    drop(sessions);

    // With no further world changes, replaying the skipped view must repair
    // every baseline after the four initial frames have arrived in order.
    tokio::task::yield_now().await;
    assert!(matches!(
        receiver.try_recv().unwrap(),
        M::Snapshot { revision: 2, .. }
    ));
    for _ in 0..3 {
        receiver.try_recv().unwrap();
    }
    assert!(receiver.is_empty());
    let mut sessions = service.sessions.lock().await;
    assert!(
        !sessions[&id]
            .subscription_delivery
            .as_ref()
            .unwrap()
            .pending()
    );
    assert!(deliver(&mut sessions, &built, BTreeMap::new()).is_empty());
    let mut delivered = [false; 4];
    while let Ok(message) = receiver.try_recv() {
        match message {
            M::FogState(fog) => {
                assert_eq!(fog, expected_fog);
                delivered[0] = true;
            }
            M::Tick { revision: 2, .. } => delivered[1] = true,
            M::WildlifeState {
                revision: 2,
                animals,
                ..
            } => {
                assert_eq!(animals, fresh.animals);
                delivered[2] = true;
            }
            M::Projectiles {
                revision: 2,
                projectiles,
                ..
            } => {
                assert_eq!(projectiles, fresh.projectiles);
                delivered[3] = true;
            }
            message => panic!("unexpected retry message: {message:?}"),
        }
    }
    assert_eq!(delivered, [true; 4]);
    let session = &sessions[&id];
    assert_eq!(session.resident_fog, Some(expected_fog));
    assert_eq!(session.vision_revision, Some(expected_vision));
    assert_eq!(session.resident_animals, fresh.animals);
    assert_eq!(session.resident_projectiles, fresh.projectiles);
}
