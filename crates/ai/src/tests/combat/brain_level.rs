//! The combat cases Python plays through `Brain.decide`: `test_logic.py` and
//! `test_review.py`. Each one decides with a whole `Brain`, as `play` does there.
use aoe_sdk::{Age, BuildingKind, CommandResult, Order, UnitKind};

use super::{army_moves, hidden, house, ids, populated, spearmen};
use crate::brain::Brain;
use crate::config::Config;
use crate::geometry::dist;
use crate::tests::fake::{
    FakeWorld, Recorder, base, enemy, foreign_building, order_result, soldier,
};
use crate::tests::play::{play, play_fresh};
use crate::view::{Building, Economy, Tile, Unit};

/// 14 spearmen at (60, 60), the army of most cases.
fn muster() -> Vec<Unit> {
    spearmen(300, 14, (60, 60))
}

fn hop_config() -> Config {
    Config {
        march_hop_tiles: 10_000,
        ..Config::default()
    }
}

/// Destinations ordered to soldiers 300 and above.
fn army_dests(seat: &Recorder) -> Vec<Tile> {
    army_moves(seat)
        .into_iter()
        .filter(|m| m.0 >= 300)
        .map(|m| m.1)
        .collect()
}

fn attacks_by_army(seat: &Recorder) -> Vec<(u32, u32)> {
    seat.attacks().into_iter().filter(|a| a.0 >= 300).collect()
}

/// Base(10) with `army` and the enemy `buildings`, at population 25 of 40.
fn assault(army: Vec<Unit>, buildings: Vec<Building>) -> FakeWorld {
    let (mut units, ours) = base(10);
    units.extend(army);
    let mut world = populated(units, ours, 25, 40);
    world.enemy_buildings = buildings;
    world
}

#[test]
fn test_a_strong_army_attacks_the_nearest_known_enemy_building() {
    let far = hidden(house(900, (250, 250)));
    let mut near = hidden(foreign_building(901, BuildingKind::Barracks, (150, 120)));
    let world = assault(muster(), vec![far.clone(), near.clone()]);
    let mut brain = Brain::new(hop_config());
    let seat = play(&world, &mut brain, &[]);
    assert!(brain.board.attacking && brain.board.attack_target == Some(901));
    let moves = army_dests(&seat);
    assert_eq!(moves.len(), 14);
    assert!(moves.iter().all(|m| dist(*m, near.center()) <= 6.0));
    // Once the target is in sight the army hits it instead of walking.
    near.visible = true;
    let (mut units, buildings) = base(10);
    units.extend(muster());
    let mut world = FakeWorld::new(units, buildings);
    world.enemy_buildings = vec![far, near];
    let seat = play(&world, &mut brain, &[]);
    assert_eq!(seat.attacks().iter().filter(|a| a.1 == 901).count(), 14);
}

#[test]
fn test_a_small_army_keeps_massing_and_a_failing_attack_retreats() {
    let (mut units, buildings) = base(10);
    units.extend(spearmen(300, 5, (60, 60)));
    let world = FakeWorld::new(units, buildings);
    let (mut brain, _) = play_fresh(&world);
    assert!(!brain.board.attacking);
    brain.board.attacking = true;
    brain.board.attack_launch_size = 14;
    play(&world, &mut brain, &[]);
    assert!(!brain.board.attacking);
    assert_eq!(brain.board.stats.get("retreats"), Some(&1));
}

#[test]
fn test_enemies_at_the_door_pull_the_army_into_defence() {
    let (mut units, buildings) = base(10);
    units.extend(spearmen(300, 4, (56, 56)));
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = vec![enemy(800, UnitKind::Spearman, (60, 56))];
    let (_, seat) = play_fresh(&world);
    assert!(seat.attacks().iter().all(|a| a.1 == 800));
    assert_eq!(seat.attacks().len(), 4);
}

#[test]
fn test_the_army_walks_to_a_tile_beside_the_enemy_building_not_onto_it() {
    let target = hidden(foreign_building(901, BuildingKind::Barracks, (150, 120)));
    let world = assault(spearmen(300, 12, (60, 60)), vec![target.clone()]);
    let seat = play(&world, &mut Brain::new(hop_config()), &[]);
    let moves = army_dests(&seat);
    assert!(!moves.is_empty());
    for m in &moves {
        assert!([2.0, 3.0, 4.0, 5.0].contains(&dist(*m, target.center())));
        assert_ne!(*m, target.center());
    }
}

#[test]
fn test_a_refused_destination_is_not_chosen_again_for_the_rally_point() {
    let (mut units, buildings) = base(20);
    units.push(soldier(300, (50, 50)));
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(Economy {
        population: 22,
        population_cap: 40,
        age: Age::Feudal,
        ..crate::tests::fake::economy()
    });
    let (mut brain, seat) = play_fresh(&world);
    let first = army_moves(&seat)
        .into_iter()
        .find(|m| m.0 == 300)
        .expect("rallied");
    let sequence = rally_sequence(&seat, 300);
    let refusal = order_result(sequence, CommandResult::RejectedInvalidDestination);
    let seat = play(&world, &mut brain, &[refusal]);
    let second = army_moves(&seat)
        .into_iter()
        .find(|m| m.0 == 300)
        .expect("rallied");
    assert_ne!(first.1, second.1);
}

/// The 1-based sequence of the first formation move that includes `unit`.
fn rally_sequence(seat: &Recorder, unit: u32) -> u64 {
    seat.orders
        .iter()
        .position(|o| matches!(o, Order::FormationMove { units, .. } if units.iter().any(|u| u.0 == unit)))
        .map(|i| i as u64 + 1)
        .expect("a formation move for the unit")
}

#[test]
fn test_an_army_marches_to_a_far_target_in_short_hops() {
    let far = hidden(house(900, (250, 250)));
    let world = assault(muster(), vec![far.clone()]);
    let (_, seat) = play_fresh(&world);
    let moves = army_dests(&seat);
    assert_eq!(moves.len(), 14);
    assert!(moves.iter().all(|m| *m == moves[0]), "{moves:?}");
    let stop = moves[0];
    assert!((dist(stop, (60, 60)) - 48.0).abs() <= 2.0);
    assert!(dist(stop, far.center()) < dist((60, 60), far.center()));
}

#[test]
fn test_an_army_that_stalls_retries_its_target_after_a_cooldown() {
    let far = hidden(house(900, (250, 250)));
    let mut world = assault(muster(), vec![far.clone()]);
    let mut brain = Brain::new(Config::default());
    let seat = play(&world, &mut brain, &[]);
    assert_eq!(army_dests(&seat).len(), 14);
    // Idle again a second later (stopped short): not re-sent at once.
    world.tick = 120;
    assert!(army_dests(&play(&world, &mut brain, &[])).is_empty());
    // A stalled approach waits before assigning the still-known target again.
    let stall = 100 + 61 * 20;
    world.tick = stall;
    play(&world, &mut brain, &[]);
    assert!(brain.board.cooling(&format!("target:{}", far.id), stall));
    let retry = stall + brain.config.march_stall_seconds * 20 + 1;
    world.tick = retry;
    let seat = play(&world, &mut brain, &[]);
    assert_eq!(brain.board.attack_target, Some(far.id));
    assert!(!army_dests(&seat).is_empty());
}

#[test]
fn test_army_chooses_a_building_on_its_own_land_component() {
    let (mut units, buildings) = base(12);
    units.extend(spearmen(300, 14, (55, 55)));
    let mut world = populated(units, buildings, 27, 40);
    world.enemy_buildings = vec![house(900, (70, 55)), house(901, (55, 90))];
    world.route_fn = Some(Box::new(|from, to| (to.0 < 65).then(|| vec![from, to])));
    let (_, seat) = play_fresh(&world);
    let attacks = attacks_by_army(&seat);
    assert!(!attacks.is_empty() && attacks.iter().all(|a| a.1 == 901));
}

#[test]
fn test_visible_enemies_across_a_river_do_not_pin_the_army() {
    let (mut units, buildings) = base(12);
    units.extend(spearmen(300, 14, (55, 55)));
    let mut world = populated(units, buildings, 27, 40);
    world.enemies = vec![enemy(900, UnitKind::Spearman, (58, 55))];
    world.route_fn = Some(Box::new(|from, to| (to.0 < 58).then(|| vec![from, to])));
    let (_, seat) = play_fresh(&world);
    assert!(seat.attacks().is_empty());
}

#[test]
fn test_each_straggler_marches_in_a_short_land_hop() {
    let army: Vec<Unit> = (0..14)
        .map(|i| soldier(300 + i, (if i == 0 { 5 } else { 60 }, 60)))
        .collect();
    let world = {
        let mut world = assault(army.clone(), vec![hidden(house(900, (250, 250)))]);
        world.economy = world.economy.map(|e| Economy {
            population: 27,
            ..e
        });
        world
    };
    let (_, seat) = play_fresh(&world);
    let moves: Vec<(u32, Tile)> = army_moves(&seat)
        .into_iter()
        .filter(|m| m.0 >= 300)
        .collect();
    assert!(!moves.is_empty());
    for (id, dest) in moves {
        let origin = army.iter().find(|u| u.id == id).expect("a soldier").tile();
        assert!(dist(origin, dest) <= 48.0);
    }
}

#[test]
fn test_a_visible_distant_building_still_uses_short_march_hops() {
    let world = assault(muster(), vec![house(900, (250, 250))]);
    let (_, seat) = play_fresh(&world);
    assert!(attacks_by_army(&seat).is_empty());
    let moves = army_dests(&seat);
    assert!(!moves.is_empty() && moves.iter().all(|m| dist((60, 60), *m) <= 48.0));
}

#[test]
fn test_defence_does_not_order_soldiers_across_a_river_to_home() {
    let (mut units, buildings) = base(12);
    units.extend(spearmen(300, 14, (80, 55)));
    let mut world = populated(units, buildings, 27, 40);
    world.enemies = vec![enemy(900, UnitKind::Spearman, (55, 55))];
    world.route_fn = Some(Box::new(|from, to| {
        ((from.0 < 65) == (to.0 < 65)).then(|| vec![from, to])
    }));
    let (_, seat) = play_fresh(&world);
    assert!(seat.attacks().is_empty());
}

#[test]
fn test_far_defenders_use_short_hops_and_keep_their_pause() {
    let (mut units, buildings) = base(12);
    units.extend(spearmen(300, 14, (200, 55)));
    let mut world = populated(units, buildings, 27, 40);
    world.enemies = vec![enemy(900, UnitKind::Spearman, (55, 55))];
    let mut brain = Brain::new(Config::default());
    let seat = play(&world, &mut brain, &[]);
    assert_eq!(army_dests(&seat).len(), 14);
    assert!(seat.attacks().is_empty());
    world.tick = 120;
    assert!(army_dests(&play(&world, &mut brain, &[])).is_empty());
}

#[test]
fn test_crowded_defence_leaves_query_budget_to_issue_orders() {
    let (mut units, buildings) = base(12);
    units.extend(muster());
    let mut world = populated(units, buildings, 27, 40);
    world.enemies = (0..32)
        .map(|i| {
            enemy(
                900 + i,
                UnitKind::Spearman,
                (42 + (i % 8) as i32, 45 + (i / 8) as i32),
            )
        })
        .collect();
    let (_, seat) = play_fresh(&world);
    assert!(!attacks_by_army(&seat).is_empty());
}

#[test]
fn test_outnumbered_at_home_the_villagers_beside_a_raider_walk_to_the_town_center() {
    let (units, buildings) = base(6);
    let mut world = FakeWorld::new(units, buildings);
    world.enemies = vec![enemy(900, UnitKind::Scout, (51, 51))];
    let (_, calm) = play_fresh(&world);
    assert!(calm.moves().iter().all(|m| m.1 != (56, 56)));
    world.enemies = (0..3)
        .map(|i| enemy(800 + i, UnitKind::Spearman, (51, 51 + i as i32)))
        .collect();
    let (brain, seat) = play_fresh(&world);
    let pulled = seat.moves().iter().filter(|m| m.1 == (56, 56)).count();
    assert!(pulled > 0);
    assert_eq!(
        brain.board.stats.get("villagers_pulled_back"),
        Some(&(pulled as u64))
    );
}

/// `test_review.test_a_refused_approach_tile_is_not_sent_again`.
#[test]
fn test_a_refused_approach_tile_is_not_sent_again() {
    let target = hidden(foreign_building(901, BuildingKind::Barracks, (150, 120)));
    let army = spearmen(300, 12, (60, 60));
    let world = || {
        let (mut units, buildings) = base(10);
        units.extend(army.clone());
        let mut world = FakeWorld::new(units, buildings);
        world.enemy_buildings = vec![target.clone()];
        world
    };
    let (mut brain, seat) = play_fresh(&world());
    let groups = seat.formation_moves();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, ids(&army));
    let sequence = rally_sequence(&seat, 300);
    let refusal = order_result(sequence, CommandResult::RejectedInvalidDestination);
    let seat = play(&world(), &mut brain, &[refusal]);
    let second = seat.formation_moves();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].0, groups[0].0);
    assert_ne!(second[0].1, groups[0].1);
}

#[test]
fn test_an_army_left_on_the_same_tile_is_not_sent_the_same_hop_again() {
    let far = hidden(house(900, (250, 250)));
    let mut world = assault(muster(), vec![far]);
    let mut brain = Brain::new(Config::default());
    let first = army_dests(&play(&world, &mut brain, &[]));
    assert_eq!(first.len(), 14);
    // Past the march pause, still idle where the hop left them: the server
    // accepted it and they did not move, so the same order would do nothing.
    world.tick = 100 + brain.config.march_pause_seconds * 20 + 20;
    assert!(army_dests(&play(&world, &mut brain, &[])).is_empty());
    // Once they have moved, the next hop is ordered.
    for unit in world.units.iter_mut().filter(|u| u.id >= 300) {
        unit.x += 8;
    }
    world.tick += brain.config.march_pause_seconds * 20 + 20;
    assert!(!army_dests(&play(&world, &mut brain, &[])).is_empty());
}
