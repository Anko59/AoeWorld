//! Assignment reservations include workers walking to work and carrying home.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn workers(world: &mut GameWorld, origin: TileCoord, count: i32) -> Vec<EntityId> {
    (0..count)
        .map(|i| {
            world
                .spawn_villager(
                    PlayerId(0),
                    position(TileCoord::new(origin.x - 8, origin.y - 2 + i % 5)).unwrap(),
                )
                .unwrap()
        })
        .collect()
}

#[test]
fn six_lumberjacks_and_ore_and_berry_workers_spread_over_free_nodes() {
    for (kind, count, capacity) in [
        (ResourceKind::Wood, 24, 12),
        (ResourceKind::Gold, 24, 12),
        (ResourceKind::Stone, 24, 12),
        (ResourceKind::Food, 24, 12),
    ] {
        let run = || {
            let (mut world, node) = replacement::cleared_work_area(kind);
            if let Terrain::Map { generator, overlay } = &mut world.terrain {
                for dx in [-2, -4, -6] {
                    let next =
                        ResourceNode::starting(TileCoord::new(node.tile.x + dx, node.tile.y), kind)
                            .unwrap();
                    overlay.insert_starting_resource(generator, next).unwrap();
                }
            }
            let ids = workers(&mut world, node.tile, count);
            for id in &ids {
                world.issue_gather(PlayerId(0), *id, node.id).unwrap();
            }
            let mut targets = BTreeMap::<u64, usize>::new();
            for id in &ids {
                *targets
                    .entry(world.gather_target(*id).unwrap())
                    .or_default() += 1;
            }
            assert!(targets.len() >= 2, "{kind:?}: {targets:?}");
            assert!(
                targets.values().all(|n| *n <= capacity),
                "{kind:?}: {targets:?}"
            );
            for _ in 0..400 {
                world.advance();
            }
            let working = ids
                .iter()
                .filter_map(|id| {
                    let u = world.unit(*id).unwrap();
                    (u.task == UnitTask::Gathering).then_some((u.position.x, u.position.y))
                })
                .collect::<Vec<_>>();
            assert_eq!(working.iter().collect::<BTreeSet<_>>().len(), working.len());
            assert_eq!(
                working.len(),
                ids.len(),
                "{kind:?}: every assigned worker must work: {:?}",
                ids.iter()
                    .filter(|id| world.unit(**id).unwrap().task != UnitTask::Gathering)
                    .map(|id| (
                        world.unit(*id),
                        world.economy.gatherers.get(id),
                        world.movement_order(*id),
                        world.units[world.lookup[id]].obstruction,
                    ))
                    .collect::<Vec<_>>()
            );
            world.canonical_hash()
        };
        assert_eq!(
            run(),
            run(),
            "{kind:?} reservations must replay deterministically"
        );
    }
}

#[test]
fn a_new_lumberjack_uses_the_slot_released_by_a_cancelled_job() {
    let (mut world, node) = replacement::cleared_work_area(ResourceKind::Wood);
    let ids = workers(&mut world, node.tile, 3);
    world.issue_gather(PlayerId(0), ids[0], node.id).unwrap();
    world.issue_gather(PlayerId(0), ids[1], node.id).unwrap();
    world
        .issue_move(
            ids[0],
            position(TileCoord::new(node.tile.x - 9, node.tile.y)).unwrap(),
        )
        .unwrap();
    world.issue_gather(PlayerId(0), ids[2], node.id).unwrap();
    assert_eq!(world.gather_target(ids[2]), Some(node.id));
}

#[test]
fn returning_workers_keep_their_node_slots_and_removal_releases_them() {
    // Twelve full loads need 120 units; a 100-wood tree would deplete.
    let (mut world, node) = replacement::cleared_work_area(ResourceKind::Gold);
    let next =
        ResourceNode::starting(TileCoord::new(node.tile.x - 3, node.tile.y), node.kind).unwrap();
    if let Terrain::Map { generator, overlay } = &mut world.terrain {
        overlay.insert_starting_resource(generator, next).unwrap();
    }
    let ids = workers(&mut world, node.tile, 13);
    for id in &ids[..12] {
        world.issue_gather(PlayerId(0), *id, node.id).unwrap();
    }
    for _ in 0..1200 {
        world.advance();
    }
    assert!(
        ids[..12].iter().all(|id| {
            world
                .unit(*id)
                .unwrap()
                .carried
                .is_some_and(|load| load.amount == 10)
        }),
        "node={node:?}, workers={:?}",
        ids[..12]
            .iter()
            .map(|id| (world.unit(*id), world.gather_target(*id)))
            .collect::<Vec<_>>()
    );
    world.issue_gather(PlayerId(0), ids[12], node.id).unwrap();
    assert_eq!(world.gather_target(ids[12]), Some(next.id));
    world.remove_unit(ids[0]);
    world.issue_gather(PlayerId(0), ids[12], node.id).unwrap();
    assert_eq!(world.gather_target(ids[12]), Some(node.id));
}

#[test]
fn exhausted_workers_skip_a_nearer_tree_whose_slots_are_full() {
    let (mut world, old) = replacement::cleared_work_area(ResourceKind::Wood);
    let mut nodes = Vec::new();
    if let Terrain::Map { generator, overlay } = &mut world.terrain {
        for dx in [-2, -5] {
            let node =
                ResourceNode::starting(TileCoord::new(old.tile.x + dx, old.tile.y), old.kind)
                    .unwrap();
            overlay.insert_starting_resource(generator, node).unwrap();
            nodes.push(node);
        }
    }
    let ids = workers(&mut world, old.tile, 13);
    for id in &ids[..12] {
        world.issue_gather(PlayerId(0), *id, nodes[0].id).unwrap();
    }
    world.issue_gather(PlayerId(0), ids[12], old.id).unwrap();
    if let Terrain::Map { generator, overlay } = &mut world.terrain {
        overlay
            .deplete(generator, old.id, old.initial_amount)
            .unwrap();
    }
    for _ in 0..600 {
        world.advance();
        if world.gather_target(ids[12]) != Some(old.id) {
            assert_eq!(world.gather_target(ids[12]), Some(nodes[1].id));
            return;
        }
    }
    panic!("the exhausted worker never retargeted");
}

#[test]
fn carcass_workers_choose_a_neighbour_when_all_work_spots_are_reserved() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let ids = workers(&mut world, TileCoord::new(100, 100), 6);
    let mut animals = Vec::new();
    for x in [100, 104] {
        let animal = world
            .spawn_animal(
                crate::AnimalKind::Deer,
                position(TileCoord::new(x, 100)).unwrap(),
            )
            .unwrap();
        world.strike_animal(animal, ids[0], 255);
        animals.push(animal);
    }
    for _ in 0..20 {
        world.advance();
    }
    for id in &ids {
        world.begin_carcass_gather(*id, animals[0]).unwrap();
    }
    let mut targets = BTreeMap::<EntityId, usize>::new();
    for id in ids {
        *targets
            .entry(world.economy.gatherers[&id].animal.unwrap())
            .or_default() += 1;
    }
    assert_eq!(targets.len(), 2);
    assert!(targets.values().all(|n| *n <= 4));
}

#[test]
fn reservations_cover_one_hundred_players_and_twenty_thousand_workers() {
    let mut jobs = Gatherers::default();
    let template = fresh_task();
    for player in 0..100_u32 {
        for worker in 0..200_u32 {
            let id = EntityId(player * 200 + worker);
            let mut task = template.clone();
            task.node = u64::from(player * 100 + worker / 2) + 1;
            jobs.insert(id, task);
        }
    }
    assert_eq!(jobs.len(), 20_000);
    for node in 1..=10_000 {
        assert_eq!(jobs.count_target(WorkTarget::Node(node)), 2);
    }
    for worker in 0..20_000 {
        let id = EntityId(worker);
        let node = jobs[&id].node;
        assert_eq!(
            jobs.occupied(WorkTarget::Node(node), id),
            usize::from(worker % 2 == 0)
        );
        jobs.remove(&id);
    }
    assert!(jobs.is_empty());
    for node in 1..=10_000 {
        assert_eq!(jobs.count_target(WorkTarget::Node(node)), 0);
    }
}

#[test]
fn a_distant_initial_order_can_find_free_nodes_in_the_clicked_work_area() {
    let (mut world, node) = replacement::cleared_work_area(ResourceKind::Wood);
    let next =
        ResourceNode::starting(TileCoord::new(node.tile.x - 3, node.tile.y), node.kind).unwrap();
    if let Terrain::Map { generator, overlay } = &mut world.terrain {
        overlay.insert_starting_resource(generator, next).unwrap();
    }
    let ids = workers(&mut world, node.tile, 12);
    for id in ids {
        world.issue_gather(PlayerId(0), id, node.id).unwrap();
    }
    let distant = world
        .spawn_villager(
            PlayerId(0),
            position(TileCoord::new((node.tile.x + 45).min(245), node.tile.y)).unwrap(),
        )
        .unwrap();
    world.issue_gather(PlayerId(0), distant, node.id).unwrap();
    assert_eq!(world.gather_target(distant), Some(next.id));
}

#[test]
fn a_hunter_uses_another_carcass_when_idle_units_block_every_work_tile() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let hunter = world
        .spawn_villager(PlayerId(0), position(TileCoord::new(97, 100)).unwrap())
        .unwrap();
    let mut animals = Vec::new();
    for x in [100, 104] {
        let animal = world
            .spawn_animal(
                crate::AnimalKind::Deer,
                position(TileCoord::new(x, 100)).unwrap(),
            )
            .unwrap();
        world.strike_animal(animal, hunter, 255);
        animals.push(animal);
    }
    for point in crate::game_world::movement_step::slots::perimeter_positions(
        TileRect::from_xywh(100, 100, 1, 1),
        crate::UnitKind::Villager,
    ) {
        world.spawn_villager(PlayerId(1), point).unwrap();
    }
    for _ in 0..20 {
        world.advance();
    }
    world.begin_carcass_gather(hunter, animals[0]).unwrap();
    assert_eq!(world.economy.gatherers[&hunter].animal, Some(animals[1]));
}

#[test]
fn a_returning_hunter_replaces_a_carcass_whose_work_tiles_became_blocked() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let hunter = world
        .spawn_villager(PlayerId(0), position(TileCoord::new(97, 100)).unwrap())
        .unwrap();
    let mut animals = Vec::new();
    for x in [100, 104] {
        let animal = world
            .spawn_animal(
                crate::AnimalKind::Deer,
                position(TileCoord::new(x, 100)).unwrap(),
            )
            .unwrap();
        world.strike_animal(animal, hunter, 255);
        animals.push(animal);
    }
    for _ in 0..20 {
        world.advance();
    }
    world.begin_carcass_gather(hunter, animals[0]).unwrap();
    let mut task = world.economy.gatherers.remove(&hunter).unwrap();
    let index = world.lookup[&hunter];
    world.stop_movement(index);
    for point in crate::game_world::movement_step::slots::perimeter_positions(
        TileRect::from_xywh(100, 100, 1, 1),
        crate::UnitKind::Villager,
    ) {
        world.spawn_villager(PlayerId(1), point).unwrap();
    }
    assert!(world.resume_resource(hunter, &mut task));
    assert_eq!(task.animal, Some(animals[1]));
}
