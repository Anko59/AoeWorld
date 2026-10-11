use super::*;
use crate::surface_mesh::directory::fixture;

#[wasm_bindgen_test]
fn webgl_terrain_texels_are_world_anchored_across_zoom_and_mesh_lod() {
    let (canvas, mut renderer) = target(&fixture::atlas());
    let mut dirt = Vec::new();
    for case in 0..fixture::CASES {
        let mut packets = fixture::scene(case)
            .iter()
            .map(instance)
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        for (probe, [x, y]) in fixture::PROBES.into_iter().enumerate() {
            let actual = pixel(&canvas, x, y);
            fixture::assert_probe(case, probe, actual, 1);
            if matches!(case, 2 | 3) {
                dirt.push(actual);
            }
        }
    }
    let (fine, coarse) = dirt.split_at(dirt.len() / 2);
    assert_eq!(fine, coarse);
}

#[wasm_bindgen_test]
fn webgl_blends_every_ground_material_pair_across_a_noisy_border() {
    use crate::surface_mesh::mix::fixture as mix;
    let (canvas, mut renderer) = target(&mix::atlas());
    for case in 0..mix::CASES {
        let mut packets = mix::scene(case).iter().map(instance).collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        for (probe, [x, y]) in mix::PROBES.into_iter().enumerate() {
            mix::assert_probe(case, probe, pixel(&canvas, x, y), 2);
        }
    }
}

#[wasm_bindgen_test]
fn webgl_four_material_junction_has_matching_pixels_on_every_shared_edge() {
    use crate::surface_mesh::mix::fixture::{self as mix, junction};
    let (canvas, mut renderer) = target(&mix::atlas());
    for pair in junction::edge_pairs() {
        let pixels = pair.map(|face| {
            render(&mut renderer, &mut [instance(&face)]);
            pixel(&canvas, 64, 64)
        });
        junction::assert_edge(&pair, pixels);
    }
}

#[wasm_bindgen_test]
fn webgl_native_snow_and_ice_keep_their_sheet_colours_at_soft_borders() {
    use crate::surface_mesh::mix::fixture as mix;
    let (canvas, mut renderer) = target(&mix::snow_atlas());
    for case in [2, 3] {
        let mut packets = mix::snow_scene(case)
            .iter()
            .map(instance)
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        for (probe, [x, y]) in mix::PROBES.into_iter().enumerate() {
            mix::assert_snow_probe(case, probe, pixel(&canvas, x, y));
        }
    }
}
