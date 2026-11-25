//! The economy cases of `ai/tests/test_logic.py`: villagers, buildings,
//! research and training, decided by `Brain::decide`.
use std::collections::BTreeSet;

use aoe_sdk::{Age, AnimalKind, BuildingKind, CommandResult, Event, Order, ResourceKind, Tech};
use aoe_sdk::{UnitKind, UnitTask};

use super::{
    animal, builds, built_kinds, eco, farms, gathers, hunts, play, play_fresh, researches,
    sequence_of, trains, with_age,
};
use crate::tests::fake::{FakeWorld, base, building, node, order_result, unit, unit_of};
use crate::view::{Building, Economy};

fn world_with(base: (Vec<crate::view::Unit>, Vec<Building>), economy: Economy) -> FakeWorld {
    let mut world = FakeWorld::new(base.0, base.1);
    world.economy = Some(economy);
    world
}

fn queued(tc: &Building) -> Building {
    Building {
        queue_length: 1,
        idle: false,
        ..tc.clone()
    }
}

#[test]
fn test_the_town_center_queue_is_kept_full_while_food_lasts() {
    let world = world_with(base(5), eco(500, 200, 100, 6, 15));
    let (_, seat) = play_fresh(&world);
    assert_eq!(
        trains(&seat),
        vec![(500, UnitKind::Villager), (500, UnitKind::Villager)]
    );
}

#[test]
fn test_idle_villagers_follow_the_wood_and_food_shares() {
    let mut world = world_with(base(6), eco(200, 200, 100, 7, 20));
    world.nodes = vec![
        node(1, ResourceKind::Wood, (56, 52)),
        node(2, ResourceKind::Food, (44, 52)),
    ];
    let (_, seat) = play_fresh(&world);
    let gathered: BTreeSet<u64> = gathers(&seat).into_iter().map(|(_, n)| n).collect();
    assert_eq!(gathered, BTreeSet::from([1, 2]), "{:?}", seat.orders);
}

#[test]
fn test_hunting_comes_before_foraging() {
    let mut world = world_with(base(4), eco(200, 200, 100, 5, 20));
    world.nodes = vec![node(2, ResourceKind::Food, (44, 52))];
    world.animals = vec![animal(900, (60, 55), AnimalKind::Sheep)];
    let (_, seat) = play_fresh(&world);
    assert_eq!(hunts(&seat).first(), Some(&900));
}

#[test]
fn test_far_wood_gets_a_lumber_camp_beside_it() {
    let mut world = world_with(base(4), eco(200, 200, 100, 5, 20));
    world.nodes = vec![node(1, ResourceKind::Wood, (80, 50))];
    let (_, seat) = play_fresh(&world);
    assert!(built_kinds(&seat).contains(&BuildingKind::LumberCamp));
    assert!(
        world
            .sites
            .borrow()
            .iter()
            .any(|(kind, at)| *kind == BuildingKind::LumberCamp && (at.0 - 80).abs() <= 8)
    );
}

#[test]
fn test_farms_start_with_a_mill_when_nothing_else_feeds_us() {
    let (units, mut buildings) = base(4);
    let rich = eco(200, 300, 100, 5, 20);
    let (mut brain, seat) = play_fresh(&world_with(
        (units.clone(), buildings.clone()),
        rich.clone(),
    ));
    assert!(built_kinds(&seat).contains(&BuildingKind::Mill));
    // With a mill standing, the next idle villager builds a farm, then works it.
    buildings.push(building(600, BuildingKind::Mill, (54, 50)));
    brain.board.plans.clear();
    brain.board.builders.clear();
    let mut world = world_with((units.clone(), buildings.clone()), rich.clone());
    world.tick = 400;
    let seat = play(&world, &mut brain, &[]);
    assert!(built_kinds(&seat).contains(&BuildingKind::Farm));
    buildings.push(building(700, BuildingKind::Farm, (56, 50)));
    brain.board.plans.clear();
    let mut world = world_with((units, buildings), rich);
    world.tick = 700;
    let seat = play(&world, &mut brain, &[]);
    assert_eq!(farms(&seat).first().map(|f| f.1), Some(700));
}

#[test]
fn test_a_refused_resource_is_never_tried_again() {
    let mut world = world_with(base(3), eco(200, 200, 100, 4, 20));
    world.nodes = vec![
        node(1, ResourceKind::Wood, (56, 52)),
        node(3, ResourceKind::Wood, (58, 52)),
    ];
    let (mut brain, seat) = play_fresh(&world);
    let first = gathers(&seat)[0].1;
    let sequence = sequence_of(
        &seat,
        |o| matches!(o, Order::Gather { resource, .. } if *resource == first),
    );
    let refusal = order_result(sequence, CommandResult::RejectedInvalidResource);
    let seat = play(&world, &mut brain, &[refusal]);
    assert!(brain.board.bad_resources.contains(&first));
    assert!(gathers(&seat).iter().all(|(_, n)| *n != first));
}

#[test]
fn test_loom_then_feudal_age_with_the_queue_drained_first() {
    let world = world_with(base(12), eco(200, 200, 100, 13, 25));
    let (_, seat) = play_fresh(&world);
    assert_eq!(researches(&seat), vec![(500, Tech::Loom)]);

    let (units, mut buildings) = base(22);
    buildings.push(building(600, BuildingKind::Mill, (54, 50)));
    buildings.push(building(601, BuildingKind::LumberCamp, (58, 50)));
    let economy = with_age(eco(600, 200, 0, 23, 30), Age::Dark, &[Tech::Loom]);
    let busy: Vec<Building> = buildings
        .iter()
        .map(|b| {
            if b.kind == BuildingKind::TownCenter {
                queued(b)
            } else {
                b.clone()
            }
        })
        .collect();
    let (_, seat) = play_fresh(&world_with((units.clone(), busy), economy.clone()));
    assert!(researches(&seat).is_empty(), "the queue must drain first");
    assert!(trains(&seat).is_empty(), "and nothing is queued behind it");
    let (_, seat) = play_fresh(&world_with((units, buildings), economy));
    assert_eq!(researches(&seat), vec![(500, Tech::FeudalAge)]);
}

#[test]
fn test_the_army_mix_trains_the_least_represented_unit() {
    let (mut units, mut buildings) = base(20);
    buildings.push(building(600, BuildingKind::Barracks, (60, 50)));
    buildings.push(building(601, BuildingKind::ArcheryRange, (64, 50)));
    units.extend((0..6).map(|i| crate::view::Unit {
        task: UnitTask::Moving,
        idle: false,
        ..unit_of(200 + i, UnitKind::Spearman, (55, 55))
    }));
    let economy = with_age(eco(900, 900, 500, 22, 40), Age::Feudal, &[]);
    let (_, seat) = play_fresh(&world_with((units, buildings), economy));
    let trained: std::collections::BTreeMap<u32, UnitKind> = trains(&seat)
        .into_iter()
        .filter(|(b, _)| *b == 600 || *b == 601)
        .collect();
    assert!(matches!(
        trained[&600],
        UnitKind::Spearman | UnitKind::Militia
    ));
    assert!(matches!(
        trained[&601],
        UnitKind::Archer | UnitKind::Skirmisher
    ));
}

#[test]
fn test_a_refused_spearman_is_replaced_by_militia_until_the_age_changes() {
    let (units, mut buildings) = base(20);
    buildings.push(building(600, BuildingKind::Barracks, (60, 50)));
    let economy = with_age(eco(900, 900, 500, 22, 40), Age::Feudal, &[]);
    let world = world_with((units, buildings), economy);
    let (mut brain, seat) = play_fresh(&world);
    let first = trains(&seat)
        .into_iter()
        .find(|t| t.0 == 600)
        .expect("trained");
    assert_eq!(first.1, UnitKind::Spearman);
    let sequence = sequence_of(
        &seat,
        |o| matches!(o, Order::Train { building, .. } if building.0 == 600),
    );
    let refusal = order_result(sequence, CommandResult::RejectedRequiresFeudalAge);
    let seat = play(&world, &mut brain, &[refusal]);
    let second = trains(&seat)
        .into_iter()
        .find(|t| t.0 == 600)
        .expect("trained");
    assert_eq!(second.1, UnitKind::Militia);
    let advanced = [Event::AgeAdvanced { age: Age::Feudal }];
    let seat = play(&world, &mut brain, &advanced);
    let third = trains(&seat)
        .into_iter()
        .find(|t| t.0 == 600)
        .expect("trained");
    assert_eq!(third.1, UnitKind::Spearman);
}

#[test]
fn test_no_unit_is_trained_in_the_dark_age() {
    let (units, mut buildings) = base(20);
    buildings.push(building(600, BuildingKind::Barracks, (60, 50)));
    let world = world_with((units, buildings), eco(900, 900, 500, 22, 40));
    let (_, seat) = play_fresh(&world);
    assert!(trains(&seat).iter().all(|(b, _)| *b != 600));
}

#[test]
fn test_the_barracks_is_raised_while_feudal_age_researches() {
    let (units, mut buildings) = base(22);
    buildings[0] = Building {
        researching: Some(Tech::FeudalAge),
        idle: false,
        ..buildings[0].clone()
    };
    buildings.push(building(600, BuildingKind::Mill, (54, 50)));
    buildings.push(building(601, BuildingKind::LumberCamp, (58, 50)));
    let economy = with_age(eco(100, 400, 100, 23, 40), Age::Dark, &[Tech::Loom]);
    let (_, seat) = play_fresh(&world_with((units, buildings), economy));
    assert!(built_kinds(&seat).contains(&BuildingKind::Barracks));
}

#[test]
fn test_feudal_age_adds_a_range_then_a_blacksmith_and_economy_upgrades() {
    let (units, mut buildings) = base(24);
    buildings.push(building(600, BuildingKind::Barracks, (60, 50)));
    buildings.push(building(601, BuildingKind::LumberCamp, (58, 56)));
    let economy = with_age(eco(900, 900, 300, 25, 60), Age::Feudal, &[Tech::Loom]);
    let (mut brain, seat) = play_fresh(&world_with(
        (units.clone(), buildings.clone()),
        economy.clone(),
    ));
    assert!(built_kinds(&seat).contains(&BuildingKind::ArcheryRange));
    assert_eq!(researches(&seat), vec![(601, Tech::DoubleBitAxe)]);
    // Finish the second distinct Feudal kind before duplicating production.
    brain.board.plans.clear();
    buildings.push(building(602, BuildingKind::ArcheryRange, (64, 50)));
    brain.board.cooldowns.clear();
    let seat = play(
        &world_with((units.clone(), buildings.clone()), economy.clone()),
        &mut brain,
        &[],
    );
    assert!(built_kinds(&seat).contains(&BuildingKind::Blacksmith));
    brain.board.plans.clear();
    buildings.push(building(603, BuildingKind::Blacksmith, (68, 50)));
    brain.board.cooldowns.clear();
    let seat = play(&world_with((units, buildings), economy), &mut brain, &[]);
    assert!(built_kinds(&seat).contains(&BuildingKind::Barracks));
}

#[test]
fn test_sheep_come_before_berries_and_a_villager_that_stays_idle_is_not_spammed() {
    let mut world = world_with(base(3), eco(200, 200, 100, 4, 20));
    world.nodes = vec![node(2, ResourceKind::Food, (44, 52))];
    world.animals = vec![animal(900, (46, 50), AnimalKind::Sheep)];
    let (mut brain, seat) = play_fresh(&world);
    assert_eq!(hunts(&seat).first(), Some(&900));
    // The same idle villagers at the next decision (one second later) get no new order.
    world.tick += 20;
    let seat = play(&world, &mut brain, &[]);
    assert!(hunts(&seat).is_empty() && gathers(&seat).is_empty());
}

#[test]
fn test_idle_hunters_take_one_quarry_each_and_never_write_off_a_fresh_herd() {
    let mut world = world_with(base(8), eco(200, 200, 100, 9, 30));
    world.animals = vec![
        animal(900, (46, 50), AnimalKind::Sheep),
        animal(901, (45, 51), AnimalKind::Sheep),
    ];
    let (mut brain, seat) = play_fresh(&world);
    let mut targets = hunts(&seat);
    targets.sort_unstable();
    assert_eq!(targets, vec![900, 901]);
    let mut last = Vec::new();
    for _ in 0..8 {
        world.tick += u64::from(world.tick_hz);
        last = hunts(&play(&world, &mut brain, &[]));
        assert!(!brain.board.bad_resources.contains(&900));
        assert!(!brain.board.bad_resources.contains(&901));
    }
    assert!(!last.is_empty(), "the herd is still worth hunting");
}

#[test]
fn test_a_world_limit_refusal_keeps_the_site_and_pauses_building() {
    let world = world_with(base(20), eco(900, 900, 500, 20, 25));
    let (mut brain, seat) = play_fresh(&world);
    let site = *builds(&seat).first().expect("a house is raised");
    let sequence = sequence_of(&seat, |o| matches!(o, Order::Build { .. }));
    let refusal = order_result(sequence, CommandResult::RejectedWorldLimit);
    let seat = play(&world, &mut brain, &[refusal]);
    assert!(
        !brain
            .board
            .bad_sites
            .contains(&(format!("{:?}", site.0), site.1.0, site.1.1))
    );
    assert!(builds(&seat).is_empty());
}

#[test]
fn test_a_villager_that_cannot_reach_its_node_is_sent_to_another() {
    // One villager and two wood nodes; the near one cannot be reached: the
    // villager is idle again a few seconds after the order.
    let units = vec![unit(1, (50, 56))];
    let buildings = vec![
        building(500, BuildingKind::TownCenter, (50, 50)),
        building(501, BuildingKind::LumberCamp, (60, 60)),
    ];
    let nodes = vec![
        node(700, ResourceKind::Wood, (58, 58)),
        node(701, ResourceKind::Wood, (64, 64)),
    ];
    let economy = Economy {
        food: 0,
        wood: 0,
        gold: 0,
        stone: 0,
        population: 1,
        population_cap: 10,
        ..crate::tests::fake::economy()
    };
    let mut brain = crate::brain::Brain::new(Default::default());
    let mut targets = Vec::new();
    for tick in [100, 200, 300] {
        let mut world = FakeWorld::new(units.clone(), buildings.clone());
        world.economy = Some(economy.clone());
        world.nodes = nodes.clone();
        world.tick = tick;
        let seat = play(&world, &mut brain, &[]);
        targets.extend(gathers(&seat).into_iter().filter(|g| g.0 == 1).map(|g| g.1));
    }
    assert_eq!(targets.first(), Some(&700), "{targets:?}");
    assert!(
        targets.len() > 1 && targets[1..].iter().all(|t| *t == 701),
        "{targets:?}"
    );
}

#[test]
fn test_town_center_rallies_to_food_then_wood_and_keeps_the_same_target() {
    let (units, mut buildings) = base(5);
    let food = node(2, ResourceKind::Food, (44, 52));
    let wood = node(1, ResourceKind::Wood, (56, 52));
    for (nodes, expected) in [(vec![food.clone(), wood.clone()], 2u64), (vec![wood], 1)] {
        let mut world = FakeWorld::new(units.clone(), buildings.clone());
        world.economy = Some(eco(500, 200, 100, 6, 15));
        world.nodes = nodes;
        let (_, seat) = play_fresh(&world);
        let rallies: Vec<_> = seat
            .orders
            .iter()
            .filter(|o| matches!(o, Order::SetRally { .. }))
            .collect();
        assert_eq!(rallies.len(), 1, "{:?}", seat.orders);
        assert!(matches!(rallies[0], Order::SetRally { building, target }
            if building.0 == 500 && *target == aoe_sdk::RallyTarget::Resource(expected)));
        buildings[0].rally_resource = Some(expected);
        world.buildings = buildings.clone();
        let (_, accepted) = play_fresh(&world);
        assert!(
            accepted
                .orders
                .iter()
                .all(|o| !matches!(o, Order::SetRally { .. }))
        );
        buildings[0].rally_resource = None;
    }
}
