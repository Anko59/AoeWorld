use super::terrain_blend::{assert_pixel, read_pixel_with_clear, surface_renderer};
use super::*;
use crate::game_renderer::species_fixture as fixture;

#[wasm_bindgen_test]
async fn webgpu_native_species_body_and_alpha_silhouette_pixels_and_removal() {
    let mut renderer = surface_renderer().await;
    renderer.upload_game_atlas(&fixture::atlas()).unwrap();
    let art = fixture::art();
    let clear = fixture::CLEAR.map(|v| f64::from(v) / 255.0);
    let color = wgpu::Color {
        r: clear[0],
        g: clear[1],
        b: clear[2],
        a: clear[3],
    };
    for family in [2, 4] {
        let sprites = fixture::drawn(&art, &[fixture::resource(family, 0)])
            .into_iter()
            .map(|(sprite, _, _, _)| sprite)
            .collect::<Vec<_>>();
        renderer.render_world_layers(&[], &sprites, clear).unwrap();
        for (probe, expected) in fixture::PROBES.into_iter().zip(fixture::expected(family)) {
            assert_pixel(
                read_pixel_with_clear(&renderer, 2, probe, color).await,
                expected,
            );
        }
        assert!(fixture::drawn(&art, &[]).is_empty());
        renderer.render_world_layers(&[], &[], clear).unwrap();
        for probe in fixture::PROBES {
            assert_pixel(
                read_pixel_with_clear(&renderer, 0, probe, color).await,
                fixture::CLEAR,
            );
        }
    }
    renderer.device.destroy();
}
