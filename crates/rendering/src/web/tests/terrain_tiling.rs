use super::terrain_blend::{read_pixels, surface_renderer_with_atlas};
use super::*;
use crate::surface_mesh::directory::fixture;

#[wasm_bindgen_test]
async fn webgpu_terrain_texels_are_world_anchored_across_zoom_and_mesh_lod() {
    let mut renderer = surface_renderer_with_atlas(&fixture::atlas()).await;
    let mut dirt = Vec::new();
    for case in 0..fixture::CASES {
        let faces = fixture::scene(case);
        renderer
            .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        // One bounded readback per case: the zoom pair (0, 1) and the mesh LOD
        // pair (2, 3) each share five probes, together covering all ten.
        let half = case / 2 % 2;
        let probes: [[u32; 2]; 5] = std::array::from_fn(|i| fixture::PROBES[half * 5 + i]);
        let pixels = read_pixels(&renderer, faces.len() as u32, probes).await;
        for (index, actual) in pixels.into_iter().enumerate() {
            fixture::assert_probe(case, half * 5 + index, actual, 1);
            if matches!(case, 2 | 3) {
                dirt.push(actual);
            }
        }
    }
    let (fine, coarse) = dirt.split_at(dirt.len() / 2);
    assert_eq!(fine, coarse);
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_blends_every_ground_material_pair_across_a_noisy_border() {
    use crate::surface_mesh::mix::fixture as mix;
    let mut renderer = surface_renderer_with_atlas(&mix::atlas()).await;
    for case in 0..mix::CASES {
        let faces = mix::scene(case);
        renderer
            .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        // One bounded readback per case; even and odd cases cover all ten probes.
        let probes: [[u32; 2]; 5] = std::array::from_fn(|i| mix::PROBES[i * 2 + case % 2]);
        let pixels = read_pixels(&renderer, faces.len() as u32, probes).await;
        for (index, actual) in pixels.into_iter().enumerate() {
            mix::assert_probe(case, index * 2 + case % 2, actual, 2);
        }
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_four_material_junction_has_matching_pixels_on_every_shared_edge() {
    use crate::surface_mesh::mix::fixture::{self as mix, junction};
    let mut renderer = surface_renderer_with_atlas(&mix::atlas()).await;
    let mut results = Vec::new();
    for pair in junction::edge_pairs() {
        let mut pixels = [[0; 4]; 2];
        for (index, face) in pair.iter().enumerate() {
            renderer
                .render_world_layers(&[*face], &[], [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            pixels[index] = read_pixels(&renderer, 1, [[64, 64]]).await[0];
        }
        results.push((pair, pixels));
    }
    renderer.device.destroy();
    for (pair, pixels) in results {
        junction::assert_edge(&pair, pixels);
    }
}

#[wasm_bindgen_test]
async fn webgpu_snow_is_white_ice_stays_blue_and_their_ground_borders_are_soft() {
    use crate::surface_mesh::mix::fixture as mix;
    let mut renderer = surface_renderer_with_atlas(&mix::snow_atlas()).await;
    for case in [2, 3, 7, 8] {
        let faces = mix::snow_scene(case);
        renderer
            .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        let probes: [[u32; 2]; 5] = std::array::from_fn(|i| mix::PROBES[i * 2 + case % 2]);
        let pixels = read_pixels(&renderer, faces.len() as u32, probes).await;
        for (i, actual) in pixels.into_iter().enumerate() {
            mix::assert_snow_probe(case, i * 2 + case % 2, actual);
        }
    }
    renderer.device.destroy();
}
