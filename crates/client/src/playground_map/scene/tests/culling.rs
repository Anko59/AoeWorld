use super::*;
use aoe_core::{Camera, ScreenPoint};

#[test]
fn culling_retains_origins_whose_art_can_overlap_each_viewport_edge() {
    for zoom in [0.25, 1.0, 3.0] {
        let mut camera = Camera::new([100.0; 2], [640.0, 480.0]);
        camera.zoom = zoom;
        let extent = [128.0, 256.0];
        for (axis, boundary, sign) in [
            (0, -extent[0] * zoom, -1.0),
            (0, 640.0 + extent[0] * zoom, 1.0),
            (1, -extent[1] * zoom, -1.0),
            (1, 480.0 + extent[1] * zoom, 1.0),
        ] {
            let at = |delta| {
                let mut p = [320.0, 240.0];
                p[axis] = boundary + delta * sign;
                camera.screen_to_world_at_height(ScreenPoint { x: p[0], y: p[1] }, 9.0)
            };
            assert!(visible(camera, at(-0.01), 9.0, extent));
            assert!(!visible(camera, at(1.0), 9.0, extent));
        }
    }
}

#[test]
fn culling_uses_the_actual_presented_height_and_camera_focus() {
    let mut camera = Camera::new([100.0; 2], [640.0, 480.0]);
    camera.focus_elevation_meters = 12.0;
    let p = camera.screen_to_world_at_height(ScreenPoint { x: 320.0, y: 240.0 }, 500.0);
    assert!(visible(camera, p, 500.0, [64.0; 2]));
    assert!(!visible(camera, p, 0.0, [64.0; 2]));
}

#[test]
fn culling_rejects_distant_actors_without_changing_world_storage() {
    let camera = Camera::new([100.0; 2], [640.0, 480.0]);
    let positions = [[100.0; 2], [1000.0, 0.0], [0.0, 1000.0]];
    let admitted: Vec<_> = positions
        .iter()
        .filter(|p| visible(camera, **p, 0.0, [512.0; 2]))
        .collect();
    assert_eq!(admitted, [&positions[0]]);
    assert_eq!(positions.len(), 3);
}

#[test]
fn uncertain_geometry_is_retained_for_the_renderer_to_handle() {
    let mut camera = Camera::new([100.0; 2], [640.0, 480.0]);
    assert!(visible(camera, [f64::NAN, 1.0], 0.0, [512.0; 2]));
    assert!(visible(camera, [1000.0; 2], 0.0, [f64::INFINITY; 2]));
    camera.zoom = 0.0;
    assert!(visible(camera, [1000.0; 2], 0.0, [512.0; 2]));
}
