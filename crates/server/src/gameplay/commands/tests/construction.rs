use super::*;

#[test]
fn a_full_world_reports_a_world_limit_for_explicit_and_shared_land_builds() {
    // The owner's report: build orders after Feudal Age answered "The order
    // queue is full" because the world's building capacity was reached.
    let (mut world, _) = aoe_simulation::GameWorld::default_with_village(Seed(1));
    let player = aoe_core::PlayerId(0);
    let villager = world
        .units()
        .find(|unit| unit.kind == aoe_simulation::UnitKind::Villager)
        .unwrap()
        .id;
    let origin = world
        .buildings()
        .next()
        .map(|building| {
            aoe_core::TileCoord::new(building.footprint.max.x + 3, building.footprint.min.y)
        })
        .unwrap();
    world.set_limits(aoe_simulation::WorldLimits {
        buildings: world.buildings().count(),
        ..world.limits()
    });
    let build = |builder_ids, kind| PendingAction::Build {
        builder_ids,
        kind,
        origin,
        queued: false,
    };
    assert_eq!(
        apply(
            &mut world,
            player,
            build(vec![villager], BuildingKind::House)
        ),
        CommandResult::RejectedWorldLimit
    );
    assert_eq!(
        apply(&mut world, player, build(Vec::new(), BuildingKind::House)),
        CommandResult::RejectedWorldLimit
    );
    assert_eq!(
        apply(
            &mut world,
            player,
            build(Vec::new(), BuildingKind::FishTrap)
        ),
        CommandResult::RejectedOrderTooLarge
    );
}

#[tokio::test]
async fn a_human_places_two_hundred_buildings_and_stops_five_thousand_units() {
    use crate::gameplay::SlotSetup;
    use aoe_core::{MAX_FORMATION_MEMBERS, PlayerId, TileCoord, WorldPosition};
    use aoe_simulation::{GameWorld, UnitKind};

    let owner = PlayerId(0);
    let (mut world, primary) = GameWorld::imperial_demo(Seed(7)).unwrap();
    let initial_buildings = world.buildings().count();
    let army = (0..5_000)
        .map(|i| {
            let tile = TileCoord::new(130 + i % 100, 150 + i / 100);
            let position = WorldPosition::from_tile_center(tile).unwrap();
            world
                .spawn_kind(owner, UnitKind::Militia, position)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let service = GameplayService::from_world_slots(
        world,
        vec![SlotSetup {
            player: owner,
            primary_unit_id: primary,
            human: true,
        }],
        None,
        None,
    );
    let (sender, mut receiver) = mpsc::channel(512);
    let (session, _) = service.register(None, sender).await;
    for i in 0..200 {
        service
            .queue_build(
                session,
                i as u64 + 1,
                Vec::new(),
                BuildingKind::House,
                TileCoord::new(20 + i % 20 * 3, 25 + i / 20 * 3),
                true,
            )
            .await;
    }
    for (i, units) in army.chunks(MAX_FORMATION_MEMBERS).enumerate() {
        service
            .queue_stop_selection(session, 201 + i as u64, units.to_vec())
            .await;
    }
    service.settle_preparation().await;
    service.tick().await;
    let mut accepted = Vec::new();
    while let Ok(message) = receiver.try_recv() {
        if let GameplayServerMessage::CommandAck {
            sequence, result, ..
        } = message
        {
            assert_eq!(result, CommandResult::Accepted, "order {sequence}");
            accepted.push(sequence);
        }
    }
    assert_eq!(accepted, (1..=202).collect::<Vec<_>>());
    let world = service.world.read().await;
    assert_eq!(world.buildings().count(), initial_buildings + 200);
    assert!(
        army.iter()
            .all(|&id| world.move_destinations(id).is_empty())
    );
}

#[test]
fn a_large_builder_group_can_place_a_long_wall_in_one_order() {
    use aoe_core::{PlayerId, TileCoord, WorldPosition};
    use aoe_simulation::GameWorld;

    let owner = PlayerId(0);
    let (mut world, _) = GameWorld::imperial_demo(Seed(7)).unwrap();
    let builders = (0..40)
        .map(|i| {
            let at = WorldPosition::from_tile_center(TileCoord::new(20 + i, 24)).unwrap();
            world.spawn_villager(owner, at).unwrap()
        })
        .collect::<Vec<_>>();
    let initial = world.buildings().count();
    let result = apply(
        &mut world,
        owner,
        PendingAction::BuildWall {
            builder_ids: builders,
            kind: BuildingKind::PalisadeWall,
            from: TileCoord::new(20, 25),
            to: TileCoord::new(219, 25),
            queued: true,
        },
    );
    assert_eq!(result, CommandResult::Accepted);
    assert_eq!(world.buildings().count(), initial + 200);
}

#[tokio::test]
async fn five_thousand_villagers_gather_in_two_list_orders_without_admission_refusal() {
    use crate::gameplay::SlotSetup;
    use aoe_core::{MAX_FORMATION_MEMBERS, PlayerId, TileCoord, WorldPosition};
    use aoe_simulation::GameWorld;

    let owner = PlayerId(0);
    let (mut world, primary) = GameWorld::imperial_demo(Seed(7)).unwrap();
    let villagers = (0..5_000)
        .map(|i| {
            let tile = TileCoord::new(130 + i % 100, 150 + i / 100);
            let at = WorldPosition::from_tile_center(tile).unwrap();
            world.spawn_villager(owner, at).unwrap()
        })
        .collect::<Vec<_>>();
    let service = GameplayService::from_world_slots(
        world,
        vec![SlotSetup {
            player: owner,
            primary_unit_id: primary,
            human: true,
        }],
        None,
        None,
    );
    let (sender, mut receiver) = mpsc::channel(64);
    let (session, _) = service.register(None, sender).await;
    let chunks = villagers.chunks(MAX_FORMATION_MEMBERS).collect::<Vec<_>>();
    assert_eq!(chunks.len(), 2);
    for (i, ids) in chunks.iter().enumerate() {
        service
            .queue_gather_selection(session, i as u64 + 1, ids.to_vec(), 1)
            .await;
    }
    service.settle_preparation().await;
    service.tick().await;
    let mut acks = 0;
    while let Ok(message) = receiver.try_recv() {
        if let GameplayServerMessage::CommandAck { result, .. } = message {
            acks += 1;
            // The resource may not exist on this map; admission must not refuse.
            assert!(
                !matches!(
                    result,
                    CommandResult::RejectedRateLimited
                        | CommandResult::RejectedQueueFull
                        | CommandResult::RejectedOrderTooLarge
                ),
                "{result:?}"
            );
        }
    }
    assert_eq!(acks, 2);
}

#[test]
fn a_build_order_covering_a_ground_relic_is_rejected_atomically() {
    use aoe_core::{PlayerId, TileCoord, WorldPosition};
    use aoe_simulation::GameWorld;

    let (mut world, scout) = GameWorld::default_with_village(Seed(1));
    let owner = PlayerId(0);
    world.set_fog_disabled(true);
    world.place_relics(&[scout], &|| false).unwrap();
    let (_, position) = world.ground_relics().next().unwrap();
    let tile = position.tile_floor();
    let origin = TileCoord::new(tile.x - 1, tile.y - 1);
    let builder = world
        .spawn_villager(
            owner,
            WorldPosition::from_tile_center(TileCoord::new(tile.x - 3, tile.y)).unwrap(),
        )
        .unwrap();
    let before = world.canonical_hash();

    assert_eq!(
        apply(
            &mut world,
            owner,
            PendingAction::Build {
                builder_ids: vec![builder],
                kind: BuildingKind::Barracks,
                origin,
                queued: false,
            }
        ),
        CommandResult::RejectedInvalidPlacement
    );
    assert_eq!(world.canonical_hash(), before);
}
