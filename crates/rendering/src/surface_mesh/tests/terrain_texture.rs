use super::*;
use crate::surface_mesh::terrain_texture_frame;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn terrain_atlas_selection_follows_reviewed_ten_by_ten_neighbor_order() {
    let frames = (0..100)
        .map(|index| GameFrame {
            uv: [index as f32, 0.0, 0.01, 0.01],
            size: [97.0, 49.0],
            anchor: [0.0; 2],
        })
        .collect::<Vec<_>>();
    let mut art = test_art(frames[0]);
    art.terrain[0] = frames.clone();
    art.grass.clone_from(&frames);

    for (tile, expected) in [
        ([0, 0], 0),
        ([1, 0], 10),
        ([0, 1], 9),
        ([9, 9], 91),
        ([-1, -1], 91),
        ([i32::MIN, i32::MIN], 28),
    ] {
        assert_eq!(
            terrain_texture_frame(&art, 0, tile).map(|frame| frame.uv[0]),
            Some(expected as f32),
            "tile {tile:?} selected the wrong texture frame"
        );
    }
}

#[wasm_bindgen_test]
fn partial_terrain_variant_sets_vary_on_both_axes() {
    let frames = (0..10)
        .map(|index| GameFrame {
            uv: [index as f32, 0.0, 0.01, 0.01],
            size: [97.0, 49.0],
            anchor: [0.0; 2],
        })
        .collect::<Vec<_>>();
    let mut art = test_art(frames[0]);
    art.terrain[1] = frames;
    let origin = terrain_texture_frame(&art, 1, [0, 0]).unwrap().uv;
    assert_ne!(origin, terrain_texture_frame(&art, 1, [1, 0]).unwrap().uv);
    assert_ne!(origin, terrain_texture_frame(&art, 1, [0, 1]).unwrap().uv);
}
