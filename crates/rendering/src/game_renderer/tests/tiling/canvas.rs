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
