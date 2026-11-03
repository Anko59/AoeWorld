use super::*;

#[test]
fn outside_focus_stays_fixed_when_zooming_at_viewport_center() {
    let viewport = [1280.0, 720.0];
    let pointer = ScreenPoint { x: 640.0, y: 360.0 };
    for center in [[-2.0, 2.0], [514.0, 256.0], [-1000.0, 2000.0]] {
        let mut camera = Camera::new(center, viewport);
        for zoom in [0.25, 0.5, 1.0, 2.0, MIN_CAMERA_ZOOM] {
            camera = camera.zoom_around(pointer, zoom);
            assert_eq!(camera.center, center);
            assert_eq!(camera.screen_to_world(pointer), center);
        }
    }
}

#[test]
fn far_zoom_keeps_off_center_pointer_anchor_across_map_edges() {
    for center in [[0.0, 0.0], [512.0, 512.0], [-2.0, 2.0]] {
        let mut camera = Camera::new(center, [1280.0, 720.0]);
        camera.focus_elevation_meters = 12.0;
        let pointer = ScreenPoint { x: 700.0, y: 300.0 };
        let anchor = camera.screen_to_world_at_height(pointer, 12.0);
        for zoom in [0.025, MIN_CAMERA_ZOOM, 0.5, 3.0] {
            camera = camera.zoom_around(pointer, zoom);
            let after = camera.screen_to_world_at_height(pointer, 12.0);
            for (before, after) in anchor.into_iter().zip(after) {
                assert!((before - after).abs() < 1e-8);
            }
        }
    }
}
