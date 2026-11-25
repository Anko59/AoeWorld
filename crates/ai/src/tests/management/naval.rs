//! `ai/tests/test_naval.py`: river decisions use public terrain knowledge and
//! keep the navy small. The two economy cases of that file (a villager picks
//! shore fish, villagers leave deep fish for ships) belong to the food code
//! and are ported with the economy tests.
use aoe_sdk::{Age, BuildingKind, ResourceKind, UnitKind};

use super::{Kingdom, Sent, scene};
use crate::play::naval::NavalManager;
use crate::tests::fake::{FakeWorld, base, building, economy, enemy, node, unit_of};
use crate::view::{Building, Economy, Unit};

/// `river_world`: a seven-tile river (x 56 to 62). Our villagers reach the
/// west bank, but neither the water nor the east bank.
fn river_world(units: Option<Vec<Unit>>, docks: Vec<Building>, tick: u64) -> FakeWorld {
    let (villagers, mut buildings) = base(4);
    buildings.extend(docks);
    let mut world = FakeWorld::new(units.unwrap_or(villagers), buildings);
    world.economy = Some(Economy {
        food: 500,
        wood: 1000,
        gold: 500,
        population: 5,
        population_cap: 30,
        age: Age::Feudal,
        ..economy()
    });
    world.nodes = vec![
        node(700, ResourceKind::Food, (56, 52)),
        node(701, ResourceKind::Food, (59, 52)),
    ];
    world.tick = tick;
    world.passable_fn = Some(Box::new(|(x, _)| Some(!(56..=62).contains(&x))));
    world.route_fn = Some(Box::new(|(x, y), (tx, ty)| {
        (x <= 55 && tx <= 55).then(|| vec![(x, y), (tx, ty)])
    }));
    world
}

fn dock(queue: usize) -> Building {
    Building {
        queue_length: queue,
        idle: queue == 0,
        ..building(600, BuildingKind::Dock, (55, 50))
    }
}

fn step(realm: &mut Kingdom) {
    let mut ctx = realm.ctx();
    NavalManager::default()
        .step(&mut ctx)
        .expect("the order budget holds");
}

fn galley_enemy(realm: &mut Kingdom) {
    realm.world.enemies = vec![enemy(999, UnitKind::Galley, (65, 55))];
}

#[test]
fn test_naval_manager_requires_known_water_before_planning_a_dock() {
    let mut world = river_world(None, vec![], 100);
    world.nodes = vec![node(703, ResourceKind::Food, (54, 52))];
    let mut realm = scene(world);
    step(&mut realm);
    assert!(realm.tape().orders().builds().is_empty());
    assert!(
        !realm
            .world
            .sites
            .borrow()
            .iter()
            .any(|site| site.0 == BuildingKind::Dock)
    );
}

#[test]
fn test_known_river_food_prompts_one_dock_and_no_duplicate_plan() {
    let mut realm = scene(river_world(None, vec![], 100));
    step(&mut realm);
    let docks = realm
        .tape()
        .orders()
        .builds()
        .into_iter()
        .filter(|(kind, _)| *kind == BuildingKind::Dock)
        .count();
    assert_eq!(docks, 1);
    assert!(
        realm
            .world
            .sites
            .borrow()
            .iter()
            .any(|site| site.0 == BuildingKind::Dock)
    );
    realm.world.tick = 110;
    step(&mut realm);
    assert_eq!(realm.tape().orders().builds().len(), 1);
}

#[test]
fn test_dock_starts_with_one_fishing_ship_and_waits_for_its_order() {
    let mut world = river_world(None, vec![dock(0)], 100);
    if let Some(eco) = world.economy.as_mut() {
        eco.population = 6_000;
        eco.population_cap = 6_010;
    }
    let mut realm = scene(world);
    step(&mut realm);
    assert_eq!(
        realm.tape().orders().trains(),
        vec![(600, UnitKind::FishingShip)]
    );
    realm.world.tick = 110;
    step(&mut realm);
    assert_eq!(
        realm.tape().orders().trains().len(),
        1,
        "the training verdict is still in flight"
    );
}

#[test]
fn test_fishing_ship_works_river_food_and_galleys_stop_at_two() {
    let fishing = unit_of(200, UnitKind::FishingShip, (59, 50));
    let mut realm = scene(river_world(Some(vec![fishing.clone()]), vec![dock(0)], 100));
    galley_enemy(&mut realm);
    step(&mut realm);
    let orders = realm.tape().orders();
    assert!(
        !orders.gathers().is_empty(),
        "the Fishing Ship needs a river fishing order"
    );
    assert_eq!(orders.gathers()[0].0, fishing.id);
    assert!([700, 701].contains(&orders.gathers()[0].1));
    assert_eq!(orders.trains(), vec![(600, UnitKind::Galley)]);

    let mut fleet = vec![fishing];
    fleet.extend((0..2).map(|i| unit_of(201 + i, UnitKind::Galley, (59, 50 + i as i32))));
    let mut full = scene(river_world(Some(fleet), vec![dock(0)], 100));
    step(&mut full);
    assert!(full.tape().orders().trains().is_empty());
}

#[test]
fn test_busy_dock_does_not_overqueue_the_bounded_fleet() {
    let mut realm = scene(river_world(None, vec![dock(1)], 100));
    step(&mut realm);
    assert!(realm.tape().orders().trains().is_empty());
}

#[test]
fn test_castle_age_keeps_feudal_capabilities_and_galley_production() {
    let fishing = unit_of(200, UnitKind::FishingShip, (59, 50));
    let mut world = river_world(Some(vec![fishing]), vec![dock(0)], 100);
    world.economy = Some(Economy {
        food: 500,
        wood: 1000,
        gold: 500,
        population: 5,
        population_cap: 30,
        age: Age::Castle,
        ..economy()
    });
    let mut realm = scene(world);
    assert!(
        realm.ctx().feudal(),
        "Castle Age retains access to Feudal military and research"
    );
    galley_enemy(&mut realm);
    step(&mut realm);
    assert_eq!(
        realm.tape().orders().trains(),
        vec![(600, UnitKind::Galley)]
    );
}

#[test]
fn test_upgraded_war_galleys_still_fill_the_two_ship_fleet_limit() {
    let mut fleet = vec![unit_of(200, UnitKind::FishingShip, (59, 50))];
    fleet.extend((0..2).map(|i| unit_of(201 + i, UnitKind::WarGalley, (59, 51 + i as i32))));
    let mut world = river_world(Some(fleet), vec![dock(0)], 100);
    world.economy = Some(Economy {
        food: 500,
        wood: 1000,
        gold: 500,
        population: 5,
        population_cap: 30,
        age: Age::Castle,
        ..economy()
    });
    let mut realm = scene(world);
    step(&mut realm);
    assert!(
        realm.tape().orders().trains().is_empty(),
        "upgrading retains the small fleet cap"
    );
}
