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

#[wasm_bindgen_test]
fn webgl_dressing_anchor_pixels() {
    let (canvas, mut renderer) = target(&fixture::atlas());
    for kind in [18, 19, 20] {
        let mut sprites = fixture::drawn(&fixture::dressing_art(), &[fixture::dressing(kind)])
            .into_iter()
            .map(|(s, _, _, _)| s)
            .collect::<Vec<_>>();
        assert!(!sprites.is_empty());
        render(&mut renderer, &mut sprites);
        assert_pixel(&canvas, 60, 63, [255, 0, 0, 255]);
    }
}
