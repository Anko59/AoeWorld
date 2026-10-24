//! Farms: they need a mill, hold 175 food, take one farmer and then vanish.
use super::{buildings::*, *};

fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

pub(super) fn mill_and_farm() -> (GameWorld, EntityId, EntityId, EntityId) {
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
    fixture::survey(&mut world, PlayerId(0), at(104, 101), 2);
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

#[test]
fn a_spent_farm_is_removed_when_its_farmer_is_ordered_away_before_delivering() {
    let (mut world, worker, _, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    world
        .economy
        .buildings
        .get_mut(&farm)
        .unwrap()
        .food_remaining = Some(1);
    let mut guard = 0;
    while world.building(farm).unwrap().food_remaining != Some(0) {
        world.advance();
        guard += 1;
        assert!(guard < 2000, "the farmer should take the last unit");
    }
    // The last food is on the farmer, who has not delivered it yet.
    assert_eq!(
        world.unit(worker).unwrap().carried.map(|l| l.amount),
        Some(1)
    );
    world
        .issue_move(
            worker,
            WorldPosition::from_tile_center(at(110, 110)).unwrap(),
        )
        .unwrap();
    assert!(
        world.building(farm).is_none(),
        "a farm with no food left must not linger once nobody works it"
    );
    assert_eq!(
        world.unit(worker).unwrap().carried.map(|l| l.amount),
        Some(1)
    );
}

#[test]
fn equidistant_dropoffs_break_ties_toward_the_lower_id() {
    let (mut world, _, _) = village();
    set_stock(&mut world, 0, 1000, 0, 0);
    let mut mills = Vec::new();
    for y in [100, 109] {
        let worker = villager_at(&mut world, 99, y);
        let id = world
            .issue_build(PlayerId(0), &[worker], BuildingKind::Mill, at(100, y))
            .unwrap();
        finish(&mut world, id, 800);
        mills.push(id);
    }
    // (99, 105) is three tiles from each mill's ring.
    let nearest = world.nearest_dropoff(PlayerId(0), at(99, 105), ResourceKind::Food);
    assert_eq!(nearest, Some(mills[0].min(mills[1])));
}

#[test]
fn a_farmers_return_trip_goes_to_the_dropoff_the_shared_rule_names() {
    let (mut world, worker, mill, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    // A second, farther mill: the shared rule must still pick the first.
    set_stock(&mut world, 0, 1000, 0, 0);
    let helper = villager_at(&mut world, 107, 100);
    let far = world
        .issue_build(PlayerId(0), &[helper], BuildingKind::Mill, at(108, 100))
        .unwrap();
    finish(&mut world, far, 800);
    let footprint = world.building(mill).unwrap().footprint;
    let mut ticks = 0;
    while stock(&world).0 == 0 {
        world.advance();
        ticks += 1;
        assert!(ticks < 6000, "the first load should be delivered");
    }
    let tile = world.unit(worker).unwrap().position.tile_floor();
    let expected = world
        .nearest_dropoff(PlayerId(0), tile, ResourceKind::Food)
        .unwrap();
    assert_eq!(expected, mill);
    assert!(
        tile.x >= footprint.min.x - 1
            && tile.x <= footprint.max.x
            && tile.y >= footprint.min.y - 1
            && tile.y <= footprint.max.y,
        "delivered at {tile:?}, not beside the nearest mill"
    );
}

#[test]
fn attacking_cancels_farming_and_removes_a_spent_farm() {
    let (mut world, worker, _, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    world
        .economy
        .buildings
        .get_mut(&farm)
        .unwrap()
        .food_remaining = Some(1);
    for _ in 0..2000 {
        if world.building(farm).unwrap().food_remaining == Some(0) {
            break;
        }
        world.advance();
    }
    assert_eq!(world.building(farm).unwrap().food_remaining, Some(0));
    let enemy = world
        .spawn_militia(PlayerId(1), world.unit(worker).unwrap().position)
        .unwrap();
    world.issue_attack(PlayerId(0), worker, enemy).unwrap();
    assert!(world.building(farm).is_none());
    assert_eq!(world.gathering_kind(worker), None);
    assert_eq!(
        world.unit(worker).unwrap().carried.map(|load| load.amount),
        Some(1)
    );
    assert_eq!(world.unit(worker).unwrap().task, UnitTask::Attacking);
}
