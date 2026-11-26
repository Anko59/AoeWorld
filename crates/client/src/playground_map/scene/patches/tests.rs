use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

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

    for _ in 0..client.terrain_chunks.len() {
        cache.advance(&client, camera);
    }
    let before = cache.scene(&client).unwrap();
    assert!(!before.triangles.is_empty());
    let identity = cache.patches[&(3, 3)].faces.as_ptr();
    client.camera.center[0] += 0.25;
    assert!(!cache.advance(&client, client.camera));
    let after = cache.scene(&client).unwrap();
    assert_eq!(cache.patches[&(3, 3)].faces.as_ptr(), identity);
    assert_ne!(
        before.triangles[0].points[0].screen,
        after.triangles[0].points[0].screen
    );
    client.canvas.remove();
}
