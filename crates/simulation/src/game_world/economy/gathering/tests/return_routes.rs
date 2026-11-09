//! A previously proven dead perimeter must not issue another route attempt.
use super::*;
use aoe_map::{MovementOutcome, RoutePlanner, RoutePlannerPoll};

fn isolated_perimeter() -> (GameWorld, EntityId, GatherTask, TileCoord) {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world
        .place_town_center(PlayerId(0), TileCoord::new(128, 128))
        .unwrap();
    let isolated = TileCoord::new(127, 129);
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx != 0 || dy != 0 {
                world
                    .economy
                    .blocked_tiles
                    .insert(TileCoord::new(isolated.x + dx, isolated.y + dy));
            }
        }
    }
    let worker = world
        .spawn_villager(PlayerId(0), position(TileCoord::new(88, 129)).unwrap())
        .unwrap();
    let index = world.lookup[&worker];
    world.units[index].state.carried = Some(CarriedResource {
        kind: ResourceKind::Wood,
        amount: 10,
    });
    (world, worker, fresh_task(), isolated)
}

#[test]
fn a_cached_unreachable_perimeter_preserves_the_choice_and_canonical_state_without_an_attempt() {
    let (mut cold, worker, mut cold_task, isolated) = isolated_perimeter();
    let (mut warm, other, mut warm_task, _) = isolated_perimeter();
    assert_eq!(worker, other);
    assert_eq!(cold.canonical_hash(), warm.canonical_hash());
    let mut planner = RoutePlanner::new(
        cold.unit(worker).unwrap().position.tile_floor(),
        isolated,
        crate::MAX_ROUTE_WORK_PER_ORDER,
    );
    assert_eq!(
        warm.probe_route_poll(&mut planner, 8192, UnitKind::Villager),
        RoutePlannerPoll::Unreachable
    );
    warm.cache_route(&planner, UnitKind::Villager, &MovementOutcome::Unreachable);
    assert_eq!(
        cold.canonical_hash(),
        warm.canonical_hash(),
        "derived proof is not canonical"
    );
    assert!(cold.begin_return(worker, &mut cold_task));
    assert!(warm.begin_return(worker, &mut warm_task));
    assert_ne!(cold_task.return_target, Some(isolated));
    assert_eq!(cold_task, warm_task);
    assert_eq!(
        cold.canonical_hash(),
        warm.canonical_hash(),
        "including route budget and failed-target memo"
    );
    assert_eq!(
        warm.gather_work().return_routes + 1,
        cold.gather_work().return_routes,
        "a proven unreachable tile is filtered before route issue"
    );
}

fn fenced_source() -> (GameWorld, EntityId) {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    for y in 98..=102 {
        for x in 98..=102 {
            if x == 98 || x == 102 || y == 98 || y == 102 {
                world
                    .insert_building(
                        PlayerId(0),
                        crate::BuildingKind::PalisadeWall,
                        TileRect::from_xywh(x, y, 1, 1),
                        true,
                    )
                    .unwrap();
            }
        }
    }
    let worker = world
        .spawn_villager(PlayerId(0), position(TileCoord::new(100, 100)).unwrap())
        .unwrap();
    (world, worker)
}

#[test]
fn source_pocket_rejection_matches_normal_orders_at_every_budget_boundary() {
    for budget in [0, 1, 82, 83, 84, 4096, 8192] {
        let (mut reference, worker) = fenced_source();
        let (mut filtered, _) = fenced_source();
        let pocket = filtered
            .route_origin_pocket(UnitKind::Villager, TileCoord::new(100, 100))
            .unwrap()
            .unwrap();
        for to in [
            TileCoord::new(99, 99),
            TileCoord::new(100, 100),
            TileCoord::new(97, 100),
            TileCoord::new(103, 100),
            TileCoord::new(110, 110),
            TileCoord::new(160, 100),
        ] {
            reference.refill_route_budgets();
            filtered.refill_route_budgets();
            reference.fast_route_budget = budget;
            filtered.fast_route_budget = budget;
            let expected = reference.issue_task_move(worker, position(to).unwrap());
            let rejected = filtered.reject_known_unreachable_task_move(worker, to, Some(&pocket));
            let actual = if rejected {
                Err(GameWorldError::Unreachable)
            } else {
                filtered.issue_task_move(worker, position(to).unwrap())
            };
            assert_eq!(expected, actual, "budget {budget}, target {to:?}");
            assert_eq!(
                reference.canonical_hash(),
                filtered.canonical_hash(),
                "budget {budget}, target {to:?}"
            );
        }
    }
}

#[test]
fn opening_the_source_component_discards_the_proof_and_cached_failures() {
    let (mut world, worker) = fenced_source();
    let to = TileCoord::new(110, 100);
    assert_eq!(
        world.issue_task_move(worker, position(to).unwrap()),
        Err(GameWorldError::Unreachable)
    );
    let wall = world.building_at_tile(TileCoord::new(102, 100)).unwrap();
    world.remove_building(wall);
    assert!(
        world
            .route_origin_pocket(UnitKind::Villager, TileCoord::new(100, 100))
            .unwrap()
            .is_none()
    );
    // The worker's short memo is existing order policy. An explicit retry
    // after it expires must see the opening, including the route-cache invalidation.
    world.tick.0 += 201;
    world.refill_route_budgets();
    assert!(!world.reject_known_unreachable_task_move(worker, to, None));
    assert!(world.issue_task_move(worker, position(to).unwrap()).is_ok());
}

#[test]
fn a_sealed_worker_checks_its_component_once_instead_of_routing_to_every_perimeter_tile() {
    let (mut normal, worker) = fenced_source();
    let (mut filtered, _) = fenced_source();
    let rect = TileRect::from_xywh(128, 128, 4, 4);
    for world in [&mut normal, &mut filtered] {
        world
            .insert_building(PlayerId(0), crate::BuildingKind::TownCenter, rect, true)
            .unwrap();
        let index = world.lookup[&worker];
        world.units[index].state.carried = Some(CarriedResource {
            kind: ResourceKind::Wood,
            amount: 10,
        });
    }
    let mut expected = fresh_task();
    let mut actual = expected.clone();
    let from = normal.unit(worker).unwrap().position.tile_floor();
    let mut tiles = perimeter(rect);
    tiles.sort_by_key(|tile| (distance_squared(from, *tile), tile.y, tile.x));
    // The ordinary movement API is the oracle for every candidate's rejection,
    // its work charge, and the final memo. The gatherer still remembers all
    // failed tiles in exactly that order before entering the same wait state.
    for &tile in &tiles {
        assert_eq!(
            normal.issue_task_move(worker, position(tile).unwrap()),
            Err(GameWorldError::Unreachable)
        );
        expected.failed_dropoffs.insert(tile);
    }
    expected.phase = GatherPhase::WaitingDropoff;
    expected.dropoff = None;
    expected.return_target = None;
    expected.retry_tick = normal.tick.0 + u64::from(normal.config.tick_hz);
    let index = normal.lookup[&worker];
    normal.units[index].state.task = UnitTask::Carrying;
    normal.refresh_spacing_order(worker);
    assert!(!filtered.begin_return(worker, &mut actual));
    assert_eq!(expected, actual);
    assert_eq!(normal.canonical_hash(), filtered.canonical_hash());
    assert_eq!(filtered.gather_work().return_routes, 1);
    assert_eq!(
        filtered.gather_work().return_known_unreachable,
        tiles.len() as u64 - 1
    );
}
