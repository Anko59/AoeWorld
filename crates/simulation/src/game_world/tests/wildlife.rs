use super::*;
use crate::{AnimalKind, AnimalTask, EconomyError, UnitTask, WILDLIFE_RULES};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}
fn run(w: &mut GameWorld, ticks: u32) {
    for _ in 0..ticks {
        w.advance();
    }
}

#[test]
fn wildlife_table_has_de_values() {
    assert_eq!(
        WILDLIFE_RULES.map(|r| (r.hp, r.food, r.attack)),
        [(7, 100, 0), (5, 140, 0), (75, 340, 7), (25, 0, 3)]
    );
    assert_eq!(WILDLIFE_RULES[1].speed_hundredths, 100);
    assert_eq!(WILDLIFE_RULES[1].gather_hundredths, 41);
    assert_eq!(WILDLIFE_RULES[0].gather_hundredths, 33);
}

#[test]
fn sheep_converts_to_nearest_living_player_with_id_tiebreak() {
    let mut w = world();
    w.spawn_villager(PlayerId(1), at(102, 100)).unwrap();
    w.spawn_unit(PlayerId(0), at(100, 101)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(100, 100)).unwrap();
    w.advance();
    assert_eq!(w.animal(sheep).unwrap().owner, Some(PlayerId(0)));
    w.spawn_villager(PlayerId(1), at(100, 100)).unwrap();
    w.advance();
    assert_eq!(
        w.animal(sheep).unwrap().owner,
        Some(PlayerId(0)),
        "owned sheep stay owned"
    );
}

#[test]
fn only_the_owner_can_herd_a_living_sheep_and_its_walk_stops_at_destination() {
    let mut w = world();
    w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
    w.advance();
    assert!(w.issue_herd(PlayerId(1), sheep, at(102, 100)).is_err());
    w.issue_herd(PlayerId(0), sheep, at(103, 100)).unwrap();
    w.advance();
    assert!(w.animal(sheep).unwrap().moving);
    run(&mut w, 100);
    assert_eq!(w.animal(sheep).unwrap().position, at(103, 100));
    assert!(!w.animal(sheep).unwrap().moving);
}

#[test]
fn shepherd_slaughters_sheep_and_delivers_all_100_food() {
    let mut w = world();
    w.place_town_center(PlayerId(0), TileCoord::new(95, 95))
        .unwrap();
    let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
    w.advance();
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    run(&mut w, 20 * 450);
    assert!(
        w.animal(sheep).is_none(),
        "the harvested carcass disappears after its decay time"
    );
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile.food, 100);
    assert_eq!(w.unit(worker).unwrap().carried, None);
}

#[test]
fn deer_flees_nearby_units_faster_than_a_villager() {
    let mut w = world();
    w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let deer = w.spawn_animal(AnimalKind::Deer, at(102, 100)).unwrap();
    run(&mut w, 20);
    let a = w.animal(deer).unwrap();
    assert_eq!(a.task, AnimalTask::Fleeing);
    assert_eq!(a.position.x - at(102, 100).x, FIXED_SUBUNITS_PER_TILE);
}

#[test]
fn hunters_and_shepherds_keep_their_profession_while_walking_to_distant_animals() {
    for (kind, task) in [
        (AnimalKind::Deer, UnitTask::Hunting),
        (AnimalKind::Sheep, UnitTask::Shepherding),
    ] {
        let mut w = world();
        let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
        let animal = w.spawn_animal(kind, at(108, 100)).unwrap();
        w.issue_hunt(PlayerId(0), &[worker], animal).unwrap();
        run(&mut w, 2);
        let hunter = w.unit(worker).unwrap();
        assert!(hunter.moving);
        assert_eq!(hunter.task, task);
    }
}

#[test]
fn a_bow_hunter_can_kill_a_deer_in_open_ground() {
    let mut w = world();
    let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let deer = w.spawn_animal(AnimalKind::Deer, at(108, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[worker], deer).unwrap();
    run(&mut w, 20 * 30);
    assert_eq!(w.animal(deer).unwrap().hp, 0);
    assert!(w.unit(worker).unwrap().position.x < at(120, 100).x);
}

#[test]
fn boar_fights_back_and_kills_a_lone_hunter() {
    let mut w = world();
    let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let boar = w.spawn_animal(AnimalKind::Boar, at(101, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[worker], boar).unwrap();
    run(&mut w, 20 * 20);
    assert!(w.unit(worker).is_none_or(|u| u.hp == 0));
    assert!(w.animal(boar).unwrap().hp > 0);
}

#[test]
fn boar_targets_last_hitter_and_stops_a_long_lure() {
    let mut w = world();
    let first = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let second = w.spawn_villager(PlayerId(0), at(102, 100)).unwrap();
    let boar = w.spawn_animal(AnimalKind::Boar, at(101, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[first], boar).unwrap();
    w.advance();
    assert_eq!(w.animal(boar).unwrap().target, Some(first));
    w.issue_move(first, at(90, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[second], boar).unwrap();
    w.advance();
    assert_eq!(w.animal(boar).unwrap().target, Some(second));
    w.issue_move(second, at(130, 100)).unwrap();
    run(&mut w, 20 * 60);
    assert_eq!(w.animal(boar).unwrap().target, None);
}

#[test]
fn unattended_carcass_decays_without_creating_carried_food() {
    let mut w = world();
    let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    for _ in 0..200 {
        w.advance();
        if w.animal(sheep).unwrap().hp == 0 {
            break;
        }
    }
    assert_eq!(w.animal(sheep).unwrap().hp, 0);
    w.issue_move(worker, at(110, 100)).unwrap();
    let food = w.animal(sheep).unwrap().food_remaining;
    run(&mut w, 20 * 30);
    assert!(w.animal(sheep).unwrap().food_remaining < food);
    assert_eq!(w.unit(worker).unwrap().carried, None);
}

#[test]
fn wolf_attacks_a_villager_in_los_but_not_a_scout_or_distant_villager() {
    let mut w = world();
    w.spawn_unit(PlayerId(0), at(100, 100)).unwrap();
    let worker = w.spawn_villager(PlayerId(0), at(104, 100)).unwrap();
    let far = w.spawn_villager(PlayerId(0), at(150, 100)).unwrap();
    let wolf = w.spawn_animal(AnimalKind::Wolf, at(101, 100)).unwrap();
    run(&mut w, 20 * 5);
    assert_eq!(w.animal(wolf).unwrap().target, Some(worker));
    assert!(w.unit(worker).unwrap().hp < 25);
    assert_eq!(w.unit(far).unwrap().hp, 25);
}

#[test]
fn hunt_validates_entire_group_and_move_cancels_it() {
    let mut w = world();
    let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let enemy = w.spawn_villager(PlayerId(1), at(100, 101)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
    assert_eq!(
        w.issue_hunt(PlayerId(0), &[worker, enemy], sheep),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(w.unit(worker).unwrap().task, UnitTask::Idle);
    assert!(w.issue_hunt(PlayerId(0), &[], sheep).is_err());
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    w.issue_move(worker, at(95, 100)).unwrap();
    run(&mut w, 100);
    assert_eq!(w.animal(sheep).unwrap().hp, 7);
}

#[test]
fn wildlife_replay_matches_every_tick_and_hash_includes_animal_state() {
    fn setup() -> GameWorld {
        let mut w = world();
        let v = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
        let b = w.spawn_animal(AnimalKind::Boar, at(101, 100)).unwrap();
        w.spawn_animal(AnimalKind::Deer, at(102, 101)).unwrap();
        w.issue_hunt(PlayerId(0), &[v], b).unwrap();
        w
    }
    let (mut a, mut b) = (setup(), setup());
    for _ in 0..500 {
        a.advance();
        b.advance();
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }
    b.spawn_animal(AnimalKind::Sheep, at(200, 200)).unwrap();
    assert_ne!(a.canonical_hash(), b.canonical_hash());
}

#[test]
fn distant_sheep_do_not_increase_tick_work() {
    fn setup(count: usize) -> GameWorld {
        let mut w = world();
        w.spawn_villager(PlayerId(0), at(10, 10)).unwrap();
        w.spawn_animal(AnimalKind::Deer, at(12, 10)).unwrap();
        for n in 0..count {
            w.spawn_animal(
                AnimalKind::Sheep,
                at(150 + (n % 50) as i32, 150 + (n / 50) as i32),
            )
            .unwrap();
        }
        w
    }
    let (mut small, mut large) = (setup(10), setup(1000));
    for _ in 0..40 {
        small.advance();
        large.advance();
        assert_eq!(small.wildlife_stats(), large.wildlife_stats());
        assert!(small.wildlife_stats().animal_steps <= 1);
    }
}

#[test]
fn village_spawns_a_deterministic_local_wildlife_kit() {
    let mut w = world();
    w.initialize_village(PlayerId(0), TileCoord::new(100, 100))
        .unwrap();
    let counts = [AnimalKind::Sheep, AnimalKind::Boar, AnimalKind::Deer]
        .map(|kind| w.animals().filter(|a| a.kind == kind).count());
    assert_eq!(counts, [8, 2, 4]);
    assert!(
        w.animals()
            .all(|a| w.tile_passable(a.position.tile_floor()))
    );
}

#[test]
fn hunting_rejects_scouts_dead_workers_duplicates_and_owned_enemy_sheep() {
    let mut w = world();
    let scout = w.spawn_unit(PlayerId(0), at(100, 100)).unwrap();
    let enemy = w.spawn_villager(PlayerId(1), at(101, 100)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(102, 100)).unwrap();
    w.advance();
    assert_eq!(w.animal(sheep).unwrap().owner, Some(PlayerId(1)));
    assert_eq!(
        w.issue_hunt(PlayerId(0), &[scout], sheep),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        w.issue_hunt(PlayerId(1), &[enemy, enemy], sheep),
        Err(EconomyError::Capacity)
    );
    let wolf = w.spawn_animal(AnimalKind::Wolf, at(104, 100)).unwrap();
    assert_eq!(
        w.issue_hunt(PlayerId(0), &[scout], wolf),
        Err(EconomyError::WrongUnitKind)
    );
    let index = w.lookup[&enemy];
    w.kill_unit(index);
    assert_eq!(
        w.issue_hunt(PlayerId(1), &[enemy], wolf),
        Err(EconomyError::UnknownEntity)
    );
}

#[test]
fn repeat_hunt_orders_do_not_accelerate_slaughter() {
    fn setup() -> (GameWorld, EntityId, EntityId) {
        let mut w = world();
        let v = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
        let a = w.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
        w.issue_hunt(PlayerId(0), &[v], a).unwrap();
        (w, v, a)
    }
    let (mut a, v, sheep) = setup();
    let (mut b, _, _) = setup();
    for _ in 0..80 {
        a.issue_hunt(PlayerId(0), &[v], sheep).unwrap();
        a.advance();
        b.advance();
        assert_eq!(a.animal(sheep).unwrap().hp, b.animal(sheep).unwrap().hp);
    }
}

#[test]
fn unattended_carcass_decays_even_after_all_units_are_removed() {
    let mut w = world();
    let v = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[v], sheep).unwrap();
    while w.animal(sheep).unwrap().hp > 0 {
        w.advance();
    }
    w.remove_unit(v);
    run(&mut w, 20 * 500);
    assert!(w.animal(sheep).is_none());
}

#[test]
fn carcass_rate_uses_hunting_and_shepherding_values() {
    for (kind, rate) in [(AnimalKind::Sheep, 33), (AnimalKind::Deer, 41)] {
        let mut w = world();
        let v = w.spawn_villager(PlayerId(0), at(254, 100)).unwrap();
        let a = w.spawn_animal(kind, at(255, 100)).unwrap();
        w.issue_hunt(PlayerId(0), &[v], a).unwrap();
        for _ in 0..400 {
            w.advance();
            if w.unit(v).unwrap().task
                == if kind == AnimalKind::Sheep {
                    UnitTask::Shepherding
                } else {
                    UnitTask::Hunting
                }
                && w.gathering_kind(v).is_some()
            {
                break;
            }
        }
        assert_eq!(w.animal(a).unwrap().hp, 0, "cornered animal is huntable");
        // Measure from a fresh gather job: no route or prior fractional credit.
        w.issue_move(v, at(254, 100)).unwrap();
        w.issue_hunt(PlayerId(0), &[v], a).unwrap();
        run(&mut w, 20 * 20);
        assert_eq!(w.unit(v).unwrap().carried.unwrap().amount, rate / 5);
    }
}

#[test]
fn animal_targets_and_fleeing_clear_when_the_last_unit_disappears() {
    let mut w = world();
    let v = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let b = w.spawn_animal(AnimalKind::Boar, at(101, 100)).unwrap();
    let d = w.spawn_animal(AnimalKind::Deer, at(102, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[v], b).unwrap();
    w.advance();
    assert_eq!(w.animal(b).unwrap().target, Some(v));
    assert_eq!(w.animal(d).unwrap().task, AnimalTask::Fleeing);
    w.remove_unit(v);
    w.advance();
    assert_eq!(w.animal(b).unwrap().target, None);
    assert_eq!(w.animal(d).unwrap().task, AnimalTask::Idle);
    w.advance();
    assert_eq!(w.wildlife_stats().animal_steps, 0);
}

#[test]
fn switching_between_hunting_and_combat_keeps_the_units_hit_timer() {
    let mut w = world();
    let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let enemy = w.spawn_villager(PlayerId(1), at(100, 101)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(101, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    w.advance();
    assert_eq!(w.animal(sheep).unwrap().hp, 4);
    w.issue_attack(PlayerId(0), worker, enemy).unwrap();
    w.advance();
    assert_eq!(w.unit(enemy).unwrap().hp, 25);
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    w.advance();
    assert_eq!(w.animal(sheep).unwrap().hp, 4);
    run(&mut w, 38);
    assert_eq!(w.animal(sheep).unwrap().hp, 1);
}

#[test]
fn a_new_hunt_ignores_an_earlier_failed_route() {
    let mut w = world();
    let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(103, 100)).unwrap();
    w.advance();
    let index = w.lookup[&worker];
    w.units[index].last_movement_error = Some(GameWorldError::Unreachable);
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    run(&mut w, 40);
    assert_ne!(w.unit(worker).unwrap().task, UnitTask::Idle);
    let near = w.unit(worker).unwrap().position.tile_floor();
    assert!(near.x > 100, "the hunter walks toward the sheep: {near:?}");
}
