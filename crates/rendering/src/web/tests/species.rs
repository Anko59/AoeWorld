use super::terrain_blend::{assert_pixel, read_pixels_with_clear, surface_renderer_with_atlas};
use super::*;
use crate::game_renderer::species_fixture as fixture;

#[wasm_bindgen_test]
async fn webgpu_native_species_body_and_alpha_silhouette_pixels_and_removal() {
    let mut renderer = surface_renderer_with_atlas(&fixture::atlas()).await;
    let art = fixture::art();
    let clear = fixture::CLEAR.map(|v| f64::from(v) / 255.0);
    for family in [2, 4] {
        let sprites = fixture::drawn(&art, &[fixture::resource(family, 0)])
            .into_iter()
            .map(|(sprite, _, _, _)| sprite)
            .collect::<Vec<_>>();
        renderer.render_world_layers(&[], &sprites, clear).unwrap();
        let pixels = read_pixels_with_clear(&renderer, 2, fixture::PROBES, clear).await;
        for (pixel, expected) in pixels.into_iter().zip(fixture::expected(family)) {
            assert_pixel(pixel, expected);
        }
        assert!(fixture::drawn(&art, &[]).is_empty());
        renderer.render_world_layers(&[], &[], clear).unwrap();
        for pixel in read_pixels_with_clear(&renderer, 0, fixture::PROBES, clear).await {
            assert_pixel(pixel, fixture::CLEAR);
        }
    }
    renderer.device.destroy();
}
