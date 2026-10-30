//! Units never stay locked inside a finished building's footprint.
use super::*;

/// Units whose body (centre or radius) touches `footprint`.
fn inside(world: &GameWorld, footprint: TileRect) -> Vec<(EntityId, WorldPosition)> {
    world
        .units()
        .filter(|u| {
            let r = i64::from(u.kind.collision_radius());
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
