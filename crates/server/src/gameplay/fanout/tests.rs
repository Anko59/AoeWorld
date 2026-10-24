use super::*;
use aoe_core::{Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
use aoe_simulation::{GameWorld, RallyTarget};

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
async fn building_views_stream_past_one_message_and_reconnect_without_oversized_frames() {
    use aoe_protocol::{GameplayServerMessage as M, MAX_ECONOMY_BUILDINGS, encode_gameplay_server};
    use aoe_simulation::WorldLimits;
    let mut world = GameWorld::new(WorldConfig::new(1024, 1024, Seed(0)).unwrap()).unwrap();
    assert!(WorldLimits::for_match(128, 5000).buildings <= aoe_protocol::MAX_RETAINED_BUILDINGS);
    world.set_limits(WorldLimits::for_match(100, 200));
    world.set_fog_disabled(true);
    for raw in 0..=MAX_ECONOMY_BUILDINGS {
        let at = TileCoord::new(4 + (raw % 200) as i32 * 5, 4 + (raw / 200) as i32 * 5);
        world
            .place_town_center(PlayerId((raw % 100) as u16), at)
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
