//! What `Brain::decide` does before the managers: the peak stats and `absorb`
//! (`ai/aoe_ai/brain.py`), against fake worlds.
use aoe_sdk::{
    Age, BuildingKind, CommandResult, EntityId, Event, Order, PlayerId, TileCoord, UnitKind,
};

use super::fake::{FakeWorld, Recorder, base, building, economy, order_result, soldier, unit_of};
use crate::blackboard::Tracked;
use crate::brain::Brain;
use crate::config::Config;
use crate::view::Economy;

pub fn decide(brain: &mut Brain, world: &FakeWorld, events: &[Event]) -> Recorder {
    let mut seat = Recorder::default();
    brain.decide(world, events, &mut seat);
    seat
}

pub fn tracked(name: &str, order: Order, tick: u64) -> Tracked {
    Tracked {
        name: name.into(),
        order,
        tick,
    }
}

fn brain() -> Brain {
    Brain::new(Config::default())
}

fn stat(brain: &Brain, name: &str) -> Option<i64> {
    brain
        .board
        .stats
        .get(name)
        .map(|v| i64::try_from(*v).unwrap_or(i64::MAX))
}

fn gather(unit: u32, resource: u64) -> Order {
    Order::Gather {
        unit: EntityId(unit),
        resource,
    }
}

#[test]
fn peaks_record_the_most_villagers_soldiers_and_population_ever_seen() {
    let (mut units, buildings) = base(12);
    units.extend([
        soldier(300, (55, 55)),
        soldier(301, (55, 56)),
        soldier(302, (55, 57)),
    ]);
    let mut world = FakeWorld::new(units, buildings.clone());
    world.economy = Some(Economy {
        population: 20,
        population_cap: 40,
        ..economy()
    });
    let mut brain = brain();
    decide(&mut brain, &world, &[]);
    assert_eq!(stat(&brain, "villagers_peak"), Some(12));
    assert_eq!(stat(&brain, "army_peak"), Some(3));
    assert_eq!(stat(&brain, "pop_peak"), Some(20));

    let (units, _) = base(5);
    let mut smaller = FakeWorld::new(units, buildings);
    smaller.economy = Some(Economy {
        population: 8,
        ..economy()
    });
    decide(&mut brain, &smaller, &[]);
    assert_eq!(
        stat(&brain, "villagers_peak"),
        Some(12),
        "a peak never falls"
    );
    assert_eq!(stat(&brain, "army_peak"), Some(3));
    assert_eq!(stat(&brain, "pop_peak"), Some(20));
}

#[test]
fn the_army_peak_counts_soldiers_a_general_leads() {
    let (mut units, buildings) = base(4);
    let mut led = soldier(302, (55, 55));
    led.commanded_by = Some(900);
    units.extend([soldier(300, (55, 55)), soldier(301, (56, 55)), led]);
    let world = FakeWorld::new(units, buildings);
    let mut brain = brain();
    decide(&mut brain, &world, &[]);
    assert_eq!(stat(&brain, "army_peak"), Some(3));
}

#[test]
fn castle_unit_peak_counts_living_castle_age_units_and_only_from_the_castle_age() {
    let (mut units, buildings) = base(4);
    units.extend([
        unit_of(300, UnitKind::Knight, (55, 55)),
        unit_of(301, UnitKind::Knight, (56, 55)),
    ]);
    let mut dead = unit_of(302, UnitKind::Knight, (57, 55));
    dead.alive = false;
    units.extend([dead, soldier(303, (58, 55))]);
    let mut world = FakeWorld::new(units, buildings);
    let mut brain = brain();
    decide(&mut brain, &world, &[]);
    assert_eq!(
        stat(&brain, "castle_units_peak"),
        None,
        "the Dark Age has no Castle units"
    );
    world.economy = Some(Economy {
        age: Age::Castle,
        ..economy()
    });
    decide(&mut brain, &world, &[]);
    assert_eq!(stat(&brain, "castle_units_peak"), Some(2));
}

#[test]
fn without_an_economy_the_peaks_are_kept_and_no_order_is_sent() {
    let (units, buildings) = base(6);
    let mut world = FakeWorld::new(units, buildings);
    world.economy = None;
    let mut brain = brain();
    let seat = decide(&mut brain, &world, &[]);
    assert!(seat.orders.is_empty());
    assert_eq!(stat(&brain, "villagers_peak"), Some(6));
    assert_eq!(stat(&brain, "pop_peak"), Some(0));
}

#[test]
fn an_accepted_order_is_forgotten_and_counted() {
    let (units, buildings) = base(3);
    let world = FakeWorld::new(units, buildings);
    let mut brain = brain();
    brain
        .board
        .tracked
        .insert(5, tracked("gather", gather(1, 7), 90));
    decide(
        &mut brain,
        &world,
        &[order_result(5, CommandResult::Accepted)],
    );
    assert!(brain.board.tracked.is_empty());
    assert_eq!(stat(&brain, "accepted"), Some(1));
    assert_eq!(stat(&brain, "rejected"), None);
}

#[test]
fn a_refused_order_is_counted_by_result_and_by_order_kind() {
    let (units, buildings) = base(3);
    let world = FakeWorld::new(units, buildings);
    let mut brain = brain();
    brain
        .board
        .tracked
        .insert(5, tracked("gather", gather(1, 7), 90));
    let refusal = order_result(5, CommandResult::RejectedNotOwner);
    decide(&mut brain, &world, &[refusal]);
    assert!(brain.board.tracked.is_empty());
    assert_eq!(stat(&brain, "rejected"), Some(1));
    assert_eq!(stat(&brain, "rejected:RejectedNotOwner"), Some(1));
    assert_eq!(stat(&brain, "rejected:gather:RejectedNotOwner"), Some(1));
}

#[test]
fn a_verdict_for_an_order_we_no_longer_track_is_only_counted() {
    let (units, buildings) = base(3);
    let world = FakeWorld::new(units, buildings);
    let mut brain = brain();
    let refusal = order_result(77, CommandResult::RejectedInvalidResource);
    decide(&mut brain, &world, &[refusal]);
    assert_eq!(stat(&brain, "rejected"), Some(1));
    assert_eq!(stat(&brain, "rejected:RejectedInvalidResource"), Some(1));
    assert!(brain.board.bad_resources.is_empty());
}

#[test]
fn a_refused_resource_is_never_tried_again() {
    let (units, buildings) = base(3);
    let world = FakeWorld::new(units, buildings);
    let mut brain = brain();
    brain
        .board
        .tracked
        .insert(1, tracked("gather", gather(1, 7), 90));
    decide(
        &mut brain,
        &world,
        &[order_result(1, CommandResult::RejectedInvalidResource)],
    );
    assert!(brain.board.bad_resources.contains(&7));
}

#[test]
fn deaths_and_completions_are_counted() {
    let (units, buildings) = base(3);
    let world = FakeWorld::new(units, buildings);
    let died = |mine| Event::UnitDied {
        id: EntityId(9),
        kind: UnitKind::Spearman,
        owner: PlayerId(if mine { 0 } else { 1 }),
        mine,
        tile: TileCoord::new(10, 10),
    };
    let done = Event::BuildingCompleted {
        id: EntityId(600),
        kind: BuildingKind::House,
    };
    let mut brain = brain();
    decide(
        &mut brain,
        &world,
        &[died(true), died(false), died(false), done],
    );
    assert_eq!(stat(&brain, "units_lost"), Some(1));
    assert_eq!(stat(&brain, "kills"), Some(2));
    assert_eq!(stat(&brain, "buildings_completed"), Some(1));
}

#[test]
fn a_refused_spearman_is_replaced_until_the_age_changes() {
    let (units, mut buildings) = base(20);
    buildings.push(building(600, BuildingKind::Barracks, (60, 50)));
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(Economy {
        age: Age::Feudal,
        ..economy()
    });
    let mut brain = brain();
    let train = Order::Train {
        building: EntityId(600),
        kind: UnitKind::Spearman,
    };
    brain.board.tracked.insert(1, tracked("train", train, 90));
    let refusal = order_result(1, CommandResult::RejectedRequiresFeudalAge);
    decide(&mut brain, &world, &[refusal]);
    assert_eq!(brain.board.blocked_units.len(), 1);
    decide(
        &mut brain,
        &world,
        &[Event::AgeAdvanced { age: Age::Feudal }],
    );
    assert!(brain.board.blocked_units.is_empty());
}

#[test]
fn an_age_advance_stamps_the_first_tick_of_the_age_the_world_is_in() {
    let (units, buildings) = base(3);
    let advanced = [Event::AgeAdvanced { age: Age::Feudal }];
    for (age, key) in [
        (Age::Dark, "feudal_tick"),
        (Age::Feudal, "feudal_tick"),
        (Age::Castle, "castle_tick"),
        (Age::Imperial, "imperial_tick"),
    ] {
        let mut world = FakeWorld::new(units.clone(), buildings.clone());
        world.economy = Some(Economy { age, ..economy() });
        let mut brain = brain();
        decide(&mut brain, &world, &advanced);
        assert_eq!(stat(&brain, "ages"), Some(1), "{age:?}");
        assert_eq!(stat(&brain, key), Some(100), "{age:?}");
        world.tick = 500;
        decide(&mut brain, &world, &advanced);
        assert_eq!(
            stat(&brain, key),
            Some(100),
            "the first stamp stays ({age:?})"
        );
        assert_eq!(stat(&brain, "ages"), Some(2));
    }
}
