use super::*;
use aoe_map::MapChunkGenerator;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn resident_chunk_measurement_counts_the_struct_and_owned_buffers() {
    let Ok(chunk) = MapChunkGenerator::new([0; 32], 1, 32).chunk(0, 0) else {
        assert!(false, "fixture chunk");
        return;
    };
    assert_eq!(
        chunk_resident_bytes(&chunk),
        size_of::<Chunk>()
            + chunk.tiles.capacity() * size_of::<Tile>()
            + chunk.resources.capacity() * size_of::<ResourceNode>()
    );
}

#[wasm_bindgen_test]
fn decoded_cache_limit_matches_the_product_budget() {
    assert_eq!(MAX_CACHED_CHUNKS, 512);
    assert_eq!(MAX_CACHED_CHUNK_BYTES, 128 * 1024 * 1024);
}

#[wasm_bindgen_test]
fn partial_edge_chunk_uses_active_map_dimensions_for_rows() {
    let Ok(chunk) = MapChunkGenerator::new([0; 32], 1, 500).chunk(15, 15) else {
        assert!(false, "partial chunk fixture");
        return;
    };
    assert_eq!(chunk.tiles.len(), 20 * 20);
    assert_eq!(chunk_tile_index(500, 500, &chunk, 480, 480), Some(0));
    assert_eq!(chunk_tile_index(500, 500, &chunk, 499, 499), Some(399));
    assert_eq!(chunk_tile_index(500, 500, &chunk, 500, 499), None);
}

#[wasm_bindgen_test]
fn unit_and_resource_contacts_follow_the_rendered_surface_diagonal() {
    let Ok(chunk) = MapChunkGenerator::new([0; 32], 1, 32).chunk(0, 0) else {
        assert!(false, "surface fixture chunk");
        return;
    };
    let mut tile = chunk.tiles[0];
    tile.game_height_level = 0;
    tile.surface.corner_game_height_levels = [0, 2, 4, 6];
    tile.surface.triangulation = aoe_map::SurfaceDiagonal::NorthwestSoutheast;
    assert_ne!(
        f64::from(tile.game_height_level),
        tile_center_elevation(&tile)
    );
    assert_eq!(tile_center_elevation(&tile), 2.0);
    assert_eq!(surface_elevation(&tile, 0.75, 0.25), 2.0);
    tile.surface.triangulation = aoe_map::SurfaceDiagonal::NortheastSouthwest;
    assert_eq!(tile_center_elevation(&tile), 4.0);
    assert_ne!(
        f64::from(tile.game_height_level),
        tile_center_elevation(&tile)
    );
    assert_eq!(surface_elevation(&tile, 0.25, 0.25), 2.0);
}

#[wasm_bindgen_test]
fn resident_surface_bounds_expand_terrain_visibility_to_high_relief() {
    let Ok(config) = aoe_core::WorldConfig::new(512, 512, aoe_core::Seed(1)) else {
        assert!(false, "valid visibility world");
        return;
    };
    let camera = Camera {
        center: [256.0, 256.0],
        zoom: 1.0,
        viewport: [4_096.0, 1_024.0],
        focus_elevation_meters: 0.0,
    };
    let ground = visible_tiles_for_height_bounds(camera, config, 0.0, None);
    let high_relief = visible_tiles_for_height_bounds(camera, config, 0.0, Some((-80, 80)));
    assert!(high_relief.width() > ground.width());
    assert!(high_relief.height() > ground.height());

    let Ok(mut chunk) = MapChunkGenerator::new([0; 32], 1, 32).chunk(0, 0) else {
        assert!(false, "fixture chunk");
        return;
    };
    for tile in &mut chunk.tiles {
        tile.surface.corner_game_height_levels = [0, 0, 80, -80];
    }
    assert_eq!(heights::chunk_height_bounds(&chunk), Some((-80, 80)));
}

#[wasm_bindgen_test]
fn initial_terrain_requests_cover_an_unseen_forty_level_plateau() {
    let Ok(config) = aoe_core::WorldConfig::new(512, 512, aoe_core::Seed(1)) else {
        assert!(false, "valid visibility world");
        return;
    };
    let camera = Camera {
        center: [256.0, 256.0],
        zoom: 1.0,
        viewport: [512.0, 256.0],
        focus_elevation_meters: 0.0,
    };
    let initial = visible_tiles_for_height_bounds(camera, config, 0.0, None);
    let high_plateau = camera.visible_tiles_at_height(config, 8.0, 40.0);
    assert!(initial.min.x <= high_plateau.min.x);
    assert!(initial.min.y <= high_plateau.min.y);
    assert!(initial.max.x >= high_plateau.max.x);
    assert!(initial.max.y >= high_plateau.max.y);
}

#[wasm_bindgen_test]
fn frontier_discovers_unseen_high_relief_that_expands_the_authoritative_bound() {
    let Ok(config) = aoe_core::WorldConfig::new(512, 512, aoe_core::Seed(1)) else {
        assert!(false, "valid visibility world");
        return;
    };
    let camera = Camera {
        center: [256.0, 256.0],
        zoom: 1.0,
        viewport: [512.0, 256.0],
        focus_elevation_meters: 0.0,
    };
    let high_tile = (290, 274);
    let initial = visible_tiles_for_height_bounds(camera, config, 0.0, None);
    assert!(
        high_tile.0 < initial.min.x
            || high_tile.0 >= initial.max.x
            || high_tile.1 < initial.min.y
            || high_tile.1 >= initial.max.y
    );

    let Ok(mut chunk) = MapChunkGenerator::new([0; 32], 1, 512).chunk(9, 8) else {
        assert!(false, "high-relief discovery chunk");
        return;
    };
    for tile in &mut chunk.tiles {
        tile.game_height_level = 0;
        tile.surface.corner_game_height_levels = [0; 4];
    }
    let evidence = &mut chunk.tiles[514];
    evidence.game_height_level = 52;
    evidence.surface.corner_game_height_levels = [52; 4];
    assert_eq!(heights::chunk_height_bounds(&chunk), Some((0, 52)));

    let budget = 1 + heights::discovery_ring([8, 8], [16, 16], 1).len();
    let discovered = heights::frontier_chunks([8, 8], [16, 16], budget, |_| false);
    assert!(discovered.contains(&(9, 8)));
    let expanded = visible_tiles_for_height_bounds(camera, config, 0.0, Some((0, 52)));
    assert!(
        high_tile.0 >= expanded.min.x
            && high_tile.0 < expanded.max.x
            && high_tile.1 >= expanded.min.y
            && high_tile.1 < expanded.max.y,
        "expanded {expanded:?} does not contain {high_tile:?}"
    );
}

#[wasm_bindgen_test]
fn rectangular_frontier_converges_over_every_chunk_without_a_height_margin() {
    let center = [8, 5];
    let extent = [17, 11];
    let expected = usize::try_from(extent[0] * extent[1]).unwrap_or(0);
    let mut discovered = std::collections::BTreeSet::new();
    let mut rounds = 0_usize;
    while discovered.len() < expected {
        let batch = heights::frontier_chunks(center, extent, 23, |coordinate| {
            discovered.contains(&coordinate)
        });
        assert!(!batch.is_empty(), "frontier stalled after {rounds} rounds");
        discovered.extend(batch);
        rounds += 1;
        assert!(rounds <= expected, "frontier revisited chunks");
    }
    assert_eq!(discovered.len(), expected);
    assert!(discovered.contains(&(0, 0)) && discovered.contains(&(extent[0] - 1, extent[1] - 1)));
}

#[wasm_bindgen_test]
fn fetch_evict_shrink_then_re_requests_high_relief_without_a_height_margin() {
    let Ok(config) = aoe_core::WorldConfig::new(512, 512, aoe_core::Seed(1)) else {
        assert!(false, "valid visibility world");
        return;
    };
    let camera = Camera {
        center: [256.0, 256.0],
        zoom: 1.0,
        viewport: [512.0, 256.0],
        focus_elevation_meters: 0.0,
    };
    let high_tile = (290, 274);
    let Ok(mut high) = MapChunkGenerator::new([0; 32], 1, 512).chunk(9, 8) else {
        assert!(false, "high-relief fetch chunk");
        return;
    };
    for tile in &mut high.tiles {
        tile.game_height_level = 0;
        tile.surface.corner_game_height_levels = [0; 4];
    }
    high.tiles[514].game_height_level = 52;
    high.tiles[514].surface.corner_game_height_levels = [52; 4];

    let Ok(mut low) = MapChunkGenerator::new([0; 32], 1, 512).chunk(8, 8) else {
        assert!(false, "low-relief fetch chunk");
        return;
    };
    for tile in &mut low.tiles {
        tile.game_height_level = 0;
        tile.surface.corner_game_height_levels = [0; 4];
    }

    let mut chunks = std::collections::BTreeMap::from([((8, 8), low), ((9, 8), high.clone())]);
    let mut request_bounds = None;
    let mut resident_bounds = None;
    let mut discovered = std::collections::BTreeSet::new();
    for coordinate in [(8, 8), (9, 8)] {
        let Some(chunk) = chunks.get(&coordinate) else {
            assert!(false, "fetched chunk is resident");
            return;
        };
        heights::merge_chunk_height_bounds(&mut request_bounds, chunk);
        heights::merge_chunk_height_bounds(&mut resident_bounds, chunk);
        discovered.insert(coordinate);
    }
    assert_eq!(request_bounds, Some((0, 52)));
    assert_eq!(resident_bounds, Some((0, 52)));

    let initial = visible_tiles_for_height_bounds(camera, config, 0.0, None);
    assert!(
        high_tile.0 < initial.min.x
            || high_tile.0 >= initial.max.x
            || high_tile.1 < initial.min.y
            || high_tile.1 >= initial.max.y
    );

    // The production eviction policy drops resident evidence and discovery
    // eligibility while preserving the monotonic request bound.
    let (removed, refreshed) = evict_distant_chunks_with_limits(
        &mut chunks,
        &mut discovered,
        camera,
        config,
        1,
        MAX_CACHED_CHUNK_BYTES,
    );
    assert!(removed);
    assert!(!discovered.contains(&(9, 8)));
    resident_bounds = refreshed;
    assert_eq!(resident_bounds, Some((0, 0)));
    assert_eq!(request_bounds, Some((0, 52)));
    let requested = visible_tiles_for_height_bounds(camera, config, 0.0, request_bounds);
    assert!(
        high_tile.0 >= requested.min.x
            && high_tile.0 < requested.max.x
            && high_tile.1 >= requested.min.y
            && high_tile.1 < requested.max.y
    );
    let re_request = heights::frontier_chunks([8, 8], [16, 16], 64, |coordinate| {
        chunks.contains_key(&coordinate) || discovered.contains(&coordinate)
    });
    assert!(re_request.contains(&(9, 8)));

    chunks.insert((9, 8), high);
    discovered.insert((9, 8));
    assert_eq!(
        heights::resident_height_bounds(chunks.values()),
        Some((0, 52))
    );
}

#[wasm_bindgen_test]
fn nonconverging_height_pick_returns_unavailable() {
    let camera = Camera {
        center: [10.0, 10.0],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let result = world_at_screen_with_height(camera, ScreenPoint { x: 128.0, y: 64.0 }, |x, y| {
        match (x, y) {
            (10, 10) => Some(2.0),
            (11, 9) => Some(0.0),
            _ => None,
        }
    });
    assert!(result.is_none());
}
