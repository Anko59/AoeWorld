use super::*;
use crate::game_renderer::filter_fixture as fixture;

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
    assert_pixel(&canvas, 48, 48, [255, 255, 255, 255]);
}
