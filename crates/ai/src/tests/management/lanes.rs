//! `ai/tests/scaling/test_fortifications.py`, part two: refused gate sites,
//! chokepoints too narrow for the gate, and the shared land-query budget.
use std::collections::BTreeSet;

use aoe_sdk::BuildingKind;

use super::fortifications::{enemy_at, enemy_default, exact_sites, home, lane, manager, step};
use super::{Sent, kingdom};
use crate::play::fortification::frontier;
use crate::play::management::ManagementManager;
use crate::tests::fake::{FakeWorld, building};
use crate::view::Tile;

#[test]
fn test_refused_gate_site_is_not_sent_again_and_queries_stay_bounded() {
    let mut realm = kingdom(60, 1);
    enemy_default(&mut realm);
    exact_sites(&mut realm);
    let center = home(&realm);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    let at = lane(&mut ctx, center).expect("a lane");
    ctx.buildings
        .push(building(900, BuildingKind::WatchTower, at));
    step(&mut ctx);
    let (kind, origin) = tape.orders().builds()[0];
    ctx.board
        .bad_sites
        .insert((format!("{kind:?}"), origin.0, origin.1));
    ctx.board.plans.clear();
    ctx.tick += 1200;
    ctx.busy.clear();
    step(&mut ctx);
    assert_eq!(tape.orders().builds().len(), 1);
    let limit = ctx.config.site_queries_per_decision as usize;
    drop(ctx);
    assert!(realm.world.sites.borrow().len() <= limit);
}

#[test]
fn test_approach_tower_leaves_gate_and_wall_footprints_free() {
    let mut realm = kingdom(60, 1);
    enemy_default(&mut realm);
    exact_sites(&mut realm);
    let center = home(&realm);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    let at = lane(&mut ctx, center).expect("a lane");
    step(&mut ctx);
    let (kind, tower) = tape.orders().builds()[0];
    assert_eq!(kind, BuildingKind::WatchTower);
    // Tower is behind the wall, clear of both its four-tile gate and wings.
    assert!((tower.0 - at.0).abs() >= 4 || (tower.1 - at.1).abs() >= 4);
}

#[test]
fn test_chokepoint_too_narrow_for_the_gate_is_rejected_for_another_cross_section() {
    let mut realm = kingdom(60, 1);
    enemy_at(&mut realm, (52, 250));
    let (x0, hy) = home(&realm);
    // Every cross-section at 18 tiles has only three open tiles: a four-tile gate cannot fit.
    let narrow = hy + 18;
    realm.world.passable_fn = Some(Box::new(move |(x, y)| {
        Some(!(y == narrow && !(x0 - 1..=x0 + 1).contains(&x)))
    }));
    {
        let mut ctx = realm.ctx();
        let found = lane(&mut ctx, (x0, hy));
        assert!(found.is_some_and(|tile| tile.1 != narrow), "{found:?}");
    }
    realm.world.passable_fn = Some(Box::new(move |(x, y)| {
        Some(!((y - hy).abs() > 5 && !(x0 - 1..=x0 + 1).contains(&x)))
    }));
    let mut ctx = realm.ctx();
    assert_eq!(
        lane(&mut ctx, (x0, hy)),
        None,
        "no sampled cross-section can hold the gate"
    );
}

#[test]
fn test_obstructed_gate_footprint_drops_the_lane_and_next_lane_is_used() {
    let mut realm = kingdom(60, 1);
    enemy_default(&mut realm);
    exact_sites(&mut realm);
    let center = home(&realm);
    let first = {
        let mut ctx = realm.ctx();
        lane(&mut ctx, center).expect("a lane")
    };
    realm
        .world
        .buildings
        .push(building(900, BuildingKind::WatchTower, first));
    // A tree blocks the first lane's gate footprint; the engine refuses it.
    let refuse = (first.0 - 2, first.1);
    realm.world.site_fn = Some(Box::new(move |_, near, _, _| {
        (near != refuse).then_some(near)
    }));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    assert!(tape.orders().builds().is_empty());
    assert!(ctx.board.fort_lanes.get(&center).is_none());
    ctx.tick += 1200;
    step(&mut ctx);
    let second = ctx.board.fort_lanes[&center];
    assert_ne!(second, first);
    ctx.buildings
        .push(building(901, BuildingKind::WatchTower, second));
    ctx.tick += 1200;
    step(&mut ctx);
    let gates: Vec<Tile> = tape
        .orders()
        .builds()
        .into_iter()
        .filter(|(kind, _)| *kind == BuildingKind::StoneGate)
        .map(|(_, tile)| tile)
        .collect();
    assert_eq!(gates.first(), Some(&(second.0 - 2, second.1)), "{gates:?}");
}

#[test]
fn test_every_lane_refused_stops_after_the_sampled_candidates() {
    let mut realm = kingdom(60, 1);
    enemy_default(&mut realm);
    realm.world.site_fn = Some(Box::new(|_, _, _, _| None));
    let center = home(&realm);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    for _ in 0..12 {
        ctx.tick += 1200;
        ctx.board.plans.clear();
        step(&mut ctx);
        if let Some(at) = ctx.board.fort_lanes.get(&center).copied() {
            let id = 900 + ctx.buildings.len() as u32;
            ctx.buildings
                .push(building(id, BuildingKind::WatchTower, at));
        }
    }
    assert!(ctx.board.fort_rejected[&center].len() <= ctx.config.fort_lane_distances.len());
    assert!(
        tape.orders()
            .builds()
            .iter()
            .all(|(kind, _)| *kind != BuildingKind::StoneGate)
    );
}

#[test]
fn test_castle_across_water_falls_back_to_a_reachable_settlement_site() {
    let mut realm = kingdom(120, 3);
    enemy_at(&mut realm, (300, 50));
    // East of every settlement is sea: no builder can walk to anything there.
    let limit = realm
        .world
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::TownCenter)
        .map(|b| b.center().0)
        .max()
        .expect("town centers");
    realm.world.route_fn = Some(Box::new(move |from, to| {
        (to.0 <= limit + 1).then(|| FakeWorld::straight(from, to))
    }));
    exact_sites(&mut realm);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    let far = frontier(&ctx).expect("a frontier");
    ManagementManager::default()
        .castles(&mut ctx)
        .expect("the order budget holds");
    let castle = tape.orders().builds()[0];
    assert_eq!(castle.0, BuildingKind::Castle);
    assert!(castle.1.0 <= limit + 1 && castle.1.0 < far.0, "{castle:?}");
}

/// `gate_ready`: a tower stands at the lane and the lane is retained.
fn gate_ready(realm: &mut super::Kingdom) -> Tile {
    enemy_default(realm);
    exact_sites(realm);
    let center = home(realm);
    let at = {
        let mut ctx = realm.ctx();
        lane(&mut ctx, center).expect("a lane")
    };
    realm
        .world
        .buildings
        .push(building(900, BuildingKind::WatchTower, at));
    realm.board.fort_lanes.insert(center, at);
    at
}

#[test]
fn test_exhausted_land_budget_is_not_an_unreachable_gate_and_keeps_the_lane() {
    let mut realm = kingdom(60, 1);
    let at = gate_ready(&mut realm);
    let center = home(&realm);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ctx.land_query_limit = 0;
    step(&mut ctx);
    assert!(tape.orders().builds().is_empty());
    assert_eq!(ctx.board.fort_lanes[&center], at);
    assert!(ctx.board.fort_rejected.is_empty());
    ctx.land_query_limit = ctx.config.land_queries_per_decision;
    ctx.tick += 1200;
    step(&mut ctx);
    assert_eq!(
        tape.orders().builds()[0].0,
        BuildingKind::StoneGate,
        "retried once answerable"
    );
}

#[test]
fn test_answered_unreachable_gate_edges_do_reject_the_lane() {
    let mut realm = kingdom(60, 1);
    let at = gate_ready(&mut realm);
    let center = home(&realm);
    realm.world.route_fn = Some(Box::new(|_, _| None));
    let mut ctx = realm.ctx();
    step(&mut ctx);
    assert_eq!(ctx.board.fort_rejected[&center], BTreeSet::from([at]));
    assert!(ctx.board.fort_lanes.get(&center).is_none());
}

#[test]
fn test_lane_choice_waits_for_land_budget_instead_of_ranking_partial_answers() {
    let mut realm = kingdom(60, 1);
    enemy_default(&mut realm);
    let center = home(&realm);
    let mut ctx = realm.ctx();
    ctx.land_query_limit = 0;
    assert_eq!(manager().lane(&mut ctx, center, &BTreeSet::new()), None);
    ctx.land_query_limit = ctx.config.land_queries_per_decision;
    assert!(manager().lane(&mut ctx, center, &BTreeSet::new()).is_some());
}

#[test]
fn test_castle_waits_for_land_budget_instead_of_falling_back_from_unanswered_frontier() {
    let mut realm = kingdom(120, 3);
    enemy_at(&mut realm, (300, 50));
    exact_sites(&mut realm);
    let hz = u64::from(realm.world.tick_hz);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ctx.land_query_limit = 0;
    ManagementManager::default()
        .castles(&mut ctx)
        .expect("the order budget holds");
    assert!(tape.orders().builds().is_empty());
    ctx.land_query_limit = ctx.config.land_queries_per_decision;
    ctx.tick += 10 * hz + 1;
    ManagementManager::default()
        .castles(&mut ctx)
        .expect("the order budget holds");
    let east = ctx.centers.iter().map(|c| c.0).max().expect("centers");
    assert!(tape.orders().builds()[0].1.0 > east);
}
