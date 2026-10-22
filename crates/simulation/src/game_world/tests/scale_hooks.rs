//! The scale-measurement hooks: capacity limits, the tick probe and the wildlife skip.
use super::*;
use crate::game_world::economy::{
    MAX_BUILDERS, MAX_ECONOMY_BUILDINGS, MAX_ECONOMY_PLAYERS, MAX_GATHERERS,
};
use crate::{AnimalKind, AnimalTask, EconomyError, TickPhase, WorldLimits};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn default_limits_equal_the_shipped_caps() {
    assert_eq!(
        world().limits(),
        WorldLimits {
            players: MAX_ECONOMY_PLAYERS,
            buildings: MAX_ECONOMY_BUILDINGS,
            gatherers: MAX_GATHERERS,
            builders: MAX_BUILDERS,
            hunters: MAX_HUNTERS,
            animals: MAX_ANIMALS,
        }
    );
    assert_eq!(WorldLimits::default(), world().limits());
}

#[test]
fn raised_limits_take_effect() {
    let mut w = world();
    w.set_limits(WorldLimits {
        players: 1,
        animals: 0,
        ..w.limits()
    });
    w.place_town_center(PlayerId(0), TileCoord::new(20, 20))
        .unwrap();
    assert_eq!(
        w.place_town_center(PlayerId(1), TileCoord::new(80, 80)),
        Err(EconomyError::Capacity)
    );
    assert_eq!(
        w.spawn_animal(AnimalKind::Sheep, at(150, 150)),
        Err(EconomyError::Capacity)
    );
    w.set_limits(WorldLimits {
        players: 2,
        animals: 1,
        ..w.limits()
    });
    w.place_town_center(PlayerId(1), TileCoord::new(80, 80))
        .unwrap();
    w.spawn_animal(AnimalKind::Sheep, at(150, 150)).unwrap();
}

fn busy_world() -> GameWorld {
    let mut w = world();
    let a = w.spawn_unit(PlayerId(0), at(100, 100)).unwrap();
    let b = w.spawn_villager(PlayerId(1), at(110, 100)).unwrap();
    w.issue_move(a, at(130, 100)).unwrap();
    w.issue_move(b, at(110, 140)).unwrap();
    w.spawn_animal(AnimalKind::Deer, at(112, 101)).unwrap();
    w
}

#[test]
fn probe_reports_ordered_phases_and_changes_nothing() {
    let mut plain = busy_world();
    let mut probed = busy_world();
    for _ in 0..40 {
        let expected = plain.advance();
        let mut phases = Vec::new();
        let got = probed.advance_with_probe(&mut |_| true, &mut |phase| phases.push(phase));
        assert_eq!(got, expected);
        assert_eq!(phases.first(), Some(&TickPhase::Movement));
        assert!(
            phases
                .windows(2)
                .all(|pair| pair[0] as usize <= pair[1] as usize)
        );
        assert_eq!(phases.last(), Some(&TickPhase::Changed));
    }
    assert_eq!(probed.canonical_hash(), plain.canonical_hash());
}

#[test]
fn idle_ticks_report_only_the_early_phases() {
    let mut w = world();
    w.spawn_unit(PlayerId(0), at(100, 100)).unwrap();
    let mut phases = Vec::new();
    w.advance_with_probe(&mut |_| true, &mut |phase| phases.push(phase));
    assert_eq!(phases, [TickPhase::Movement, TickPhase::Wildlife]);
}

#[test]
fn wildlife_skip_counts_every_cell_and_pair_like_the_full_scan() {
    let mut w = world();
    // Units near the animals and units far from every animal.
    for (x, y) in [(100, 100), (101, 100), (200, 200), (30, 220), (240, 20)] {
        w.spawn_unit(PlayerId(0), at(x, y)).unwrap();
    }
    let near = w.spawn_animal(AnimalKind::Deer, at(103, 100)).unwrap();
    let far = w.spawn_animal(AnimalKind::Deer, at(150, 40)).unwrap();
    w.advance();
    let stats = w.wildlife_stats();
    // Animal-driven scan: an animal probes its nine cells only when some live
    // unit sits in them, and only such animals are counted.
    let reached = w
        .animals()
        .filter(|a| {
            let (ax, ay) = crate::game_world::wildlife::cell(a.position);
            w.units().any(|u| {
                let (ux, uy) = crate::game_world::wildlife::cell(u.position);
                u.hp > 0 && ax.abs_diff(ux) <= 1 && ay.abs_diff(uy) <= 1
            })
        })
        .count() as u32;
    assert!(reached > 0, "the near animal must be reached");
    assert_eq!(
        stats.cells,
        9 * reached,
        "each animal a unit can reach counts nine cells, and no other"
    );
    let brute: u32 = w
        .units()
        .map(|u| {
            let (cx, cy) = crate::game_world::wildlife::cell(u.position);
            w.animals()
                .filter(|a| {
                    let (ax, ay) = crate::game_world::wildlife::cell(a.position);
                    ax.abs_diff(cx) <= 1 && ay.abs_diff(cy) <= 1
                })
                .count() as u32
        })
        .sum();
    assert_eq!(stats.candidate_pairs, brute);
    assert!(brute > 0);
    assert_eq!(w.animal(near).unwrap().task, AnimalTask::Fleeing);
    assert_ne!(w.animal(far).unwrap().task, AnimalTask::Fleeing);
}
