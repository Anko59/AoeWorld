use super::*;
use crate::game_renderer::filter_fixture as fixture;

#[wasm_bindgen_test]
fn canvas_terrain_filter_attenuates_checker_preserves_coverage_depth_and_rects() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = fixture::atlas();
    for index in 0..fixture::CASES {
        let (mut faces, [x, y], expected) = fixture::case(index);
        for _ in 0..2 {
            let mut baseline = faces;
            for face in &mut baseline {
                face.appearance = 0;
            }
            canvas_depth::render_canvas_world(
                &canvas,
                &context,
                &atlas,
                &mut presentation,
                &baseline.map(WorldLayer::Surface),
                test_camera(),
                false,
            )
            .unwrap();
            let depth_bits: Vec<_> = presentation
                .depth_buffer
                .iter()
                .map(|value| value.to_bits())
                .collect();
            let baseline_pick = crate::surface_mesh::pick_surface_point(
                &baseline,
                aoe_core::ScreenPoint {
                    x: f64::from(x) + 0.5,
                    y: f64::from(y) + 0.5,
                },
            );
            canvas_depth::render_canvas_world(
                &canvas,
                &context,
                &atlas,
                &mut presentation,
                &faces.map(WorldLayer::Surface),
                test_camera(),
                false,
            )
            .unwrap();
            assert_eq!(
                pixel(&presentation.color_buffer, x, y),
                expected,
                "filter case{index}"
            );
            let pick = crate::surface_mesh::pick_surface_point(
                &faces,
                aoe_core::ScreenPoint {
                    x: f64::from(x) + 0.5,
                    y: f64::from(y) + 0.5,
                },
            )
            .unwrap();
            assert_eq!(pick, [0.0, 0.0]);
            assert_eq!(Some(pick), baseline_pick);
            assert_eq!(
                presentation
                    .depth_buffer
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                depth_bits,
                "entire depth buffer changed in filter case{index}"
            );
            faces.reverse();
        }
    }
    let (sprite, frame) = fixture::sprite();
    canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut presentation,
        &[WorldLayer::Sprite(sprite, frame, 0.0, 0)],
        test_camera(),
        false,
    )
    .unwrap();
    assert_eq!(
        pixel(&presentation.color_buffer, 48, 48),
        [255, 255, 255, 255]
    );
}
