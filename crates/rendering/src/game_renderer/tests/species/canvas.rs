use super::*;
use crate::game_renderer::species_fixture as fixture;

#[wasm_bindgen_test]
fn canvas_native_species_body_and_alpha_silhouette_pixels_and_removal() {
    let (canvas, context) = target_canvas().expect("actual Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = fixture::atlas();
    let art = fixture::art();
    for family in [2, 4] {
        let drawn = fixture::drawn(&art, &[fixture::resource(family, 0)]);
        let layers = drawn
            .iter()
            .map(|(sprite, frame, depth, id)| WorldLayer::Sprite(*sprite, *frame, *depth, *id))
            .collect::<Vec<_>>();
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &layers,
            fixture::camera(),
            false,
        )
        .unwrap();
        for (probe, expected) in fixture::PROBES.into_iter().zip(fixture::expected(family)) {
            assert_eq!(
                pixel(&presentation.color_buffer, probe[0], probe[1]),
                expected
            );
        }
        let removed = fixture::drawn(&art, &[]);
        assert!(removed.is_empty());
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &[],
            fixture::camera(),
            false,
        )
        .unwrap();
        for [x, y] in fixture::PROBES {
            assert_eq!(pixel(&presentation.color_buffer, x, y), fixture::CLEAR);
        }
    }
}
