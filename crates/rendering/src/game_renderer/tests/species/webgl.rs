use super::*;
use crate::game_renderer::species_fixture as fixture;

#[wasm_bindgen_test]
fn webgl_native_species_body_and_alpha_silhouette_pixels_and_removal() {
    let (canvas, mut renderer) = target(&fixture::atlas());
    let art = fixture::art();
    for family in [2, 4] {
        let mut sprites = fixture::drawn(&art, &[fixture::resource(family, 0)])
            .into_iter()
            .map(|(sprite, _, _, _)| sprite)
            .collect::<Vec<_>>();
        render(&mut renderer, &mut sprites);
        for (probe, expected) in fixture::PROBES.into_iter().zip(fixture::expected(family)) {
            assert_pixel(&canvas, probe[0], probe[1], expected);
        }
        assert!(fixture::drawn(&art, &[]).is_empty());
        render(&mut renderer, &mut []);
        for [x, y] in fixture::PROBES {
            assert_pixel(&canvas, x, y, fixture::CLEAR);
        }
    }
}
