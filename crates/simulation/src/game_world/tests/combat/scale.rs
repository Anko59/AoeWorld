//! Death bookkeeping, deterministic replay and bounded per-tick work.
use super::*;
use aoe_core::{Seed, WorldConfig};

fn kill_scenario() -> (GameWorld, EntityId, [EntityId; 3]) {
    let mut world = world();
    world
        .place_town_center(PlayerId(0), TileCoord::new(40, 40))
        .unwrap();
    world
        .place_town_center(PlayerId(1), TileCoord::new(150, 150))
        .unwrap();
    // The first-spawned victim is swapped out of the dense array when it dies;
    // the others live in different spatial chunks.
    let victims = [at(101, 100), at(60, 200), at(200, 30)]
        .map(|position| world.spawn_villager(PlayerId(1), position).unwrap());
    let militia = world.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    (world, militia, victims)
}

#[test]
fn death_frees_population_at_once_and_removes_the_body_after_native_die_and_decay() {
    let (mut world, militia, [first, second, third]) = kill_scenario();
    assert_eq!(
        world.player_economy(PlayerId(1)).unwrap().population_used,
        3
    );
    world.issue_attack(PlayerId(0), militia, first).unwrap();
    ticks_until(&mut world, 400, |w| w.unit(first).unwrap().hp == 0);
    assert_eq!(
        world.player_economy(PlayerId(1)).unwrap().population_used,
        2
    );
    assert!(world.unit_exists(first));
    assert_eq!(world.unit_count(), 4);
    advance(&mut world, 629);
    assert!(world.unit_exists(first));
    advance(&mut world, 1);
    assert!(!world.unit_exists(first));
    assert_eq!(world.unit_count(), 3);
    assert_eq!(
        world.player_economy(PlayerId(1)).unwrap().population_used,
        2
    );
    // The survivors are still reachable through every index.
    assert_eq!(world.unit(second).unwrap().position, at(60, 200));
    assert_eq!(world.unit(third).unwrap().position, at(200, 30));
    let around =
        |x: i32, y: i32| TileRect::new(TileCoord::new(x - 1, y - 1), TileCoord::new(x + 2, y + 2));
    assert_eq!(world.query(around(60, 200)).0.len(), 1);
    assert_eq!(
        world
            .query(around(101, 100))
            .0
            .iter()
            .map(|u| u.id)
            .collect::<Vec<_>>(),
        [militia]
    );
    world.issue_move(second, at(64, 200)).unwrap();
    advance(&mut world, 100);
    assert_eq!(world.unit(second).unwrap().position, at(64, 200));
    // Removing the last unit of a chunk also drops the chunk.
    world.issue_attack(PlayerId(0), militia, third).unwrap();
    ticks_until(&mut world, 5_000, |w| !w.unit_exists(third));
    assert!(world.unit_exists(second) && world.unit_exists(militia));
}

#[test]
fn a_scripted_fight_replays_to_the_same_hash_and_differs_from_no_fight() {
    let run = |fight: bool| {
        let (mut world, militia, [first, ..]) = kill_scenario();
        if fight {
            world.issue_attack(PlayerId(0), militia, first).unwrap();
        }
        advance(&mut world, 300);
        world.canonical_hash()
    };
    assert_eq!(run(true), run(true));
    assert_ne!(run(true), run(false));
}

/// Players 0 and 1 share a crowd; `attackers` of player 0 each attack one of player 1.
fn crowd(units: u32, attackers: usize) -> GameWorld {
    let config = WorldConfig::new(256, 256, Seed(11)).unwrap();
    let mut world = GameWorld::with_population_in_extent(config, units, 0, 2, 64).unwrap();
    let ids = |player| {
        world
            .units()
            .filter(|unit| unit.player == PlayerId(player))
            .map(|unit| unit.id)
            .take(attackers)
            .collect::<Vec<_>>()
    };
    let (ours, theirs) = (ids(0), ids(1));
    for (attacker, target) in ours.into_iter().zip(theirs) {
        world.issue_attack(PlayerId(0), attacker, target).unwrap();
    }
    world
}

#[test]
fn combat_work_follows_the_attackers_not_the_crowd() {
    let mut worst = Vec::new();
    for units in [400, 6_000] {
        let mut world = crowd(units, 16);
        let mut peak = 0;
        for _ in 0..200 {
            world.advance();
            peak = peak.max(world.combat_stats().attackers);
            assert!(world.combat_stats().hits <= world.combat_stats().attackers);
        }
        worst.push(peak);
    }
    // Unit obstruction changes approach timing in the denser crowd, hence
    // which retaliations overlap. Bounded yields and legal arrivals change
    // those peaks again. Pin both current seeded peaks; each still
    // stays under 16 ordered attackers plus at most 16 retaliating victims.
    assert_eq!(worst, [22, 19]);
    assert!(
        worst.iter().all(|peak| (16..=32).contains(peak)),
        "{worst:?}"
    );
    let mut idle = crowd(6_000, 0);
    for _ in 0..20 {
        idle.advance();
        assert_eq!(idle.combat_stats(), crate::CombatStats::default());
    }
}
