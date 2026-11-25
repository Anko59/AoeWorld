//! `ai/tests/scaling/test_fortifications.py`, part one: towers at threatened
//! sites, the gate before the walls, and the approach lane. The cases about
//! refused sites and the land budget are in `lanes`.
use std::collections::BTreeSet;

use aoe_sdk::{Age, BuildingKind, UnitKind};

use super::{Kingdom, Sent, kingdom};
use crate::context::Context;
use crate::play::fortification::{FortificationManager, frontier};
use crate::play::management::ManagementManager;
use crate::roster::building_price;
use crate::tests::fake::{building, enemy, foreign_building};
use crate::view::{Building, Tile};

pub fn manager() -> FortificationManager {
    FortificationManager::default()
}

pub fn step(ctx: &mut Context<'_>) {
    manager().step(ctx).expect("the order budget holds");
}

/// `manager.lane(ctx, center)`: no rejected approach points.
pub fn lane(ctx: &mut Context<'_>, center: Tile) -> Option<Tile> {
    manager().lane(ctx, center, &BTreeSet::new())
}

/// `resource_site`: a visible raider near a Lumber Camp of ours.
pub fn resource_site(realm: &mut Kingdom) -> Building {
    realm.world.enemies = vec![enemy(998, UnitKind::Militia, (85, 55))];
    let camp = building(900, BuildingKind::LumberCamp, (76, 52));
    realm.world.buildings.push(camp.clone());
    camp
}

/// `enemy(ctx, tile)`: one known enemy Town Center.
pub fn enemy_at(realm: &mut Kingdom, tile: Tile) {
    realm.world.enemy_buildings = vec![foreign_building(999, BuildingKind::TownCenter, tile)];
}

pub fn enemy_default(realm: &mut Kingdom) {
    enemy_at(realm, (220, 90));
}

/// `exact_sites`: the world answers every site query with the asked tile.
pub fn exact_sites(realm: &mut Kingdom) {
    realm.world.site_fn = Some(Box::new(|_, near, _, _| Some(near)));
}

pub fn home(realm: &Kingdom) -> Tile {
    realm.world.town_center_center()
}

#[test]
fn test_resource_tower_precedes_optional_spending_and_is_not_duplicated() {
    let mut realm = kingdom(40, 1);
    let camp = resource_site(&mut realm);
    let hz = u64::from(realm.world.tick_hz);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    let orders = tape.orders().builds();
    assert_eq!(orders.len(), 1, "{orders:?}");
    assert_eq!(orders[0].0, BuildingKind::WatchTower);
    assert!((orders[0].1.0 - camp.center().0).abs() < 12);
    ctx.tick += hz * 60;
    step(&mut ctx);
    assert_eq!(
        tape.orders().builds().len(),
        1,
        "pending tower already protects this camp"
    );
}

#[test]
fn test_low_income_and_age_reserves_keep_defences_from_spending() {
    for change in [
        |e: &mut crate::view::Economy| e.wood = 80,
        |e: &mut crate::view::Economy| e.stone = 200,
        |e: &mut crate::view::Economy| e.age = Age::Dark,
    ] {
        let mut realm = kingdom(40, 1);
        change(realm.economy());
        resource_site(&mut realm);
        let tape = realm.tape();
        let mut ctx = realm.ctx();
        step(&mut ctx);
        assert!(tape.orders().builds().is_empty());
    }
}

#[test]
fn test_more_workers_fund_more_resource_towers() {
    for (workers, wanted) in [(30, 1), (120, 4)] {
        let mut realm = kingdom(workers, 4);
        for i in 0..wanted {
            realm.world.buildings.push(building(
                900 + i,
                BuildingKind::WatchTower,
                (91, 52 + i as i32),
            ));
        }
        resource_site(&mut realm);
        let tape = realm.tape();
        let mut ctx = realm.ctx();
        step(&mut ctx);
        assert!(
            tape.orders().builds().is_empty(),
            "towers serving the threatened site count against its cap and the income ceiling"
        );
    }
}

#[test]
fn test_known_enemy_lane_gets_gate_before_walls_and_keeps_its_opening() {
    let mut realm = kingdom(60, 1);
    enemy_default(&mut realm);
    exact_sites(&mut realm);
    let center = home(&realm);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    // A tower at the lane lets this decision concentrate on the gate.
    let at = lane(&mut ctx, center).expect("a lane");
    ctx.buildings
        .push(building(900, BuildingKind::WatchTower, at));
    step(&mut ctx);
    let gate = tape.orders().builds()[0];
    assert_eq!(gate.0, BuildingKind::StoneGate);
    ctx.buildings.push(building(901, gate.0, gate.1));
    ctx.board.plans.clear();
    ctx.busy.clear();
    ctx.tick += 1200;
    step(&mut ctx);
    let builds = tape.orders().builds();
    let wall = *builds.last().expect("a wall");
    assert_eq!(wall.0, BuildingKind::StoneWall);
    let opening: Vec<Tile> = (0..4).map(|i| (gate.1.0 + i, gate.1.1)).collect();
    assert!(!opening.contains(&wall.1), "{wall:?} in the gate's opening");
    assert_eq!(builds.len(), 2);
}

#[test]
fn test_unknown_enemy_does_not_get_invented_approach_walls() {
    let mut realm = kingdom(60, 1);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    assert!(tape.orders().builds().is_empty());
}

#[test]
fn test_unknown_or_unreachable_lane_never_receives_defensive_orders() {
    let mut realm = kingdom(60, 1);
    enemy_default(&mut realm);
    realm.world.route_fn = Some(Box::new(|_, _| None));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    assert!(tape.orders().builds().is_empty());
}

#[test]
fn test_frontier_castle_follows_enemy_facing_settlement() {
    let mut realm = kingdom(120, 3);
    enemy_at(&mut realm, (300, 50));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    let point = frontier(&ctx).expect("a frontier");
    let east = ctx.centers.iter().map(|c| c.0).max().expect("centers");
    assert!(point.0 > east);
    ManagementManager::default()
        .castles(&mut ctx)
        .expect("the order budget holds");
    let castle = tape.orders().builds()[0];
    assert_eq!(castle.0, BuildingKind::Castle);
    assert!(castle.1.0 > east);
}

#[test]
fn test_exact_wall_footprints_are_never_shifted_into_gate_or_worker_lanes() {
    let mut realm = kingdom(60, 1);
    enemy_default(&mut realm);
    let center = home(&realm);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    let at = lane(&mut ctx, center).expect("a lane");
    ctx.buildings
        .push(building(900, BuildingKind::WatchTower, at));
    step(&mut ctx);
    assert!(
        tape.orders().builds().is_empty(),
        "relocated site cannot preserve wall geometry"
    );
}

#[test]
fn test_terrain_narrows_the_selected_known_enemy_approach() {
    let mut realm = kingdom(60, 1);
    enemy_at(&mut realm, (52, 250));
    let (hx, hy) = home(&realm);
    // A gate-wide (four-tile) opening south of home is preferable to a broad open lane.
    let narrow = hy + 18;
    realm.world.passable_fn = Some(Box::new(move |(x, y)| {
        Some(!(y == narrow && !(hx - 2..=hx + 1).contains(&x)))
    }));
    let mut ctx = realm.ctx();
    let found = lane(&mut ctx, (hx, hy)).expect("a lane");
    assert_eq!(found.1, narrow);
}

#[test]
fn test_unexplored_terrain_does_not_masquerade_as_a_chokepoint() {
    let mut realm = kingdom(60, 1);
    enemy_at(&mut realm, (52, 250));
    realm.world.passable_fn = Some(Box::new(|_| None));
    let center = home(&realm);
    let mut ctx = realm.ctx();
    assert_eq!(lane(&mut ctx, center), None);
}

#[test]
fn test_tower_can_use_expansion_float_while_preserving_house_money() {
    let mut realm = kingdom(40, 1);
    realm.economy().wood = building_price(BuildingKind::TownCenter)[1] - 25;
    resource_site(&mut realm);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ManagementManager::default()
        .expand(&mut ctx)
        .expect("the order budget holds");
    step(&mut ctx);
    assert!(!tape.orders().towers().is_empty());
    let wood = ctx.economy.as_ref().expect("economy").wood;
    assert!(wood - ctx.spent[1] >= ctx.config.wood_reserve);
}

#[test]
fn test_a_tree_at_one_approach_corner_does_not_block_an_otherwise_reachable_tower() {
    let mut realm = kingdom(40, 1);
    let camp = resource_site(&mut realm);
    exact_sites(&mut realm);
    // North-west is wooded; a worker can still reach the east footprint edge.
    let blocked = (camp.center().0 - 1, camp.center().1 - 1);
    realm.world.route_fn = Some(Box::new(move |from, to| {
        (to != blocked).then(|| vec![from, to])
    }));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    step(&mut ctx);
    assert!(!tape.orders().towers().is_empty());
}

#[test]
fn test_vassal_defences_do_not_wait_for_every_future_castle() {
    let mut realm = kingdom(120, 5);
    realm.push_building(super::seated(
        950,
        BuildingKind::Castle,
        (65, 65),
        (951, UnitKind::Vassal),
    ));
    enemy_at(&mut realm, (130, 80));
    let tape = realm.tape();
    {
        // Native Vassals already preserve 200 food/wood/stone and 100 gold.
        // A growing castle target must not prevent all defensive development.
        let mut ctx = realm.ctx();
        ManagementManager::default()
            .settings(&mut ctx)
            .expect("the order budget holds");
    }
    assert_eq!(tape.orders().vassal_development(), vec![(951, true)]);
    realm.config.fortifications = false;
    let mut ctx = realm.ctx();
    for _ in 0..2 {
        ManagementManager::default()
            .settings(&mut ctx)
            .expect("the order budget holds");
    }
    assert_eq!(
        tape.orders().vassal_development(),
        vec![(951, true), (951, false)]
    );
}
