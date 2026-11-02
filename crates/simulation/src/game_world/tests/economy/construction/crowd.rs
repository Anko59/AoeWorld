//! Many villagers sent to one small foundation must all get to work.
use super::*;

/// Builders start crowded on the footprint's edge, on it, and on one tile.
fn crowd(world: &mut GameWorld, count: i32) -> Vec<EntityId> {
    (0..count)
        .map(|i| match i % 3 {
            0 => villager_at(world, 100 + i % 2, 100 + (i / 2) % 2),
            1 => villager_at(world, 99, 100 + i % 2),
            _ => villager_at(world, 98, 99),
        })
        .collect()
}

#[test]
fn a_crowd_ordered_onto_one_house_starts_and_finishes_it() {
    for count in [8, 10, 12] {
        let (mut world, _, _) = village();
        fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
        let builders = crowd(&mut world, count);
        let id = world
            .issue_build(
                PlayerId(0),
                &builders,
                BuildingKind::House,
                at(100, 100),
                false,
            )
            .unwrap();
        let mut started = None;
        for tick in 1..=1200 {
            world.advance();
            if started.is_none() && world.building(id).unwrap().progress_permille() > 0 {
                started = Some(tick);
            }
            if world.building(id).unwrap().is_complete() {
                break;
            }
        }
        assert!(
            started.is_some_and(|t| t <= 200),
            "x{count}: first progress at {started:?}"
        );
        assert!(world.building(id).unwrap().is_complete(), "x{count}");
    }
}

#[test]
fn builders_all_starting_inside_the_footprint_still_build() {
    for count in [10, 12] {
        let (mut world, _, _) = village();
        fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
        let builders = (0..count)
            .map(|i| villager_at(&mut world, 100 + i % 2, 100 + (i / 2) % 2))
            .collect::<Vec<_>>();
        let id = world
            .issue_build(
                PlayerId(0),
                &builders,
                BuildingKind::House,
                at(100, 100),
                false,
            )
            .unwrap();
        let mut started = None;
        for tick in 1..=1200 {
            world.advance();
            if started.is_none() && world.building(id).unwrap().progress_permille() > 0 {
                started = Some(tick);
            }
        }
        assert!(started.is_some_and(|t| t <= 200), "x{count}: {started:?}");
        assert!(world.building(id).unwrap().is_complete(), "x{count}");
    }
}

/// A working builder crowded to within movement's edge clearance of the
/// footprint (not on it) must not stop the work for everyone.
#[test]
fn a_builder_pressed_against_the_edge_does_not_stall_the_house() {
    let (mut world, _, _) = village();
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
    let builders = (0..6)
        .map(|i| villager_at(&mut world, 97 + i % 3, 99 + i / 3))
        .collect::<Vec<_>>();
    let id = world
        .issue_build(
            PlayerId(0),
            &builders,
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    let footprint = world.building(id).unwrap().footprint;
    while world.building(id).unwrap().progress_permille() < 200 {
        world.advance();
    }
    let pushed = builders
        .iter()
        .copied()
        .find(|b| world.unit(*b).unwrap().position.x < footprint.min.x * 1024)
        .unwrap();
    let index = world.lookup[&pushed];
    let from = world.units[index].state.position.tile_floor();
    let y = world.units[index].state.position.y;
    world.units[index].state.position = WorldPosition::new(footprint.min.x * 1024 - 60, y);
    let state = world.units[index].state;
    world.track_unit_step(pushed, from, &state);
    let before = world.building(id).unwrap().progress_permille();
    for _ in 0..40 {
        world.advance();
    }
    let after = world.building(id).unwrap().progress_permille();
    assert!(after > before + 10, "{before} -> {after}");
}

/// Put `id` on the centre of `tile`, as a crowd shove would.
fn shove(world: &mut GameWorld, id: EntityId, tile: TileCoord) {
    let index = world.lookup[&id];
    let from = world.units[index].state.position.tile_floor();
    world.units[index].state.position = WorldPosition::from_tile_center(tile).unwrap();
    let state = world.units[index].state;
    world.track_unit_step(id, from, &state);
}

/// Advance until `id` is working on the foundation.
fn settle(world: &mut GameWorld, id: EntityId) {
    for _ in 0..100 {
        if world.unit(id).unwrap().task == UnitTask::Building {
            return;
        }
        world.advance();
    }
    panic!("builder never started work");
}

#[test]
fn a_builder_on_the_footprint_is_set_on_the_nearest_free_tile() {
    let (mut world, _, _) = village();
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
    let builder = villager_at(&mut world, 99, 100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    let footprint = world.building(id).unwrap().footprint;
    settle(&mut world, builder);
    shove(&mut world, builder, footprint.min);
    world.advance();
    let tile = world.unit(builder).unwrap().position.tile_floor();
    assert!(
        !footprint.contains(tile)
            && (footprint.min.x - 1..=footprint.max.x).contains(&tile.x)
            && (footprint.min.y - 1..=footprint.max.y).contains(&tile.y),
        "{tile:?} is not on the ring next to {footprint:?}"
    );
}

#[test]
fn a_builder_inside_a_finished_house_is_put_out_even_when_eight_rings_are_taken() {
    let (mut world, _, _) = village();
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 8);
    let stuck = villager_at(&mut world, 99, 101);
    let worker = villager_at(&mut world, 99, 100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[stuck, worker],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    let footprint = world.building(id).unwrap().footprint;
    // Everything within eight rings is taken except the worker's own spot.
    for y in footprint.min.y - 9..footprint.max.y + 9 {
        for x in footprint.min.x - 9..footprint.max.x + 9 {
            let dx = (footprint.min.x - x).max(x - footprint.max.x + 1).max(0);
            let dy = (footprint.min.y - y).max(y - footprint.max.y + 1).max(0);
            let tile = at(x, y);
            if (1..=8).contains(&dx.max(dy))
                && world.tile_free(tile)
                && tile != world.unit(worker).unwrap().position.tile_floor()
            {
                let at = WorldPosition::from_tile_center(tile).unwrap();
                world.spawn_villager(PlayerId(1), at).unwrap();
            }
        }
    }
    for _ in 0..600 {
        world.advance();
        if world.building(id).unwrap().progress_permille() >= 900 {
            break;
        }
    }
    assert!(world.building(id).unwrap().progress_permille() >= 900);
    settle(&mut world, stuck);
    let old = world.unit(stuck).unwrap().position;
    shove(&mut world, stuck, footprint.min);
    world.spawn_villager(PlayerId(1), old).unwrap();
    for _ in 0..600 {
        world.advance();
        if world.building(id).unwrap().is_complete() {
            break;
        }
    }
    assert!(world.building(id).unwrap().is_complete());
    let tile = world.unit(stuck).unwrap().position.tile_floor();
    assert!(!footprint.contains(tile), "builder left inside at {tile:?}");
}
