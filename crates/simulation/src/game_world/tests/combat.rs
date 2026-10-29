//! Dark Age combat: damage, time to kill, approach, retaliation and death.
mod castle;
use super::*;
use crate::{BUILDING_COMBAT, DamageKind, EconomyError, UNIT_COMBAT, UnitTask, damage_dealt};

mod acquisition;
mod approach;
mod benchmark;
mod buildings;
mod garrison;
mod hidden_bonuses;
mod military;
mod research_tests;
mod scale;
mod telemetry_tests;
mod tower_budget;

pub(super) fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

pub(super) fn advance(world: &mut GameWorld, ticks: u32) {
    for _ in 0..ticks {
        world.advance();
    }
}

/// Advances until `done` holds and returns the tick count it took.
pub(super) fn ticks_until(
    world: &mut GameWorld,
    limit: u32,
    done: impl Fn(&GameWorld) -> bool,
) -> u32 {
    for tick in 1..=limit {
        world.advance();
        if done(world) {
            return tick;
        }
    }
    panic!("condition not reached within {limit} ticks");
}

/// A player-0 militia and an adjacent player-1 villager.
fn duel() -> (GameWorld, EntityId, EntityId) {
    let mut world = world();
    let militia = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let villager = world.spawn_villager(PlayerId(1), at(101, 100)).unwrap();
    (world, militia, villager)
}

#[test]
fn damage_is_attack_minus_the_matching_armour_with_a_floor_of_one() {
    assert_eq!(damage_dealt(4, DamageKind::Melee, 0, 1), 4);
    assert_eq!(damage_dealt(4, DamageKind::Melee, 3, 5), 1);
    assert_eq!(damage_dealt(3, DamageKind::Melee, 9, 0), 1);
    assert_eq!(damage_dealt(6, DamageKind::Pierce, 0, 2), 4);
    assert_eq!(damage_dealt(0, DamageKind::Melee, 0, 0), 1);
}

#[test]
fn the_combat_table_is_ordered_by_kind_and_holds_the_dark_age_values() {
    for (index, row) in UNIT_COMBAT.iter().enumerate() {
        assert_eq!(row.kind as usize, index);
    }
    for (index, row) in BUILDING_COMBAT.iter().enumerate() {
        assert_eq!(row.kind as usize, index);
    }
    let [villager, scout, militia, fishing_ship, ..] = UNIT_COMBAT;
    assert_eq!(
        (
            fishing_ship.hp,
            fishing_ship.attack,
            fishing_ship.melee_armour,
            fishing_ship.pierce_armour
        ),
        (50, 0, 1, 1)
    );
    assert_eq!((villager.hp, villager.attack), (25, 3));
    assert_eq!((scout.hp, scout.attack, scout.pierce_armour), (45, 3, 2));
    assert_eq!(
        (militia.hp, militia.attack, militia.pierce_armour),
        (40, 4, 1)
    );
    assert_eq!(militia.attack_period_centiseconds, 200);
    let training = militia.training.unwrap();
    assert_eq!(
        (training.cost.food, training.cost.gold, training.seconds),
        (50, 20, 21)
    );
    assert_eq!(crate::BUILDING_RULES[0].max_hp, 2400);
}

#[test]
fn spawned_units_start_at_full_hit_points_without_a_target() {
    let (world, militia, villager) = duel();
    let (m, v) = (world.unit(militia).unwrap(), world.unit(villager).unwrap());
    assert_eq!(
        (m.hp, m.max_hp, m.attack_target, m.last_hit),
        (40, 40, None, None)
    );
    assert_eq!((v.hp, v.max_hp), (25, 25));
}

#[test]
fn military_attack_orders_can_kill_wildlife_and_death_has_authoritative_timing() {
    let mut w = world();
    let militia = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let sheep = w
        .spawn_animal(crate::AnimalKind::Sheep, at(101, 100))
        .unwrap();
    w.issue_attack(PlayerId(0), militia, sheep).unwrap();
    ticks_until(&mut w, 100, |w| w.animal(sheep).unwrap().hp == 0);
    let dead = w.animal(sheep).unwrap();
    assert_eq!(dead.death_tick, Some(w.tick()));
    assert!(!dead.moving);
    let (mut w, militia, victim) = duel();
    w.issue_attack(PlayerId(0), militia, victim).unwrap();
    ticks_until(&mut w, 500, |w| w.unit(victim).unwrap().hp == 0);
    let died = w.tick();
    assert_eq!(w.unit(victim).unwrap().death_tick, Some(died));
    advance(&mut w, 40);
    assert!(
        w.unit(victim).is_some(),
        "the corpse survives the die animation"
    );
    advance(&mut w, crate::CORPSE_SECONDS * 20);
    assert!(w.unit(victim).is_none());
}

#[test]
fn corpses_expire_by_due_tick_when_native_die_durations_differ() {
    let mut w = world();
    let scout = w.spawn_unit(PlayerId(0), at(100, 100)).unwrap();
    let militia = w.spawn_militia(PlayerId(0), at(101, 100)).unwrap();
    w.kill_unit(w.lookup[&scout]);
    w.advance();
    w.kill_unit(w.lookup[&militia]);
    advance(&mut w, 620);
    assert!(w.unit(militia).is_none());
    assert!(w.unit(scout).is_some());
    advance(&mut w, 17);
    assert!(w.unit(scout).is_none());
}

#[test]
fn a_militia_kills_a_villager_in_seven_hits_two_seconds_apart() {
    let (mut world, militia, villager) = duel();
    world.issue_attack(PlayerId(0), militia, villager).unwrap();
    let first = ticks_until(&mut world, 5, |w| w.unit(villager).unwrap().hp < 25);
    assert_eq!(first, 1);
    assert_eq!(world.unit(villager).unwrap().hp, 21);
    advance(&mut world, 39);
    assert_eq!(world.unit(villager).unwrap().hp, 21);
    advance(&mut world, 1);
    assert_eq!(world.unit(villager).unwrap().hp, 17);
    // Hits land on ticks 1, 41, ..., 241: the seventh drops 1 HP to 0.
    advance(&mut world, 199);
    assert_eq!(world.unit(villager).unwrap().hp, 1);
    advance(&mut world, 1);
    let dead = world.unit(villager).unwrap();
    assert_eq!((dead.hp, dead.task), (0, UnitTask::Dying));
    // The villager fought back three damage every two seconds meanwhile.
    assert_eq!(world.unit(militia).unwrap().hp, 40 - 6 * 3);
    assert_eq!(world.unit(militia).unwrap().attack_target, None);
    assert_eq!(world.unit(militia).unwrap().task, UnitTask::Idle);
}

#[test]
fn militia_against_militia_is_decided_by_the_first_hit() {
    let mut world = world();
    let a = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let b = world.spawn_militia(PlayerId(1), at(101, 100)).unwrap();
    world.issue_attack(PlayerId(0), a, b).unwrap();
    // Ten hits of four damage; the aggressor's tenth lands before the
    // defender's tenth, which arrives one tick later.
    let ticks = ticks_until(&mut world, 1_000, |w| w.unit(b).unwrap().hp == 0);
    assert_eq!(ticks, 361);
    assert_eq!(world.unit(a).unwrap().hp, 40 - 9 * 4);
    assert_eq!(world.unit(b).unwrap().task, UnitTask::Dying);
}

#[test]
fn an_attack_order_walks_into_melee_range_then_hits() {
    let mut world = world();
    let militia = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let villager = world.spawn_villager(PlayerId(1), at(110, 100)).unwrap();
    world.issue_attack(PlayerId(0), militia, villager).unwrap();
    let start = world.unit(militia).unwrap();
    assert_eq!(start.attack_target, Some(villager));
    assert!(world.movement_order(militia).is_some());
    advance(&mut world, 20);
    let walked = world.unit(militia).unwrap().position.x - start.position.x;
    // 0.9 tiles per second for one second.
    assert!((870..=990).contains(&walked), "walked {walked} subunits");
    assert_eq!(world.unit(militia).unwrap().task, UnitTask::Attacking);
    let ticks = ticks_until(&mut world, 400, |w| w.unit(villager).unwrap().hp < 25);
    assert!(
        (150..=240).contains(&ticks),
        "first hit after {ticks} ticks"
    );
    let (m, v) = (world.unit(militia).unwrap(), world.unit(villager).unwrap());
    let gap = i64::from(v.position.x - m.position.x);
    assert!((0..=1_024).contains(&gap), "gap {gap}");
    assert!(!m.moving && world.movement_order(militia).is_none());
    assert_eq!(v.last_hit.map(|hit| hit.attacker), Some(militia));
    assert_eq!(v.last_hit.map(|hit| hit.tick), Some(world.tick()));
}

#[test]
fn an_attacker_chases_a_target_that_walks_away_and_stops() {
    let mut world = world();
    let militia = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let villager = world.spawn_villager(PlayerId(1), at(106, 100)).unwrap();
    world.issue_move(villager, at(110, 104)).unwrap();
    world.issue_attack(PlayerId(0), militia, villager).unwrap();
    ticks_until(&mut world, 1_200, |w| w.unit(villager).unwrap().hp < 25);
    assert_eq!(world.unit(militia).unwrap().attack_target, Some(villager));
}

#[test]
fn an_idle_villager_fights_back_but_a_walking_one_does_not() {
    let (mut world, militia, villager) = duel();
    world.issue_attack(PlayerId(0), militia, villager).unwrap();
    advance(&mut world, 1);
    assert_eq!(world.unit(villager).unwrap().attack_target, Some(militia));
    assert_eq!(world.unit(villager).unwrap().task, UnitTask::Attacking);
    advance(&mut world, 2);
    assert_eq!(world.unit(militia).unwrap().hp, 37);
    assert_eq!(
        world
            .unit(militia)
            .unwrap()
            .last_hit
            .map(|hit| hit.attacker),
        Some(villager)
    );

    let (mut world, militia, villager) = duel();
    world.issue_move(villager, at(120, 100)).unwrap();
    world.issue_attack(PlayerId(0), militia, villager).unwrap();
    advance(&mut world, 3);
    assert_eq!(world.unit(villager).unwrap().attack_target, None);
    assert_eq!(world.unit(militia).unwrap().hp, 40);
}

#[test]
fn a_move_order_cancels_an_attack() {
    let (mut world, militia, villager) = duel();
    world.issue_attack(PlayerId(0), militia, villager).unwrap();
    advance(&mut world, 1);
    world.issue_move(militia, at(90, 100)).unwrap();
    assert_eq!(world.unit(militia).unwrap().attack_target, None);
    advance(&mut world, 100);
    assert_eq!(world.unit(villager).unwrap().hp, 21);
}

#[test]
fn units_attack_only_other_players_living_targets() {
    let (mut world, militia, villager) = duel();
    let friend = world.spawn_villager(PlayerId(0), at(99, 100)).unwrap();
    let invalid = |r| matches!(r, Err(EconomyError::InvalidTarget));
    assert!(invalid(world.issue_attack(PlayerId(0), militia, friend)));
    assert!(invalid(world.issue_attack(PlayerId(0), militia, militia)));
    assert!(invalid(world.issue_attack(
        PlayerId(0),
        militia,
        EntityId(999)
    )));
    assert!(matches!(
        world.issue_attack(PlayerId(1), militia, villager),
        Err(EconomyError::NotOwner)
    ));
    assert!(matches!(
        world.issue_attack(PlayerId(0), EntityId(999), villager),
        Err(EconomyError::UnknownEntity)
    ));
    world.issue_attack(PlayerId(0), militia, villager).unwrap();
    advance(&mut world, 241);
    // The body is dying: it cannot be attacked again, nor can it be ordered.
    assert!(invalid(world.issue_attack(PlayerId(0), friend, villager)));
    assert!(world.issue_move(villager, at(120, 100)).is_err());
}

#[test]
fn re_ordering_attacks_never_strikes_faster_than_one_hit_per_period() {
    // Alternating between two adjacent enemies, every tick, for five periods.
    let mut world = world();
    let militia = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let b = world.spawn_militia(PlayerId(1), at(101, 100)).unwrap();
    let c = world.spawn_militia(PlayerId(1), at(99, 100)).unwrap();
    // Isolate the weapon cooldown from the targets' new default aggression.
    world
        .issue_stance(PlayerId(1), &[b, c], aoe_core::CombatStance::NoAttack)
        .unwrap();
    for tick in 0..200 {
        let target = if tick % 2 == 0 { b } else { c };
        world.issue_attack(PlayerId(0), militia, target).unwrap();
        world.advance();
    }
    let lost = |id| 40 - world.unit(id).unwrap().hp;
    assert_eq!(lost(b) + lost(c), 5 * 4);

    // A move to the spot it stands on, then an attack, every tick.
    let (mut world, militia, villager) = duel();
    for _ in 0..200 {
        world.issue_move(militia, at(100, 100)).unwrap();
        world.issue_attack(PlayerId(0), militia, villager).unwrap();
        world.advance();
    }
    assert_eq!(world.unit(villager).unwrap().hp, 25 - 5 * 4);
}

#[test]
fn a_stale_movement_error_does_not_make_the_attacker_give_up_the_chase() {
    let (mut world, militia, villager) = duel();
    let index = world.lookup[&militia];
    world.units[index].last_movement_error = Some(GameWorldError::Unreachable);
    world.issue_attack(PlayerId(0), militia, villager).unwrap();
    advance(&mut world, 1);
    let first = world.unit(villager).unwrap().hp;
    assert_eq!(first, 21);
    world.issue_move(villager, at(108, 100)).unwrap();
    ticks_until(&mut world, 1_200, |w| w.unit(villager).unwrap().hp < first);
    assert_eq!(world.unit(militia).unwrap().attack_target, Some(villager));
}
