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

#[wasm_bindgen_test]
fn gpu_ellipse_depth_grows_downward_to_clear_flat_ground_under_the_lower_arc() {
    let camera = SceneCamera {
        center: [0.5; 2],
        viewport: [1280.0, 720.0],
        zoom: 1.0,
        focus_elevation_meters: 0.0,
    };
    let ellipse = selection_ellipse(
        camera,
        [0.5; 2],
        0.0,
        [2.0 * ISO_TILE_WIDTH, 2.0 * ISO_TILE_HEIGHT],
    );
    let (sprite, depth) = ellipse[0];
    let origin = surface_depth([0.5, 0.5, 0.0]);
    let [top, bottom, ..] = selection_instance_depths(&sprite, depth, origin);
    // One depth unit is 16 pixels at zoom one; the stroke reaches 65 below.
    assert!(f64::from(bottom) >= 65.0 / 16.0, "{bottom}");
    assert!(top < bottom);
    assert!(f64::from(top) <= -63.0 / 16.0 + 1.4 + 1e-3);
}
