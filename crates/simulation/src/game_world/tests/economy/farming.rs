//! Farms: they need a mill, hold 175 food, take one farmer and then vanish.
use super::{buildings::*, *};
mod contact;

fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

pub(super) fn mill_and_farm() -> (GameWorld, EntityId, EntityId, EntityId) {
    let (mut world, _, _) = village();
    set_stock(&mut world, 0, 1000, 0, 0);
    let worker = villager_at(&mut world, 99, 100);
    let farm_site = at(103, 100);
    assert_eq!(
        world.issue_build(PlayerId(0), &[worker], BuildingKind::Farm, farm_site, false),
        Err(EconomyError::RequirementMissing)
    );
    let mill = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::Mill,
            at(100, 100),
            false,
        )
        .unwrap();
    // An unfinished mill does not satisfy the requirement.
    assert_eq!(
        world.issue_build(PlayerId(0), &[worker], BuildingKind::Farm, farm_site, false),
        Err(EconomyError::RequirementMissing)
    );
    finish(&mut world, mill, 800);
    fixture::survey(&mut world, PlayerId(0), at(104, 101), 2);
    let farm = world
        .issue_build(PlayerId(0), &[worker], BuildingKind::Farm, farm_site, false)
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
fn farm_builder_farms_instead_of_automatically_taking_a_nearby_foundation() {
    let (mut world, farmer, _, farm) = mill_and_farm();
    let helper = villager_at(&mut world, 107, 104);
    let house = world
        .issue_build(
            PlayerId(0),
            &[helper],
            BuildingKind::House,
            at(108, 104),
            false,
        )
        .unwrap();
    world
        .issue_move(
            helper,
            WorldPosition::from_tile_center(at(115, 110)).unwrap(),
        )
        .unwrap();
    finish(&mut world, farm, 700);
    advance(&mut world, 150);
    assert_eq!(world.unit(farmer).unwrap().task, UnitTask::Farming);
    assert!(!world.building(house).unwrap().is_complete());
    assert!(!world.economy.builders.contains_key(&farmer));
    assert!(
        world
            .building(farm)
            .unwrap()
            .footprint
            .contains(world.unit(farmer).unwrap().position.tile_floor())
    );
}

#[test]
fn a_farm_builder_waits_for_a_work_tile_instead_of_losing_its_farm_job() {
    let (mut world, farmer, _, farm) = mill_and_farm();
    let mut blockers = Vec::new();
    let rect = world.building(farm).unwrap().footprint;
    for point in crate::game_world::movement_step::slots::perimeter_positions(
        aoe_core::TileRect::from_xywh(rect.min.x + 1, rect.min.y + 1, 1, 1),
        UnitKind::Villager,
    ) {
        blockers.push(world.spawn_villager(PlayerId(1), point).unwrap());
    }
    finish(&mut world, farm, 700);
    assert_eq!(world.gather_target(farmer), None);
    assert!(world.economy.builders.contains_key(&farmer));
    for id in blockers {
        world.remove_unit(id);
    }
    advance(&mut world, 200);
    assert_eq!(world.unit(farmer).unwrap().task, UnitTask::Farming);
    assert_eq!(world.gathering_kind(farmer), Some(ResourceKind::Food));
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
    assert!(
        world.economy.farm_production.is_empty(),
        "spent farm production must be released"
    );
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
            .issue_build(
                PlayerId(0),
                &[worker],
                BuildingKind::Mill,
                at(100, y),
                false,
            )
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
        .issue_build(
            PlayerId(0),
            &[helper],
            BuildingKind::Mill,
            at(108, 100),
            false,
        )
        .unwrap();
    finish(&mut world, far, 800);
    let footprint = world.building(mill).unwrap().footprint;
    // Perimeter contact can deliver a load while the second Mill is built.
    // Observe a new deposit, rather than food from that already finished trip.
    let food_before = stock(&world).0;
    let mut ticks = 0;
    while stock(&world).0 == food_before {
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

#[test]
fn mill_reseed_queue_charges_refunds_and_validates_authority() {
    let (mut world, _, mill, farm) = mill_and_farm();
    let wood = stock(&world).1;
    assert_eq!(
        world.issue_farm_reseed(PlayerId(1), mill, true),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        world.issue_farm_reseed(PlayerId(0), farm, true),
        Err(EconomyError::FarmUnavailable)
    );
    world.issue_farm_reseed(PlayerId(0), mill, true).unwrap();
    assert_eq!(world.building(mill).unwrap().farm_reseeds, 1);
    assert_eq!(stock(&world).1, wood - 60);
    world.issue_farm_reseed(PlayerId(0), mill, false).unwrap();
    assert_eq!(world.building(mill).unwrap().farm_reseeds, 0);
    assert_eq!(stock(&world).1, wood);
    assert!(world.issue_farm_reseed(PlayerId(0), mill, false).is_err());
    set_stock(&mut world, 0, 59, 0, 0);
    assert_eq!(
        world.issue_farm_reseed(PlayerId(0), mill, true),
        Err(EconomyError::InsufficientResources)
    );
}

#[test]
fn queued_reseed_rebuilds_once_and_keeps_the_same_farmer() {
    let (mut world, worker, mill, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    world.issue_farm_reseed(PlayerId(0), mill, true).unwrap();
    let wood = stock(&world).1;
    world
        .economy
        .buildings
        .get_mut(&farm)
        .unwrap()
        .food_remaining = Some(1);
    let mut rebuilding = false;
    for _ in 0..3000 {
        world.advance();
        let b = world
            .building(farm)
            .expect("queued reseed retains the farm");
        rebuilding |= !b.is_complete();
        if rebuilding && b.is_complete() && world.unit(worker).unwrap().task == UnitTask::Farming {
            assert_eq!(world.building(mill).unwrap().farm_reseeds, 0);
            assert_eq!(stock(&world).1, wood, "wood was paid when queued");
            assert_eq!(stock(&world).0, 1, "last load delivered before rebuilding");
            assert!(b.food_remaining.unwrap() > 160);
            return;
        }
    }
    panic!("same farmer must rebuild and resume without another order");
}

#[test]
fn prepaid_reseeding_keeps_the_farmer_across_three_full_cycles_near_other_work() {
    let (mut world, farmer, mill, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    for _ in 0..3 {
        world.issue_farm_reseed(PlayerId(0), mill, true).unwrap();
    }
    let paid_wood = stock(&world).1;
    let house = world
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(108, 104, 2, 2),
            false,
        )
        .unwrap();
    let mut cycles = 0;
    let mut rebuilding = false;
    for _ in 0..65_000 {
        world.advance();
        let b = world
            .building(farm)
            .expect("a prepaid farm must stay available");
        rebuilding |= !b.is_complete();
        if rebuilding && world.unit(farmer).unwrap().task == UnitTask::Farming {
            cycles += 1;
            rebuilding = false;
            assert_eq!(world.building(mill).unwrap().farm_reseeds, 3 - cycles);
            assert_eq!(stock(&world).0, 175 * u32::from(cycles));
            assert_eq!(stock(&world).1, paid_wood);
            assert_eq!(world.building(house).unwrap().work_done, 0);
            if cycles == 3 {
                return;
            }
        }
    }
    panic!(
        "reseeding stopped after {cycles} cycles: {:?}",
        world.unit(farmer)
    );
}

#[test]
fn reseeds_are_bounded_and_an_unfinished_mill_cannot_take_payment() {
    let (mut world, worker, mill, _) = mill_and_farm();
    set_stock(&mut world, 0, 10_000, 0, 0);
    for _ in 0..99 {
        world.issue_farm_reseed(PlayerId(0), mill, true).unwrap();
    }
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_farm_reseed(PlayerId(0), mill, true),
        Err(EconomyError::Capacity)
    );
    assert_eq!(
        world.canonical_hash(),
        before,
        "full queue must not charge again"
    );
    fixture::survey(&mut world, PlayerId(0), at(112, 100), 4);
    let unfinished = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::Mill,
            at(111, 99),
            false,
        )
        .unwrap();
    let before = stock(&world).1;
    assert_eq!(
        world.issue_farm_reseed(PlayerId(0), unfinished, true),
        Err(EconomyError::UnderConstruction)
    );
    assert_eq!(stock(&world).1, before);
}

#[test]
fn destroying_a_mill_loses_its_prepaid_reseeds() {
    let (mut world, worker, mill, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    world.issue_farm_reseed(PlayerId(0), mill, true).unwrap();
    let wood = stock(&world).1;
    world.remove_building(mill);
    assert_eq!(stock(&world).1, wood);
    world
        .economy
        .buildings
        .get_mut(&farm)
        .unwrap()
        .food_remaining = Some(1);
    for _ in 0..6000 {
        world.advance();
        if world.building(farm).is_none() {
            assert_eq!(world.unit(worker).unwrap().task, UnitTask::Idle);
            return;
        }
    }
    panic!("destroyed queue must not rebuild the spent farm");
}

#[test]
fn prepaid_reseed_counts_are_part_of_authoritative_replay_state() {
    let (mut world, _, mill, _) = mill_and_farm();
    let before = world.canonical_hash();
    // Isolate queue state from its wood payment: both must be replay inputs.
    world.economy.buildings.get_mut(&mill).unwrap().farm_reseeds = 1;
    assert_ne!(world.canonical_hash(), before);
}
