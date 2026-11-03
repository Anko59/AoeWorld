//! A General's 100 followers use the shared formation corridor and detours,
//! never straight-line slot seeking. Every follower arrives at its exact
//! slot, the last arrival is within 1.3 times the slowest follower's shortest
//! route, and no follower stays still for more than 60 ticks (three seconds
//! at 20 Hz).
use super::landscapes::offered;
use super::*;
use crate::game_world::terrain_fixtures::build;
use metrics::{Tracker, reference_from};

fn march(case: &str, limit: u32) {
    let mut fixture = build(case);
    assert_eq!(fixture.ids.len(), 101, "100 followers plus their General");
    assert_eq!(
        fixture
            .world
            .general_status(fixture.primary)
            .unwrap()
            .army_size,
        100
    );
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
    // Oracle work stays outside the measured movement and collision counters.
    result.ideal_ticks = reference_from(&fixture.world, &starts, &goals);
    eprintln!("general-followers {case} {result:?}");
    assert_eq!(result.max_speed_excess, 0, "{result:?}");
    assert_eq!(result.lost_group_ticks, 0, "{result:?}");
    assert_eq!(result.arrived, 101, "{result:?}");
    assert!(
        result.last_tick * 10 <= result.ideal_ticks * 13,
        "{result:?}"
    );
    assert_eq!(result.max_slot_error, 0, "{result:?}");
    assert!(result.max_stuck_ticks <= 60, "{result:?}");
}

#[test]
fn hundred_followers_take_a_river_detour_to_the_far_bank() {
    march("general-river", 16_000);
}

#[test]
fn hundred_followers_march_through_a_tree_patch() {
    march("general-forest", 12_000);
}

#[test]
fn followers_across_a_river_from_their_general_go_around_to_it() {
    march("general-far-bank", 16_000);
}
