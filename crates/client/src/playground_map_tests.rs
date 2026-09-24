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
