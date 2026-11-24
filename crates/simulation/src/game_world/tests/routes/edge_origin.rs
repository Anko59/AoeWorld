//! A body that stops over the edge of a tile no route can start from (the
//! water beside a bridge deck) plans from the passable tile it overlaps
//! (#1040); a body on passable ground plans from where it stands.
use super::*;
use crate::UnitKind;

#[test]
fn a_route_from_the_edge_of_water_starts_in_the_land_tile_it_overlaps() {
    let w = crate::game_world::terrain_fixtures::build("river").world;
    let passable = |tile: TileCoord| {
        w.unit_tile_passable(UnitKind::Villager, tile, &|| false)
            .unwrap_or(false)
    };
    let (land, water) = (0..w.config.height_tiles)
        .flat_map(|y| (0..w.config.width_tiles - 1).map(move |x| TileCoord::new(x, y)))
        .map(|land| (land, TileCoord::new(land.x + 1, land.y)))
        .find(|&(land, water)| passable(land) && !passable(water))
        .expect("fixture: land beside water");
    // Exactly on the shared edge: the position floors onto the water tile.
    let edge = WorldPosition::new(water.x * 1024, land.y * 1024 + 512);
    assert_eq!(edge.tile_floor(), water);
    let origin = w.route_origin(UnitKind::Villager, edge);
    assert_eq!(origin.tile_floor(), land);
    assert_eq!(origin, WorldPosition::new(water.x * 1024 - 1, edge.y));
    let inland = WorldPosition::new(land.x * 1024 + 512, edge.y);
    assert_eq!(w.route_origin(UnitKind::Villager, inland), inland);
}
