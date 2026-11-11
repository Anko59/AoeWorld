//! The owner's frozen match (100 players, 11,000 units): 669 of the human's
//! 874 "Moving" units did not move in five minutes. Two near searches held
//! their player's near slots for hours: orders into sealed ground flooding a
//! continent, restarted from nothing by every building placed on the flood.
//! Everyone behind them was refused a slot, and the fair share was divided by
//! all waiting units (847 of them), so the holders got 1 or 2 work a tick
//! while the rest of the share went unused. Every waiting unit must now get
//! an answer, a route or a failure that ends its order, in bounded time.
use super::fairness::tile_position;
use super::*;

use crate::game_world::route_scheduler::NEAR_ROUTE_ANSWER_TICKS as NEAR_ANSWER_TICKS;

const CX: i32 = 500;
const CY: i32 = 500;
/// Radius of sealed ground too large for the sealed-destination proof, whose
/// centre is still a near route (at most 64 tiles) from just outside it.
const RADIUS: i32 = 40;

fn sealed_near_world() -> GameWorld {
    let config = WorldConfig::new(1_024, 1_024, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    for dx in -RADIUS..=RADIUS {
        for dy in -RADIUS..=RADIUS {
            if dx.abs() == RADIUS || dy.abs() == RADIUS {
                world
                    .economy
                    .blocked_tiles
                    .insert(TileCoord::new(CX + dx, CY + dy));
            }
        }
    }
    // A wall 140 long: going around it costs more than the fast attempt, so
    // each order needs a search slot, yet it has an ordinary near route.
    for x in 130..270 {
        world.economy.blocked_tiles.insert(TileCoord::new(x, 200));
    }
    world
}

/// Orders into the sealed ground that no fast attempt can settle.
fn hopeless_orders(world: &mut GameWorld, player: PlayerId) -> Vec<EntityId> {
    let out = RADIUS + 1;
    [(0, -out), (0, out)]
        .into_iter()
        .map(|(dx, dy)| {
            let id = world
                .spawn_unit(player, tile_position(CX + dx, CY + dy))
                .expect("unit");
            assert_eq!(world.issue_task_move(id, tile_position(CX, CY)), Ok(true));
            id
        })
        .collect()
}

/// Orders across the wall, spaced so that no two share a route's start.
fn detour_orders(world: &mut GameWorld, player: PlayerId, count: i32) -> Vec<EntityId> {
    (0..count)
        .map(|n| {
            let x = 180 + (n % 20) * 2;
            let y = 190 - (n / 20) * 2;
            let id = world.spawn_unit(player, tile_position(x, y)).expect("unit");
            assert_eq!(world.issue_move(id, tile_position(x, 210)), Ok(true));
            id
        })
        .collect()
}

fn answered(world: &GameWorld, id: EntityId) -> bool {
    let unit = world.unit(id).expect("unit");
    world.movement_order(id).is_none() || !unit.planning
}

#[test]
fn hopeless_near_searches_do_not_starve_the_routes_of_their_player() {
    let mut world = sealed_near_world();
    let player = PlayerId(3);
    let hopeless = hopeless_orders(&mut world, player);
    for _ in 0..10 {
        world.advance();
    }
    assert_eq!(
        world.route_scheduler_stats().searches,
        0,
        "sealed searches are proved without holding slots"
    );
    let detours = detour_orders(&mut world, player, 60);
    let mut served = std::collections::BTreeSet::new();
    let mut oldest = 0;
    for _ in 0..NEAR_ANSWER_TICKS + 5 {
        world.advance();
        oldest = oldest.max(world.route_scheduler_stats().oldest_wait_ticks);
        for &id in &detours {
            if answered(&world, id) && served.insert(id) {
                assert_eq!(world.movement_failure(id), None, "route answer for {id:?}");
                // This checks route service. Stop answered units before
                // their many overlapping detours meet at the wall's ends.
                world.issue_stop(player, id).expect("stop served unit");
            }
        }
    }
    let stats = world.route_scheduler_stats();
    let unanswered = detours.iter().filter(|id| !answered(&world, **id)).count();
    assert_eq!(
        unanswered, 0,
        "units behind the hopeless searches: {stats:?}"
    );
    assert_eq!(served.len(), detours.len(), "{stats:?}");
    assert!(oldest <= NEAR_ANSWER_TICKS, "waited {oldest}: {stats:?}");
    for id in hopeless {
        let unit = world.unit(id).expect("unit");
        assert!(world.movement_order(id).is_none(), "{stats:?}");
        assert!(world.movement_failure(id).is_some());
        assert_eq!(unit.task, crate::UnitTask::Idle);
        assert!(!unit.planning && !unit.moving);
    }
    assert_eq!(world.route_scheduler_stats().searches, 0);
    assert_eq!(world.route_accounting_error(), None);
}

/// The match's pattern: the oldest waiter's work fell from 1.1 million back
/// to 1,037 and rose again, over and over, because every change to the ground
/// it had searched restarted it. Restarts must not extend its answer.
#[test]
fn a_search_restarted_by_ground_changes_still_answers_in_bounded_time() {
    let mut world = sealed_near_world();
    world.fast_route_budget = 0;
    let hopeless = hopeless_orders(&mut world, PlayerId(3));
    let area = aoe_core::TileRect::new(
        TileCoord::new(CX, CY - RADIUS - 2),
        TileCoord::new(CX + 1, CY - RADIUS - 1),
    );
    for _ in 0..10 {
        world.collision_closed(area);
        world.planning_budget = 1;
        world.fast_route_budget = 0;
        world.advance();
        assert!(
            hopeless
                .iter()
                .all(|&id| world.movement_order(id).is_none()),
            "region proof must not wait for a search budget"
        );
    }
    for id in hopeless {
        assert!(world.movement_order(id).is_none());
        assert_eq!(world.unit(id).expect("unit").task, crate::UnitTask::Idle);
    }
}

/// Hopeless orders keep arriving; the wait stays bounded the whole time.
#[test]
fn one_player_answers_every_order_under_sustained_hard_demand() {
    let mut world = sealed_near_world();
    let player = PlayerId(3);
    hopeless_orders(&mut world, player);
    let mut sent = detour_orders(&mut world, player, 40);
    for tick in 0..NEAR_ANSWER_TICKS * 2 {
        // Fresh hopeless orders keep arriving, as the AI re-sends them.
        if tick % 100 == 50 {
            let out = RADIUS + 1;
            let id = world
                .spawn_unit(player, tile_position(CX - out, CY))
                .expect("unit");
            let _ = world.issue_task_move(id, tile_position(CX, CY));
            sent.push(id);
        }
        world.advance();
        let oldest = world.route_scheduler_stats().oldest_wait_ticks;
        assert!(oldest <= NEAR_ANSWER_TICKS, "tick {tick} waited {oldest}");
    }
}

/// A group's leader carries its route; polling it must not restart its wait.
#[test]
fn a_group_sent_into_sealed_ground_is_answered_in_bounded_time() {
    let mut world = sealed_near_world();
    let out = RADIUS + 1;
    let units = (0..6)
        .map(|n| {
            world
                .spawn_unit(PlayerId(3), tile_position(CX - 3 + n, CY - out - 2))
                .expect("unit")
        })
        .collect::<Vec<_>>();
    world
        .issue_formation_move(
            PlayerId(3),
            &units,
            aoe_core::Formation::Line,
            tile_position(CX, CY),
            false,
            false,
        )
        .expect("accepted");
    let waits = |world: &GameWorld| {
        units
            .iter()
            .map(|id| &world.units[world.lookup[id]])
            .map(|unit| (unit.state.planning, unit.planning_since))
            .collect::<Vec<_>>()
    };
    let mut before = waits(&world);
    for tick in 0..NEAR_ANSWER_TICKS + 5 {
        world.advance();
        let now = waits(&world);
        for (was, is) in before.iter().zip(&now) {
            assert!(
                !(was.0 && is.0) || was.1 == is.1,
                "tick {tick}: {was:?} {is:?}"
            );
        }
        before = now;
        let oldest = world.route_scheduler_stats().oldest_wait_ticks;
        assert!(oldest <= NEAR_ANSWER_TICKS, "waited {oldest}");
    }
    // It searched its bound, so its wait is not renewed as a queued one is.
    assert_eq!(world.route_metrics.attribution.group_wait_renewals, 0);
}

/// A fast attempt with no slot to keep its frontier spends half as much.
#[test]
fn an_attempt_with_no_slot_for_its_frontier_is_cut_short() {
    let mut world = sealed_near_world();
    let player = PlayerId(3);
    world.fast_route_budget = 0;
    detour_orders(&mut world, player, 2);
    world.planning_budget = 0;
    world.advance();
    let before = world.route_metrics.issue_work;
    detour_orders(&mut world, player, 1);
    assert_eq!(
        world.route_metrics.issue_work - before,
        u64::from(crate::game_world::route_scheduler::NO_SLOT_ROUTE_WORK)
    );
}

/// Sealed targets project to a reachable layout and settle, without a deadline.
#[test]
fn a_near_group_stops_at_its_frontier_without_waiting_for_an_expiry() {
    frontier_group_settles(RADIUS + 3, 600);
}

#[test]
fn a_far_group_stops_at_its_frontier_without_wait_renewals() {
    frontier_group_settles(RADIUS + 100, 3_000);
}

fn frontier_group_settles(distance: i32, ticks: usize) {
    let mut world = sealed_near_world();
    let units = (0..6)
        .map(|n| {
            world
                .spawn_unit(PlayerId(3), tile_position(CX - 3 + n, CY - distance))
                .unwrap()
        })
        .collect::<Vec<_>>();
    world
        .issue_formation_move(
            PlayerId(3),
            &units,
            aoe_core::Formation::Line,
            tile_position(CX, CY),
            false,
            false,
        )
        .unwrap();
    for _ in 0..ticks {
        world.advance();
    }
    assert_eq!(world.route_metrics.attribution.group_orders_expired, 0);
    assert_eq!(world.route_metrics.attribution.group_wait_renewals, 0);
    assert_eq!(world.route_metrics.attribution.group_stopped_short, 1);
    for id in units {
        let unit = world.unit(id).unwrap();
        assert!(
            !unit.moving && !unit.planning,
            "{id:?} still pressing: {unit:?}"
        );
        assert!(
            unit.position.tile_floor().y < CY - RADIUS,
            "outside the sealed ring"
        );
    }
    assert_eq!(world.route_accounting_error(), None);
}

/// A near group that only queued for a slot keeps its order past the wait
/// bound: in a 100-AI probe 35 group orders, nearly all near, were ended so
/// after one minute and their armies stood idle short of the destination.
#[test]
fn a_near_group_queued_for_a_slot_outlives_its_wait() {
    let mut world = sealed_near_world();
    let player = PlayerId(3);
    world.fast_route_budget = 0;
    detour_orders(&mut world, player, 2);
    world.planning_budget = 0;
    world.advance();
    assert_eq!(world.route_slots[&player].searches, 2, "both slots held");
    let units = (0..6)
        .map(|n| {
            world
                .spawn_unit(player, tile_position(220 + n, 190))
                .expect("unit")
        })
        .collect::<Vec<_>>();
    let goal = tile_position(223, 210);
    world
        .issue_formation_move(
            player,
            &units,
            aoe_core::Formation::Line,
            goal,
            false,
            false,
        )
        .expect("accepted");
    world.planning_budget = 0;
    world.fast_route_budget = 0;
    world.advance();
    let carrier = *world.formations.keys().next().expect("group");
    let index = world.lookup[&carrier];
    assert!(world.units[index].state.planning, "carrier waits");
    assert!(world.units[index].planner.is_none(), "no slot for it");
    world.tick.0 += NEAR_ANSWER_TICKS;
    for _ in 0..3 {
        world.advance();
    }
    assert_eq!(world.formations.len(), 1, "the group keeps its order");
    assert!(world.movement_order(carrier).is_some());
    assert_eq!(world.route_metrics.attribution.group_orders_expired, 0);
    assert_eq!(world.route_metrics.attribution.group_wait_renewals, 1);
    // It is answered with its route once a slot frees.
    for _ in 0..2_000 {
        world.advance();
    }
    for id in units {
        let unit = world.unit(id).expect("unit");
        assert!(unit.position.tile_floor().y > 200, "{id:?} crossed");
    }
}
