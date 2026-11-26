//! The scout manager through `Brain::decide`, ported from the scouting cases
//! of `ai/tests/test_logic.py` and `ai/tests/test_navigation.py`; bridges are
//! in their own file (`test_bridges.py`).
mod bridges;

use std::collections::BTreeSet;

use aoe_sdk::{CommandResult, UnitKind};

use crate::brain::Brain;
use crate::config::Config;
use crate::geometry::dist;
use crate::tests::fake::{FakeWorld, Recorder, base, order_result, unit_of};
use crate::tests::play::{eco, play};
use crate::view::Tile;

/// A world whose economy does nothing, so every move is the scout's.
fn quiet(villagers: u32) -> FakeWorld {
    let (units, buildings) = base(villagers);
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(eco(0, 0, 100, 5, 20));
    world
}

/// The moves ordered for the unit `id`.
fn moves_of(seat: &Recorder, id: u32) -> Vec<Tile> {
    seat.moves()
        .into_iter()
        .filter(|m| m.0 == id)
        .map(|m| m.1)
        .collect()
}

#[test]
fn test_the_scout_checks_home_then_rides_the_map_circle_far_side_first() {
    let mut world = quiet(4);
    let mut brain = Brain::new(Config::default());
    let mut visited: Vec<Tile> = Vec::new();
    for _ in 0..14 {
        let moves = moves_of(&play(&world, &mut brain, &[]), 100);
        assert_eq!(moves.len(), 1, "one scout move per decision: {visited:?}");
        visited.push(moves[0]);
        // The scout arrives before it is idle again.
        let scout = world.units.last_mut().expect("the scout");
        (scout.x, scout.y) = moves[0];
    }
    let home = (52, 52);
    assert!(
        visited[..6]
            .iter()
            .all(|p| (dist(*p, home) - 18.0).abs() < 2.0),
        "{visited:?}"
    );
    let far = visited[9]; // first point of the map circle: the mirror of home
    let distinct: BTreeSet<&Tile> = visited.iter().collect();
    assert!(dist(far, home) > 150.0 && distinct.len() == visited.len());
}

#[test]
fn test_a_scout_that_cannot_reach_a_far_waypoint_drops_the_far_ones() {
    // The fake scout never moves: its ride to the first map-circle point ends
    // far short of it, so the rest of the circle is dropped for nearer rings.
    let world = quiet(4);
    let mut brain = Brain::new(Config::default());
    let mut visited: Vec<Tile> = Vec::new();
    for _ in 0..14 {
        let moves = moves_of(&play(&world, &mut brain, &[]), 100);
        visited.push(moves[0]);
    }
    let home = (52, 52);
    let far: Vec<usize> = (0..visited.len())
        .filter(|i| dist(visited[*i], home) > 150.0)
        .collect();
    assert!(!far.is_empty(), "{visited:?}");
    assert!(
        visited[far[0] + 1..]
            .iter()
            .all(|p| dist(*p, home) <= 110.0),
        "{visited:?}"
    );
}

#[test]
fn test_a_scout_that_is_busy_is_left_alone() {
    let mut world = quiet(4);
    world.units.last_mut().expect("the scout").idle = false;
    let seat = play(&world, &mut Brain::new(Config::default()), &[]);
    assert!(moves_of(&seat, 100).is_empty());
}

// ----- ai/tests/test_navigation.py ------------------------------------------

/// `frontier_world`: the scout 200 beside the Town Center, a river past x = 55
/// toward (200, 50) with the frontier at (70, 50).
fn frontier_world(tick: u64) -> FakeWorld {
    let (mut units, buildings) = base(12);
    units.push(unit_of(200, UnitKind::Scout, (50, 50)));
    let mut world = FakeWorld::new(units, buildings);
    world.tick = tick;
    world.economy = Some(eco(200, 200, 100, 14, 30));
    world.route_fn = Some(Box::new(|from, to| {
        (to != (200, 50)).then(|| vec![from, to])
    }));
    world.frontier_fn = Some(Box::new(|_, _| Some((70, 50))));
    world
}

/// The brain after its first frontier ride, and the sequence of that order.
fn first_frontier() -> (Brain, u64) {
    let mut brain = Brain::new(Config::default());
    brain.board.scout_waypoints = vec![(200, 50)];
    let seat = play(&frontier_world(100), &mut brain, &[]);
    assert_eq!(moves_of(&seat, 200), vec![(70, 50)]);
    let sequence = seat
        .moves()
        .iter()
        .position(|m| m.0 == 200)
        .map(|i| i as u64 + 1)
        .expect("the scout rides");
    (brain, sequence)
}

#[test]
fn test_a_refused_frontier_uses_another_proven_land_tile() {
    let (mut brain, sequence) = first_frontier();
    let refusal = order_result(sequence, CommandResult::RejectedInvalidDestination);
    let seat = play(&frontier_world(120), &mut brain, &[refusal]);
    let moves = moves_of(&seat, 200);
    assert!(!moves.is_empty() && moves.iter().all(|m| *m != (70, 50)));
}

#[test]
fn test_an_idle_scout_does_not_repeat_the_same_unreached_frontier_forever() {
    let (mut brain, _) = first_frontier();
    let seat = play(&frontier_world(200), &mut brain, &[]);
    let moves = moves_of(&seat, 200);
    assert!(!moves.is_empty() && moves.iter().all(|m| *m != (70, 50)));
}

#[test]
fn test_an_off_map_view_does_not_end_a_scout_mission() {
    let mut world = frontier_world(100);
    world
        .units
        .retain(|u| u.kind != UnitKind::Scout || u.id == 200);
    world.visibility_fn = Some(Box::new(|_| aoe_sdk::Visibility::Unexplored));
    world.route_fn = Some(Box::new(|_, _| None));
    world.frontier_fn = Some(Box::new(|_, _| None));
    let mut brain = Brain::new(Config::default());
    brain.board.scout_stage = 2;
    brain.board.scout_waypoints = vec![(200, 50)];

    let seat = play(&world, &mut brain, &[]);

    assert!(seat.moves().contains(&(200, (200, 50))));
}
