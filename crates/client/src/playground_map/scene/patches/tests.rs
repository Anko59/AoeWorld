use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn flat_chunk(source: &aoe_map::MapChunkGenerator, (x, y): Coordinate) -> CachedChunk {
    let mut chunk = source.landscape_chunk_with_cancel(x, y, &|| false).unwrap();
    for tile in &mut chunk.tiles {
        tile.terrain.surface.corner_game_height_levels = [0; 4];
    }
    CachedChunk::from(chunk)
}

#[wasm_bindgen_test]
async fn a_complete_ground_patch_can_paint_before_material_refinement() {
    let camera = Camera::new([16.0; 2], [128.0; 2]);
    let fixture = PreparedScene::new(camera, vec![]);
    let mut client = super::super::review_tests::client(&fixture).await;
    client.map_content_hash = Some([1; 32]);
    let source = aoe_map::MapChunkGenerator::new([1; 32], 1, 32);
    let chunk = flat_chunk(&source, (0, 0));
    include_chunk_height_bounds(&mut client, &chunk);
    client.terrain_chunks.insert((0, 0), chunk);
    let mut cache = PatchCache::default();
    assert!(cache.advance(&client, camera));
    let base = cache
        .scene(&client)
        .expect("base patch already covers the view");
    assert!(
        cache.patches.get(&(0, 0)).unwrap().dirty,
        "material refinement is still queued"
    );
    let height = base.height(camera.center).unwrap();
    assert!(cache.advance(&client, camera));
    assert!(!cache.patches.get(&(0, 0)).unwrap().dirty);
    assert_eq!(
        cache.scene(&client).unwrap().height(camera.center),
        Some(height)
    );
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn unfinished_prefetch_does_not_hold_complete_visible_terrain() {
    let camera = Camera::new([50.0; 2], [320.0; 2]);
    let fixture = PreparedScene::new(camera, vec![]);
    let mut client = super::super::review_tests::client(&fixture).await;
    client.map_content_hash = Some([1; 32]);
    let source = aoe_map::MapChunkGenerator::new([1; 32], 1, 64);
    for key in [(0, 0), (1, 1)] {
        let chunk = flat_chunk(&source, key);
        include_chunk_height_bounds(&mut client, &chunk);
        client.terrain_chunks.insert(key, chunk);
    }
    assert!(heights::resident_may_be_visible(&client, (0, 0)));
    let mut cache = PatchCache::default();
    cache
        .patches
        .insert((1, 1), Patch::build(&client, (1, 1), 1, true));
    let scene = cache.scene(&client).expect("visible patch is ready");
    assert_eq!(scene.height([50.0; 2]), Some(0.0));
    client.camera.center = [16.0; 2];
    assert!(cache.scene(&client).is_none(), "visible patch still blocks");
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn prepares_visible_chunks_over_turns_and_reuses_mesh_when_camera_moves() {
    let camera = Camera {
        center: [96.0; 2],
        zoom: 0.102,
        viewport: [1280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    let fixture = PreparedScene::new(camera, vec![]);
    let mut client = super::super::review_tests::client(&fixture).await;
    client.map_content_hash = Some([1; 32]);
    client.config.width_tiles = 256;
    client.config.height_tiles = 256;
    let source = aoe_map::MapChunkGenerator::new([1; 32], 1, 256);
    for y in 2..=4 {
        for x in 2..=4 {
            let chunk =
                CachedChunk::from(source.landscape_chunk_with_cancel(x, y, &|| false).unwrap());
            client.terrain_chunks.insert((x, y), chunk);
        }
    }

    let mut cache = PatchCache::default();
    assert!(cache.advance(&client, camera));
    assert!(cache.patches.len() <= MAX_PREPARATIONS);
    assert!(cache.scene(&client).is_none());

    for _ in 0..2 * client.terrain_chunks.len() {
        cache.advance(&client, camera);
    }
    let before = cache.scene(&client).unwrap();
    assert!(!before.triangles.is_empty());
    let identity = cache.patches.get(&(3, 3)).unwrap().faces.as_ptr();
    client.camera.center[0] += 0.25;
    assert!(!cache.advance(&client, client.camera));
    let after = cache.scene(&client).unwrap();
    assert_eq!(cache.patches.get(&(3, 3)).unwrap().faces.as_ptr(), identity);
    assert_ne!(
        before.triangles[0].points[0].screen,
        after.triangles[0].points[0].screen
    );
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn evicted_sources_invalidate_neighboring_patches_without_discarding_them() {
    let camera = Camera::new([32.0, 16.0], [320.0; 2]);
    let fixture = PreparedScene::new(camera, vec![]);
    let mut client = super::super::review_tests::client(&fixture).await;
    let source = aoe_map::MapChunkGenerator::new([1; 32], 1, 64);
    let mut cache = PatchCache {
        size: 1,
        ..Default::default()
    };
    for key in [(0, 0), (1, 0)] {
        client.terrain_chunks.insert(key, flat_chunk(&source, key));
    }
    for key in [(0, 0), (1, 0)] {
        cache
            .patches
            .insert(key, Patch::build(&client, key, 1, true));
    }
    let neighbor = cache.patches.get(&(1, 0)).unwrap().faces.as_ptr();
    client.terrain_chunks.remove(&(0, 0));
    cache.source_evicted(&client.terrain_chunks);
    assert!(cache.patches.get(&(0, 0)).is_none());
    assert!(cache.patches.get(&(1, 0)).unwrap().dirty);
    assert_eq!(cache.patches.get(&(1, 0)).unwrap().faces.as_ptr(), neighbor);
    client.canvas.remove();
}
