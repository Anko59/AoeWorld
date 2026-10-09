use super::*;

// Frozen pre-optimization empty-terrain path, including y-major insertion and
// BTreeMap lexicographic traversal, canonical position and scale overwrite.
fn original(art: &GameArt, camera: SceneCamera) -> Vec<(Sprite, GameFrame)> {
    if art.grass.is_empty() {
        return Vec::new();
    }
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let bounds = visible_bounds(projection);
    let mut size = 1_i32;
    while canonical_cell_count(bounds, size) > MAX_VISIBLE_TERRAIN_SPRITES {
        size = size.saturating_add(1);
    }
    let ((min_x, max_x), (min_y, max_y)) = canonical_cell_bounds(bounds, size);
    let mut cells = BTreeMap::new();
    for y in min_y..max_y {
        for x in min_x..max_x {
            let tile = [x.saturating_mul(size), y.saturating_mul(size)];
            let Some(frame) = terrain_texture_frame(art, 0, tile).map(terrain_frame) else {
                continue;
            };
            let sample = SceneTerrain {
                position: cell_center((x, y), size),
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: None,
            };
            if let Some(candidate) = terrain_candidate(&projection, sample, frame, camera, size) {
                assert!(
                    cells.insert((x, y), candidate).is_none(),
                    "canonical keys are unique"
                );
            }
        }
    }
    render_cells(&projection, camera, size, cells)
}

#[wasm_bindgen_test]
fn empty_direct_frames_match_original_btree_order_packets_scale_and_frame_selection() {
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 1,
            uv: [0.0, 0.0, 0.01, 0.01],
        },
        size: [97.0, 49.0],
        anchor: [48.0, 24.0],
    };
    let mut art = test_art(frame);
    art.terrain[0] = (0..7)
        .map(|index| GameFrame {
            atlas: crate::AtlasAddress {
                page: (index % 2) as u32,
                uv: [index as f32 / 100.0, 0.0, 0.01, 0.01],
            },
            ..frame
        })
        .collect();
    for (center, zoom, viewport) in [
        ([0.5, 0.5], 1.0, [256.0, 128.0]),
        ([300.0, 200.0], 0.25, [1280.0, 720.0]),
        ([-30.25, -40.75], 3.0, [641.0, 481.0]),
        ([0.0, 0.0], 0.05, [1280.0, 720.0]),
    ] {
        let camera = SceneCamera {
            center,
            zoom,
            viewport,
            focus_elevation_meters: 0.0,
        };
        let expected = original(&art, camera);
        let actual = empty_terrain_frames(&art, camera);
        assert_eq!(actual.len(), expected.len());
        assert!(actual.len() <= MAX_VISIBLE_TERRAIN_SPRITES);
        for ((packet, frame), (old_packet, old_frame)) in actual.iter().zip(expected.iter()) {
            assert_eq!(bytemuck::bytes_of(packet), bytemuck::bytes_of(old_packet));
            assert_eq!(frame.atlas, old_frame.atlas);
            assert_eq!(
                frame.size.map(f32::to_bits),
                old_frame.size.map(f32::to_bits)
            );
            assert_eq!(
                frame.anchor.map(f32::to_bits),
                old_frame.anchor.map(f32::to_bits)
            );
        }
    }
    art.grass.clear();
    assert!(
        empty_terrain_frames(
            &art,
            SceneCamera {
                center: [0.0; 2],
                zoom: 1.0,
                viewport: [256.0, 128.0],
                focus_elevation_meters: 0.0
            }
        )
        .is_empty()
    );
}
