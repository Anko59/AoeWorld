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
