use super::terrain_blend::{read_pixels, surface_renderer_with_atlas};
use super::*;
use crate::surface_mesh::directory::fixture;

#[wasm_bindgen_test]
async fn webgpu_shallow_water_fades_into_sand_without_extra_draws_or_programs() {
    use crate::surface_mesh::mix::fixture as shore;
    let mut renderer = surface_renderer_with_atlas(&shore::atlas()).await;
    let terrain = shore::terrain(0)
        .into_iter()
        .map(|mut sample| {
            sample.material = 3;
            sample
        })
        .collect::<Vec<_>>();
    let mut inland = crate::surface_mesh::projected_surface_triangles(&terrain, shore::camera(0));
    crate::surface_mesh::apply_terrain_textures(&mut inland, &shore::art());
    let baseline = renderer
        .render_world_layers(&inland, &[], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    let baseline_mixed_program = renderer.mixed_pipeline.is_some();
    let mut results = Vec::new();
    for zoom in [0.2, 0.4] {
        let faces = shore::shore_scene(zoom);
        let counters = renderer
            .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        assert_eq!(
            counters.draw_calls, baseline.draw_calls,
            "shore blending adds no draw calls"
        );
        assert_eq!(
            renderer.mixed_pipeline.is_some(),
            baseline_mixed_program,
            "shore blending adds no shader program"
        );
        results.push(read_pixels(&renderer, faces.len() as u32, shore::SHORE_PROBES).await);
    }
    renderer.device.destroy();
    for pixels in results {
        shore::assert_shore(pixels);
    }
}

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

#[wasm_bindgen_test]
async fn webgpu_one_level_sand_water_bank_covers_the_review_probe() {
    use crate::surface_mesh::mix::fixture::{self as shore, gap};
    let mut renderer = surface_renderer_with_atlas(&shore::atlas()).await;
    renderer.resize(256, 128);
    let mut pixels = Vec::new();
    for halo in [false, true] {
        let faces = gap::scene(halo);
        renderer
            .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        pixels.push(read_pixels(&renderer, faces.len() as u32, [gap::PROBE]).await[0]);
    }
    renderer.device.destroy();
    for (halo, pixel) in [false, true].into_iter().zip(pixels) {
        gap::assert_pixel(pixel, halo);
    }
}

#[wasm_bindgen_test]
async fn webgpu_building_wear_darkens_native_textured_terrain() {
    use crate::game_renderer::wear_fixture as wear;
    let mut renderer = surface_renderer_with_atlas(&fixture::atlas()).await;
    let mut probes = Vec::new();
    for building in [false, true] {
        let faces = wear::faces(building);
        renderer
            .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        let five = std::array::from_fn::<_, 5, _>(|i| fixture::PROBES[i]);
        probes.push(read_pixels(&renderer, faces.len() as u32, five).await);
    }
    renderer.device.destroy();
    wear::assert_worn(&probes[0], &probes[1]);
}

#[wasm_bindgen_test]
async fn webgpu_slope_shading_is_directional_and_zoom_consistent() {
    use crate::game_renderer::relief_fixture as relief;
    let mut renderer = surface_renderer_with_atlas(&relief::atlas()).await;
    let mut ratios = Vec::new();
    for far in [false, true] {
        let mut probes = Vec::new();
        for rise in [0, 1, -1] {
            let faces = relief::faces(far, rise);
            renderer
                .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            probes.push(read_pixels(&renderer, faces.len() as u32, relief::PROBES).await);
        }
        ratios.push(relief::assert_directional(
            far, &probes[0], &probes[1], &probes[2],
        ));
    }
    renderer.device.destroy();
    relief::assert_zoom_agreement(ratios[0], ratios[1]);
}

#[wasm_bindgen_test]
async fn webgpu_slope_shade_is_continuous_across_shared_edges() {
    use crate::game_renderer::relief_fixture as relief;
    let mut renderer = surface_renderer_with_atlas(&relief::atlas()).await;
    let mut results = Vec::new();
    for (far, textured) in [(false, true), (true, true), (false, false), (true, false)] {
        let (faces, probes, gap) = relief::seam(far, textured);
        renderer
            .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        results.push((
            far,
            gap,
            read_pixels(&renderer, faces.len() as u32, probes).await,
        ));
    }
    renderer.device.destroy();
    for (far, gap, probes) in results {
        relief::assert_continuous(far, gap, &probes);
    }
}

#[wasm_bindgen_test]
async fn webgpu_slope_beside_missing_terrain_still_renders_brighter_than_flat() {
    use crate::game_renderer::relief_fixture as relief;
    let mut renderer = surface_renderer_with_atlas(&relief::atlas()).await;
    let (faces, probes) = relief::clipped_slope();
    renderer
        .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    let pixels = read_pixels(&renderer, faces.len() as u32, probes).await;
    renderer.device.destroy();
    relief::assert_clipped_lit(&pixels);
}
