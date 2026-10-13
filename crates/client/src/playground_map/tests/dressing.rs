use super::*;

#[wasm_bindgen_test]
fn every_dressing_family_maps_to_scenery_without_mutating_map_authority() {
    let generator = MapChunkGenerator::new([0; 32], 1, 32);
    let chunk = generator
        .landscape_chunk_with_cancel(0, 0, &|| false)
        .unwrap();
    let before = (chunk.tiles.clone(), chunk.resources.clone());
    let dressed = CachedChunk::from(chunk.clone());
    let mut bare = chunk.clone();
    bare.decorations.clear();
    let bare = CachedChunk::from(bare);
    assert_eq!(
        dressed.base_tiles().collect::<Vec<_>>(),
        bare.base_tiles().collect::<Vec<_>>()
    );
    assert_eq!(
        dressed.resources().collect::<Vec<_>>(),
        bare.resources().collect::<Vec<_>>()
    );
    let mut scene = scene::PreparedScene::new(
        Camera {
            center: [16.0; 2],
            zoom: 1.0,
            viewport: [1280.0, 720.0],
            focus_elevation_meters: 0.0,
        },
        Vec::new(),
    );
    for (family, kind) in [(0, 18), (1, 19), (2, 17), (3, 20), (4, 16)] {
        scene.decorations = (0..4096)
            .map(|tile| SceneDecoration {
                position: [f64::from(tile % 64) + 0.5, f64::from(tile / 64) + 0.5],
                family,
                visual_variant: (tile % 256) as u8,
                orientation: (tile % 8) as u8,
                elevation_meters: 7.0,
            })
            .collect();
        let first = decorations::scenery(&scene).collect::<Vec<_>>();
        assert!(!first.is_empty(), "family {family} has art");
        assert!(first.iter().all(|sprite| sprite.kind == kind
            && sprite.kind >= 16
            && sprite.elevation_meters == 7.0));
        for sprite in &first {
            let source = scene
                .decorations
                .iter()
                .find(|d| d.position == sprite.position)
                .unwrap();
            assert_eq!(sprite.visual_variant, source.visual_variant);
            assert_eq!(sprite.visual_family, source.orientation);
            assert_eq!(sprite.id & 1, 1, "scenery namespace");
        }
        let expected = first.iter().map(|s| s.id).collect::<Vec<_>>();
        scene.decorations.reverse();
        let mut reversed = decorations::scenery(&scene)
            .map(|s| s.id)
            .collect::<Vec<_>>();
        reversed.reverse();
        assert_eq!(expected, reversed, "tile hash is independent of traversal");
        if family == 1 {
            assert!((850..1200).contains(&first.len()), "quarter-density grass");
        }
        if family == 0 || family == 3 {
            assert!(
                (1800..2300).contains(&first.len()),
                "half-density woody accents"
            );
        }
        if family == 2 || family == 4 {
            assert_eq!(first.len(), 4096);
        }
        scene.decorations.clear();
        assert_eq!(decorations::scenery(&scene).count(), 0);
    }
    assert_eq!(chunk.tiles, before.0, "passability remains authoritative");
    assert_eq!(chunk.resources, before.1, "dressing is never a resource");
}
