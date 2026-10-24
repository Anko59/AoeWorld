//! Route saturation at 100 players. A 60-minute 99-AI match froze because
//! route demand outgrew the scheduler: trips of a few tiles waited behind
//! searches into sealed ground (a forest gap, a boxed-in tile) that each
//! burned up to 262,144 work, a walking unit's next leg took a search slot
//! past its player's cap until the world's slots were gone, and the route
//! cache was emptied by every felled tree. These tests pin the fixes.
use super::fairness::{beside_ring, sealed_ring_world, tile_position};
use super::*;

const PLAYERS: u16 = 100;

/// Boxes the tile `(x, y)` in: a walkable tile no route can reach.
fn seal_pocket(world: &mut GameWorld, x: i32, y: i32) {
    for dx in -1..=1 {
        for dy in -1..=1 {
            if dx != 0 || dy != 0 {
                world
                    .economy
                    .blocked_tiles
                    .insert(TileCoord::new(x + dx, y + dy));
            }
        }
    }
}

#[test]
fn an_order_into_sealed_ground_is_refused_when_it_is_issued() {
    let mut world = sealed_ring_world(1_024, 500, 500);
    seal_pocket(&mut world, 300, 300);
    let id = world
        .spawn_unit(PlayerId(1), tile_position(296, 300))
        .expect("unit");
    assert_eq!(
        world.issue_move(id, tile_position(300, 300)),
        Err(GameWorldError::Unreachable)
    );
    assert!(world.route_slots.is_empty());
    // The proof costs a small fraction of the hopeless search it replaces.
    assert!(world.route_metrics.issue_work < 16_384);
}

/// Every one of 100 players floods its two search slots with searches into
/// sealed ground too large to prove sealed, and re-sends them (as the AI
/// re-sends refused orders) along with orders into small sealed pockets.
/// Short orders of any player still start on their own tick, and nothing the
/// scheduler holds grows: per-player slots stay at their cap and the world
/// stays below its bound.
#[test]
fn short_orders_start_at_once_while_every_player_floods_hopeless_searches() {
    let mut world = sealed_ring_world(1_024, 500, 500);
    seal_pocket(&mut world, 300, 300);
    let mut flood = Vec::new();
    for player in 0..PLAYERS {
        for raw in 0..2 {
            let id = world
                .spawn_unit(PlayerId(player), beside_ring(500, 500, raw))
                .expect("flood unit");
            flood.push(id);
        }
    }
    // Units beside a small sealed pocket, re-sent into it one per tick.
    let pocketed = (0..4)
        .map(|n| {
            world
                .spawn_unit(PlayerId(n), tile_position(296, 299 + n as i32))
                .expect("pocket unit")
        })
        .collect::<Vec<_>>();
    let center = tile_position(500, 500);
    let mut started = 0;
    let mut refused = 0;
    let mut pocket_sent = std::collections::BTreeMap::new();
    for tick in 0..120_i32 {
        // Re-send every flood order that has ended, and one pocket order.
        for &id in &flood {
            if world.movement_order(id).is_none() {
                let _ = world.issue_move(id, center);
            }
        }
        // A pocket order is refused when issued or, when the flood has spent
        // this tick's fast budget, on one of the next ticks: never a search.
        for &pocket in &pocketed {
            if world.movement_order(pocket).is_some() {
                assert!(
                    tick - pocket_sent[&pocket] <= 3,
                    "tick {tick}: a pocket order still searches: fast_tried={}, fast_budget={}, planner_work={:?}",
                    world.units[world.lookup[&pocket]].fast_route_tried,
                    world.fast_route_budget,
                    world.units[world.lookup[&pocket]]
                        .planner
                        .as_ref()
                        .map(|p| p.work())
                );
            } else if tick % 4 == 0 {
                match world.issue_move(pocket, tile_position(300, 300)) {
                    Err(GameWorldError::Unreachable) => refused += 1,
                    Ok(true) => {
                        pocket_sent.insert(pocket, tick);
                    }
                    other => panic!("pocket order: {other:?}"),
                }
            } else if world.movement_failure(pocket) == Some(GameWorldError::Unreachable) {
                refused += 1;
                world.units[world.lookup[&pocket]].last_movement_error = None;
            }
        }
        if tick % 6 == 0 {
            let player = PlayerId((tick as u16 * 13) % PLAYERS);
            let at = tile_position(100 + tick, 100);
            let id = world.spawn_unit(player, at).expect("worker");
            assert_eq!(
                world.issue_move(id, tile_position(108 + tick, 104)),
                Ok(true)
            );
            let waited = (0..3).position(|_| {
                world.advance();
                world.unit(id).expect("worker").position != at
            });
            assert!(
                waited.is_some(),
                "tick {tick}: a short order of {player:?} waited"
            );
            started += 1;
        } else {
            world.advance();
        }
        // Every player's fast share is whole again for the next tick.
        assert!(world.fast_route_spent.is_empty());
        for slots in world.route_slots.values() {
            assert!(slots.class_searches(false) <= MAX_ROUTE_SEARCHES_PER_PLAYER);
            assert!(slots.class_searches(true) <= MAX_ROUTE_SEARCHES_PER_PLAYER);
        }
        let stats = world.route_scheduler_stats();
        assert!(!stats.world_capacity_exhausted);
        assert!(stats.waiting_units <= flood.len() + pocketed.len() + 20);
    }
    assert_eq!(started, 20);
    assert!(refused >= 60, "only {refused} pocket orders were refused");
    assert!(world.active_route_searches <= 2 * MAX_ROUTE_SEARCHES_PER_PLAYER * PLAYERS as usize);
    assert!(
        world.active_route_searches > 0,
        "the flood never held a search"
    );
}

/// A walking unit whose planned legs run out searches its next leg; that
/// search needs one of its player's slots like any other (it used to take a
/// slot past the cap, and 100 players' armies filled the world's 256).
#[test]
fn a_next_leg_never_takes_a_search_slot_past_its_players_cap() {
    let config = WorldConfig::new(512, 512, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    // A wall with one far gap makes every leg a real search.
    for y in 0..480 {
        if y != 400 {
            world.economy.blocked_tiles.insert(TileCoord::new(200, y));
        }
    }
    for n in 0..30 {
        let id = world
            .spawn_unit(PlayerId(3), tile_position(100 + n % 10, 100 + n / 10))
            .expect("soldier");
        world
            .issue_move(id, tile_position(300, 100 + n))
            .expect("accepted");
    }
    let mut peak = 0;
    for _ in 0..1_000 {
        world.advance();
        let searches = world
            .route_slots
            .get(&PlayerId(3))
            .map_or(0, |slots| slots.searches);
        peak = peak.max(searches);
        assert!(searches <= MAX_ROUTE_SEARCHES_PER_PLAYER);
    }
    assert!(peak > 0, "the army never searched");
}

/// A villager's trip between the same two tiles is answered from the route
/// cache the second time, with the same route, and a tree felled elsewhere
/// does not empty the cache.
#[test]
fn a_repeated_trip_is_answered_from_the_route_cache() {
    let config = WorldConfig::new(256, 256, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    // Any blocked tile turns map navigation on for a uniform world.
    world.economy.blocked_tiles.insert(TileCoord::new(60, 60));
    let (from, to) = (tile_position(20, 20), tile_position(30, 24));
    let id = world.spawn_unit(PlayerId(0), from).expect("unit");
    assert_eq!(world.issue_move(id, to), Ok(true));
    let first = world.units[world.lookup[&id]].route.clone();
    assert_eq!(world.route_metrics.cache_hits, 0);
    assert_eq!(world.navigation_cache.entry_count(), 1);
    for _ in 0..200 {
        world.advance();
    }
    assert_eq!(world.unit(id).expect("unit").position, to);
    world.issue_move(id, from).expect("walk back");
    for _ in 0..200 {
        world.advance();
    }
    assert_eq!(world.unit(id).expect("unit").position, from);
    let far = aoe_core::TileRect::new(TileCoord::new(200, 200), TileCoord::new(203, 203));
    world.collision_opened(far);
    world.collision_closed(far);
    let hits = world.route_metrics.cache_hits;
    assert_eq!(world.issue_move(id, to), Ok(true));
    assert_eq!(world.route_metrics.cache_hits, hits + 1);
    assert_eq!(world.units[world.lookup[&id]].route, first);
    // Ground the cached route read changes: the entry is dropped.
    let near = aoe_core::TileRect::new(TileCoord::new(25, 22), TileCoord::new(26, 23));
    world.collision_closed(near);
    world.issue_move(id, from).expect("redirect");
    for _ in 0..200 {
        world.advance();
    }
    assert_eq!(world.issue_move(id, to), Ok(true));
    assert_eq!(world.route_metrics.cache_hits, hits + 1);
}

/// Busy far slots leave the player's near slots available. A long wall's
/// open ends make the target reachable, so the fast cutoff cannot certify it
/// unreachable: retain its search until the existing total-work bound. This
/// replaces the previous assertion that pinned the premature approach bug.
#[test]
fn an_unresolved_near_order_retains_a_bounded_search_beside_busy_far_slots() {
    let mut world = sealed_ring_world(1_024, 500, 500);
    let player = PlayerId(4);
    for n in 0..MAX_ROUTE_SEARCHES_PER_PLAYER as i32 {
        let from = TileCoord::new(100 + n, 100);
        let id = world
            .spawn_unit(player, tile_position(from.x, from.y))
            .expect("scout");
        let index = world.lookup[&id];
        world.store_planner(
            index,
            Some(aoe_map::RoutePlanner::new(
                from,
                TileCoord::new(500, 500),
                4_096,
            )),
        );
    }
    assert!(world.route_capacity_exhausted(player, 0, 0, true));
    // A wall from y = 100 to y = 900 at x = 200: its ends are far away.
    for y in 100..900 {
        world.economy.blocked_tiles.insert(TileCoord::new(200, y));
    }
    let villager = world
        .spawn_unit(player, tile_position(190, 300))
        .expect("villager");
    assert_eq!(
        world.issue_move(villager, tile_position(210, 300)),
        Ok(true)
    );
    assert!(world.unit(villager).expect("villager").planning);
    assert_eq!(world.route_metrics.approached, 0);
    assert_eq!(world.route_slots[&player].class_searches(false), 1);
    assert_eq!(
        world.route_slots[&player].class_searches(true),
        MAX_ROUTE_SEARCHES_PER_PLAYER
    );
    let mut ended = None;
    for tick in 0..400 {
        world.advance();
        if world.movement_order(villager).is_none() {
            ended = Some(tick);
            break;
        }
    }
    assert!(ended.is_some(), "the bounded search never ended");
    assert_eq!(
        world.movement_failure(villager),
        Some(GameWorldError::PathBudgetExceeded)
    );
    assert_eq!(world.route_slots[&player].class_searches(false), 0);
}

/// A fast attempt's work allowance is not a proof of disconnection: this
/// short wall has two open ends, but finding either detour takes more work
/// than the issue-time allowance. Retained polling must finish that search.
#[test]
fn a_reachable_near_order_keeps_searching_past_the_fast_allowance() {
    let config = WorldConfig::new(1_024, 1_024, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("flat world");
    // Completed palisade collision, with open ground around both ends.
    for y in 280..=320 {
        world.economy.blocked_tiles.insert(TileCoord::new(200, y));
    }
    let scout = world
        .spawn_kind(PlayerId(0), UnitKind::Scout, tile_position(190, 300))
        .expect("scout");
    world.refill_route_budgets();
    let target = tile_position(210, 300);
    assert_eq!(world.issue_move(scout, target), Ok(true));

    let mut went_around_end = false;
    for _ in 0..2_000 {
        world.advance();
        let tile = world.unit(scout).expect("scout").position.tile_floor();
        assert!(
            tile.x != 200 || !(280..=320).contains(&tile.y),
            "scout crossed the completed palisade at {tile:?}"
        );
        went_around_end |= tile.y < 280 || tile.y > 320;
        if world.movement_order(scout).is_none() {
            break;
        }
    }
    assert_eq!(
        world.movement_failure(scout),
        None,
        "exhausting the fast allowance must retain the reachable search"
    );
    assert_eq!(world.unit(scout).expect("scout").position, target);
    assert!(went_around_end, "scout did not use an open wall end");
    assert_eq!(world.route_accounting_error(), None);
}
