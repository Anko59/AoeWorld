use super::*;

fn blended_triangle() -> ProjectedSurfaceTriangle {
    let mut triangle = triangle(
        [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]],
        [0.0; 3],
        [0.0; 3],
    );
    let rect = |x: f32| [x / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0];
    triangle.texture_uv = Some(rect(0.0));
    triangle.texture_blend = Some([rect(1.0), rect(2.0)]);
    triangle
}

fn blend_atlas() -> Vec<u8> {
    let mut atlas = vec![0; crate::GAME_ATLAS_SIDE as usize * crate::GAME_ATLAS_SIDE as usize * 4];
    atlas[..12].copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
    atlas
}

fn assert_blended(pixel: &[u8]) {
    for (actual, expected) in pixel.iter().zip([82_u8, 86, 86, 255]) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "unexpected blend pixel: {pixel:?}"
        );
    }
}

#[wasm_bindgen_test]
fn canvas_crossfades_three_native_materials_with_shared_barycentric_weights() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &blend_atlas(),
        &mut presentation,
        &[WorldLayer::Surface(blended_triangle())],
        test_camera(),
        false,
    )
    .expect("Canvas splat rendering");
    assert_blended(&pixel(&presentation.color_buffer, 48, 48));
    assert_eq!(presentation.depth_buffer[48 * 128 + 48], 0.0);
}
