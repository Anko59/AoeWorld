use super::*;
use aoe_map::MapChunkGenerator;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn snow_ice_mud_and_shallows_have_distinct_scene_materials() {
    let materials = [
        GroundMaterial::DryGrass,
        GroundMaterial::Sand,
        GroundMaterial::Water,
        GroundMaterial::Snow,
        GroundMaterial::Ice,
        GroundMaterial::Mud,
        GroundMaterial::Shore,
    ];
    for (index, material) in materials.iter().enumerate() {
        for other in &materials[..index] {
            assert_ne!(terrain_material(*material), terrain_material(*other));
        }
    }
    assert_eq!(terrain_material(GroundMaterial::Snow), 7);
    assert_eq!(terrain_material(GroundMaterial::Rock), 4);
}

#[path = "playground_map/resource_viewport_tests.rs"]
mod resource_viewport;

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
fn authoritative_bounds_request_unseen_high_relief_before_residency() {
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
    let high_tile = (290, 272);
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
    let high_index = fixture_chunk_tile_index(high_tile, (9, 8));
    let evidence = &mut chunk.tiles[high_index];
    evidence.game_height_level = 52;
    evidence.surface.corner_game_height_levels = [52; 4];
    assert_eq!(heights::chunk_height_bounds(&chunk), Some((0, 52)));

    let discovered = heights::candidate_chunks(camera, config, (0, 52), MAX_CACHED_CHUNKS);
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
fn bounded_height_discovery_is_independent_of_map_area() {
    let Ok(small) = aoe_core::WorldConfig::new(512, 512, aoe_core::Seed(1)) else {
        assert!(false, "valid test configuration");
        return;
    };
    let Ok(large) = aoe_core::WorldConfig::new(262_144, 262_144, aoe_core::Seed(1)) else {
        assert!(false, "valid test configuration");
        return;
    };
    let camera = Camera::new([256.0, 256.0], [1280.0, 720.0]);
    let near = heights::candidate_chunks(camera, small, (0, 10), MAX_CACHED_CHUNKS);
    let far = heights::candidate_chunks(camera, large, (0, 10), MAX_CACHED_CHUNKS);
    assert_eq!(near, far);
    assert!(near.len() < 32);
    assert!(!near.contains(&(0, 0)));
    assert!(!near.contains(&(15, 15)));
    let extreme = heights::candidate_chunks(camera, large, (i16::MIN, i16::MAX), MAX_CACHED_CHUNKS);
    assert_eq!(extreme.len(), MAX_CACHED_CHUNKS);
    assert!(heights::candidate_chunks(camera, large, (0, 0), 0).is_empty());
}

#[wasm_bindgen_test]
fn height_sweep_contains_projected_visible_chunks_at_every_tested_height() {
    let Ok(config) = aoe_core::WorldConfig::new(4096, 4096, aoe_core::Seed(1)) else {
        assert!(false, "valid test configuration");
        return;
    };
    for zoom in [0.25, 1.0, 3.0] {
        let camera = Camera {
            center: [2048.0, 2048.0],
            zoom,
            viewport: [1280.0, 720.0],
            focus_elevation_meters: 20.0,
        };
        let chunks = heights::candidate_chunks(camera, config, (-100, 300), MAX_CACHED_CHUNKS);
        assert!(chunks.len() < MAX_CACHED_CHUNKS);
        for height in [-100.0, 0.0, 52.0, 300.0] {
            for x in (0..=32).map(|index| f64::from(index) * 40.0) {
                for y in (0..=24).map(|index| f64::from(index) * 30.0) {
                    let point = camera.screen_to_world_at_height(ScreenPoint { x, y }, height);
                    let chunk = (
                        (point[0].floor() as i32).div_euclid(CHUNK_TILES),
                        (point[1].floor() as i32).div_euclid(CHUNK_TILES),
                    );
                    assert!(
                        chunks.contains(&chunk),
                        "height {height}, zoom {zoom}: missing {chunk:?}"
                    );
                }
            }
        }
    }
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
    let high_tile = (290, 272);
    let high_index = fixture_chunk_tile_index(high_tile, (9, 8));
    let Ok(mut high) = MapChunkGenerator::new([0; 32], 1, 512).chunk(9, 8) else {
        assert!(false, "high-relief fetch chunk");
        return;
    };
    for tile in &mut high.tiles {
        tile.game_height_level = 0;
        tile.surface.corner_game_height_levels = [0; 4];
    }
    high.tiles[high_index].game_height_level = 52;
    high.tiles[high_index].surface.corner_game_height_levels = [52; 4];

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
        &[],
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
    let re_request = heights::candidate_chunks(camera, config, (0, 52), MAX_CACHED_CHUNKS);
    assert!(re_request.contains(&(9, 8)));

    chunks.insert((9, 8), high);
    // A visible high chunk can be farther in world coordinates than an
    // off-screen low chunk. Eviction must retain requested relief.
    evict_distant_chunks_with_limits(
        &mut chunks,
        &mut discovered,
        camera,
        config,
        1,
        MAX_CACHED_CHUNK_BYTES,
        &[(9, 8)],
    );
    assert!(!chunks.contains_key(&(8, 8)));
    discovered.insert((9, 8));
    let Some(resident) = chunks.get(&(9, 8)) else {
        assert!(false, "high-relief chunk is resident");
        return;
    };
    assert_eq!(resident.tiles[high_index].game_height_level, 52);
    assert_eq!(
        heights::resident_height_bounds(chunks.values()),
        Some((0, 52))
    );
}

fn fixture_chunk_tile_index(tile: (i32, i32), chunk: (i32, i32)) -> usize {
    let Ok(local_x) = usize::try_from(tile.0 - chunk.0 * CHUNK_TILES) else {
        assert!(false, "local x is inside the chunk");
        return 0;
    };
    let Ok(local_y) = usize::try_from(tile.1 - chunk.1 * CHUNK_TILES) else {
        assert!(false, "local y is inside the chunk");
        return 0;
    };
    local_y * (CHUNK_TILES as usize) + local_x
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

#[wasm_bindgen_test]
fn remote_high_relief_is_probed_beyond_the_decoded_cache_and_low_probes_are_skipped() {
    let Ok(config) = aoe_core::WorldConfig::new(262_144, 262_144, aoe_core::Seed(1)) else {
        assert!(false, "valid large map");
        return;
    };
    let camera = Camera {
        zoom: 0.25,
        ..Camera::new([65_536.0, 65_536.0], [1280.0, 720.0])
    };
    let world = camera.screen_to_world_at_height(ScreenPoint { x: 640.0, y: 360.0 }, 9_000.0);
    let high = (
        (world[0] as i32).div_euclid(CHUNK_TILES),
        (world[1] as i32).div_euclid(CHUNK_TILES),
    );
    let truncated = heights::candidate_chunks(camera, config, (0, 9_000), MAX_CACHED_CHUNKS);
    assert!(
        !truncated.contains(&high),
        "fixture must exceed decoded cache radius"
    );
    let discovery = heights::candidate_chunks(camera, config, (0, 9_000), 65_536);
    assert!(discovery.len() < 65_536);
    assert!(discovery.contains(&high));
    assert!(heights::chunk_may_be_visible(
        camera,
        config,
        high,
        (9_000, 9_000)
    ));
    assert!(!heights::chunk_may_be_visible(camera, config, high, (0, 0)));
    let moved = Camera::new(world, [1280.0, 720.0]);
    assert!(heights::chunk_may_be_visible(moved, config, high, (0, 0)));
}

#[wasm_bindgen_test]
fn height_probe_eviction_keeps_new_low_coordinates_and_bounds_duplicate_metadata() {
    let mut probes = std::collections::BTreeMap::new();
    let mut order = std::collections::VecDeque::new();
    for coordinate in [(100, 100), (200, 200), (0, 0)] {
        heights::remember_probe(&mut probes, &mut order, coordinate, (0, 10), 2);
    }
    assert_eq!(probes.len(), 2);
    assert_eq!(order.len(), 2);
    assert!(probes.contains_key(&(0, 0)));
    assert!(!probes.contains_key(&(100, 100)));
    for _ in 0..100 {
        heights::remember_probe(&mut probes, &mut order, (0, 0), (-1, 20), 2);
    }
    assert_eq!(order.len(), 2);
    assert_eq!(probes[&(0, 0)], (-1, 20));
    heights::remember_probe(&mut probes, &mut order, (0, 0), (0, 0), 0);
    assert!(probes.is_empty() && order.is_empty());
}
