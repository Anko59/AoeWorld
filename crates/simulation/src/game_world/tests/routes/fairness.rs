//! Route searches are shared fairly between players: one player's flood of
//! orders, or a search that cannot finish, never delays another player's.
//! Sealed searches use exact task goals: spreading a group's destinations can
//! make a shifted goal cheaper to reject, changing the intended search pressure.
use super::*;

/// Half a tick's fair budget, set before an order in these tests.
pub(super) const FAIR_ROUTE_WORK_RESERVE: u32 = crate::MAX_ROUTE_WORK_PER_TICK / 2;
/// Ring radius: the sealed ground (35 x 35 tiles) is larger than the planner's
/// sealed-destination proof recognises, so a route into it stays a search
/// that cannot finish before it has explored the whole map.
pub(super) const RING: i32 = 18;

/// A uniform world whose tiles around `(cx, cy)` (radius `RING - 1`) are
/// sealed by a ring of blocked tiles. From the four outside tiles nearest the
/// centre no tile is nearer, so a route into the ring searches the whole map.
pub(super) fn sealed_ring_world(side: i32, cx: i32, cy: i32) -> GameWorld {
    let config = WorldConfig::new(side, side, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    for dx in -RING..=RING {
        for dy in -RING..=RING {
            if dx.abs() == RING || dy.abs() == RING {
                world
                    .economy
                    .blocked_tiles
                    .insert(TileCoord::new(cx + dx, cy + dy));
            }
        }
    }
    world
}

pub(super) fn tile_position(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).expect("tile")
}

/// One of the four outside tiles nearest the sealed centre.
pub(super) fn beside_ring(cx: i32, cy: i32, raw: i32) -> WorldPosition {
    let out = RING + 1;
    let [dx, dy] = [[0, -out], [0, out], [-out, 0], [out, 0]][(raw % 4) as usize];
    tile_position(cx + dx, cy + dy)
}

#[test]
fn one_players_route_flood_never_delays_another_players_order() {
    // Player 1 floods: every unit is sent into a sealed ring, a search that
    // cannot finish before it has explored the whole map.
    let mut world = sealed_ring_world(256, 200, 200);
    let mut flood = Vec::new();
    for raw in 0..60 {
        let id = world
            .spawn_unit(PlayerId(1), beside_ring(200, 200, raw))
            .expect("flood unit");
        world.planning_budget = FAIR_ROUTE_WORK_RESERVE;
        assert!(
            world
                .issue_task_move(id, tile_position(200, 200))
                .expect("accepted")
        );
        flood.push(id);
    }
    for _ in 0..5 {
        world.advance();
    }
    assert!(
        flood
            .iter()
            .all(|id| world.unit(*id).expect("unit").planning)
    );
    assert!(world.route_slots[&PlayerId(1)].searches <= MAX_ROUTE_SEARCHES_PER_PLAYER);

    // Later orders of other players get no issue-time work: the fair share alone serves them.
    for player in [PlayerId(0), PlayerId(2)] {
        let x = 60 + i32::from(player.0) * 40;
        let id = world
            .spawn_unit(player, tile_position(x, 60))
            .expect("player unit");
        world.planning_budget = FAIR_ROUTE_WORK_RESERVE;
        assert!(
            world
                .issue_move(id, tile_position(x + 30, 90))
                .expect("accepted")
        );
        let start = world.unit(id).expect("unit").position;
        world.advance();
        let unit = world.unit(id).expect("unit");
        assert!(
            unit.moving,
            "{player:?} still waits behind another player's flood"
        );
        assert_ne!(
            unit.position, start,
            "{player:?} moved in the order's first tick"
        );
    }
}

#[test]
fn a_players_own_sealed_searches_do_not_block_other_players_slots() {
    let mut world = sealed_ring_world(128, 100, 100);
    for raw in 0..8 {
        let id = world
            .spawn_unit(PlayerId(1), beside_ring(100, 100, raw))
            .expect("unit");
        world.planning_budget = FAIR_ROUTE_WORK_RESERVE;
        world
            .issue_task_move(id, tile_position(100, 100))
            .expect("accepted");
    }
    world.advance();
    assert_eq!(
        world.route_slots[&PlayerId(1)].searches,
        MAX_ROUTE_SEARCHES_PER_PLAYER
    );
    assert!(!world.route_capacity_exhausted(PlayerId(0), 0, 0, false));
    assert!(world.route_capacity_exhausted(PlayerId(1), 0, 0, false));
}

/// Both of the player's search slots are held by hopeless searches; its next
/// short order is answered by its fast attempt and walks at once (it used to
/// wait until one of those searches had proven hopeless).
#[test]
fn a_short_order_never_waits_for_slots_held_by_hopeless_searches() {
    let mut world = sealed_ring_world(256, 200, 200);
    let mut sealed = Vec::new();
    for raw in 0..2 {
        let id = world
            .spawn_unit(PlayerId(1), beside_ring(200, 200, raw))
            .expect("sealed unit");
        world
            .issue_task_move(id, tile_position(200, 200))
            .expect("accepted");
        sealed.push(id);
    }
    world.advance();
    assert_eq!(
        world.route_slots[&PlayerId(1)].searches,
        MAX_ROUTE_SEARCHES_PER_PLAYER
    );
    let next = world
        .spawn_unit(PlayerId(1), tile_position(20, 20))
        .expect("next unit");
    world
        .issue_move(next, tile_position(40, 30))
        .expect("accepted");
    assert!(world.unit(next).expect("unit").moving);
    world.advance();
    assert_ne!(
        world.unit(next).expect("unit").position,
        tile_position(20, 20)
    );
    let ended = (0..2_000).find(|_| {
        world.advance();
        sealed.iter().any(|id| {
            world.units[world.lookup[id]].last_movement_error
                == Some(GameWorldError::PathBudgetExceeded)
        })
    });
    assert!(ended.is_some(), "a hopeless search still holds its slot");
}

#[test]
fn sustained_near_searches_cannot_starve_a_retained_far_search() {
    let mut world = sealed_ring_world(256, 200, 200);
    // A long wall keeps the far search from committing a cheap partial leg;
    // its fast attempt must finish with a retained frontier, not a walking route.
    for x in 0..256 {
        world.economy.blocked_tiles.insert(TileCoord::new(x, 110));
    }
    let near = (0..2)
        .map(|raw| {
            let id = world
                .spawn_unit(PlayerId(1), beside_ring(200, 200, raw))
                .unwrap();
            world.issue_task_move(id, tile_position(200, 200)).unwrap();
            id
        })
        .collect::<Vec<_>>();
    let far = world
        .spawn_unit(PlayerId(2), tile_position(200, 100))
        .unwrap();
    world.issue_task_move(far, tile_position(200, 200)).unwrap();
    world.planning_budget = FAIR_ROUTE_WORK_RESERVE;
    world.advance();
    assert!(world.unit(far).unwrap().planning);
    let mut previous = world.units[world.lookup[&far]]
        .planner
        .as_ref()
        .unwrap()
        .work();
    // Reissue the near searches whenever they expire: pressure never subsides.
    for _ in 0..12 {
        for id in &near {
            if world.units[world.lookup[id]].order.is_none() {
                world.issue_task_move(*id, tile_position(200, 200)).unwrap();
            }
        }
        world.planning_budget = FAIR_ROUTE_WORK_RESERVE;
        let fair_before = world.route_metrics.fair_work;
        world.advance();
        let unit = &world.units[world.lookup[&far]];
        let Some(planner) = &unit.planner else {
            assert!(matches!(
                unit.last_movement_error,
                Some(GameWorldError::PathBudgetExceeded | GameWorldError::Unreachable)
            ));
            return;
        };
        assert!(
            planner.work() > previous,
            "far route received no recurring share"
        );
        previous = planner.work();
        assert!(world.route_metrics.fair_work - fair_before <= u64::from(FAIR_ROUTE_WORK_RESERVE));
    }
}
