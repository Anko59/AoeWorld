use super::*;
use crate::game_renderer::filter_fixture as fixture;

#[wasm_bindgen_test]
fn webgl_ground_shows_black_explored_and_visible_fog() {
    let (canvas, mut renderer) = target(&fixture::atlas());
    let (faces, [x, y], _) = fixture::case(1);
    for (light, expected) in [(0.0, 0), (0.45, 115), (1.0, 255)] {
        let mut packet = instance(&faces[1]);
        crate::game_renderer::fog::surface_packet(&mut packet, [light; 3]);
        render(&mut renderer, &mut [packet]);
        assert_pixel(&canvas, x, y, [expected, expected, expected, 255]);
    }
}

#[wasm_bindgen_test]
fn webgl_terrain_filter_attenuates_checker_preserves_coverage_depth_and_rects() {
    let (canvas, mut renderer) = target(&fixture::atlas());
    for index in 0..fixture::CASES {
        let (mut faces, [x, y], expected) = fixture::case(index);
        for _ in 0..2 {
            let mut packets = faces.map(|face| instance(&face));
            render(&mut renderer, &mut packets);
            let actual = pixel(&canvas, x, y);
            assert!(
                actual
                    .into_iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.abs_diff(expected) <= 1),
                "filter case{index} pixel{x},{y}: actual{actual:?} expected{expected:?}, geometry{:?}",
                packets.map(|p| (p.position, p.radius, p.color, p.depths))
            );
            faces.reverse();
        }
    }
    render(&mut renderer, &mut [fixture::sprite().0]);
    // Minified object sprites take the bounded box kernel (two white, two
    // black taps of the checker at 8x8 pixels), matching the Canvas kernel.
    assert_pixel(&canvas, 48, 48, [128, 128, 128, 255]);
}
