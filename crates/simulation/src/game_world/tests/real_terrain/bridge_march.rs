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
    let mut failed = false;
    for tick in 1..=limit {
        let root = &fixture.world.units[fixture.world.lookup[&fixture.primary]];
        let before = root.order;
        let planner = root.planner.as_ref().map(|p| {
            (
                p.origin(),
                p.destination(),
                p.certified_destination(),
                p.work(),
                p.leg_work(),
            )
        });
        fixture.world.advance();
        if !failed && fixture.world.movement_failure(fixture.primary).is_some() {
            failed = true;
            eprintln!(
                "first-failure {case} tick={tick} before={before:?} planner={planner:?} error={:?} samples={:?}",
                fixture.world.movement_failure(fixture.primary),
                fixture.world.route_scheduler_stats().metrics.samples
            );
        }
        tracker.attach(&offered(&fixture));
        if tracker.observe(&fixture.world, tick) {
            break;
        }
    }
    let mut samples = std::collections::BTreeMap::new();
    for &id in &fixture.ids {
        let unit = fixture.world.unit(id).unwrap();
        if !unit.moving && fixture.world.formation_destination_slot(id) == Some(unit.position) {
            continue;
        }
        let index = fixture.world.lookup[&id];
        let leader = fixture.world.units[index].formation.unwrap_or(id);
        let count = samples.entry(leader).or_insert(0);
        if *count >= 2 {
            continue;
        }
        *count += 1;
        let root = &fixture.world.units[fixture.world.lookup[&leader]];
        eprintln!(
            "unfinished {case} id={id:?} unit={unit:?} anchor={:?} motion={:?} carrier={leader:?} order={:?} route={:?} error={:?}",
            fixture.world.formation_anchor(id),
            fixture.world.formation_motion(index),
            root.order,
            root.route.iter().take(4).collect::<Vec<_>>(),
            root.last_movement_error
        );
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

fn solo_three_bridges(formation: bool, slow: bool) {
    let mut f = build("long-bridges");
    let g = f.primary;
    if slow {
        f.world.config.tick_hz = 47;
    }
    for id in f
        .ids
        .iter()
        .copied()
        .filter(|&id| id != g)
        .collect::<Vec<_>>()
    {
        f.world.remove_unit(id);
    }
    let goal = center(610, 110);
    f.world
        .issue_general_move(PlayerId(0), g, f.world.unit(g).unwrap().position)
        .unwrap();
    if formation {
        f.world
            .issue_formation_move(
                PlayerId(0),
                &[g],
                aoe_core::Formation::Line,
                goal,
                false,
                false,
            )
            .unwrap();
    } else {
        f.world.issue_general_move(PlayerId(0), g, goal).unwrap();
    }
    let ideal = reference_ticks(&f.world, &[(g, goal)]);
    let mut tracker = Tracker::new(&f.world, &[g]);
    tracker.attach(&[Some(goal)]);
    let mut failed = false;
    for tick in 1..=30_000 {
        let before = f.world.movement_order(g);
        let planner = f.world.units[f.world.lookup[&g]].planner.as_ref().map(|p| {
            (
                p.origin(),
                p.destination(),
                p.certified_destination(),
                p.work(),
                p.leg_work(),
            )
        });
        f.world.advance();
        if !failed && f.world.movement_failure(g).is_some() {
            failed = true;
            eprintln!(
                "solo-three-bridge formation={formation} tick={tick} before={before:?} planner={planner:?} error={:?} samples={:?}",
                f.world.movement_failure(g),
                f.world.route_scheduler_stats().metrics.samples
            );
        }
        if tracker.observe(&f.world, tick) {
            break;
        }
    }
    let result = tracker.finish(ideal);
    eprintln!("solo-three-bridge formation={formation} {result:?}");
    assert_eq!(result.arrived, 1, "{result:?}");
    assert!(
        result.last_tick * 10 <= result.ideal_ticks * 13,
        "{result:?}"
    );
}

#[test]
fn three_bridge_solo_general_uses_all_ordinary_route_continuations() {
    solo_three_bridges(false, false);
}
#[test]
fn three_bridge_solo_formation_uses_all_virtual_route_continuations() {
    solo_three_bridges(true, false);
}

#[test]
fn slow_three_bridge_solo_formation_retains_continuations() {
    solo_three_bridges(true, true);
}
