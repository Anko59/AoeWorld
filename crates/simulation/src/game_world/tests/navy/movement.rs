use super::*;

#[test]
fn every_naval_kind_moves_on_water_and_rejects_land() {
    for kind in [
        UnitKind::Galley,
        UnitKind::FireShip,
        UnitKind::DemolitionShip,
        UnitKind::TransportShip,
        UnitKind::WarGalley,
    ] {
        let mut world = coast();
        let (land, water) = bank(&world);
        assert!(world.spawn_kind(PlayerId(0), kind, at(land)).is_err());
        let id = ship(&mut world, kind, water, 0);
        assert_eq!(
            world.issue_move(id, at(land)),
            Err(GameWorldError::InvalidPosition)
        );
        let destination = TileCoord::new(water.x + 8, water.y);
        world.issue_move(id, at(destination)).unwrap();
        for _ in 0..400 {
            world.advance();
            let unit = world.unit(id).unwrap();
            assert!(
                world
                    .unit_tile_passable(UnitKind::FishingShip, unit.position.tile_floor(), &|| {
                        false
                    })
                    .unwrap()
            );
            if !unit.moving && !unit.planning {
                break;
            }
        }
        assert_eq!(
            world.unit(id).unwrap().position,
            at(destination),
            "{kind:?}"
        );
    }
}

#[test]
fn land_armies_cannot_cross_water_without_loading_a_transport() {
    let mut world = coast();
    let (land, water) = bank(&world);
    let soldier = world.spawn_militia(PlayerId(0), at(land)).unwrap();
    assert_eq!(
        world.issue_move(soldier, at(water)),
        Err(GameWorldError::InvalidPosition)
    );
    assert_eq!(world.unit(soldier).unwrap().position, at(land));
}

#[test]
fn ships_path_through_shallow_coastal_water() {
    let mut world = coast_coverage(40);
    let (land, water) = bank(&world);
    let boat = ship(&mut world, UnitKind::Galley, water, 0);
    let destination = TileCoord::new(water.x + 8, water.y);
    let Terrain::Map { generator, .. } = world.terrain() else {
        unreachable!()
    };
    assert_eq!(
        generator.tile_at(destination).unwrap().water,
        aoe_map::WaterKind::Shallow
    );
    assert!(world.spawn_militia(PlayerId(0), at(destination)).is_err());
    world.issue_move(boat, at(destination)).unwrap();
    advance(&mut world, 400);
    assert_eq!(world.unit(boat).unwrap().position, at(destination));
    assert!(world.tile_passable(land));
}
