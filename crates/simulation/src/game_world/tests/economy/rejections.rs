//! Rejected orders leave the unit's current task and the world untouched.
use super::{buildings::*, farming::mill_and_farm, *};

fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

/// A builder at (150, 150) on its way to a house, then walled in on all sides.
fn sealed_builder(world: &mut GameWorld) -> (EntityId, EntityId) {
    let builder = villager_at(world, 150, 150);
    let house = world
        .issue_build(PlayerId(0), &[builder], BuildingKind::House, at(152, 150))
        .unwrap();
    for (dx, dy) in [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ] {
        world.economy.blocked_tiles.insert(at(150 + dx, 150 + dy));
    }
    world.resource_collision_changed();
    (builder, house)
}

#[test]
fn a_rejected_farm_order_keeps_the_builders_current_task() {
    let (mut world, worker, _, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    world.stop_gathering(worker);
    let (builder, house) = sealed_builder(&mut world);
    assert!(world.issue_farm(PlayerId(0), builder, farm).is_err());
    assert_eq!(
        world
            .economy
            .builders
            .get(&builder)
            .map(|task| task.building),
        Some(house),
        "the refused order must not cancel the construction job"
    );
}

#[test]
fn a_rejected_build_order_keeps_the_builders_current_task() {
    let (mut world, _, _) = village();
    let (builder, house) = sealed_builder(&mut world);
    let wood = stock(&world).1;
    let count = world.buildings().count();
    assert!(
        world
            .issue_build(PlayerId(0), &[builder], BuildingKind::House, at(160, 160))
            .is_err()
    );
    assert_eq!(
        world
            .economy
            .builders
            .get(&builder)
            .map(|task| task.building),
        Some(house)
    );
    assert_eq!((stock(&world).1, world.buildings().count()), (wood, count));
}
