use super::*;
use aoe_core::{Seed, project};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn backing_resolution_stays_bounded_on_large_high_dpi_screens() {
    for (width, height, ratio) in [
        (3840.0, 2160.0, 2.0),
        (8000.0, 1000.0, 3.0),
        (1280.0, 720.0, 1.0),
    ] {
        let [w, h] = backing_size(width, height, ratio);
        assert!(u64::from(w) * u64::from(h) <= MAX_BACKING_PIXELS as u64);
        assert!(w <= MAX_BACKING_AXIS as u32 && h <= MAX_BACKING_AXIS as u32);
        assert!((f64::from(w) / width - f64::from(h) / height).abs() < 0.002);
    }
    assert_eq!(backing_size(1280.0, 720.0, 1.0), [1280, 720]);
}

#[wasm_bindgen_test]
fn wheel_zoom_preserves_finite_nonfinite_and_clamped_pointer_zoom() {
    let camera = Camera {
        center: [256.0, 256.0],
        zoom: 1.0,
        viewport: [1280.0, 720.0],
        focus_elevation_meters: 12.0,
    };
    let pointer = ScreenPoint { x: 384.0, y: 216.0 };
    for delta in [
        -1e6,
        -480.0,
        -16.0,
        -0.0,
        0.0,
        16.0,
        480.0,
        1e6,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NAN,
    ] {
        let old_zoom = camera.zoom * (-delta * 0.0015).exp();
        let new_zoom = wheel_zoom(camera.zoom, delta);
        if old_zoom.is_nan() {
            assert!(new_zoom.is_nan());
            assert!(camera.zoom_around(pointer, new_zoom).zoom.is_nan());
            continue;
        }
        if old_zoom.is_infinite() {
            assert_eq!(new_zoom, old_zoom);
        } else {
            assert!((new_zoom - old_zoom).abs() <= old_zoom.abs() * 1e-14);
        }
        let old = camera.zoom_around(pointer, old_zoom);
        let new = camera.zoom_around(pointer, new_zoom);
        assert!((new.zoom - old.zoom).abs() < 1e-13);
        assert!((aoe_core::MIN_CAMERA_ZOOM..=3.0).contains(&new.zoom));
        let before = camera.screen_to_world_at_height(pointer, 12.0);
        let after = new.screen_to_world_at_height(pointer, 12.0);
        for axis in 0..2 {
            assert!((new.center[axis] - old.center[axis]).abs() < 1e-10);
            assert!((after[axis] - before[axis]).abs() < 1e-10);
        }
    }
}

#[wasm_bindgen_test]
fn camera_pan_moves_in_the_requested_screen_direction() {
    let config = WorldConfig::new(512, 512, Seed(1)).unwrap();
    let camera = Camera {
        center: [256.0, 256.0],
        zoom: 1.0,
        viewport: [1280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    let initial = project(camera.center);

    let left = pan_camera(camera, config, [-60.0, 0.0]);
    let right = pan_camera(camera, config, [60.0, 0.0]);
    let up = pan_camera(camera, config, [0.0, -60.0]);
    let down = pan_camera(camera, config, [0.0, 60.0]);

    assert!(project(left.center).x < initial.x);
    assert!(project(right.center).x > initial.x);
    assert!(project(up.center).y < initial.y);
    assert!(project(down.center).y > initial.y);
}
