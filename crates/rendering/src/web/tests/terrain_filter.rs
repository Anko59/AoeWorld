use super::terrain_blend::{assert_pixel, read_pixel, surface_renderer_with_atlas};
use super::*;
use crate::game_renderer::filter_fixture as fixture;

#[wasm_bindgen_test]
async fn webgpu_terrain_filter_attenuates_checker_preserves_coverage_depth_and_rects() {
    let mut renderer = surface_renderer_with_atlas(&fixture::atlas()).await;
    for index in 0..fixture::CASES {
        let (mut faces, probe, expected) = fixture::case(index);
        for _ in 0..2 {
            renderer
                .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            assert_pixel(read_pixel(&renderer, 2, probe).await, expected);
            faces.reverse();
        }
    }
    renderer
        .render_world_layers(&[], &[fixture::sprite().0], [0.0, 0.0, 0.0, 1.0])
        .unwrap();
    assert_pixel(
        read_pixel(&renderer, 1, [48, 48]).await,
        [255, 255, 255, 255],
    );
    renderer.device.destroy();
}
