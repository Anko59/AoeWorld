use super::*;

fn body_inside(footprint: aoe_core::TileRect, at: WorldPosition) -> bool {
    let radius = UnitKind::Villager.collision_radius();
    let tile = aoe_core::FIXED_SUBUNITS_PER_TILE;
    at.x - radius >= footprint.min.x * tile
        && at.x + radius <= footprint.max.x * tile
        && at.y - radius >= footprint.min.y * tile
        && at.y + radius <= footprint.max.y * tile
}

#[test]
fn farm_contact_finishes_approach_before_the_reserved_point() {
    let (mut world, farmer, _, farm) = mill_and_farm();
    finish(&mut world, farm, 700);
    let footprint = world.building(farm).unwrap().footprint;
    let outside =
        WorldPosition::from_tile_center(TileCoord::new(footprint.max.x + 8, footprint.min.y + 1))
            .unwrap();
    world.issue_move(farmer, outside).unwrap();
    for _ in 0..800 {
        world.advance();
        if world.movement_order(farmer).is_none() {
            break;
        }
    }
    assert_eq!(world.unit(farmer).unwrap().position, outside);
    assert!(!footprint.contains(outside.tile_floor()));
    world.issue_farm(PlayerId(0), farmer, farm).unwrap();

    // Observe entry between ordinary movement and gathering phases. Crop
    // centre entry alone must not strand the farmer's body across the
    // next plot's entrance. It must keep approaching until fully inside.
    let mut entered = false;
    for _ in 0..800 {
        let _ = world.advance_movement();
        let unit = world.unit(farmer).unwrap();
        if footprint.contains(unit.position.tile_floor()) {
            let order = world
                .movement_order(farmer)
                .expect("first crop entry must precede the reserved approach point");
            assert_ne!(unit.position, order.destination);
            entered = true;
            break;
        }
    }
    assert!(entered, "normal approach must enter the farm footprint");
    assert!(!body_inside(
        footprint,
        world.unit(farmer).unwrap().position
    ));
    world.advance();
    let unit = world.unit(farmer).unwrap();
    assert!(footprint.contains(unit.position.tile_floor()));
    assert!(!body_inside(footprint, unit.position));
    assert_eq!(
        unit.task,
        UnitTask::Moving,
        "a straddling body must keep approaching"
    );
    assert!(world.movement_order(farmer).is_some());

    let mut whole_body_entered = false;
    for _ in 0..800 {
        let _ = world.advance_movement();
        let unit = world.unit(farmer).unwrap();
        if body_inside(footprint, unit.position) {
            assert!(footprint.contains(unit.position.tile_floor()));
            let order = world
                .movement_order(farmer)
                .expect("whole-body entry must precede the reserved approach point");
            assert_ne!(unit.position, order.destination);
            whole_body_entered = true;
            break;
        }
    }
    assert!(
        whole_body_entered,
        "the farmer must walk fully inside its plot"
    );
    world.advance();
    let unit = world.unit(farmer).unwrap();
    assert!(footprint.contains(unit.position.tile_floor()));
    assert!(body_inside(footprint, unit.position));
    assert_eq!(unit.task, UnitTask::Farming);
    assert_eq!(world.gathering_kind(farmer), Some(ResourceKind::Food));
    assert!(world.movement_order(farmer).is_none());
    assert!(!unit.moving && !unit.planning);
}
