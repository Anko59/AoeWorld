//! The combat cases of `ai/tests/test_navigation.py`: soldiers walk short land
//! hops, and soldiers in a fight are not recalled to exploration.
use aoe_sdk::UnitKind;

use super::{army_moves, populated, spearmen};
use crate::brain::Brain;
use crate::config::Config;
use crate::geometry::dist;
use crate::tests::fake::{FakeWorld, base, enemy, soldier};
use crate::tests::play::play;

#[test]
fn test_engaging_soldiers_are_not_recalled_to_exploration_in_the_same_decision() {
    let (mut units, buildings) = base(12);
    units.extend(spearmen(300, 14, (150, 150)));
    let mut world = populated(units, buildings, 27, 40);
    world.enemies = vec![enemy(900, UnitKind::Spearman, (153, 150))];
    let seat = play(&world, &mut Brain::new(Config::default()), &[]);
    assert!(seat.attacks().iter().any(|a| a.0 >= 300));
    assert!(seat.moves().iter().all(|m| m.0 < 300));
}

#[test]
fn test_far_rally_uses_short_land_hops_and_keeps_a_pause() {
    let (mut units, buildings) = base(12);
    units.push(soldier(300, (200, 55)));
    let world = |tick: u64| {
        let mut world = populated(units.clone(), buildings.clone(), 14, 30);
        world.tick = tick;
        world
    };
    let mut brain = Brain::new(Config::default());
    let seat = play(&world(100), &mut brain, &[]);
    let hops: Vec<_> = army_moves(&seat)
        .into_iter()
        .filter(|m| m.0 == 300)
        .collect();
    assert_eq!(hops.len(), 1);
    assert!(dist((200, 55), hops[0].1) <= 48.0);
    let seat = play(&world(120), &mut brain, &[]);
    assert!(army_moves(&seat).iter().all(|m| m.0 != 300));
    let mut blocked: FakeWorld = world(300);
    blocked.disconnected = true;
    let seat = play(&blocked, &mut brain, &[]);
    assert!(army_moves(&seat).iter().all(|m| m.0 != 300));
}
