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

/// Orders into the sealed ground that no fast attempt can settle. A contact
/// order the region layer proves disconnected is refused when issued (#978).
fn hopeless_orders(world: &mut GameWorld, player: PlayerId) -> Vec<EntityId> {
    let out = RADIUS + 1;
    [(0, -out), (0, out)]
        .into_iter()
        .map(|(dx, dy)| {
            let id = world
                .spawn_unit(player, tile_position(CX + dx, CY + dy))
                .expect("unit");
            assert_eq!(
                world.issue_task_move(id, tile_position(CX, CY)),
                Err(GameWorldError::Unreachable)
            );
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
    world.fast_route_budget = Some(0);
    let hopeless = hopeless_orders(&mut world, PlayerId(3));
    let area = aoe_core::TileRect::new(
        TileCoord::new(CX, CY - RADIUS - 2),
        TileCoord::new(CX + 1, CY - RADIUS - 1),
    );
    for _ in 0..10 {
        world.collision_closed(area);
        world.planning_budget = Some(1);
        world.fast_route_budget = Some(0);
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

#[test]
fn repeated_waiting_goals_retain_search_progress_and_the_original_deadline() {
    let mut w = sealed_near_world();
    // Region proofs now answer sealed destinations immediately. A connected
    // detour with a small first allowance leaves a real frontier to retain.
    w.fast_route_budget = Some(256);
    w.planning_budget = Some(0);
    let id = detour_orders(&mut w, PlayerId(0), 1)[0];
    let other = w.spawn_unit(PlayerId(0), tile_position(182, 190)).unwrap();
    assert_eq!(w.issue_task_move(other, tile_position(180, 210)), Ok(true));
    w.advance();
    let index = w.lookup[&id];
    assert!(w.destination_point_reserved(tile_position(180, 210), id));
    assert!(w.units[index].state.planning);
    assert!(
        w.units[index]
            .planner
            .as_deref()
            .is_some_and(|p| p.work() > 0)
    );
    let started = w.units[index].planning_since;
    let before = w.canonical_hash();
    let requests = w.order_demand()[crate::OrderIssuer::Automation as usize].route_requests;
    for goal in [tile_position(180, 210), tile_position(181, 210)] {
        assert_eq!(w.issue_move(id, goal), Ok(true));
        assert_eq!(w.units[index].planning_since, started);
        assert_eq!(
            w.canonical_hash(),
            before,
            "do not discard the retained frontier"
        );
    }
    assert_eq!(
        w.order_demand()[crate::OrderIssuer::Automation as usize].route_requests,
        requests
    );
}

#[test]
fn route_failure_issuer_counts_an_issue_time_refusal_once() {
    let mut world = sealed_near_world();
    let id = world
        .spawn_unit(PlayerId(3), tile_position(CX, CY - RADIUS - 1))
        .unwrap();
    assert_eq!(
        world.with_order_issuer(crate::OrderIssuer::Steward, |w| {
            w.issue_task_move(id, tile_position(CX, CY))
        }),
        Err(GameWorldError::Unreachable)
    );
    assert_eq!(
        world.order_route_failures().map(|(_, count)| count),
        [0, 0, 0, 1, 0, 0, 0, 0, 0]
    );
}

#[test]
fn route_failure_issuer_counts_a_queued_region_proof_once() {
    use crate::OrderIssuer;

    let mut world = sealed_near_world();
    let id = world
        .spawn_unit(PlayerId(3), tile_position(CX, CY - RADIUS - 1))
        .unwrap();
    world.fast_route_budget = Some(0);
    world.planning_budget = Some(0);
    assert_eq!(
        world.with_order_issuer(OrderIssuer::PythonAi, |w| {
            w.issue_move(id, tile_position(CX, CY))
        }),
        Ok(true)
    );
    assert!(world.waiting_for_route(world.lookup[&id]));
    world.with_order_issuer(OrderIssuer::General, |w| w.advance());
    assert_eq!(
        world.movement_failure(id),
        Some(GameWorldError::Unreachable)
    );
    assert_eq!(
        world.order_route_failures().map(|(_, count)| count),
        [0, 1, 0, 0, 0, 0, 0, 0, 0]
    );
}
