//! Single-lane bridge crossings: the straight Paris-like deck reached along
//! a river strip, a 45-degree deck threaded through forest lanes, and a long
//! march over a straight, an off-grid and a diagonal bridge between forest
//! belts; for a General with 100 followers, a 40-unit mixed formation, two
//! formations meeting head-on, and villagers meeting a Knight on the deck.
//! Every unit reaches its exact slot, no unit stays still for more than 60
//! ticks, and the last arrival is within 1.3 times the single-file ideal: the
//! slowest unit's own shortest route plus the time every commanded body
//! (all but one, nose to tail) needs to file over the deck at the slowest
//! speed. The deck carries one direction at a time, so opposing formations
//! count both columns.
use super::landscapes::offered;
use super::*;
use crate::game_world::terrain_fixtures::build;
use metrics::{Tracker, reference_from};

fn single_file_ideal(w: &GameWorld, starts: &[(EntityId, WorldPosition)], route: u32) -> u32 {
    let units = starts
        .iter()
        .map(|&(id, _)| w.unit(id).unwrap())
        .collect::<Vec<_>>();
    let (speed, denominator) = units
        .iter()
        .map(|u| w.movement_speed(u.kind, u.player))
        .min_by(|a, b| {
            (u128::from(a.0) * u128::from(b.1)).cmp(&(u128::from(b.0) * u128::from(a.1)))
        })
        .unwrap();
    let column = units
        .iter()
        .map(|u| u64::from(u.kind.collision_radius().unsigned_abs()) * 2)
        .sum::<u64>()
        - units
            .iter()
            .map(|u| u64::from(u.kind.collision_radius().unsigned_abs()) * 2)
            .max()
            .unwrap();
    route + (column * denominator).div_ceil(speed) as u32
}

fn cross(case: &str, count: usize, limit: u32) {
    cross_fixture(case, build(case), count, limit, |f| f.issue());
}

pub(super) fn cross_fixture(
    case: &str,
    mut fixture: crate::game_world::terrain_fixtures::Fixture,
    count: usize,
    limit: u32,
    issue: impl FnOnce(&mut crate::game_world::terrain_fixtures::Fixture),
) {
    assert_eq!(fixture.ids.len(), count);
    let mut tracker = Tracker::new(&fixture.world, &fixture.ids);
    issue(&mut fixture);
    tracker.attach(&offered(&fixture));
    for tick in 1..=limit {
        fixture.world.advance();
        tracker.attach(&offered(&fixture));
        if tracker.observe(&fixture.world, tick) {
            break;
        }
    }
    let goals = tracker.goals();
    let starts = tracker.starts.clone();
    let mut result = tracker.finish(0);
    let route = reference_from(&fixture.world, &starts, &goals);
    result.ideal_ticks = single_file_ideal(&fixture.world, &starts, route);
    eprintln!("bridge-march {case} route={route} {result:?}");
    assert_eq!(result.max_speed_excess, 0, "{result:?}");
    assert_eq!(result.lost_group_ticks, 0, "{result:?}");
    assert_eq!(result.arrived, count, "every unit crosses: {result:?}");
    assert_eq!(result.max_slot_error, 0, "{result:?}");
    assert!(
        result.last_tick * 10 <= result.ideal_ticks * 13,
        "{result:?}"
    );
    assert!(result.max_stuck_ticks <= 60, "{result:?}");
}

#[test]
fn hundred_followers_and_their_general_file_over_a_single_lane_bridge() {
    cross("general-bridge", 101, 20_000);
}

#[test]
fn forty_in_formation_file_over_a_single_lane_bridge() {
    cross("bridge-40", 40, 16_000);
}

#[test]
fn opposing_formations_take_turns_on_a_single_lane_bridge() {
    cross("bridge-opposed", 40, 20_000);
}

#[test]
fn villagers_and_a_knight_meeting_on_the_deck_never_lock_it() {
    cross("bridge-meet", 4, 4_000);
}

#[test]
fn hundred_followers_thread_forest_lanes_onto_a_diagonal_deck() {
    cross("forest-bridge", 101, 20_000);
}

#[test]
fn forty_in_formation_thread_forest_lanes_onto_a_diagonal_deck() {
    cross("forest-bridge-40", 40, 16_000);
}

#[test]
fn opposing_formations_meet_at_a_wooded_diagonal_deck() {
    cross("forest-bridge-opposed", 40, 20_000);
}

#[test]
fn hundred_followers_cross_three_bridged_rivers_and_forest_belts() {
    cross("long-bridges", 101, 40_000);
}

#[test]
fn forty_in_formation_cross_three_bridged_rivers_and_forest_belts() {
    cross("long-bridges-40", 40, 40_000);
}
