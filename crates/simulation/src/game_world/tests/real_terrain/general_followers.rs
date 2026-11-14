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
    if offered(&fixture).iter().any(Option::is_none) {
        let army = &fixture.world.economy.generals.armies[&fixture.primary];
        eprintln!(
            "unfinished {case}: commander={:?} final={:?} destination={:?}",
            army.commander_march, army.routed, army.destination
        );
        for squad in &army.squads {
            let leader = squad.members[0];
            eprintln!(
                "squad final={} march={:?} leader={:?} at={:?} moving={:?} actual_goal={:?}",
                squad.final_leg,
                squad.march,
                leader,
                fixture.world.unit(leader).map(|u| u.position),
                fixture.world.unit(leader).map(|u| (u.moving, u.planning)),
                fixture.world.movement_goal(leader)
            );
        }
    }
    let goals = tracker.goals();
    for &(id, goal) in goals
        .iter()
        .filter(|(id, goal)| fixture.world.unit(*id).unwrap().position != *goal)
        .take(4)
    {
        let u = fixture.world.unit(id).unwrap();
        let near: Vec<_> = fixture
            .world
            .units()
            .filter(|other| {
                i64::from(other.position.x - goal.x).pow(2)
                    + i64::from(other.position.y - goal.y).pow(2)
                    < 1536_i64.pow(2)
            })
            .map(|v| {
                (
                    v.id,
                    v.kind,
                    v.position,
                    fixture.world.formation_destination_slot(v.id),
                )
            })
            .collect();
        eprintln!(
            "unfinished-final {case} actor={u:?} expected={goal:?} nearby={near:?} status={} density={}",
            fixture.world.formation_member_status(id),
            fixture.world.group_density_probe(id)
        );
    }
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

#[test]
fn river_recording_setup_preserves_map_water_and_adopts_one_hundred() {
    let mut f = build("general-river");
    f.world
        .initialize_village(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let (g, span, goal) = f.world.prepare_general_river_demo().unwrap();
    assert_ne!(span.from.x, span.to.x, "grid-diagonal bridge changes x");
    assert_ne!(span.from.y, span.to.y, "grid-diagonal bridge changes y");
    assert_eq!(
        (span.to.x - span.from.x).abs(),
        (span.to.y - span.from.y).abs()
    );
    assert_eq!(f.world.general_status(g).unwrap().army_size, 100);
    assert_eq!(f.world.unit_count(), 113);
    assert_eq!(
        f.world.units().filter(|u| u.player == PlayerId(1)).count(),
        12
    );
    assert_eq!(
        f.world.player_economy(PlayerId(0)).unwrap().population_used,
        101
    );
    let Terrain::Map { generator, .. } = f.world.terrain() else {
        panic!("the recording must keep its map");
    };
    for tile in span
        .tiles()
        .into_iter()
        .filter(|&t| t != span.from && t != span.to)
    {
        assert_ne!(
            generator
                .tile_and_node_with_cancel(tile, &|| false)
                .unwrap()
                .unwrap()
                .0
                .water,
            aoe_map::WaterKind::None
        );
    }
    assert!(
        goal.x != f.world.unit(g).unwrap().position.x
            || goal.y != f.world.unit(g).unwrap().position.y
    );
    f.world.issue_move(g, goal).unwrap();
    let end = WorldPosition::from_tile_center(span.to).unwrap();
    let dx = i64::from(span.to.x - span.from.x);
    let dy = i64::from(span.to.y - span.from.y);
    let mut crossed = std::collections::BTreeSet::new();
    for _ in 0..5_000 {
        f.world.advance();
        crossed.extend(
            f.world
                .units()
                .filter(|u| {
                    u.player == PlayerId(0)
                        && i64::from(u.position.x - end.x) * dx
                            + i64::from(u.position.y - end.y) * dy
                            > 0
                })
                .map(|u| u.id),
        );
    }
    assert_eq!(f.world.unit(g).unwrap().position, goal);
    for u in f
        .world
        .units()
        .filter(|u| u.player == PlayerId(0) && !crossed.contains(&u.id))
    {
        let army = &f.world.economy.generals.armies[&g];
        eprintln!(
            "not-crossed span={span:?} objective={goal:?} actor={u:?} move={:?} squad={:?}",
            f.world.movement_goal(u.id),
            army.squads.iter().find(|s| s.members.contains(&u.id))
        );
        eprintln!(
            "member {} motion={:?} density={}",
            f.world.formation_member_status(u.id),
            f.world.formation_motion(f.world.lookup[&u.id]),
            f.world.group_density_probe(u.id)
        );
    }
    // Wounded soldiers may withdraw after crossing into the far-bank fight.
    assert_eq!(
        crossed.len(),
        101,
        "the whole recording army crosses the river"
    );
}
