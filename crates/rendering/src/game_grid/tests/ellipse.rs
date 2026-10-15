use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn selected_units_use_one_continuous_constant_pixel_ellipse_at_every_zoom() {
    for zoom in [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0] {
        let camera = SceneCamera {
            center: [0.0; 2],
            viewport: [1280.0, 720.0],
            zoom,
            focus_elevation_meters: 0.0,
        };
        let ring = selection_ring(camera, [0.0; 2], 0.0);
        assert_eq!(ring.len(), 1);
        let (sprite, depth) = ring[0];
        assert_eq!(sprite.pages[3], 2);
        assert_eq!(sprite.uv[2], 2.0);
        assert!(sprite.radius[0] > 0.0 && sprite.radius[1] > 0.0);
        assert!(depth > surface_depth([0.0; 3]));
    }
}
