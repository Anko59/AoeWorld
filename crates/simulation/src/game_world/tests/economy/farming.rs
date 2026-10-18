//! Farms: they need a mill, hold 175 food, take one farmer and then vanish.
use super::{buildings::*, *};

fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

fn mill_and_farm() -> (GameWorld, EntityId, EntityId, EntityId) {
    let (mut world, _, _) = village();
    set_stock(&mut world, 0, 1000, 0, 0);
    let worker = villager_at(&mut world, 99, 100);
    let farm_site = at(103, 100);
    assert_eq!(
        world.issue_build(PlayerId(0), &[worker], BuildingKind::Farm, farm_site),
        Err(EconomyError::RequirementMissing)
    );
    let mill = world
        .issue_build(PlayerId(0), &[worker], BuildingKind::Mill, at(100, 100))
        .unwrap();
    // An unfinished mill does not satisfy the requirement.
    assert_eq!(
        world.issue_build(PlayerId(0), &[worker], BuildingKind::Farm, farm_site),
        Err(EconomyError::RequirementMissing)
    );
    finish(&mut world, mill, 800);
    let farm = world
        .issue_build(PlayerId(0), &[worker], BuildingKind::Farm, farm_site)
        .unwrap();
    (world, worker, mill, farm)
}

#[test]
fn a_farm_needs_a_completed_mill_and_holds_one_hundred_seventy_five_food() {
    let (mut world, worker, _, farm) = mill_and_farm();
    assert_eq!(world.building(farm).unwrap().food_remaining, Some(175));
    assert_eq!(stock(&world).1, 1000 - 100 - 60);
    assert_eq!(
        world.issue_farm(PlayerId(0), worker, farm),
        Err(EconomyError::UnderConstruction)
    );
    // 15 s = 300 ticks for one builder, who then works the farm.
    // 300 ticks of work after walking around the mill.
    finish(&mut world, farm, 700);
    advance(&mut world, 100);
    assert_eq!(world.unit(worker).unwrap().task, UnitTask::Farming);
    assert_eq!(world.gathering_kind(worker), Some(ResourceKind::Food));
    // Farms are walked on, so the farmer stands inside the footprint.
    let footprint = world.building(farm).unwrap().footprint;
    assert!(footprint.contains(world.unit(worker).unwrap().position.tile_floor()));
    assert!(world.tile_passable(footprint.min));
}

#[test]
fn a_farm_takes_one_farmer_depletes_and_is_removed_when_exhausted() {
    let (mut world, worker, mill, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    let second = villager_at(&mut world, 104, 104);
    assert_eq!(
        world.issue_farm(PlayerId(0), second, farm),
        Err(EconomyError::FarmUnavailable),
        "the builder already works it"
    );
    advance(&mut world, 20 * 60);
    assert!(stock(&world).0 > 0, "the farmer delivers to the mill");
    let remaining = world.building(farm).unwrap().food_remaining.unwrap();
    assert!(remaining < 175 && remaining > 0);
    let mut ticks = 0;
    while world.building(farm).is_some() {
        world.advance();
        ticks += 1;
        assert!(ticks < 20_000, "the farm must run dry");
    }
    // Every one of the 175 units reached the stockpile, and the farmer rests.
    assert_eq!(stock(&world).0, 175);
    assert_eq!(world.unit(worker).unwrap().task, UnitTask::Idle);
    assert!(world.unit(worker).unwrap().carried.is_none());
    assert!(world.building(mill).is_some());
    assert_eq!(
        world.issue_farm(PlayerId(0), second, farm),
        Err(EconomyError::UnknownEntity)
    );
}
