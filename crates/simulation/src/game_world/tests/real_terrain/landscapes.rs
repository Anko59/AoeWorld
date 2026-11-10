use super::*;
use crate::game_world::terrain_fixtures::{Command, Fixture, build};
use metrics::{Causes, Tracker, reference_from};

fn causes(contact: u32, replan: u32, rerank: u32, other: u32) -> Causes {
    Causes {
        contact,
        replan,
        rerank,
        other,
    }
}

pub(super) fn offered(f: &Fixture) -> Vec<Option<WorldPosition>> {
    f.ids
        .iter()
        .map(|&id| {
            f.world.formation_destination_slot(id).or_else(|| {
                f.commands.iter().find_map(|command| match command {
                    Command::Move(actor, goal) if *actor == id => Some(*goal),
                    _ => None,
                })
            })
        })
        .collect()
}

fn run(case: &str, count: usize, limit: u32) {
    let mut fixture = build(case);
    assert_eq!(fixture.ids.len(), count);
    eprintln!("terrain-fixture {case} {}", fixture.fingerprint());
    let mut tracker = Tracker::new(&fixture.world, &fixture.ids);
    fixture.issue();
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
    // Oracle work stays outside the measured movement/collision counters.
    result.ideal_ticks = reference_from(&fixture.world, &starts, &goals);
    eprintln!("real-terrain {case} {result:?}");
    // First terrain milestone: protect full arrival, <=1.3x time, physical
    // speed, membership, exact slots and bounded work. Pin the two known
    // recovery/oscillation deficits explicitly until the next small commit.
    // Independent typed squads intentionally change the current-output pins.
    // Keep all arrival, work, speed, membership and 60-tick acceptance bounds.
    // The audit reports increased long-march work/reversals, not just gains.
    // Pre-existing building-overlap recovery adds collision checks only; these
    // current-output counts retain every movement and bounded-work limit above.
    // Static-cell reuse and lazy contact policy intentionally reduce collision
    // queries. Oracle validation is excluded; arrival/route/hash pins stay put.
    match case {
        "river" => {
            result.assert_arrival_and_work(count);
            assert!(result.max_tick_route_work <= 24_576, "{result:?}");
            assert_eq!((result.max_stuck_ticks, result.oscillations), (8, 3));
            // Current-output split of the 3 reversals by cause.
            assert_eq!(result.oscillation_causes, causes(0, 0, 0, 3));
            assert_eq!(
                (result.last_tick, result.route_work, result.collision_work),
                (5522, 55349, 3107314)
            );
        }
        "long" => {
            result.assert_arrival_and_work(count);
            assert!(result.max_tick_route_work <= 16_384, "{result:?}");
            // 57: one arrived soldier steps aside and waits 56 ticks for the march to pass.
            assert_eq!((result.max_stuck_ticks, result.oscillations), (57, 18));
            // 12 of the 18 reversals are traffic contact; the other 6 have none
            // of the recorded causes. Phase 2 takes the contact ones to 0.
            assert_eq!(result.oscillation_causes, causes(12, 0, 0, 6));
            assert_eq!(
                (result.last_tick, result.route_work, result.collision_work),
                (36804, 378134, 102322464)
            );
        }
        _ => {
            result.assert_works(count);
            // Current-output pins also retain work for the complete report.
            let expected = match case {
                "forest" => (3288, 1027, 2198882),
                "town" => (3740, 1193, 2510772),
                "crossing" => (3278, 1933, 3051342),
                "bridge" => (479, 354, 12365),
                "siege" => (1556, 867, 33972),
                "gap" => (580, 512, 8908),
                _ => unreachable!(),
            };
            assert_eq!(
                (result.last_tick, result.route_work, result.collision_work),
                expected,
                "{result:?}"
            );
        }
    }
}

#[test]
fn mixed_forty_follow_a_river_detour_and_reform() {
    run("river", 40, 12_000);
}

#[test]
fn mixed_forty_cross_a_forest_gap_and_reform() {
    run("forest", 40, 10_000);
}

#[test]
fn mixed_forty_march_through_town_streets() {
    run("town", 40, 12_000);
}

#[test]
fn opposing_formations_cross_between_forest_edges() {
    run("crossing", 80, 10_000);
}

#[test]
fn general_hundred_march_a_thousand_tiles_past_rivers_and_forests() {
    run("long", 100, 75_000);
}

#[test]
fn off_grid_diagonal_bridge_supports_both_villagers() {
    run("bridge", 2, 2_000);
}

#[test]
fn siege_routes_around_a_foundation_footprint() {
    run("siege", 1, 4_000);
}

#[test]
fn two_villagers_meet_in_a_one_tile_gap() {
    run("gap", 2, 2_000);
}
