use super::*;

#[tokio::test]
async fn no_fog_tick_orders_owned_plans_among_public_buildings() {
    use aoe_protocol::{GameplayServerMessage, encode_gameplay_server};

    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let owner = PlayerId(0);
    let tile = |x, y| TileCoord::new(x, y);
    let primary = world.initialize_village(owner, tile(20, 20)).unwrap();
    let worker = world
        .spawn_villager(
            owner,
            WorldPosition::from_tile_center(tile(40, 40)).unwrap(),
        )
        .unwrap();
    world.set_fog_disabled(true);
    let service = super::super::super::GameplayService::from_scale_world(world, primary);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
    let (session, _) = service.register(None, sender).await;
    service
        .subscribe(session, 1, TileRect::from_xywh(0, 0, 128, 128))
        .await;
    while receiver.try_recv().is_ok() {}
    let expected = {
        let mut world = service.world.write().await;
        let first = world.place_town_center(PlayerId(1), tile(48, 48)).unwrap();
        let plan = world
            .issue_build(
                owner,
                &[worker],
                aoe_simulation::BuildingKind::House,
                tile(64, 64),
                false,
            )
            .unwrap();
        let last = world.place_town_center(PlayerId(1), tile(80, 80)).unwrap();
        [first, plan, last]
    };
    service.tick().await;
    assert!(
        service
            .world
            .read()
            .await
            .building(expected[1])
            .unwrap()
            .is_unstarted()
    );
    while let Ok(message) = receiver.try_recv() {
        encode_gameplay_server(&message).expect("mixed building tick is valid on the wire");
        if let GameplayServerMessage::Tick {
            changed_buildings, ..
        } = message
        {
            let ids: Vec<_> = changed_buildings.iter().map(|b| b.id).collect();
            assert_eq!(ids, expected);
            return;
        }
    }
    panic!("no tick delivered");
}

#[test]
fn full_fanout_reveals_a_foundation_only_after_work_even_with_fog_disabled() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let owner = PlayerId(0);
    let other = PlayerId(1);
    world
        .initialize_village(owner, TileCoord::new(20, 20))
        .unwrap();
    world.set_fog_disabled(true);
    let worker = world
        .spawn_villager(
            owner,
            WorldPosition::from_tile_center(TileCoord::new(63, 64)).unwrap(),
        )
        .unwrap();
    let plan = world
        .issue_build(
            owner,
            &[worker],
            aoe_simulation::BuildingKind::House,
            TileCoord::new(64, 64),
            false,
        )
        .unwrap();
    let views = [owner, other].map(|viewer| View {
        session_id: u64::from(viewer.0),
        subscription: Subscription {
            revision: 1,
            region: TileRect::from_xywh(0, 0, 256, 256),
        },
        vision: None,
        fog_disabled: Some(true),
        viewer,
        primary: worker,
        ai: false,
        rebuild: false,
    });
    let has_plan = |states: &BTreeMap<PlayerId, ViewerState>, player| {
        let Buildings::Full(buildings, private) = &states[&player].buildings else {
            panic!("no-fog fanout builds a full view");
        };
        buildings.contains_key(&plan) || private.contains_key(&plan)
    };
    let private = viewer_states(&world, &views, &BTreeMap::new());
    assert!(has_plan(&private, owner));
    assert!(!has_plan(&private, other));
    assert!(
        economy::subscription_buildings(&world, owner, views[0].subscription.region)
            .contains_key(&plan)
    );
    assert!(
        !economy::subscription_buildings(&world, other, views[1].subscription.region)
            .contains_key(&plan)
    );
    world.advance();
    assert!(!world.building(plan).unwrap().is_unstarted());
    let public = viewer_states(&world, &views, &BTreeMap::new());
    assert!(has_plan(&public, owner));
    assert!(has_plan(&public, other));
    assert!(
        economy::subscription_buildings(&world, other, views[1].subscription.region)
            .contains_key(&plan)
    );
}
