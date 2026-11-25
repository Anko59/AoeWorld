//! `ai/tests/test_predator_defence.py`: predators are threats to workers,
//! including before an army exists.
use std::collections::BTreeSet;

use aoe_sdk::{AnimalKind, AnimalTask, UnitTask};

use super::step;
use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::play::combat::CombatManager;
use crate::play::scouting::ScoutManager;
use crate::tests::fake::{FakeWorld, animal, base, with_context};

fn workers_fight_a_predator_together(kind: AnimalKind) {
    let (mut units, buildings) = base(4);
    let scout = units.len() - 1;
    units[scout].x = 200;
    units[scout].y = 200;
    for worker in &mut units[..scout] {
        worker.task = UnitTask::Gathering;
        worker.idle = false;
    }
    let mut wolf = animal(900, (50, 53), kind, 0);
    wolf.task = AnimalTask::Attacking;
    let mut world = FakeWorld::new(units, buildings);
    world.animals = vec![wolf];
    let seat = step(&world, &[], &mut Blackboard::default(), &Config::default());

    let attacks = seat.attacks();
    assert!(attacks.len() >= 3);
    assert_eq!(
        attacks.iter().map(|a| a.1).collect::<BTreeSet<u32>>(),
        BTreeSet::from([900])
    );
}

#[test]
fn test_workers_fight_a_predator_together_when_the_scout_is_away_wolf() {
    workers_fight_a_predator_together(AnimalKind::Wolf);
}

#[test]
fn test_workers_fight_a_predator_together_when_the_scout_is_away_boar() {
    workers_fight_a_predator_together(AnimalKind::Boar);
}

#[test]
fn test_a_nearby_scout_protects_workers_without_a_scout_move_overwriting_it() {
    let (units, buildings) = base(3);
    let scout = units[units.len() - 1].id;
    let mut world = FakeWorld::new(units, buildings);
    world.animals = vec![animal(900, (50, 53), AnimalKind::Wolf, 0)];
    let mut board = Blackboard::default();
    let (done, seat) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        CombatManager::default().step(ctx)?;
        ScoutManager::default().step(ctx)
    });
    done.expect("the order budget holds");
    assert_eq!(seat.attacks(), vec![(scout, 900)]);
    assert!(seat.moves().is_empty());
}

#[test]
fn test_an_unprovoked_boar_is_left_alone() {
    let (units, buildings) = base(3);
    let mut world = FakeWorld::new(units, buildings);
    world.animals = vec![animal(900, (50, 53), AnimalKind::Boar, 100)];
    let seat = step(&world, &[], &mut Blackboard::default(), &Config::default());
    assert!(seat.orders.is_empty());
}

#[test]
fn test_workers_already_defending_are_reserved_without_repeating_their_orders() {
    let (mut units, buildings) = base(4);
    let scout = units.len() - 1;
    units[scout].x = 200;
    units[scout].y = 200;
    for worker in &mut units[..scout] {
        worker.task = UnitTask::Attacking;
        worker.idle = false;
        worker.attack_target = Some(900);
    }
    let workers: BTreeSet<u32> = units[..scout].iter().map(|u| u.id).collect();
    let mut world = FakeWorld::new(units, buildings);
    world.animals = vec![animal(900, (50, 53), AnimalKind::Wolf, 0)];
    let mut board = Blackboard::default();
    let (busy, seat) = with_context(&world, &[], &mut board, &Config::default(), |ctx| {
        CombatManager::default().step(ctx).expect("within budget");
        ctx.busy.clone()
    });
    assert!(workers.is_subset(&busy));
    assert!(seat.orders.is_empty());
}
