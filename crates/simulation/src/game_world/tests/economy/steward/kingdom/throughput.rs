use super::*;

fn assert_other_work_stays_bounded(w: &GameWorld) {
    let work = w.steward_work();
    assert!(work.domains <= 1, "fixture has one active domain");
    assert!(work.unit_visits <= 1_024);
    assert!(work.building_visits <= 2_048);
    assert!(work.resource_blocks <= 25 * work.resource_searches);
    assert!(work.resource_nodes <= 1_608 * work.resource_searches);
}

fn throughput_realm(count: usize) -> (GameWorld, EntityId, Vec<EntityId>, ResourceNode) {
    let (mut w, steward, workers, node) = wood_realm(count);
    // The old twenty-worker fixture has only three reachable trees (36 slots).
    // Add ordinary finite-yield trees so this tests scheduling forty real jobs;
    // one visit now tasks all forty, so the slots must exceed forty at once.
    let mut added = 0;
    for dy in -6..=6 {
        for dx in -6..=6 {
            let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
            if !w.tile_free(tile) {
                continue;
            }
            let tree = ResourceNode::starting(tile, ResourceKind::Wood).unwrap();
            if let Terrain::Map { generator, overlay } = &mut w.terrain
                && overlay.insert_starting_resource(generator, tree).is_ok()
            {
                added += 1;
            }
            if added == 12 {
                break;
            }
        }
        if added == 12 {
            break;
        }
    }
    assert_eq!(
        added, 12,
        "forty-worker fixture needs enough real tree slots"
    );
    w.resource_chunks.get_mut().unwrap().clear();
    w.resource_collision_changed();
    (w, steward, workers, node)
}

pub(super) fn missing_work(w: &GameWorld, steward: EntityId, workers: &[EntityId]) -> String {
    let mut targets = std::collections::BTreeMap::new();
    let mut missing = Vec::new();
    for &worker in workers {
        if let Some(target) = w.gather_target(worker) {
            *targets.entry(target).or_insert(0) += 1;
        }
        if w.steward_controller(worker) == Some(steward)
            && w.gathering_kind(worker) == Some(ResourceKind::Wood)
        {
            continue;
        }
        let u = w.unit(worker).unwrap();
        let eligible = |node: &ResourceNode| {
            w.node_has_work_slot(node.id, worker)
                && w.economy.steward_homes[&steward].owns(position(node.tile))
        };
        let local = w.nearest_resource_matching(
            u.position.tile_floor(),
            ResourceKind::Wood,
            10,
            None,
            eligible,
        );
        let camp = w
            .player_buildings_of(PlayerId(0), BuildingKind::LumberCamp)
            .next()
            .and_then(|b| {
                w.nearest_resource_matching(b.footprint.min, ResourceKind::Wood, 10, None, eligible)
            });
        missing.push((
            worker,
            u.task,
            u.moving,
            u.planning,
            w.gathering_kind(worker),
            w.steward_controller(worker),
            local.map(|n| (n.id, n.tile)),
            camp.map(|n| (n.id, n.tile)),
        ));
    }
    format!("targets {targets:?}; missing {missing:?}")
}

#[test]
fn forty_idle_workers_get_real_wood_orders_within_two_game_seconds_and_replay() {
    let (mut w, steward, workers, _) = throughput_realm(40);
    let (mut replay, replay_steward, replay_workers, _) = throughput_realm(40);
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    reserves(&mut replay, 30_000, 1_000, 30_000, 30_000);
    assert!(
        workers
            .iter()
            .all(|id| w.unit(*id).unwrap().task == UnitTask::Idle)
    );
    assert_eq!(w.canonical_hash(), replay.canonical_hash());
    let mut searches = 0;
    let mut passes = Vec::new();
    let deadline = 2 * w.config().tick_hz;
    for _ in 0..deadline {
        w.advance();
        replay.advance();
        assert_other_work_stays_bounded(&w);
        assert_other_work_stays_bounded(&replay);
        searches += w.steward_work().resource_searches;
        if w.steward_work().domains > 0 {
            passes.push(w.steward_work());
        }
        assert_eq!(w.canonical_hash(), replay.canonical_hash());
    }
    // One finder per idle worker at most, plus its retries: work follows
    // the idle workers, not a fixed count.
    assert!(searches <= 2 * workers.len(), "{searches} searches");
    assert_eq!(
        assigned(&w, steward, &workers, ResourceKind::Wood),
        40,
        "every idle worker must receive a real productive order in <=2 game seconds; passes {:?}; {}",
        passes,
        missing_work(&w, steward, &workers)
    );
    assert_eq!(
        assigned(&replay, replay_steward, &replay_workers, ResourceKind::Wood),
        40
    );
}

#[test]
fn forty_productive_managed_farmers_keep_their_jobs_without_taking_manual_workers() {
    let (mut w, steward, workers, _) = throughput_realm(41);
    reserves(&mut w, 1_000, 30_000, 30_000, 30_000);
    let town_center = w.steward_town_center(steward).unwrap();
    let anchor = w.building(town_center).unwrap().footprint.min;
    make_farmers(&mut w, &workers, TileCoord::new(anchor.x + 7, anchor.y + 7));
    assert!(
        workers
            .iter()
            .all(|id| w.steward_controller(*id) == Some(steward))
    );
    let manual = workers[40];
    let destination = distant_manual_destination(&w, w.unit(manual).unwrap().position);
    w.issue_move(manual, destination).unwrap();
    assert_eq!(w.steward_controller(manual), None);
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    let deadline = 2 * w.config().tick_hz;
    let mut passes = Vec::new();
    for _ in 0..deadline {
        w.advance();
        if w.steward_work().domains > 0 {
            passes.push(w.steward_work());
        }
        assert_other_work_stays_bounded(&w);
        assert_eq!(
            w.steward_controller(manual),
            None,
            "manual movement must remain protected"
        );
    }
    assert_eq!(
        assigned(&w, steward, &workers[..40], ResourceKind::Food),
        40,
        "all forty productive farmers must keep working during the shortage; passes {passes:?}"
    );
    assert!(w.unit(manual).unwrap().moving || w.unit(manual).unwrap().planning);
}

#[test]
fn dry_ore_searches_leave_enough_budget_to_assign_wood_workers() {
    let (mut world, steward, workers, _) = throughput_realm(40);
    let home = world.economy.steward_homes[&steward].center.tile_floor();
    let Terrain::Map { generator, overlay } = &world.terrain else {
        panic!("prepared map fixture");
    };
    let radius = crate::STEWARD_RULES.radius_tiles;
    let span = |axis: i32| {
        (axis - radius).max(0).div_euclid(aoe_map::CHUNK_TILES)
            ..=(axis + radius).div_euclid(aoe_map::CHUNK_TILES)
    };
    let mut ores = overlay.starting_nodes().collect::<Vec<_>>();
    for y in span(home.y) {
        for x in span(home.x) {
            ores.extend(generator.chunk(x, y).unwrap().resources);
        }
    }
    for node in ores
        .into_iter()
        .filter(|n| matches!(n.kind, ResourceKind::Gold | ResourceKind::Stone))
    {
        world
            .deplete_resource(node.id, node.initial_amount)
            .unwrap();
    }
    reserves(&mut world, 30_000, 1_000, 0, 0);
    for _ in 0..2 * world.config().tick_hz {
        world.advance();
        assert_other_work_stays_bounded(&world);
    }
    assert!(
        assigned(&world, steward, &workers, ResourceKind::Wood) >= 24,
        "repeated ore misses must not starve available wood work: {}",
        missing_work(&world, steward, &workers)
    );
}

#[test]
fn a_severe_stone_shortage_redirects_a_productive_wood_worker() {
    let (mut world, steward, workers, wood) = throughput_realm(8);
    let stone = (3..=9)
        .flat_map(|dx| (-4..=4).map(move |dy| TileCoord::new(wood.tile.x + dx, wood.tile.y + dy)))
        .find(|tile| world.tile_free(*tile))
        .and_then(|tile| ResourceNode::starting(tile, ResourceKind::Stone))
        .unwrap();
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        panic!("prepared map fixture");
    };
    overlay.insert_starting_resource(generator, stone).unwrap();
    world.resource_chunks.get_mut().unwrap().clear();
    world.resource_collision_changed();
    place_near(&mut world, BuildingKind::MiningCamp, stone.tile);
    reserves(&mut world, 30_000, 1_000, 30_000, 30_000);
    for _ in 0..2 * world.config().tick_hz {
        world.advance();
    }
    assert_eq!(assigned(&world, steward, &workers, ResourceKind::Wood), 8);

    reserves(&mut world, 30_000, 1_000, 30_000, 0);
    for _ in 0..100 * world.config().tick_hz {
        world.advance();
        if resource_stock(&world, ResourceKind::Stone) > 0 {
            assert!(assigned(&world, steward, &workers, ResourceKind::Stone) > 0);
            assert!(assigned(&world, steward, &workers, ResourceKind::Wood) > 0);
            return;
        }
    }
    panic!(
        "an existing Steward must redirect productive labor and deliver stone: {}",
        missing_work(&world, steward, &workers)
    );
}
