use super::*;

fn assert_other_work_stays_bounded(w: &GameWorld) {
    let work = w.vassal_work();
    assert!(work.domains <= 1, "fixture has one active domain");
    assert!(work.orders <= 64);
    assert!(work.unit_visits <= 1_024);
    assert!(work.building_visits <= 2_048);
    assert!(work.resource_blocks <= 25 * work.resource_searches);
    assert!(work.resource_nodes <= 1_608 * work.resource_searches);
    assert!(
        work.resource_searches <= 8,
        "more orders must not increase finder admission"
    );
    assert!(
        work.site_checks <= 32,
        "more orders must not increase placement admission"
    );
}

fn throughput_realm(count: usize) -> (GameWorld, EntityId, Vec<EntityId>, ResourceNode) {
    let (mut w, vassal, workers, node) = wood_realm(count);
    // The old twenty-worker fixture has only three reachable trees (36 slots).
    // Add ordinary finite-yield trees so this tests scheduling forty real jobs.
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
            if added == 8 {
                break;
            }
        }
        if added == 8 {
            break;
        }
    }
    assert_eq!(
        added, 8,
        "forty-worker fixture needs enough real tree slots"
    );
    w.resource_chunks.get_mut().unwrap().clear();
    w.resource_collision_changed();
    (w, vassal, workers, node)
}

pub(super) fn missing_work(w: &GameWorld, vassal: EntityId, workers: &[EntityId]) -> String {
    let mut targets = std::collections::BTreeMap::new();
    let mut missing = Vec::new();
    for &worker in workers {
        if let Some(target) = w.gather_target(worker) {
            *targets.entry(target).or_insert(0) += 1;
        }
        if w.vassal_controller(worker) == Some(vassal)
            && w.gathering_kind(worker) == Some(ResourceKind::Wood)
        {
            continue;
        }
        let u = w.unit(worker).unwrap();
        let eligible = |node: &ResourceNode| {
            w.node_has_work_slot(node.id, worker)
                && w.economy.vassal_homes[&vassal].owns(position(node.tile))
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
            w.vassal_controller(worker),
            local.map(|n| (n.id, n.tile)),
            camp.map(|n| (n.id, n.tile)),
        ));
    }
    format!("targets {targets:?}; missing {missing:?}")
}

#[test]
fn forty_idle_workers_get_real_wood_orders_within_two_game_seconds_and_replay() {
    let (mut w, vassal, workers, _) = throughput_realm(40);
    let (mut replay, replay_vassal, replay_workers, _) = throughput_realm(40);
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
        searches += w.vassal_work().resource_searches;
        if w.vassal_work().domains > 0 {
            passes.push(w.vassal_work());
        }
        assert_eq!(w.canonical_hash(), replay.canonical_hash());
    }
    assert!(searches <= 16);
    assert_eq!(
        assigned(&w, vassal, &workers, ResourceKind::Wood),
        40,
        "every idle worker must receive a real productive order in <=2 game seconds; passes {:?}; {}",
        passes,
        missing_work(&w, vassal, &workers)
    );
    assert_eq!(
        assigned(&replay, replay_vassal, &replay_workers, ResourceKind::Wood),
        40
    );
}

#[test]
fn forty_unloaded_managed_farmers_retask_within_two_seconds_without_taking_manual_workers() {
    let (mut w, vassal, workers, _) = throughput_realm(41);
    reserves(&mut w, 1_000, 30_000, 30_000, 30_000);
    let castle = w.vassal_castle(vassal).unwrap();
    let anchor = w.building(castle).unwrap().footprint.min;
    assign_farms(&mut w, &workers, TileCoord::new(anchor.x + 7, anchor.y + 7));
    advance(&mut w, 20);
    assert!(
        workers
            .iter()
            .all(|id| w.vassal_controller(*id) == Some(vassal))
    );
    assert!(
        workers
            .iter()
            .all(|id| w.unit(*id).unwrap().carried.is_none())
    );
    let manual = workers[40];
    let destination = distant_manual_destination(&w, w.unit(manual).unwrap().position);
    w.issue_move(manual, destination).unwrap();
    assert_eq!(w.vassal_controller(manual), None);
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    let deadline = 2 * w.config().tick_hz;
    let mut passes = Vec::new();
    for _ in 0..deadline {
        w.advance();
        if w.vassal_work().domains > 0 {
            passes.push(w.vassal_work());
        }
        assert_other_work_stays_bounded(&w);
        assert_eq!(
            w.vassal_controller(manual),
            None,
            "manual movement must remain protected"
        );
    }
    assert_eq!(
        assigned(&w, vassal, &workers[..40], ResourceKind::Wood),
        40,
        "all forty eligible managed workers must respond in <=2 game seconds; passes {:?}; {}",
        passes,
        missing_work(&w, vassal, &workers[..40])
    );
    assert!(w.unit(manual).unwrap().moving || w.unit(manual).unwrap().planning);
}
