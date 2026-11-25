//! `ai/tests/test_management.py`: Stewards, Vassals and Generals are trained,
//! seated and led.
use aoe_sdk::{BuildingKind, GeneralState, Tech, UnitKind};

use super::{Sent, Tape, castle_base, castle_stock, world_of};
use crate::brain::Brain;
use crate::config::Config;
use crate::context::Context;
use crate::play::combat::CombatManager;
use crate::play::management::ManagementManager;
use crate::tests::fake::{FakeWorld, base, building, foreign_building, unit_of};
use crate::tests::play::{eco, play, trains};
use crate::view::{Building, Economy, Unit};

fn rich() -> Economy {
    castle_stock((3000, 900, 3000, 900), 70, 150)
}

fn stewards(config_stewards: u32) -> Config {
    Config {
        stewards: config_stewards,
        steward_min_villagers: 20,
        ..Config::default()
    }
}

fn steward_trains(seat: &crate::tests::fake::Recorder) -> usize {
    trains(seat)
        .into_iter()
        .filter(|(_, kind)| *kind == UnitKind::Steward)
        .count()
}

fn general(army: u16, state: GeneralState, tile: (i32, i32)) -> Unit {
    Unit {
        army_size: Some(army),
        general_state: Some(state),
        ..unit_of(902, UnitKind::General, tile)
    }
}

fn play_new(world: &FakeWorld, config: Config) -> (Brain, crate::tests::fake::Recorder) {
    let mut brain = Brain::new(config);
    let seat = play(world, &mut brain, &[]);
    (brain, seat)
}

#[test]
fn test_a_steward_is_trained_at_the_town_center_and_then_seated() {
    let (units, buildings) = castle_base(40, 24);
    let config = stewards(1);
    let world = world_of(units.clone(), buildings.clone(), rich());
    let (_, seat) = play_new(&world, config.clone());
    assert!(trains(&seat).contains(&(500, UnitKind::Steward)));
    let mut with_steward = units;
    with_steward.push(unit_of(900, UnitKind::Steward, (52, 52)));
    let world = world_of(with_steward, buildings, rich());
    let (_, seat) = play_new(&world, config);
    assert!(seat.orders.garrisons().contains(&(900, 500)));
}

#[test]
fn test_a_seated_steward_is_not_sent_again_and_a_second_is_not_trained() {
    let (units, mut buildings) = castle_base(40, 24);
    buildings[0].garrison = vec![(900, UnitKind::Steward, 50, 50)];
    let (_, seat) = play_new(&world_of(units, buildings, rich()), stewards(1));
    assert!(!trains(&seat).contains(&(500, UnitKind::Steward)));
    assert!(seat.orders.garrisons().is_empty());
}

#[test]
fn test_a_castle_is_raised_with_stone_and_hosts_a_vassal_then_a_general() {
    let (units, mut buildings) = castle_base(40, 24);
    let enemies = vec![foreign_building(800, BuildingKind::Barracks, (130, 80))];
    let config = Config {
        castles: 1,
        vassals: 1,
        generals: 1,
        ..Config::default()
    };
    let decide = |buildings: &[Building]| {
        let mut world = world_of(units.clone(), buildings.to_vec(), rich());
        world.enemy_buildings = enemies.clone();
        play_new(&world, config.clone()).1
    };
    let seat = decide(&buildings);
    assert!(
        seat.orders
            .builds()
            .iter()
            .any(|(kind, _)| *kind == BuildingKind::Castle)
    );
    buildings.push(building(700, BuildingKind::Castle, (80, 60)));
    assert!(
        decide(&buildings)
            .orders
            .trains()
            .contains(&(700, UnitKind::Vassal))
    );
    let last = buildings.len() - 1;
    buildings[last].garrison = vec![(901, UnitKind::Vassal, 80, 60)];
    assert!(
        decide(&buildings)
            .orders
            .trains()
            .contains(&(700, UnitKind::General))
    );
}

#[test]
fn test_a_new_general_gets_train_and_mix_and_marches_a_big_army_at_the_enemy() {
    let (mut units, buildings) = castle_base(40, 24);
    units.push(general(40, GeneralState::Idle, (60, 60)));
    let mut world = world_of(units, buildings, rich());
    world.enemy_buildings = vec![foreign_building(800, BuildingKind::House, (200, 200))];
    let config = Config {
        generals: 1,
        general_attack_army: 30,
        ..Config::default()
    };
    let (_, seat) = play_new(&world, config);
    assert!(seat.orders.general_trains().contains(&(902, true)));
    assert_eq!(seat.orders.general_mixes().first(), Some(&902));
    assert!(seat.orders.moves().iter().all(|(unit, _)| *unit != 902));
    let ordered: Vec<_> = seat
        .orders
        .formation_moves()
        .into_iter()
        .filter(|(members, ..)| members == &[902])
        .collect();
    assert_eq!(ordered.len(), 1, "{:?}", seat.orders);
    assert!(ordered[0].2, "the General marches by attack move");
}

#[test]
fn test_soldiers_a_general_leads_get_no_orders_from_the_ai() {
    let (mut units, buildings) = castle_base(40, 0);
    let led: Vec<Unit> = (0..12)
        .map(|i| Unit {
            commanded_by: Some(902),
            ..unit_of(950 + i, UnitKind::Spearman, (70, 70))
        })
        .collect();
    units.extend(led.clone());
    let (_, seat) = play_new(&world_of(units, buildings, rich()), Config::default());
    let mut moved: Vec<u32> = seat.orders.moves().iter().map(|m| m.0).collect();
    moved.extend(
        seat.orders
            .formation_moves()
            .into_iter()
            .flat_map(|(members, ..)| members),
    );
    assert!(led.iter().all(|soldier| !moved.contains(&soldier.id)));
}

#[test]
fn test_saving_for_a_steward_stops_soldiers_but_gives_up_after_the_limit() {
    let (units, buildings) = castle_base(40, 3);
    let config = Config {
        save_seconds: 60,
        ..stewards(1)
    };
    let short = castle_stock((900, 900, 900, 200), 50, 150);
    let mut world = world_of(units, buildings, short);
    let (mut brain, seat) = play_new(&world, config);
    assert!(
        trains(&seat)
            .iter()
            .all(|(_, kind)| *kind == UnitKind::Villager),
        "soldiers wait while saving"
    );
    world.tick = 100 + 70 * 20;
    let seat = play(&world, &mut brain, &[]);
    assert!(
        trains(&seat)
            .iter()
            .any(|(_, kind)| *kind != UnitKind::Villager),
        "the army spends again after the limit"
    );
}

#[test]
fn test_a_town_center_busy_with_villagers_is_drained_so_the_steward_can_be_bought() {
    let (units, mut buildings) = castle_base(40, 3);
    buildings[0].queue_length = 2;
    let eco = castle_stock((900, 900, 1200, 200), 50, 150);
    let (brain, seat) = play_new(&world_of(units, buildings, eco), stewards(1));
    assert!(
        brain
            .board
            .stats
            .get("saving_for_management")
            .is_some_and(|n| *n > 0),
        "the saving starts although the queue is busy"
    );
    assert!(
        trains(&seat).is_empty(),
        "no soldiers and no new villagers while it drains"
    );
}

#[test]
fn test_a_steward_in_production_still_counts_after_the_ten_second_cooldown() {
    let (units, mut buildings) = castle_base(40, 24);
    buildings.push(building(501, BuildingKind::TownCenter, (90, 50)));
    let mut world = world_of(units, buildings, rich());
    let (mut brain, seat) = play_new(&world, stewards(1));
    assert_eq!(steward_trains(&seat), 1);
    for seconds in [15, 30, 50] {
        // past the 10 s cooldown, before the 60 s training ends
        world.tick = 100 + seconds * 20;
        let seat = play(&world, &mut brain, &[]);
        assert_eq!(steward_trains(&seat), 0, "{seconds}");
    }
}

#[test]
fn test_one_finished_steward_clears_only_its_own_order() {
    let (units, mut buildings) = castle_base(40, 24);
    buildings.extend([
        building(501, BuildingKind::TownCenter, (90, 50)),
        building(502, BuildingKind::TownCenter, (50, 90)),
    ]);
    let mut world = world_of(units, buildings, rich());
    let (mut brain, seat) = play_new(&world, stewards(2));
    let mut ordered = steward_trains(&seat);
    for seconds in [12, 24] {
        world.tick = 100 + seconds * 20;
        ordered += steward_trains(&play(&world, &mut brain, &[]));
    }
    assert_eq!(ordered, 2);
    world.units.push(unit_of(903, UnitKind::Steward, (62, 62))); // the first one is out
    for seconds in [40, 55] {
        world.tick = 100 + seconds * 20;
        let seat = play(&world, &mut brain, &[]);
        assert_eq!(steward_trains(&seat), 0, "{seconds}");
    }
}

#[test]
fn test_a_general_who_lost_soldiers_after_a_full_army_gets_train_again() {
    let (mut units, buildings) = castle_base(40, 24);
    units.push(general(100, GeneralState::Idle, (60, 60)));
    let config = Config {
        generals: 1,
        ..Config::default()
    };
    let mut world = world_of(units, buildings, rich());
    let (mut brain, seat) = play_new(&world, config);
    assert!(!seat.orders.general_trains().is_empty());
    world.units.last_mut().expect("the General").army_size = Some(60);
    world.tick += 40 * 20;
    let seat = play(&world, &mut brain, &[]);
    assert!(seat.orders.general_trains().contains(&(902, true)));
}

#[test]
fn test_a_researching_first_town_center_does_not_stop_villagers_at_the_others() {
    let (units, mut buildings) = base(12);
    buildings[0].researching = Some(Tech::Loom);
    buildings.push(building(501, BuildingKind::TownCenter, (90, 50)));
    let world = world_of(units, buildings, eco(500, 200, 100, 14, 50));
    let (_, seat) = play_new(&world, Config::default());
    assert!(trains(&seat).contains(&(501, UnitKind::Villager)));
    assert!(!trains(&seat).contains(&(500, UnitKind::Villager)));
}

#[test]
fn test_a_general_recalls_by_attack_move_after_their_soldiers_finish_fighting() {
    let (mut units, buildings) = castle_base(40, 0);
    units.push(general(40, GeneralState::Marching, (120, 120)));
    units.push(Unit {
        commanded_by: Some(902),
        attack_target: Some(904),
        ..unit_of(903, UnitKind::Spearman, (120, 119))
    });
    let world = world_of(units, buildings, rich());
    let (mut brain, _) = play_new(&world, Config::default());
    brain.board.raid_tile = Some((53, 52));
    brain.board.raid_until = world.tick as i64 + 100;
    let mut seat = Tape::default();
    let sent = seat.clone();
    let mut ctx = Context::new(&world, &[], &mut brain.board, &brain.config, &mut seat);
    let combat = CombatManager::default();

    ManagementManager::default()
        .lead(&mut ctx, &combat)
        .expect("the order budget holds");
    assert!(sent.orders().is_empty());
    for soldier in ctx.led.iter_mut() {
        soldier.attack_target = None;
    }
    ManagementManager::default()
        .lead(&mut ctx, &combat)
        .expect("the order budget holds");

    let moves = sent.orders().formation_moves();
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert_eq!(moves[0].0, vec![902]);
    assert!(moves[0].2, "the recall is an attack move");
}
