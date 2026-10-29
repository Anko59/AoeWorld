use super::*;

pub(super) fn cleared_work_area(kind: ResourceKind) -> (GameWorld, ResourceNode) {
    let (mut world, original) = resource_mainland();
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        unreachable!()
    };
    overlay
        .deplete(generator, original.id, original.initial_amount)
        .unwrap();
    let tile = (40..210)
        .flat_map(|y| (40..210).map(move |x| TileCoord::new(x, y)))
        .find(|at| {
            (-10..=4).all(|dx| {
                (-3..=3).all(|dy| {
                    let tile = TileCoord::new(at.x + dx, at.y + dy);
                    generator.object_at(tile).is_none()
                        && generator.tile_at(tile).is_some_and(|t| t.passable)
                })
            })
        })
        .expect("clear deterministic work area");
    let node = ResourceNode::starting(tile, kind).unwrap();
    overlay.insert_starting_resource(generator, node).unwrap();
    (world, node)
}

#[test]
fn exhausted_workers_choose_from_the_used_dropoff_for_wood_gold_and_stone() {
    for kind in [ResourceKind::Wood, ResourceKind::Gold, ResourceKind::Stone] {
        let (mut world, old) = cleared_work_area(kind);
        let camp_tile = TileCoord::new(old.tile.x - 9, old.tile.y - 2);
        let camp = world.place_town_center(PlayerId(0), camp_tile).unwrap();
        let near_camp = TileCoord::new(old.tile.x - 3, old.tile.y);
        let near_old = TileCoord::new(old.tile.x + 1, old.tile.y);
        let Terrain::Map { generator, overlay } = &mut world.terrain else {
            unreachable!()
        };
        let mut ids = Vec::new();
        for tile in [near_camp, near_old] {
            let node = ResourceNode::starting(tile, kind).unwrap();
            overlay.insert_starting_resource(generator, node).unwrap();
            ids.push(node.id);
        }
        overlay
            .deplete(generator, old.id, old.initial_amount)
            .unwrap();
        let worker = world
            .spawn_villager(
                PlayerId(0),
                position(TileCoord::new(old.tile.x - 5, old.tile.y)).unwrap(),
            )
            .unwrap();
        let mut task = fresh_task();
        task.node = old.id;
        task.tile = old.tile;
        task.kind = kind;
        task.dropoff = Some(camp);
        assert!(world.resume_resource(worker, &mut task));
        assert_eq!(
            task.node, ids[0],
            "{kind:?}: rank from the drop-off, not the exhausted node"
        );
    }
}

#[test]
fn replacement_skips_a_sealed_node_and_is_deterministic() {
    let run = || {
        let (mut world, old) = cleared_work_area(ResourceKind::Wood);
        let near = TileCoord::new(old.tile.x - 4, old.tile.y);
        let far = TileCoord::new(old.tile.x + 2, old.tile.y);
        let Terrain::Map { generator, overlay } = &mut world.terrain else {
            unreachable!()
        };
        let mut ids = Vec::new();
        for tile in [near, far] {
            let node = ResourceNode::starting(tile, ResourceKind::Wood).unwrap();
            overlay.insert_starting_resource(generator, node).unwrap();
            ids.push(node.id);
        }
        overlay
            .deplete(generator, old.id, old.initial_amount)
            .unwrap();
        for dy in -1..=1 {
            for dx in -1..=1 {
                world
                    .economy
                    .blocked_tiles
                    .insert(TileCoord::new(near.x + dx, near.y + dy));
            }
        }
        let worker = world
            .spawn_villager(
                PlayerId(0),
                position(TileCoord::new(old.tile.x - 7, old.tile.y)).unwrap(),
            )
            .unwrap();
        let mut task = fresh_task();
        task.node = old.id;
        task.tile = old.tile;
        assert!(world.resume_resource(worker, &mut task));
        assert_eq!(task.node, ids[1]);
        (task.node, world.canonical_hash())
    };
    assert_eq!(run(), run());
}

#[test]
fn a_long_dropoff_trip_still_replaces_a_tree_in_the_reachable_work_area() {
    let (mut world, old) = cleared_work_area(ResourceKind::Wood);
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        unreachable!()
    };
    let camp_tile = (old.tile.y..old.tile.y + 20)
        .flat_map(|y| (old.tile.x + 40..old.tile.x + 60).map(move |x| TileCoord::new(x, y)))
        .find(|at| {
            (0..4).all(|dy| {
                (0..4).all(|dx| {
                    generator
                        .object_at(TileCoord::new(at.x + dx, at.y + dy))
                        .is_none()
                })
            })
        })
        .expect("clear distant Town Center");
    let next = ResourceNode::starting(
        TileCoord::new(old.tile.x + 1, old.tile.y),
        ResourceKind::Wood,
    )
    .unwrap();
    overlay.insert_starting_resource(generator, next).unwrap();
    overlay
        .deplete(generator, old.id, old.initial_amount)
        .unwrap();
    let camp = world.place_town_center(PlayerId(0), camp_tile).unwrap();
    let worker = world
        .spawn_villager(
            PlayerId(0),
            position(TileCoord::new(camp_tile.x - 1, camp_tile.y)).unwrap(),
        )
        .unwrap();
    let mut task = fresh_task();
    task.node = old.id;
    task.tile = old.tile;
    task.dropoff = Some(camp);
    task.work_tile = Some(TileCoord::new(old.tile.x - 1, old.tile.y));
    assert!(
        world.resume_resource(worker, &mut task),
        "the search bound is local to the forest, not the long return trip"
    );
    assert_eq!(task.node, next.id);
}

#[test]
fn vassal_refreshes_lumber_camp_after_tree_depletion() {
    use crate::{Age, Stockpile, VASSAL_RULES};
    let (mut world, old) = cleared_work_area(ResourceKind::Wood);
    world.set_fog_disabled(true);
    let next = ResourceNode::starting(
        TileCoord::new(old.tile.x + 2, old.tile.y),
        ResourceKind::Wood,
    )
    .unwrap();
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        unreachable!()
    };
    overlay.insert_starting_resource(generator, next).unwrap();
    let camp_rect = aoe_core::TileRect::from_xywh(old.tile.x - 10, old.tile.y - 1, 2, 2);
    let camp = world
        .insert_building(
            PlayerId(0),
            crate::BuildingKind::LumberCamp,
            camp_rect,
            true,
        )
        .unwrap();
    let castle_rect = aoe_core::TileRect::from_xywh(old.tile.x - 5, old.tile.y + 8, 4, 4);
    let castle = world
        .insert_building(PlayerId(0), crate::BuildingKind::Castle, castle_rect, true)
        .unwrap();
    let p = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.stockpile = Stockpile {
        food: 0,
        wood: 1000,
        gold: 0,
        stone: 0,
    };
    let worker = world
        .spawn_villager(
            PlayerId(0),
            position(TileCoord::new(old.tile.x - 1, old.tile.y)).unwrap(),
        )
        .unwrap();
    let vassal = world
        .spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            position(TileCoord::new(old.tile.x - 6, old.tile.y + 8)).unwrap(),
        )
        .unwrap();
    world.bind_vassal(vassal, castle, castle_rect);
    world.issue_garrison(PlayerId(0), vassal, castle).unwrap();
    world.advance_garrisons(&mut Vec::new());
    let mut task = GatherTask::new(old.id, old.tile, ResourceKind::Wood, None);
    task.dropoff = Some(camp);
    task.work_tile = Some(TileCoord::new(old.tile.x - 1, old.tile.y));
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        unreachable!()
    };
    overlay
        .deplete(generator, old.id, old.initial_amount)
        .unwrap();
    assert!(world.resume_resource(worker, &mut task));
    assert_eq!(task.node, next.id);
    assert_eq!(
        super::super::super::dropoff::ring_distance(camp_rect, old.tile),
        64
    );
    assert_eq!(
        super::super::super::dropoff::ring_distance(camp_rect, next.tile),
        100
    );
    world.economy.gatherers.insert(worker, task);
    world
        .economy
        .vassals
        .controlled
        .insert(worker, (vassal, ResourceKind::Wood));
    world
        .economy
        .vassal_homes
        .get_mut(&vassal)
        .unwrap()
        .workers
        .insert(worker);
    world.tick = aoe_core::Tick(VASSAL_RULES.period_ticks);
    world.advance_vassals(&mut Vec::new());
    assert_eq!(
        world
            .player_buildings_of(PlayerId(0), crate::BuildingKind::LumberCamp)
            .count(),
        2
    );
}

fn direct_work_reach(w: &GameWorld, kind: UnitKind, start: TileCoord) -> Vec<TileCoord> {
    let mut seen = std::collections::BTreeSet::from([start]);
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(tile) = queue.pop_front() {
        for (dx, dy) in [
            (0, -1),
            (-1, 0),
            (1, 0),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
            (1, 1),
        ] {
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            if seen.len() < 1024
                && !seen.contains(&next)
                && w.building_step_clear(tile, next)
                && w.unit_step_clear(kind, tile, next).unwrap_or(false)
            {
                seen.insert(next);
                queue.push_back(next);
            }
        }
    }
    seen.into_iter().collect()
}

#[test]
fn work_reach_cache_matches_direct_edges_after_ground_changes() {
    let mut w = GameWorld::new(WorldConfig::new(128, 128, Seed(1)).unwrap()).unwrap();
    let check = |w: &GameWorld| {
        for kind in [UnitKind::Villager, UnitKind::Militia, UnitKind::FishingShip] {
            for start in [TileCoord::new(40, 40), TileCoord::new(42, 42)] {
                assert_eq!(
                    w.replacement_reach(kind, start).tiles,
                    direct_work_reach(w, kind, start)
                );
            }
        }
    };
    check(&w);
    let house = w
        .insert_building(
            PlayerId(0),
            crate::BuildingKind::House,
            TileRect::from_xywh(41, 40, 2, 2),
            true,
        )
        .unwrap();
    check(&w);
    w.remove_building(house);
    check(&w);
}

#[test]
fn queued_land_work_waits_for_movers_without_targeting_a_settled_body() {
    let (mut w, node) = cleared_work_area(ResourceKind::Wood);
    let id = w
        .spawn_villager(
            PlayerId(0),
            position(TileCoord::new(node.tile.x - 2, node.tile.y)).unwrap(),
        )
        .unwrap();
    w.issue_gather(PlayerId(0), id, node.id).unwrap();
    let points = w.static_work_positions(UnitKind::Villager, node.tile);
    let occupants = points
        .inner
        .iter()
        .enumerate()
        .map(|(i, &at)| w.spawn_villager(PlayerId(u16::from(i != 0)), at).unwrap())
        .collect::<Vec<_>>();
    w.issue_gather(PlayerId(0), occupants[0], node.id).unwrap();
    w.advance();
    assert_eq!(
        w.unit(occupants[0]).unwrap().task,
        crate::UnitTask::Gathering
    );
    assert!(
        !w.queue_resource_approach(id, node.tile),
        "settled occupants cannot clear a queue"
    );
    let first = w.unit(occupants[0]).unwrap().position;
    w.issue_move(occupants[0], WorldPosition::new(first.x - 1024, first.y))
        .unwrap();
    assert!(w.unit(occupants[0]).unwrap().moving);
    assert!(
        w.queue_resource_approach(id, node.tile),
        "moving traffic can clear the work point"
    );
    for _ in 0..200 {
        w.advance();
        if w.unit(id).unwrap().carried.is_some() {
            return;
        }
    }
    panic!("queued work must finish its approach and collect");
}

#[test]
fn an_unavailable_land_work_point_ends_its_old_order_while_the_owner_collects() {
    let (mut w, node) = cleared_work_area(ResourceKind::Gold);
    w.set_fog_disabled(true);
    let room = TileCoord::new(node.tile.x - 1, node.tile.y - 1);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let tile = TileCoord::new(room.x + dx, room.y + dy);
            if tile != room && tile != node.tile {
                w.insert_building(
                    PlayerId(0),
                    crate::BuildingKind::PalisadeWall,
                    TileRect::from_xywh(tile.x, tile.y, 1, 1),
                    true,
                )
                .unwrap();
            }
        }
    }
    let actor = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::new(node.tile.x * 1024 - 774, node.tile.y * 1024 - 274),
        )
        .unwrap();
    w.issue_gather(PlayerId(0), actor, node.id).unwrap();
    let goal = w.movement_order(actor).unwrap().destination;
    assert_eq!(goal.tile_floor(), room);
    let owner = w.spawn_villager(PlayerId(0), goal).unwrap();
    w.issue_gather(PlayerId(0), owner, node.id).unwrap();
    for _ in 0..128 {
        w.advance();
        assert!(w.movement_obstruction_ticks(actor).unwrap_or(0) <= 128);
    }
    assert!(
        w.gather_target(actor).is_none(),
        "unavailable work must end, not keep circling: {:?}",
        w.unit(actor)
    );
    assert!(w.movement_order(actor).is_none());
    assert_eq!(w.unit(actor).unwrap().task, crate::UnitTask::Idle);
    assert!(
        w.unit(owner).unwrap().carried.is_some(),
        "the reachable owner keeps collecting"
    );
}
