//! Units never stay locked inside a finished building's footprint.
use super::*;

/// Units inside `footprint` or closer to it than movement allows.
fn inside(world: &GameWorld, footprint: TileRect) -> Vec<(EntityId, WorldPosition)> {
    world
        .units()
        .filter(|u| {
            let r = 128;
            let (lo_x, hi_x) = (
                i64::from(footprint.min.x) * 1024,
                i64::from(footprint.max.x) * 1024,
            );
            let (lo_y, hi_y) = (
                i64::from(footprint.min.y) * 1024,
                i64::from(footprint.max.y) * 1024,
            );
            let (x, y) = (i64::from(u.position.x), i64::from(u.position.y));
            let dx = (lo_x - x).max(x - hi_x).max(0);
            let dy = (lo_y - y).max(y - hi_y).max(0);
            dx * dx + dy * dy < r * r
        })
        .map(|u| (u.id, u.position))
        .collect()
}

fn finish(world: &mut GameWorld, id: EntityId) {
    for _ in 0..4000 {
        world.advance();
        if world.building(id).unwrap().is_complete() {
            return;
        }
    }
    panic!("building never completed");
}

#[test]
fn builders_finishing_a_building_end_outside_its_footprint() {
    for (kind, count) in [
        (BuildingKind::Mill, 1),
        (BuildingKind::Mill, 5),
        (BuildingKind::House, 6),
        (BuildingKind::Barracks, 8),
    ] {
        let (mut world, _, _) = village();
        fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
        let builders = (0..count)
            .map(|i| villager_at(&mut world, 96 + i % 3, 99 + i / 3))
            .collect::<Vec<_>>();
        let id = world
            .issue_build(PlayerId(0), &builders, kind, at(100, 100), false)
            .unwrap();
        let footprint = world.building(id).unwrap().footprint;
        finish(&mut world, id);
        for _ in 0..40 {
            world.advance();
        }
        assert_eq!(inside(&world, footprint), vec![], "{kind:?} x{count}");
        for b in &builders {
            let tile = world.unit(*b).unwrap().position.tile_floor();
            assert!(!footprint.contains(tile), "{kind:?}: builder at {tile:?}");
        }
    }
}

/// Whatever put a body inside (the owner's report: a villager locked inside
/// a finished Mill), completion must release it. The scenario is forced here.
#[test]
fn a_body_found_inside_a_building_at_completion_is_put_outside() {
    let (mut world, _, _) = village();
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
    let builders = [
        villager_at(&mut world, 98, 100),
        villager_at(&mut world, 98, 101),
    ];
    let id = world
        .issue_build(
            PlayerId(0),
            &builders,
            BuildingKind::Mill,
            at(100, 100),
            false,
        )
        .unwrap();
    let footprint = world.building(id).unwrap().footprint;
    while world.building(id).unwrap().progress_permille() < 900 {
        world.advance();
    }
    let trapped = builders[1];
    let index = world.lookup[&trapped];
    let from = world.units[index].state.position.tile_floor();
    world.units[index].state.position =
        WorldPosition::from_tile_center(at(footprint.min.x, footprint.min.y)).unwrap();
    let state = world.units[index].state;
    world.track_unit_step(trapped, from, &state);
    finish(&mut world, id);
    assert_eq!(inside(&world, footprint), vec![]);
}

#[test]
fn a_house_placed_on_idle_villagers_is_accepted_and_they_step_aside() {
    let (mut world, _, _) = village();
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
    let builder = villager_at(&mut world, 98, 100);
    let idle = [
        villager_at(&mut world, 100, 100),
        villager_at(&mut world, 101, 100),
        villager_at(&mut world, 100, 101),
    ];
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .expect("units on the footprint do not refuse a foundation");
    let footprint = world.building(id).unwrap().footprint;
    let mut last = idle.map(|u| world.unit(u).unwrap().position);
    let mut biggest_step = 0;
    for _ in 0..4000 {
        world.advance();
        for (n, u) in idle.iter().enumerate() {
            let at = world.unit(*u).unwrap().position;
            let (dx, dy) = (
                i64::from(at.x) - i64::from(last[n].x),
                i64::from(at.y) - i64::from(last[n].y),
            );
            biggest_step = biggest_step.max(dx * dx + dy * dy);
            last[n] = at;
        }
        if world.building(id).unwrap().is_complete() {
            break;
        }
    }
    assert!(world.building(id).unwrap().is_complete());
    assert!(
        biggest_step < 512 * 512,
        "villagers walk off (ordinary movement), they are not teleported"
    );
    for u in idle {
        let tile = world.unit(u).unwrap().position.tile_floor();
        assert!(!footprint.contains(tile), "{u:?} still at {tile:?}");
    }
    assert_eq!(inside(&world, footprint), vec![]);
}

#[test]
fn an_enemy_on_a_foundation_stalls_construction_until_it_leaves() {
    let (mut world, _, _) = village();
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
    let builder = villager_at(&mut world, 98, 100);
    let enemy = world
        .spawn_villager(
            PlayerId(1),
            WorldPosition::from_tile_center(at(100, 100)).unwrap(),
        )
        .unwrap();
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    advance(&mut world, 700);
    assert!(!world.building(id).unwrap().is_complete());
    assert!(world.unit(enemy).is_some());
    world.remove_unit(enemy);
    finish(&mut world, id);
}

#[test]
fn a_builder_pushed_onto_its_foundation_walks_off_and_finishes_the_job() {
    let (mut world, _, _) = village();
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
    let builder = villager_at(&mut world, 98, 100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::Mill,
            at(100, 100),
            false,
        )
        .unwrap();
    let footprint = world.building(id).unwrap().footprint;
    advance(&mut world, 60);
    let index = world.lookup[&builder];
    let from = world.units[index].state.position.tile_floor();
    world.units[index].state.position =
        WorldPosition::from_tile_center(at(footprint.min.x, footprint.min.y)).unwrap();
    let state = world.units[index].state;
    world.track_unit_step(builder, from, &state);
    finish(&mut world, id);
    assert_eq!(inside(&world, footprint), vec![]);
}

#[test]
fn a_corpse_on_a_foundation_stays_dying_and_does_not_stall_the_house() {
    // Owner playtest 2026-10-30: a House placed over a fallen villager slid the
    // corpse off as an Idle unit with no hp, breaking the wire's dying rule.
    let (mut world, _, _) = village();
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
    let builder = villager_at(&mut world, 98, 100);
    let fallen = villager_at(&mut world, 100, 100);
    world.kill_unit(world.lookup[&fallen]);
    let corpse_at = world.unit(fallen).unwrap().position;
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    for _ in 0..1500 {
        world.advance();
        if let Some(u) = world.unit(fallen) {
            assert_eq!(
                u.task,
                crate::UnitTask::Dying,
                "a corpse is never re-tasked"
            );
            assert_eq!(u.position, corpse_at, "a corpse is never moved");
        }
    }
    assert!(
        world.building(id).unwrap().is_complete(),
        "a corpse does not occupy the foundation"
    );
}
