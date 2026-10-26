//! The owner's long-game freeze: after about half an hour most of a player's
//! units stopped taking orders while a few still moved. A live 100-player
//! match showed units waiting for a route for the whole match, their search
//! restarted from nothing by every refused order (the AI re-sends refused
//! moves), so a search sealed off from its target never reached the
//! hopeless bound and held one of its player's two search slots forever.
use super::fairness::{FAIR_ROUTE_WORK_RESERVE, beside_ring, sealed_ring_world, tile_position};
use super::*;

/// Sends `id` into the sealed ring at `(cx, cy)`: a search with no route.
fn send_into_ring(world: &mut GameWorld, id: EntityId, cx: i32, cy: i32) {
    world.planning_budget = FAIR_ROUTE_WORK_RESERVE;
    assert!(
        world
            .issue_move(id, tile_position(cx, cy))
            .expect("accepted")
    );
}

/// An order the world refuses after planning: its outcome is cached as
/// unreachable from where the unit stands.
fn refused_order(world: &mut GameWorld, id: EntityId, to: TileCoord) -> GameWorldError {
    let origin = world.unit(id).expect("unit").position.tile_floor();
    world.navigation_cache.insert(
        origin,
        to,
        0,
        unchanged_area(),
        aoe_map::MovementOutcome::Unreachable,
    );
    let destination = WorldPosition::from_tile_center(to).expect("tile");
    world.issue_move(id, destination).expect_err("refused")
}

/// Ground no test changes, as the explored area of a planted cache entry.
fn unchanged_area() -> aoe_core::TileRect {
    aoe_core::TileRect::new(TileCoord::new(0, 0), TileCoord::new(1, 1))
}

#[test]
fn a_refused_order_keeps_the_route_search_already_spent() {
    let mut world = sealed_ring_world(512, 250, 250);
    let id = world
        .spawn_unit(PlayerId(1), beside_ring(250, 250, 0))
        .expect("unit");
    send_into_ring(&mut world, id, 250, 250);
    // One tick retains its search instead of restarting a refused order.
    world.advance();
    let work = world.units[world.lookup[&id]]
        .planner
        .as_ref()
        .expect("searching")
        .work();
    assert!(work > 0);
    let order = world.movement_order(id);

    let error = refused_order(&mut world, id, TileCoord::new(20, 20));

    assert_eq!(error, GameWorldError::Unreachable);
    let unit = &world.units[world.lookup[&id]];
    assert_eq!(unit.planner.as_ref().map(|p| p.work()), Some(work));
    assert_eq!(world.movement_order(id), order);
    assert_eq!(world.route_slots[&PlayerId(1)].searches, 1);
    assert_eq!(world.route_accounting_error(), None);
}

#[test]
fn a_sealed_off_search_ends_on_its_own_and_frees_its_slot() {
    // Nothing else of the player waits, so nothing used to preempt it: the
    // search ran on toward the planner's 12.8M-work bound.
    let mut world = sealed_ring_world(1_024, 500, 500);
    let id = world
        .spawn_unit(PlayerId(1), beside_ring(500, 500, 0))
        .expect("unit");
    send_into_ring(&mut world, id, 500, 500);
    // Scheduling slices no longer end searches. This generic uniform-terrain
    // fixture has no portal atlas; its finite planner safety cap still frees
    // the slot. Real map nearest-point proofs are covered by detours.rs.
    let bound = 4 * crate::MAX_ROUTE_WORK_PER_ORDER / FAIR_ROUTE_WORK_RESERVE;
    let ended = (0..bound).find(|_| {
        world.advance();
        world.movement_order(id).is_none()
    });
    assert!(ended.is_some(), "the hopeless search still runs");
    assert_eq!(
        world.movement_failure(id),
        Some(GameWorldError::PathBudgetExceeded)
    );
    assert!(!world.route_slots.contains_key(&PlayerId(1)));
}

/// Two explorers sent into sealed-off ground, re-ordered every second with
/// orders the world refuses, as the AI does: before the fix their searches
/// restarted forever, held both of the player's search slots, and every
/// later order of that player waited for good.
#[test]
fn refused_orders_to_sealed_off_explorers_never_freeze_a_players_army() {
    let mut world = sealed_ring_world(1_024, 500, 500);
    let player = PlayerId(1);
    let explorers = (0..2)
        .map(|raw| {
            let id = world
                .spawn_unit(player, beside_ring(500, 500, raw))
                .expect("explorer");
            send_into_ring(&mut world, id, 500, 500);
            id
        })
        .collect::<Vec<_>>();
    let army = (0..20)
        .map(|n| {
            world
                .spawn_unit(player, tile_position(100 + n, 100))
                .expect("soldier")
        })
        .collect::<Vec<_>>();
    world.advance();
    for &id in &army {
        world.planning_budget = 0;
        world
            .issue_move(id, tile_position(160, 300))
            .expect("accepted");
    }
    let mut arrived_by = None;
    for tick in 0..6_000_u32 {
        if tick % 20 == 0 {
            for &id in &explorers {
                if world.movement_order(id).is_some() {
                    let _ = refused_order(&mut world, id, TileCoord::new(20, 20 + tick as i32 % 7));
                }
            }
        }
        world.advance();
        if army.iter().all(|id| {
            let unit = world.unit(*id).expect("soldier");
            unit.moving || world.movement_order(*id).is_none()
        }) {
            arrived_by = Some(tick);
            break;
        }
    }
    let waiting = army
        .iter()
        .filter(|id| world.unit(**id).expect("soldier").planning)
        .count();
    assert!(
        arrived_by.is_some(),
        "{waiting} of 20 soldiers still wait for a route; slots {:?}",
        world.route_slots.get(&player)
    );
}

/// An attacker chasing a target that keeps moving re-plans once a second.
/// While its route is still being searched the chase is not re-planned: a
/// new order restarted the search, so a chase whose search needs more than
/// a second never moved.
#[test]
fn a_chase_does_not_restart_the_route_search_it_waits_for() {
    let mut world = sealed_ring_world(1_024, 500, 500);
    let target = world
        .spawn_unit(PlayerId(2), tile_position(500, 500))
        .expect("target");
    let attacker = world
        .spawn_unit(PlayerId(1), beside_ring(500, 500, 0))
        .expect("attacker");
    world.planning_budget = FAIR_ROUTE_WORK_RESERVE;
    world
        .issue_attack(PlayerId(1), attacker, target)
        .expect("attack accepted");
    let mut last_work = 0;
    // A busy tick's share: the search needs several repath intervals.
    for tick in 0..80_i32 {
        // The target paces inside its sealed ring, so every repath is stale.
        if tick % 5 == 0 {
            let x = 499 + (tick / 5) % 3;
            let _ = world.issue_move(target, tile_position(x, 500));
        }
        world.planning_budget = 2_048;
        world.advance();
        let Some(planner) = world.units[world.lookup[&attacker]].planner.as_ref() else {
            break;
        };
        assert!(
            planner.work() >= last_work,
            "tick {tick}: the chase restarted its search ({} after {last_work})",
            planner.work()
        );
        last_work = planner.work();
    }
    assert!(last_work > 0, "the attacker never searched");
}

/// A tree felled or a building placed far away used to clear every route
/// planner in the world. In a busy match that happens every few seconds, so a
/// search needing longer than that never finished. Opened ground now keeps
/// every planner; a newly blocked area replans only the planners it touches.
#[test]
fn collision_changes_elsewhere_keep_route_searches() {
    let mut world = sealed_ring_world(1_024, 500, 500);
    let id = world
        .spawn_unit(PlayerId(1), beside_ring(500, 500, 0))
        .expect("unit");
    send_into_ring(&mut world, id, 500, 500);
    world.planning_budget = 2_048;
    world.advance();
    let work = |world: &GameWorld| {
        world.units[world.lookup[&id]]
            .planner
            .as_ref()
            .map(|p| p.work())
    };
    let before = work(&world).expect("searching");
    world.collision_opened(unchanged_area());
    assert_eq!(work(&world), Some(before), "opened ground keeps the search");
    let far = aoe_core::TileRect::new(TileCoord::new(10, 10), TileCoord::new(12, 12));
    world.collision_closed(far);
    assert_eq!(
        work(&world),
        Some(before),
        "a far building keeps the search"
    );
    let searched = aoe_core::TileRect::new(TileCoord::new(500, 429), TileCoord::new(501, 430));
    assert!(
        world.units[world.lookup[&id]]
            .planner
            .as_ref()
            .expect("searching")
            .touches(searched)
    );
    world.collision_closed(searched);
    assert_eq!(
        work(&world),
        None,
        "a building on the searched ground replans"
    );
    assert_eq!(world.route_accounting_error(), None);
}

/// F7: a reverse flood has already rejected the corridor's entrance when
/// its foundation is cancelled. Finishing that stale flood must not stop
/// the existing order as unreachable.
#[test]
fn cancelling_an_entrance_during_a_reverse_flood_keeps_the_move_reachable() {
    use crate::BuildingKind;

    let config = WorldConfig::new(512, 512, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    let wall = |world: &mut GameWorld, x, y, complete| {
        world
            .insert_building(
                PlayerId(0),
                BuildingKind::PalisadeWall,
                aoe_core::TileRect::from_xywh(x, y, 1, 1),
                complete,
            )
            .expect("wall")
    };
    for x in 199..=264 {
        wall(&mut world, x, 199, true);
        wall(&mut world, x, 201, true);
    }
    wall(&mut world, 264, 200, true);
    let entrance = wall(&mut world, 199, 200, false);
    let destination = tile_position(200, 200);
    // One hundred players share the fair budget; no issue-time fast work
    // is left. Serve their searches without moving the stacked test units.
    let mut units = Vec::new();
    world.fast_route_budget = 0;
    for player in 0..100 {
        let id = world
            .spawn_unit(PlayerId(player), tile_position(198, 200))
            .expect("unit");
        assert_eq!(world.issue_move(id, destination), Ok(true));
        units.push(id);
    }
    let id = units[0];
    let index = world.lookup[&id];
    for _ in 0..7 {
        world.serve_route_searches(MAX_ROUTE_WORK_PER_TICK, &mut Vec::new());
    }
    let planner = world.units[index].planner.as_ref().expect("searching");
    let reverse_work = planner.work() - planner.leg_work();
    assert!(
        (9..512).contains(&reverse_work),
        "the destination's eight neighbours were processed, but the 64-tile flood is unfinished: {reverse_work}"
    );
    let order = world.movement_order(id);
    world
        .issue_cancel_construction(PlayerId(0), entrance)
        .expect("cancel entrance");
    assert!(world.tile_passable(TileCoord::new(199, 200)));
    assert_eq!(world.movement_order(id), order);
    assert_eq!(world.route_accounting_error(), None);
    // Only route planning matters here; remove the background pressure
    // before checking that the original unit actually crosses the opening.
    for other in units.into_iter().skip(1) {
        let player = world.unit(other).expect("background unit").player;
        world
            .issue_stop(player, other)
            .expect("stop background unit");
    }
    for _ in 0..200 {
        world.advance();
        assert_eq!(world.movement_failure(id), None);
        if world.movement_order(id).is_none() {
            break;
        }
    }
    assert_eq!(world.unit(id).expect("unit").position, destination);
    assert_eq!(world.route_accounting_error(), None);
}
