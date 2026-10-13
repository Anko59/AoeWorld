use super::*;
use crate::surface_mesh::directory::fixture;

#[wasm_bindgen_test]
fn webgl_shallow_water_fades_into_sand_at_two_zooms() {
    use crate::surface_mesh::mix::fixture as shore;
    let (canvas, mut renderer) = target(&shore::atlas());
    for zoom in [0.2, 0.4] {
        let mut packets = shore::shore_scene(zoom)
            .iter()
            .map(instance)
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        shore::assert_shore(shore::SHORE_PROBES.map(|[x, y]| pixel(&canvas, x, y)));
        shore::assert_soft(0, &|x, y| pixel(&canvas, x, y));
    }
}

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
fn webgl_snow_is_white_ice_stays_blue_and_their_ground_borders_are_soft() {
    use crate::surface_mesh::mix::fixture as mix;
    let (canvas, mut renderer) = target(&mix::snow_atlas());
    for case in [2, 3, 7, 8] {
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

#[wasm_bindgen_test]
fn webgl_one_level_sand_water_bank_covers_the_review_probe() {
    use crate::surface_mesh::mix::fixture::{self as shore, gap};
    let (canvas, mut renderer) = target(&shore::atlas());
    renderer.resize(256, 128).unwrap();
    for halo in [false, true] {
        let mut packets = gap::scene(halo)
            .iter()
            .map(|face| surface_instance(face, [256.0, 128.0], 0.0))
            .collect::<Vec<_>>();
        render(&mut renderer, &mut packets);
        let [x, y] = gap::PROBE;
        gap::assert_pixel(pixel(&canvas, x, y), halo);
    }
}
