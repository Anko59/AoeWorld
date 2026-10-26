use super::*;
use aoe_core::Seed;

#[test]
fn reachable_tiles_stops_at_the_requested_bound() {
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    assert_eq!(
        Terrain::uniform(1).reachable_tiles(TileCoord::new(32, 32), config, 256),
        256
    );
}

#[test]
fn start_selection_uses_center_distance_then_canonical_tile_order() {
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    assert_eq!(
        Terrain::uniform(1).starting_tile(config),
        Some(TileCoord::new(31, 31))
    );
}

#[test]
fn uniform_terrain_bounds_material_queries_and_chunk_rows() {
    let config = WorldConfig::new(40, 32, Seed(3)).expect("config");
    let grass = UniformGrass::new(9);
    assert_eq!(grass.material_at(TileCoord::new(-1, 0), config), None);
    assert_eq!(grass.material_at(TileCoord::new(0, -1), config), None);
    assert_eq!(grass.material_at(TileCoord::new(40, 0), config), None);
    assert_eq!(grass.material_at(TileCoord::new(0, 32), config), None);
    let material = grass
        .material_at(TileCoord::new(7, 5), config)
        .expect("material");
    assert_eq!(
        material,
        grass
            .material_at(TileCoord::new(7, 5), config)
            .expect("stable")
    );

    let terrain = Terrain::uniform(9);
    assert!(!terrain.has_map_navigation());
    assert!(terrain.passable(TileCoord::new(0, 0), config));
    assert!(!terrain.passable(TileCoord::new(-1, 0), config));
    assert!(terrain.crossable(TileCoord::new(0, 0), TileCoord::new(1, 1), config));
    assert_eq!(
        terrain.route(TileCoord::new(0, 0), TileCoord::new(1, 1)),
        None
    );
    assert_eq!(
        terrain.route_outcome(TileCoord::new(0, 0), TileCoord::new(1, 1)),
        None
    );
    assert_eq!(
        terrain.route_segment(TileCoord::new(0, 0), TileCoord::new(1, 1)),
        None
    );
    assert!(
        terrain
            .route_planner(TileCoord::new(0, 0), TileCoord::new(1, 1), 8)
            .is_none()
    );
    let mut planner = RoutePlanner::new(TileCoord::new(0, 0), TileCoord::new(1, 1), 8);
    assert!(
        terrain
            .poll_route_planner(&mut planner, 8, &|| false)
            .is_none()
    );

    let passability = terrain
        .chunk_passability_with_cancel(0, 0, config, &|| false)
        .expect("uniform chunk");
    assert!(passability.iter().all(|passable| *passable));
    let outside = terrain
        .chunk_passability_with_cancel(-1, 0, config, &|| false)
        .expect("outside chunk");
    assert!(outside.iter().all(|passable| !*passable));
}

#[test]
fn map_terrain_reports_outside_tiles_and_empty_edge_chunks() {
    let package =
        aoe_map::MapPackage::new(1, aoe_map::MapRequest::default(), Vec::new()).expect("package");
    let config = package.request.estimate().expect("estimate").tiles_per_side;
    let width = i32::try_from(config).expect("bounded width");
    let world_config = WorldConfig::new(width, width, Seed(1)).expect("config");
    let terrain = Terrain::from_package(&package);
    assert!(!terrain.passable(TileCoord::new(-1, 0), world_config,));
    assert_eq!(
        terrain.chunk_passability_with_cancel(
            (width + CHUNK_TILES - 1) / CHUNK_TILES,
            0,
            world_config,
            &|| false,
        ),
        Ok(vec![false; (CHUNK_TILES * CHUNK_TILES) as usize])
    );
}
