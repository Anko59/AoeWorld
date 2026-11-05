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
    assert_eq!(world.route_slots[&player].searches, 2, "both slots held");
    let detours = detour_orders(&mut world, player, 60);
    let mut oldest = 0;
    for _ in 0..NEAR_ANSWER_TICKS + 5 {
        world.advance();
        oldest = oldest.max(world.route_scheduler_stats().oldest_wait_ticks);
    }
    let stats = world.route_scheduler_stats();
    let unanswered = detours.iter().filter(|id| !answered(&world, **id)).count();
    assert_eq!(
        unanswered, 0,
        "units behind the hopeless searches: {stats:?}"
    );
    // They were answered with routes, not merely turned away.
    let failed = detours
        .iter()
        .filter(|id| world.movement_failure(**id).is_some())
        .count();
    assert_eq!(failed, 0, "{stats:?}");
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
    let hopeless = hopeless_orders(&mut world, PlayerId(3));
    let area = aoe_core::TileRect::new(
        TileCoord::new(CX, CY - RADIUS - 2),
        TileCoord::new(CX + 1, CY - RADIUS - 1),
    );
    let mut restarted = 0;
    for tick in 0..NEAR_ANSWER_TICKS + 5 {
        if tick % 4 == 0 {
            let before = world.route_scheduler_stats().planners;
            world.collision_closed(area);
            restarted += before - world.route_scheduler_stats().planners;
        }
        world.advance();
    }
    assert!(restarted > 2, "the ground changes reached the searches");
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
}

/// A fast attempt with no slot to keep its frontier spends half as much.
#[test]
fn an_attempt_with_no_slot_for_its_frontier_is_cut_short() {
    let mut world = sealed_near_world();
    let player = PlayerId(3);
    hopeless_orders(&mut world, player);
    world.advance();
    let before = world.route_metrics.issue_work;
    detour_orders(&mut world, player, 1);
    assert_eq!(
        world.route_metrics.issue_work - before,
        u64::from(crate::game_world::route_scheduler::NO_SLOT_ROUTE_WORK)
    );
}

/// When a group's route wait expires, the whole group's move fails: the
/// members must not keep steering toward their slots without a carrier.
#[test]
fn an_expired_group_route_ends_the_move_of_every_member() {
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
    // The carrier searches the sealed ground; its wait runs out before any
    // member is blocked long enough to detach with a route of its own.
    for _ in 0..40 {
        world.advance();
    }
    let carrier = *world.formations.keys().next().expect("group");
    assert!(world.unit(carrier).expect("unit").planning, "carrier waits");
    world.tick.0 += NEAR_ANSWER_TICKS;
    for _ in 0..5 {
        world.advance();
    }
    assert!(world.formations.is_empty(), "no group state steers them");
    for id in units {
        let unit = world.unit(id).expect("unit");
        assert!(world.movement_order(id).is_none());
        assert_eq!(unit.task, crate::UnitTask::Idle, "{id:?}");
        assert!(!unit.moving && !unit.planning, "{id:?}");
        assert!(world.units[world.lookup[&id]].formation.is_none());
    }
}
