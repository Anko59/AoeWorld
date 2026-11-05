//! Idle friends step aside for a blocked formation: a crowd is not a wall, and
//! nobody who has something to do is disturbed.
use super::*;
use crate::game_world::movement_step::yield_candidate;
use crate::game_world::terrain_fixtures::{Fixture, build};
use aoe_core::Formation;

fn tile(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

const CENTRE: (i32, i32) = (100, 100);

/// Idle Champions 640 subunits apart all round a ram: open ground, but even at
/// the friendly squeeze distance (2 x 333) a ram does not fit between two of them.
/// The second ram stands outside, so the order is a real formation.
fn ram_in_a_crowd() -> (Fixture, Vec<EntityId>, Vec<EntityId>) {
    let mut fixture = build("army-forest");
    let centre = tile(CENTRE.0, CENTRE.1);
    let mut crowd = Vec::new();
    for x in -6_i32..=6 {
        for y in -6_i32..=6 {
            if x.abs() <= 1 && y.abs() <= 1 {
                continue;
            }
            let at = WorldPosition::new(centre.x + x * 640, centre.y + y * 640);
            crowd.push(
                fixture
                    .world
                    .spawn_kind(PlayerId(0), UnitKind::Champion, at)
                    .unwrap(),
            );
        }
    }
    let rams = [centre, tile(CENTRE.0, CENTRE.1 + 12)].map(|at| {
        fixture
            .world
            .spawn_kind(PlayerId(0), UnitKind::SiegeRam, at)
            .unwrap()
    });
    (fixture, crowd, rams.to_vec())
}

/// Ticks until both rams stopped within two tiles of their slots.
fn march_rams(fixture: &mut Fixture, rams: &[EntityId], ticks: u32) -> u32 {
    let goal = tile(CENTRE.0 + 30, CENTRE.1 + 6);
    let world = &mut fixture.world;
    world
        .issue_formation_move(PlayerId(0), rams, Formation::Line, goal, false, false)
        .unwrap();
    let slots = rams
        .iter()
        .map(|id| world.formation_destination_slot(*id).unwrap())
        .collect::<Vec<_>>();
    for tick in 0..ticks {
        world.advance();
        if rams.iter().zip(&slots).all(|(id, slot)| {
            let unit = world.unit(*id).unwrap();
            let (x, y) = (
                i64::from(unit.position.x - slot.x),
                i64::from(unit.position.y - slot.y),
            );
            !unit.moving && !unit.planning && x * x + y * y <= (2 * 1024_i64).pow(2)
        }) {
            return tick;
        }
    }
    panic!(
        "rams never reached their slots: {:?} of {slots:?}",
        rams.iter()
            .map(|id| world.unit(*id).unwrap().position)
            .collect::<Vec<_>>()
    );
}

#[test]
fn idle_friends_step_aside_for_a_ram_in_a_crowd() {
    let (mut fixture, crowd, rams) = ram_in_a_crowd();
    let before = crowd
        .iter()
        .map(|id| fixture.world.unit(*id).unwrap().position)
        .collect::<Vec<_>>();
    march_rams(&mut fixture, &rams, 6_000);
    // Only the bodies on the ram's way were asked: the army did not wake up.
    let shifted = crowd
        .iter()
        .zip(&before)
        .filter(|(id, at)| fixture.world.unit(**id).unwrap().position != **at)
        .count();
    assert!(shifted >= 1 && shifted <= crowd.len() / 3, "{shifted}");
    for (id, at) in crowd.iter().zip(&before) {
        let unit = fixture.world.unit(*id).unwrap();
        let far = i64::from(unit.position.x - at.x)
            .abs()
            .max(i64::from(unit.position.y - at.y).abs());
        assert!(far <= 3 * 1024, "{id:?} pushed {far} away");
        assert!(!unit.moving, "{id:?} woke up");
    }
}

#[test]
fn stepping_aside_agrees_between_one_and_four_workers() {
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let (mut fixture, _, rams) = ram_in_a_crowd();
                let at = march_rams(&mut fixture, &rams, 6_000);
                (at, fixture.world.canonical_hash())
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn only_an_idle_orderless_friend_may_be_asked() {
    let mut fixture = build("army-forest");
    let world = &mut fixture.world;
    let idle = world
        .spawn_kind(PlayerId(0), UnitKind::Champion, tile(50, 150))
        .unwrap();
    assert!(yield_candidate(world, idle));

    let ordered = world
        .spawn_kind(PlayerId(0), UnitKind::Champion, tile(52, 150))
        .unwrap();
    world.issue_task_move(ordered, tile(70, 150)).unwrap();
    assert!(!yield_candidate(world, ordered), "a live order");

    let fighting = world
        .spawn_kind(PlayerId(0), UnitKind::Champion, tile(54, 150))
        .unwrap();
    let index = world.lookup[&fighting];
    world.units[index].state.attack_target = Some(idle);
    assert!(!yield_candidate(world, fighting), "a fight");

    let working = world
        .spawn_kind(PlayerId(0), UnitKind::Villager, tile(56, 150))
        .unwrap();
    let index = world.lookup[&working];
    world.units[index].state.task = crate::UnitTask::Gathering;
    assert!(!yield_candidate(world, working), "work");
}
