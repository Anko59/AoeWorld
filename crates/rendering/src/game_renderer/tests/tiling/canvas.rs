use super::*;
use crate::surface_mesh::directory::fixture;

/// The same world point shows the same native texel at two zooms and at two
/// mesh LODs; coarse patches tile by world coordinate, not by patch origin.
#[wasm_bindgen_test]
fn canvas_terrain_texels_are_world_anchored_across_zoom_and_mesh_lod() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = fixture::atlas();
    let mut dirt = Vec::new();
    for case in 0..fixture::CASES {
        let layers = fixture::scene(case)
            .into_iter()
            .map(WorldLayer::Surface)
            .collect::<Vec<_>>();
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &layers,
            fixture::camera(case),
            false,
        )
        .unwrap();
        for (probe, [x, y]) in fixture::PROBES.into_iter().enumerate() {
            let actual = pixel(&presentation.color_buffer, x, y);
            fixture::assert_probe(case, probe, actual, 0);
            if matches!(case, 2 | 3) {
                dirt.push(actual);
            }
        }
    }
    // Fine and coarse meshes under one camera: no texel changes at an LOD switch.
    let (fine, coarse) = dirt.split_at(dirt.len() / 2);
    assert_eq!(fine, coarse);
}

/// Every pair of ground materials meets in a soft, noisy band: the reference
/// blend at each probe, pure sheets away from the border, no hard step.
#[wasm_bindgen_test]
fn canvas_blends_every_ground_material_pair_across_a_noisy_border() {
    use crate::surface_mesh::mix::fixture as mix;
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = mix::atlas();
    for case in 0..mix::CASES {
        let layers = mix::scene(case)
            .into_iter()
            .map(WorldLayer::Surface)
            .collect::<Vec<_>>();
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &layers,
            mix::camera(case),
            false,
        )
        .unwrap();
        let row = mix::PROBES.map(|[x, y]| pixel(&presentation.color_buffer, x, y));
        for (probe, actual) in row.into_iter().enumerate() {
            mix::assert_probe(case, probe, actual, 1);
        }
        mix::assert_soft(case, &|x, y| pixel(&presentation.color_buffer, x, y));
    }
}

#[wasm_bindgen_test]
fn canvas_four_material_junction_has_matching_pixels_on_every_shared_edge() {
    use crate::surface_mesh::mix::fixture::{self as mix, junction};
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = mix::atlas();
    for pair in junction::edge_pairs() {
        let pixels = pair.map(|face| {
            canvas_depth::render_canvas_world(
                &canvas,
                &context,
                &atlas,
                &mut presentation,
                &[WorldLayer::Surface(face)],
                junction::camera(),
                false,
            )
            .unwrap();
            pixel(&presentation.color_buffer, 64, 64)
        });
        junction::assert_edge(&pair, pixels);
    }
}
