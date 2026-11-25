//! `ai/tests/test_imperial.py`: when the AI saves for the Imperial Age, what it
//! raises and researches, and that Castle Age capabilities keep working in it.
use aoe_sdk::{Age, BuildingKind, Tech, UnitKind};

use super::{built_kinds, eco, play, play_fresh, researches, trains, with_age};
use crate::brain::Brain;
use crate::config::Config;
use crate::tests::fake::{FakeWorld, base, building, unit_of};
use crate::view::{Building, Economy, Unit};

const CASTLE: [Tech; 3] = [Tech::Loom, Tech::FeudalAge, Tech::CastleAge];

fn army(count: u32, kind: UnitKind) -> Vec<Unit> {
    (0..count)
        .map(|i| unit_of(300 + i, kind, (60, 60)))
        .collect()
}

fn castle_base(villagers: u32, soldiers: u32) -> (Vec<Unit>, Vec<Building>) {
    let (mut units, mut buildings) = base(villagers);
    units.extend(army(soldiers, UnitKind::Spearman));
    buildings.extend([
        building(600, BuildingKind::Barracks, (60, 50)),
        building(601, BuildingKind::ArcheryRange, (64, 50)),
        building(602, BuildingKind::Blacksmith, (68, 50)),
        building(603, BuildingKind::Stable, (72, 50)),
    ]);
    (units, buildings)
}

fn castle_economy() -> Economy {
    with_age(eco(1500, 900, 1200, 70, 120), Age::Castle, &CASTLE)
}

fn world(base: (Vec<Unit>, Vec<Building>), economy: Economy) -> FakeWorld {
    let mut world = FakeWorld::new(base.0, base.1);
    world.economy = Some(economy);
    world
}

fn university_and_workshop() -> [Building; 2] {
    [
        building(610, BuildingKind::University, (60, 60)),
        building(611, BuildingKind::SiegeWorkshop, (64, 60)),
    ]
}

#[test]
fn test_imperial_age_is_clicked_once_two_castle_age_buildings_and_an_army_stand() {
    let (units, mut buildings) = castle_base(40, 24);
    buildings.extend(university_and_workshop());
    let (_, seat) = play_fresh(&world((units, buildings), castle_economy()));
    assert!(researches(&seat).contains(&(500, Tech::ImperialAge)));
    assert!(
        trains(&seat).is_empty(),
        "the age's budget is saved, not spent on units"
    );
}

#[test]
fn test_imperial_age_waits_for_the_second_building_the_villagers_and_the_army() {
    let one = vec![building(610, BuildingKind::University, (60, 60))];
    let two = university_and_workshop().to_vec();
    for (villagers, soldiers, extra) in [(40, 24, one), (30, 24, two.clone()), (40, 10, two)] {
        let (units, mut buildings) = castle_base(villagers, soldiers);
        buildings.extend(extra);
        let (_, seat) = play_fresh(&world((units, buildings), castle_economy()));
        assert!(
            !researches(&seat).contains(&(500, Tech::ImperialAge)),
            "{villagers} villagers, {soldiers} soldiers"
        );
    }
}

#[test]
fn test_imperial_age_is_not_bought_without_its_food_and_gold() {
    let (units, mut buildings) = castle_base(40, 24);
    buildings.extend(university_and_workshop());
    let poor = Economy {
        gold: 700,
        ..castle_economy()
    };
    let (_, seat) = play_fresh(&world((units, buildings), poor));
    assert!(!researches(&seat).contains(&(500, Tech::ImperialAge)));
}

#[test]
fn test_the_castle_economy_raises_a_university_and_a_siege_workshop_for_the_age() {
    let (units, buildings) = castle_base(36, 10);
    let w = world((units, buildings), castle_economy());
    let (_, seat) = play_fresh(&w);
    assert!(built_kinds(&seat).contains(&BuildingKind::University));
    assert!(
        w.sites
            .borrow()
            .iter()
            .any(|s| s.0 == BuildingKind::University)
    );
    let (units, buildings) = castle_base(20, 10);
    let w = world((units, buildings), castle_economy());
    play_fresh(&w);
    assert!(
        w.sites
            .borrow()
            .iter()
            .all(|s| s.0 != BuildingKind::University)
    );
}

#[test]
fn test_imperial_army_trains_the_newest_identity_of_every_line() {
    let (units, buildings) = castle_base(40, 0);
    let researched: Vec<Tech> = Tech::ALL.to_vec();
    let w = world(
        (units, buildings),
        with_age(castle_economy(), Age::Imperial, &researched),
    );
    // Give these three lines the largest shares of the mix.
    let config = Config {
        army_mix: vec![(UnitKind::Archer, 3.0), (UnitKind::Spearman, 3.0)],
        castle_knight_weight: 3.0,
        ..Config::default()
    };
    let mut brain = Brain::new(config);
    let seat = play(&w, &mut brain, &[]);
    let trained = trains(&seat);
    assert!(
        trained.contains(&(601, UnitKind::Arbalester)),
        "{trained:?}"
    );
    assert!(
        trained.contains(&(600, UnitKind::Halberdier)),
        "{trained:?}"
    );
    assert!(trained.contains(&(603, UnitKind::Paladin)), "{trained:?}");
}

#[test]
fn test_imperial_age_keeps_castle_age_training_and_the_villager_economy() {
    let (units, buildings) = castle_base(34, 6);
    let mut researched = CASTLE.to_vec();
    researched.push(Tech::ImperialAge);
    let economy = Economy {
        food: 3000,
        gold: 3000,
        ..with_age(castle_economy(), Age::Imperial, &researched)
    };
    let w = world((units, buildings), economy);
    let mut brain = Brain::new(Config::default());
    // Keep the Stable free of research so it trains.
    for tech in Tech::ALL {
        brain
            .board
            .cool(&format!("research:{tech:?}"), w.tick, u64::from(w.tick_hz));
    }
    let seat = play(&w, &mut brain, &[]);
    let trained = trains(&seat);
    assert!(trained.contains(&(603, UnitKind::Knight)), "{trained:?}");
    assert!(trained.iter().any(|(_, kind)| *kind == UnitKind::Villager));
}

#[test]
fn test_imperial_upgrades_follow_the_castle_ones_for_fielded_lines() {
    let (mut units, buildings) = castle_base(40, 12);
    units.extend((0..6).map(|i| unit_of(400 + i, UnitKind::Crossbowman, (64, 60))));
    let mut researched = CASTLE.to_vec();
    researched.extend([Tech::ImperialAge, Tech::Crossbowman, Tech::Pikeman]);
    let economy = Economy {
        food: 1500,
        gold: 1500,
        ..with_age(castle_economy(), Age::Imperial, &researched)
    };
    let (_, seat) = play_fresh(&world((units, buildings), economy));
    let done = researches(&seat);
    assert!(done.contains(&(600, Tech::Halberdier)), "{done:?}");
    assert!(done.contains(&(601, Tech::Arbalester)), "{done:?}");
}
