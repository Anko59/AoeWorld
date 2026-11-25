//! Land-query budget and caches, and the build-site budget (`ai/aoe_ai/context.py`).
use aoe_sdk::{Age, BuildingKind};

use super::fake::{FakeWorld, base, economy, with_context};
use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::view::Economy;

fn world() -> FakeWorld {
    let (units, buildings) = base(3);
    FakeWorld::new(units, buildings)
}

#[test]
fn disconnected_land_queries_are_bounded_and_retried_after_the_cooldown() {
    let mut world = world();
    world.disconnected = true;
    let mut board = Blackboard::default();
    let config = Config::default();
    for tick in [100, 120, 200] {
        world.tick = tick;
        let (route, _) = with_context(&world, &[], &mut board, &config, |ctx| {
            ctx.land_route((10, 10), (20, 10))
        });
        assert_eq!(route, None);
    }
    assert_eq!(world.route_queries.borrow().len(), 1);
    world.tick = 301;
    let (route, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        ctx.land_route((10, 10), (20, 10))
    });
    assert_eq!(route, None);
    assert_eq!(world.route_queries.borrow().len(), 2);
    assert!(board.land_failures.len() <= config.land_cache_entries);
}

#[test]
fn a_failed_route_is_remembered_for_the_retry_time_of_the_config() {
    let mut world = world();
    world.disconnected = true;
    let mut board = Blackboard::default();
    with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.land_route((10, 10), (20, 10))
    });
    // 10 seconds at 20 ticks a second after tick 100.
    assert_eq!(
        board.land_failures.get(&((10, 10), (20, 10))).copied(),
        Some(300)
    );
}

#[test]
fn a_found_route_is_asked_once_per_decision() {
    let world = world();
    let mut board = Blackboard::default();
    let (routes, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        (
            ctx.land_route((10, 10), (13, 10)),
            ctx.land_route((10, 10), (13, 10)),
        )
    });
    let path = Some(vec![(10, 10), (11, 10), (12, 10), (13, 10)]);
    assert_eq!(routes, (path.clone(), path));
    assert_eq!(world.route_queries.borrow().len(), 1);
}

#[test]
fn the_land_query_budget_leaves_later_routes_unanswered_not_failed() {
    let world = world();
    let mut board = Blackboard::default();
    let config = Config {
        land_queries_per_decision: 2,
        ..Config::default()
    };
    let (seen, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        let first = ctx.land_route((10, 10), (12, 10));
        let second = ctx.land_route((10, 10), (14, 10));
        let third = ctx.land_route((10, 10), (16, 10));
        (
            first.is_some(),
            second.is_some(),
            third,
            ctx.land_unanswered((10, 10), (16, 10)),
            ctx.land_unanswered((10, 10), (12, 10)),
        )
    });
    assert_eq!(seen, (true, true, None, true, false));
    assert_eq!(world.route_queries.borrow().len(), 2);
    assert!(
        board.land_failures.is_empty(),
        "an unasked route is not a failure"
    );
}

#[test]
fn a_frontier_query_spends_the_same_land_budget() {
    let world = world();
    let mut board = Blackboard::default();
    let config = Config {
        land_queries_per_decision: 1,
        ..Config::default()
    };
    let (seen, _) = with_context(&world, &[], &mut board, &config, |ctx| {
        let frontier = ctx.land_frontier((10, 10), (14, 10));
        (
            frontier,
            ctx.land_route((10, 10), (12, 10)),
            ctx.land_budget_exhausted(),
        )
    });
    assert_eq!(seen, (Some((14, 10)), None, true));
    assert_eq!(world.frontier_queries.borrow().len(), 1);
    assert!(world.route_queries.borrow().is_empty());
}

#[test]
fn in_a_late_economy_routes_between_unknown_tiles_are_not_even_asked() {
    let (units, buildings) = base(128);
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(Economy {
        age: Age::Castle,
        ..economy()
    });
    let mut board = Blackboard::default();
    let (seen, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        assert!(ctx.late_economy);
        (
            ctx.land_route((-5, -5), (10, 10)),
            ctx.land_frontier((-5, -5), (10, 10)),
        )
    });
    assert_eq!(seen, (None, None));
    assert!(world.route_queries.borrow().is_empty() && world.frontier_queries.borrow().is_empty());
}

#[test]
fn a_land_hop_follows_the_proven_route_up_to_the_hop_length() {
    let world = world();
    let mut board = Blackboard::default();
    let (hop, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        (
            ctx.land_hop((60, 60), (250, 250)),
            ctx.land_hop((60, 60), (65, 60)),
        )
    });
    // 33 diagonal steps are 46.7 tiles; the 34th would pass the 48 of `march_hop_tiles`.
    assert_eq!(hop, ((93, 93), (65, 60)));
}

#[test]
fn a_land_hop_without_a_route_stays_where_it_is() {
    let mut world = world();
    world.disconnected = true;
    let mut board = Blackboard::default();
    let (hop, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.land_hop((60, 60), (250, 250))
    });
    assert_eq!(hop, (60, 60));
}

#[test]
fn a_land_hop_avoids_a_destination_the_server_refused() {
    let world = world();
    let mut board = Blackboard::default();
    board.bad_moves.insert((93, 93));
    let (hop, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        ctx.land_hop((60, 60), (250, 250))
    });
    assert_ne!(hop, (93, 93));
    assert_ne!(hop, (60, 60));
}

#[test]
fn build_site_searches_stop_at_the_per_decision_limit() {
    let world = world();
    let mut board = Blackboard::default();
    let (sites, _) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        (0..6)
            .map(|i| ctx.find_build_site(BuildingKind::LumberCamp, (80 + i, 50), 24, 1))
            .collect::<Vec<_>>()
    });
    assert_eq!(sites[0], Some((83, 53)));
    assert!(sites[..4].iter().all(Option::is_some));
    assert_eq!(&sites[4..], &[None, None]);
    assert_eq!(world.sites.borrow().len(), 4);
    assert_eq!(
        world.sites.borrow()[0],
        (BuildingKind::LumberCamp, (80, 50))
    );
}
